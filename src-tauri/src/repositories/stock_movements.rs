//! Stock Movements Repository Module
//!
//! Handles all stock movement-related database operations.

use crate::errors::AppError;
use crate::models::{NewStockMovement, StockMovement, StockMovementDbRow, StockMovementQuery};
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

/// Repository for stock movement-related database operations
pub struct StockMovementRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> StockMovementRepository<'a> {
    /// Create a new StockMovementRepository with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Insert a stock movement row (SQL-only).
    ///
    /// Orchestration rules:
    /// - ID and timestamp generation belong to service.
    /// - Updating inventory stock belongs to service (cross-repo orchestration).
    pub fn insert_stock_movement(
        &self,
        id: &str,
        movement: &NewStockMovement,
        balance_before: f64,
        balance_after: f64,
        timestamp: &str,
        fiscal_year: i32,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO stock_movements 
             (id, product_id, movement_type, quantity, balance_before, balance_after, 
              reference_type, reference_id, notes, timestamp, user_id, username, unit_id, fiscal_year)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
                 quantity = excluded.quantity,
                 balance_before = excluded.balance_before,
                 balance_after = excluded.balance_after,
                 notes = excluded.notes,
                 timestamp = excluded.timestamp",
            rusqlite::params![
                id,
                &movement.product_id,
                movement.movement_type.as_str(),
                movement.quantity,
                balance_before,
                balance_after,
                movement.reference_type.as_deref(),
                movement.reference_id.as_deref(),
                movement.notes.as_deref(),
                timestamp,
                &movement.user_id,
                &movement.username,
                movement.unit_id.as_deref(),
                fiscal_year,
            ],
        )?;
        Ok(())
    }

    pub fn count_stock_movements(&self, q: &StockMovementQuery) -> Result<i64, AppError> {
        Ok(self.executor.query_row(
            r#"SELECT COUNT(*)
               FROM stock_movements sm
               WHERE (?1 IS NULL OR sm.product_id = ?1)
                 AND (?2 IS NULL OR sm.movement_type = ?2)
                 AND (?3 IS NULL OR sm.timestamp >= ?3)
                 AND (?4 IS NULL OR sm.timestamp <= ?4)
                 AND (?5 IS NULL OR sm.reference_type = ?5)
                 AND (?6 IS NULL OR sm.reference_id = ?6)
                 AND (?7 IS NULL OR sm.unit_id = ?7)"#,
            params![
                q.product_id.as_deref(),
                q.movement_type.as_deref(),
                q.start_timestamp.as_deref(),
                q.end_timestamp.as_deref(),
                q.reference_type.as_deref(),
                q.reference_id.as_deref(),
                q.unit_id.as_deref(),
            ],
            |row| row.get(0),
        )?)
    }

    pub fn fetch_stock_movements(
        &self,
        q: &StockMovementQuery,
    ) -> Result<Vec<StockMovementDbRow>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT sm.id, sm.product_id, p.name as product_name, sm.movement_type,
                      sm.quantity, sm.balance_before, sm.balance_after, sm.reference_type,
                      sm.reference_id, sm.notes, sm.timestamp, sm.user_id, sm.username, sm.unit_id, sm.fiscal_year
               FROM stock_movements sm
               LEFT JOIN products p ON sm.product_id = p.id
               WHERE (?1 IS NULL OR sm.product_id = ?1)
                 AND (?2 IS NULL OR sm.movement_type = ?2)
                 AND (?3 IS NULL OR sm.timestamp >= ?3)
                 AND (?4 IS NULL OR sm.timestamp <= ?4)
                 AND (?5 IS NULL OR sm.reference_type = ?5)
                 AND (?6 IS NULL OR sm.reference_id = ?6)
                 AND (?7 IS NULL OR sm.unit_id = ?7)
               ORDER BY sm.timestamp DESC
               LIMIT ?8 OFFSET ?9"#,
            params![
                q.product_id.as_deref(),
                q.movement_type.as_deref(),
                q.start_timestamp.as_deref(),
                q.end_timestamp.as_deref(),
                q.reference_type.as_deref(),
                q.reference_id.as_deref(),
                q.unit_id.as_deref(),
                q.limit,
                q.offset,
            ],
            |row| {
                Ok(StockMovementDbRow {
                    id: row.get(0)?,
                    product_id: row.get(1)?,
                    product_name: row.get(2)?,
                    movement_type: row.get(3)?,
                    quantity: row.get(4)?,
                    balance_before: row.get(5)?,
                    balance_after: row.get(6)?,
                    reference_type: row.get(7)?,
                    reference_id: row.get(8)?,
                    notes: row.get(9)?,
                    timestamp: row.get(10)?,
                    user_id: row.get(11)?,
                    username: row.get(12)?,
                    unit_id: row.get(13)?,
                    fiscal_year: row.get(14)?,
                })
            },
        )?)
    }

    /// Insert raw stock movement from external source (for UNIT to WILAYA sync)
    pub fn insert_raw_stock_movement(&self, movement: &StockMovement) -> Result<(), AppError> {
        // Use OR IGNORE to handle duplicates if the same movement is imported twice
        self.executor
            .execute(
                "INSERT OR IGNORE INTO stock_movements
             (id, product_id, movement_type, quantity,
              balance_before, balance_after, reference_type,
              reference_id, notes, timestamp, user_id, username, unit_id, fiscal_year)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                rusqlite::params![
                    movement.id,
                    movement.product_id,
                    movement.movement_type.as_str(),
                    movement.quantity,
                    movement.balance_before,
                    movement.balance_after,
                    movement.reference_type,
                    movement.reference_id,
                    movement.notes,
                    movement.timestamp,
                    movement.user_id,
                    movement.username,
                    movement.unit_id,
                    movement.fiscal_year,
                ],
            )
            .map_err(AppError::from)?;

        Ok(())
    }

    /// Check if a movement exists by its ID
    pub fn movement_exists(&self, id: &str) -> Result<bool, AppError> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM stock_movements WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn count_by_year(&self, year: i32) -> Result<i64, AppError> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM stock_movements WHERE fiscal_year = ?1",
            params![year],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn get_stock_movements_in_range(
        &self,
        start: &str,
        end: &str,
    ) -> Result<Vec<StockMovementDbRow>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT sm.id, sm.product_id, p.name as product_name, sm.movement_type,
                      sm.quantity, sm.balance_before, sm.balance_after, sm.reference_type,
                      sm.reference_id, sm.notes, sm.timestamp, sm.user_id, sm.username, sm.unit_id, sm.fiscal_year
               FROM stock_movements sm
               LEFT JOIN products p ON sm.product_id = p.id
               WHERE sm.timestamp >= ?1 AND sm.timestamp <= ?2
               ORDER BY sm.timestamp ASC"#,
            params![start, end],
            |row| {
                Ok(StockMovementDbRow {
                    id: row.get(0)?,
                    product_id: row.get(1)?,
                    product_name: row.get(2)?,
                    movement_type: row.get(3)?,
                    quantity: row.get(4)?,
                    balance_before: row.get(5)?,
                    balance_after: row.get(6)?,
                    reference_type: row.get(7)?,
                    reference_id: row.get(8)?,
                    notes: row.get(9)?,
                    timestamp: row.get(10)?,
                    user_id: row.get(11)?,
                    username: row.get(12)?,
                    unit_id: row.get(13)?,
                    fiscal_year: row.get(14)?,
                })
            },
        )?)
    }

    /// Memory-aware Stock Movement Scan (bounded peak RAM)
    pub fn fetch_stock_movements_iter<F>(
        &self,
        q: &StockMovementQuery,
        consumer: F,
    ) -> Result<(), AppError>
    where
        F: FnMut(StockMovementDbRow) -> Result<(), String>,
    {
        self.executor
            .query_iter(
                r#"SELECT sm.id, sm.product_id, p.name as product_name, sm.movement_type,
                      sm.quantity, sm.balance_before, sm.balance_after, sm.reference_type,
                      sm.reference_id, sm.notes, sm.timestamp, sm.user_id, sm.username, sm.unit_id, sm.fiscal_year
               FROM stock_movements sm
               LEFT JOIN products p ON sm.product_id = p.id
               WHERE (?1 IS NULL OR sm.product_id = ?1)
                 AND (?2 IS NULL OR sm.movement_type = ?2)
                 AND (?3 IS NULL OR sm.timestamp >= ?3)
                 AND (?4 IS NULL OR sm.timestamp <= ?4)
                 AND (?5 IS NULL OR sm.reference_type = ?5)
                 AND (?6 IS NULL OR sm.reference_id = ?6)
                 AND (?7 IS NULL OR sm.unit_id = ?7)
               ORDER BY sm.timestamp ASC"#,
                params![
                    q.product_id.as_deref(),
                    q.movement_type.as_deref(),
                    q.start_timestamp.as_deref(),
                    q.end_timestamp.as_deref(),
                    q.reference_type.as_deref(),
                    q.reference_id.as_deref(),
                    q.unit_id.as_deref(),
                ],
                |row| {
                    Ok(StockMovementDbRow {
                        id: row.get(0)?,
                        product_id: row.get(1)?,
                        product_name: row.get(2)?,
                        movement_type: row.get(3)?,
                        quantity: row.get(4)?,
                        balance_before: row.get(5)?,
                        balance_after: row.get(6)?,
                        reference_type: row.get(7)?,
                        reference_id: row.get(8)?,
                        notes: row.get(9)?,
                        timestamp: row.get(10)?,
                        user_id: row.get(11)?,
                        username: row.get(12)?,
                        unit_id: row.get(13)?,
                        fiscal_year: row.get(14)?,
                    })
                },
                consumer,
            )
            .map_err(AppError::from)
    }
}
