//! Inventory Repository Module
//!
//! Handles inventory view and snapshot operations.

use crate::errors::AppError;
use crate::models::{InventoryStock, StockSummary};
use crate::repositories::executor::DbExecutor;
use crate::repositories::numeric_row;
use rusqlite::params;

/// Repository for inventory-related database operations
pub struct InventoryRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> InventoryRepository<'a> {
    /// Create a new InventoryRepository with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Get stock for a specific product
    pub fn get_stock(&self, product_id: &str) -> Result<Option<InventoryStock>, AppError> {
        let result = self.executor.query_row_optional(
            r#"SELECT s.id, s.product_id, p.name, s.quantity, s.unit, s.last_updated 
                   FROM inventory_stocks s 
                   JOIN products p ON s.product_id = p.id 
                   WHERE s.product_id = ?1"#,
            [product_id],
            |row| {
                let last_updated_str: String = row.get(5)?;
                let last_updated = crate::errors::parse_datetime_rfc3339(&last_updated_str)
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(InventoryStock {
                    id: row.get(0)?,
                    product_id: row.get(1)?,
                    product_name: row.get(2)?,
                    quantity: numeric_row::qty_col(3, row.get::<_, i64>(3)?)?,
                    unit: row.get(4)?,
                    last_updated,
                })
            },
        )?;
        Ok(result)
    }

    /// Check if stock exists for a given product
    pub fn stock_exists_for_product(&self, product_id: &str) -> Result<bool, AppError> {
        let exists = self
            .executor
            .query_row_optional(
                "SELECT 1 FROM inventory_stocks WHERE product_id = ?1",
                [product_id],
                |_row| Ok(()),
            )?
            .is_some();
        Ok(exists)
    }

    /// Insert empty stock record for sync
    pub fn insert_empty_stock(
        &self,
        stock_id: &str,
        product_id: &str,
        updated_at: &str,
        node_id: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT OR IGNORE INTO inventory_stocks (id, product_id, quantity, last_updated, updated_at, node_id, deleted) 
             VALUES (?1, ?2, 0, ?3, ?3, ?4, 0)",
            rusqlite::params![stock_id, product_id, updated_at, node_id],
        )?;
        Ok(())
    }

    /// Create initial empty stock for a newly imported product
    pub fn create_initial_stock_for_product(
        &self,
        stock_id: &str,
        product_id: &str,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT OR IGNORE INTO inventory_stocks (id, product_id, quantity, unit, last_updated) VALUES (?1, ?2, 0, 'unit', ?3)",
            rusqlite::params![stock_id, product_id, now],
        )?;
        Ok(())
    }

    /// Get all stocks
    pub fn get_all_stocks(&self) -> Result<Vec<InventoryStock>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT s.id, s.product_id, p.name, s.quantity, s.unit, s.last_updated
               FROM inventory_stocks s
               JOIN products p ON s.product_id = p.id
               ORDER BY p.name"#,
            [],
            |row| {
                let last_updated_str: String = row.get(5)?;
                let last_updated = crate::errors::parse_datetime_rfc3339(&last_updated_str)
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(InventoryStock {
                    id: row.get(0)?,
                    product_id: row.get(1)?,
                    product_name: row.get(2)?,
                    quantity: numeric_row::qty_col(3, row.get::<_, i64>(3)?)?,
                    unit: row.get(4)?,
                    last_updated,
                })
            },
        )?)
    }

    /// Update stock quantity for a product (wire f64 → scaled-3 INTEGER at the
    /// repository write boundary, exactly once through Quantity).
    pub fn update_stock(&self, product_id: &str, quantity: f64) -> Result<(), AppError> {
        let quantity_scaled = numeric_row::qty_scaled(quantity)?;
        self.executor.execute(
            "UPDATE inventory_stocks SET quantity = ?1, last_updated = ?2 WHERE product_id = ?3",
            params![
                quantity_scaled,
                &chrono::Utc::now().to_rfc3339(),
                &product_id
            ],
        )?;
        Ok(())
    }

    /// Get stock summary for all products.
    ///
    /// When `fiscal_year` is `Some`, movement-based statistics (total_in,
    /// total_out, movement_count) are scoped to that fiscal year only.
    /// `current_quantity` is always the real inventory stock, unfiltered.
    pub fn get_stock_summary(
        &self,
        fiscal_year: Option<i32>,
    ) -> Result<Vec<StockSummary>, AppError> {
        let sql = r#"
            SELECT
                p.id as product_id,
                p.name as product_name,
                COALESCE(s.quantity, 0) as current_quantity,
                COALESCE(sm.total_in, 0) as total_in,
                COALESCE(sm.total_out, 0) as total_out,
                sm.last_movement,
                COALESCE(sm.movement_count, 0) as movement_count
            FROM products p
            LEFT JOIN inventory_stocks s ON p.id = s.product_id
            LEFT JOIN (
                SELECT 
                    product_id,
                    SUM(CASE WHEN movement_type = 'IN' THEN quantity ELSE 0 END) as total_in,
                    SUM(CASE WHEN movement_type = 'OUT' THEN quantity ELSE 0 END) as total_out,
                    MAX(timestamp) as last_movement,
                    COUNT(*) as movement_count
                FROM stock_movements
                WHERE (?1 IS NULL OR fiscal_year = ?1)
                GROUP BY product_id
            ) sm ON p.id = sm.product_id
            ORDER BY p.name
        "#;

        Ok(self.executor.query_all(sql, [fiscal_year], |row| {
            Ok(StockSummary {
                product_id: row.get(0)?,
                product_name: row.get(1)?,
                current_quantity: numeric_row::qty_col(2, row.get::<_, i64>(2)?)?,
                total_in: numeric_row::qty_col(3, row.get::<_, i64>(3)?)?,
                total_out: numeric_row::qty_col(4, row.get::<_, i64>(4)?)?,
                last_movement: row.get(5)?,
                movement_count: row.get(6)?,
            })
        })?)
    }

    pub fn check_existing_snapshot_count(
        &self,
        unit_id: &str,
        year: i32,
        month: u32,
    ) -> Result<i64, AppError> {
        let count = self.executor.query_row(
            "SELECT COUNT(*) FROM unit_monthly_snapshots WHERE unit_id=?1 AND report_year=?2 AND report_month=?3",
            rusqlite::params![unit_id, year, month],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn count_snapshot_balance_anomalies(
        &self,
        unit_id: &str,
        year: i32,
        month: u32,
    ) -> Result<i64, AppError> {
        let count = self.executor.query_row(
            "SELECT COUNT(*) FROM unit_monthly_snapshots WHERE unit_id=?1 AND report_year=?2 AND report_month=?3 AND has_balance_anomaly=1",
            rusqlite::params![unit_id, year, month],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn count_snapshot_consumption_anomalies(
        &self,
        unit_id: &str,
        year: i32,
        month: u32,
    ) -> Result<i64, AppError> {
        let count = self.executor.query_row(
            "SELECT COUNT(*) FROM unit_monthly_snapshots WHERE unit_id=?1 AND report_year=?2 AND report_month=?3 AND has_consumption_anomaly=1",
            rusqlite::params![unit_id, year, month],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn get_active_products_for_month(
        &self,
        unit_id: &str,
        month_start: &str,
        month_end: &str,
    ) -> Result<Vec<(String, String)>, AppError> {
        Ok(self.executor.query_all(
            "SELECT DISTINCT p.id, p.name FROM products p WHERE EXISTS (SELECT 1 FROM stock_movements sm JOIN daily_reports dr ON sm.reference_id = dr.id WHERE sm.product_id = p.id AND sm.timestamp >= ?1 AND sm.timestamp <= ?2 AND sm.movement_type = 'OUT' AND dr.unit_id = ?3) OR EXISTS (SELECT 1 FROM stock_movements sm WHERE sm.product_id = p.id AND sm.timestamp >= ?1 AND sm.timestamp <= ?2 AND sm.movement_type = 'OUT' AND sm.unit_id = ?3) OR EXISTS (SELECT 1 FROM daily_report_meal_items dci JOIN daily_report_meals dm ON dci.meal_id = dm.id JOIN daily_reports dr ON dm.daily_report_id = dr.id JOIN stock_movements sm ON sm.product_id = dci.product_id WHERE dci.product_id = p.id AND dr.unit_id = ?3 AND sm.timestamp >= ?1 AND sm.timestamp <= ?2 AND sm.movement_type = 'OPENING') OR EXISTS (SELECT 1 FROM stock_movements sm WHERE sm.product_id = p.id AND sm.timestamp >= ?1 AND sm.timestamp <= ?2 AND sm.movement_type = 'OPENING' AND sm.unit_id = ?3) ORDER BY p.name",
            rusqlite::params![month_start, month_end, unit_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    pub fn delete_snapshot(&self, unit_id: &str, year: i32, month: u32) -> Result<(), AppError> {
        self.executor.execute(
            "DELETE FROM unit_monthly_snapshots WHERE unit_id=?1 AND report_year=?2 AND report_month=?3",
            rusqlite::params![unit_id, year, month],
        )?;
        Ok(())
    }

    pub fn get_opening_stock(
        &self,
        product_id: &str,
        unit_id: &str,
        month_start: &str,
    ) -> Result<f64, AppError> {
        let stock: i64 = self.executor.query_row(
            "SELECT COALESCE((SELECT sm.balance_after FROM stock_movements sm LEFT JOIN daily_reports dr ON sm.reference_id = dr.id WHERE sm.product_id = ?1 AND sm.timestamp < ?2 AND (dr.unit_id = ?3 OR sm.unit_id = ?3) ORDER BY sm.timestamp DESC LIMIT 1), 0)",
            rusqlite::params![product_id, month_start, unit_id],
            |row| row.get(0),
        )?;
        Ok(numeric_row::qty_scaled_i64_to_f64(stock)?)
    }

    pub fn get_total_in(
        &self,
        product_id: &str,
        unit_id: &str,
        month_start: &str,
        month_end: &str,
    ) -> Result<f64, AppError> {
        let total: i64 = self.executor.query_row(
            "SELECT COALESCE(SUM(sm.quantity),0) FROM stock_movements sm WHERE sm.product_id = ?1 AND sm.timestamp >= ?2 AND sm.timestamp <= ?3 AND sm.movement_type='IN' AND sm.unit_id = ?4",
            rusqlite::params![product_id, month_start, month_end, unit_id],
            |row| row.get(0),
        )?;
        Ok(numeric_row::qty_scaled_i64_to_f64(total)?)
    }

    pub fn get_total_out(
        &self,
        product_id: &str,
        unit_id: &str,
        month_start: &str,
        month_end: &str,
    ) -> Result<f64, AppError> {
        let total: i64 = self.executor.query_row(
            "SELECT COALESCE(SUM(sm.quantity), 0) FROM stock_movements sm LEFT JOIN daily_reports dr ON sm.reference_id = dr.id WHERE sm.product_id = ?1 AND sm.timestamp >= ?2 AND sm.timestamp <= ?3 AND sm.movement_type = 'OUT' AND (dr.unit_id = ?4 OR sm.unit_id = ?4)",
            rusqlite::params![product_id, month_start, month_end, unit_id],
            |row| row.get(0),
        )?;
        Ok(numeric_row::qty_scaled_i64_to_f64(total)?)
    }

    pub fn get_reported_closing(
        &self,
        product_id: &str,
        unit_id: &str,
        month_start: &str,
        month_end: &str,
        default_val_scaled: i64,
    ) -> Result<f64, AppError> {
        let closing: i64 = self.executor.query_row(
            "SELECT COALESCE((SELECT sm.balance_after FROM stock_movements sm LEFT JOIN daily_reports dr ON sm.reference_id = dr.id WHERE sm.product_id = ?1 AND sm.timestamp >= ?2 AND sm.timestamp <= ?3 AND sm.movement_type = 'OUT' AND (dr.unit_id = ?4 OR sm.unit_id = ?4) ORDER BY sm.timestamp DESC LIMIT 1), ?5)",
            rusqlite::params![product_id, month_start, month_end, unit_id, default_val_scaled],
            |row| row.get(0),
        )?;
        Ok(numeric_row::qty_scaled_i64_to_f64(closing)?)
    }

    // INTENTIONAL: uses timestamp ranges instead of fiscal_year.
    // Rolling 3-month consumption average may cross fiscal-year boundaries by design.
    // Do NOT replace strftime here with fiscal_year column filtering.
    pub fn get_avg_consumption_3months(
        &self,
        product_id: &str,
        unit_id: &str,
        prev_start: &str,
        month_start: &str,
    ) -> Result<Option<f64>, AppError> {
        let avg: Option<f64> = self.executor.query_row(
            "SELECT AVG(monthly_out) FROM (SELECT CAST(strftime('%Y', sm.timestamp) AS INTEGER) as y, CAST(strftime('%m', sm.timestamp) AS INTEGER) as m, SUM(sm.quantity) as monthly_out FROM stock_movements sm WHERE sm.product_id = ?1 AND sm.movement_type = 'OUT' AND sm.reference_id IN (SELECT dr.id FROM daily_reports dr WHERE dr.unit_id = ?2) AND sm.timestamp >= ?3 AND sm.timestamp < ?4 GROUP BY y, m HAVING COUNT(*) > 0)",
            rusqlite::params![product_id, unit_id, prev_start, month_start],
            |row| row.get::<usize, Option<f64>>(0),
        )?;
        Ok(avg)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_snapshot(
        &self,
        snapshot_id: &str,
        unit_id: &str,
        unit_name: &str,
        year: i32,
        month: u32,
        product_id: &str,
        product_name: &str,
        opening_stock: f64,
        total_in: f64,
        total_out: f64,
        computed_closing: f64,
        reported_closing: f64,
        variance_scaled: i64,
        has_balance_anomaly: bool,
        avg_consumption_3months: Option<f64>,
        has_consumption_anomaly: bool,
        is_stale: bool,
        computed_at: &str,
    ) -> Result<(), AppError> {
        let opening_stock_scaled = numeric_row::qty_scaled(opening_stock)?;
        let total_in_scaled = numeric_row::qty_scaled(total_in)?;
        let total_out_scaled = numeric_row::qty_scaled(total_out)?;
        let computed_closing_scaled = numeric_row::qty_scaled(computed_closing)?;
        let reported_closing_scaled = numeric_row::qty_scaled(reported_closing)?;
        self.executor.execute(
            "INSERT OR REPLACE INTO unit_monthly_snapshots (id, unit_id, unit_name, report_year, report_month, product_id, product_name, opening_stock, total_in, total_out, computed_closing, reported_closing, variance, has_balance_anomaly, avg_consumption_3months, has_consumption_anomaly, is_stale, computed_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
            rusqlite::params![snapshot_id, unit_id, unit_name, year, month, product_id, product_name, opening_stock_scaled, total_in_scaled, total_out_scaled, computed_closing_scaled, reported_closing_scaled, variance_scaled, has_balance_anomaly as i32, avg_consumption_3months, has_consumption_anomaly as i32, is_stale as i32, computed_at],
        )?;
        Ok(())
    }

    pub fn refresh_staleness(
        &self,
        unit_id: &str,
        year: i32,
        month: u32,
        month_start_ts: &str,
        month_end_ts: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "UPDATE unit_monthly_snapshots SET is_stale = CASE WHEN (SELECT MAX(timestamp) FROM stock_movements WHERE product_id IN (SELECT DISTINCT dci.product_id FROM daily_report_meal_items dci JOIN daily_report_meals dm ON dci.meal_id = dm.id JOIN daily_reports dr ON dm.daily_report_id = dr.id WHERE dr.unit_id = ?1) AND timestamp >= ?2 AND timestamp <= ?3) > computed_at THEN 1 ELSE 0 END WHERE unit_id=?1 AND report_year=?4 AND report_month=?5",
            rusqlite::params![unit_id, month_start_ts, month_end_ts, year, month],
        )?;
        Ok(())
    }

    /// Get all unit monthly snapshots for a specific month
    pub fn get_unit_monthly_snapshots(
        &self,
        unit_id: &str,
        year: i32,
        month: u32,
    ) -> Result<Vec<crate::models::UnitMonthlySnapshot>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, unit_id, unit_name, report_year, report_month, product_id, product_name, opening_stock, total_in, total_out, computed_closing, reported_closing, variance, has_balance_anomaly, avg_consumption_3months, has_consumption_anomaly, is_stale, computed_at FROM unit_monthly_snapshots WHERE unit_id=?1 AND report_year=?2 AND report_month=?3 ORDER BY has_balance_anomaly DESC, has_consumption_anomaly DESC, product_name ASC",
            rusqlite::params![unit_id, year, month],
            |row| Ok(crate::models::UnitMonthlySnapshot {
                id: row.get(0)?,
                unit_id: row.get(1)?,
                unit_name: row.get(2)?,
                report_year: row.get(3)?,
                report_month: row.get(4)?,
                product_id: row.get(5)?,
                product_name: row.get(6)?,
                opening_stock: numeric_row::qty_col(7, row.get::<_, i64>(7)?)?,
                total_in: numeric_row::qty_col(8, row.get::<_, i64>(8)?)?,
                total_out: numeric_row::qty_col(9, row.get::<_, i64>(9)?)?,
                computed_closing: numeric_row::qty_col(10, row.get::<_, i64>(10)?)?,
                reported_closing: numeric_row::qty_col(11, row.get::<_, i64>(11)?)?,
                variance: numeric_row::signed_qty_col(12, row.get::<_, i64>(12)?)?,
                has_balance_anomaly: row.get::<_, i32>(13)? != 0,
                avg_consumption_3months: row.get(14)?,
                has_consumption_anomaly: row.get::<_, i32>(15)? != 0,
                is_stale: row.get::<_, i32>(16)? != 0,
                computed_at: row.get(17)?,
            }),
        )?)
    }

    // INTENTIONAL: uses strftime on date/timestamp for UI month navigation.
    // This is a display helper, not a fiscal filter. Crossing fiscal boundaries
    // is correct here — the user needs to see all months with data.
    pub fn get_available_report_months_for_unit(
        &self,
        unit_id: &str,
    ) -> Result<Vec<(i32, u32)>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT DISTINCT CAST(strftime('%Y', date) AS INTEGER) as y, CAST(strftime('%m', date) AS INTEGER) as m FROM daily_reports WHERE unit_id = ?1
               UNION
               SELECT DISTINCT CAST(strftime('%Y', timestamp) AS INTEGER) as y, CAST(strftime('%m', timestamp) AS INTEGER) as m FROM stock_movements WHERE unit_id = ?1
               UNION
               SELECT DISTINCT report_year as y, report_month as m FROM monthly_reports WHERE unit_id = ?1
               ORDER BY y DESC, m DESC"#,
            [unit_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    #[allow(clippy::type_complexity)]
    pub fn verify_consistency_for_year(
        &self,
        fiscal_year: i32,
    ) -> Result<Vec<(String, f64, f64, f64, f64)>, AppError> {
        self.executor
            .query_all(
                r#"SELECT s.product_id,
       COALESCE(obs.opening_quantity,0),
       COALESCE(SUM(CASE WHEN sm.movement_type IN ('IN','OPENING') THEN sm.quantity ELSE 0 END),0),
       COALESCE(SUM(CASE WHEN sm.movement_type='OUT' THEN sm.quantity ELSE 0 END),0),
       COALESCE(s.quantity,0)
       FROM inventory_stocks s
       LEFT JOIN opening_balance_snapshots obs ON obs.product_id=s.product_id AND obs.fiscal_year=?1
       LEFT JOIN stock_movements sm ON sm.product_id=s.product_id AND sm.fiscal_year=?1
       GROUP BY s.product_id,s.quantity,obs.opening_quantity"#,
                rusqlite::params![fiscal_year],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        numeric_row::qty_col(1, r.get::<_, i64>(1)?)?,
                        numeric_row::qty_col(2, r.get::<_, i64>(2)?)?,
                        numeric_row::qty_col(3, r.get::<_, i64>(3)?)?,
                        numeric_row::qty_col(4, r.get::<_, i64>(4)?)?,
                    ))
                },
            )
            .map_err(Into::into)
    }

    pub fn get_total_inventory_value(&self) -> Result<f64, AppError> {
        // Scaled-2 Money aggregate: SUM(qty_scaled * unit_cost_scaled) equals
        // value_DA × 100000; `/1000` with MidpointAwayFromZero yields centimes,
        // entirely in exact INTEGER arithmetic (see numeric_row::money_sum_col).
        let sum: i64 = self
            .executor
            .query_row(
                r#"
                SELECT COALESCE(SUM(qty_remaining * unit_cost), 0)
                FROM fifo_stock_layers
                WHERE qty_remaining > 0
                "#,
                [],
                |r| r.get(0),
            )
            .map_err(AppError::from)?;
        Ok(numeric_row::money_sum_col(sum)?)
    }
}
