//! SQL-only access to `opening_balance_snapshots`.

use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

pub struct OpeningBalanceRepository<'a> {
    executor: DbExecutor<'a>,
}

pub struct CreateSnapshotParams<'a> {
    pub id: &'a str,
    pub product_id: &'a str,
    pub fiscal_year: i32,
    pub quantity: f64,
    pub unit_cost: f64,
    pub total_value: f64,
    pub snapshot_reason: &'a str,
    pub carried_from: Option<i32>,
    pub created_by: &'a str,
}

impl<'a> OpeningBalanceRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Insert a new opening balance snapshot for a product/fiscal-year pair.
    /// The UNIQUE(product_id, fiscal_year) constraint enforces idempotency at DB level.
    pub fn create_snapshot(&self, params: CreateSnapshotParams) -> Result<(), AppError> {
        crate::application::services::FiscalHistoricalGuard::new(self.executor)
            .assert_opening_snapshot_mutable(params.fiscal_year)?;
        if let Some(from) = params.carried_from {
            crate::application::services::FiscalHistoricalGuard::new(self.executor)
                .assert_year_not_archived(from)?;
        }
        let now = chrono::Utc::now().to_rfc3339();

        self.executor
            .execute(
                "INSERT INTO opening_balance_snapshots \
             (id, product_id, fiscal_year, opening_quantity, unit_cost, total_value, \
              snapshot_reason, carried_from_year, created_at, created_by) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    params.id,
                    params.product_id,
                    params.fiscal_year,
                    params.quantity,
                    params.unit_cost,
                    params.total_value,
                    params.snapshot_reason,
                    params.carried_from,
                    now,
                    params.created_by
                ],
            )
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn get_by_year(
        &self,
        year: i32,
    ) -> Result<Vec<crate::models::OpeningBalanceSnapshot>, AppError> {
        self.executor
            .query_all(
                "SELECT id, product_id, fiscal_year, opening_quantity, unit_cost, total_value, \
              snapshot_reason, carried_from_year, created_at, created_by \
             FROM opening_balance_snapshots WHERE fiscal_year = ?1",
                params![year],
                |row| {
                    Ok(crate::models::OpeningBalanceSnapshot {
                        id: row.get(0)?,
                        product_id: row.get(1)?,
                        fiscal_year: row.get(2)?,
                        opening_quantity: row.get(3)?,
                        unit_cost: row.get(4)?,
                        total_value: row.get(5)?,
                        snapshot_reason: row.get(6)?,
                        carried_from_year: row.get(7)?,
                        created_at: row.get(8)?,
                        created_by: row.get(9)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn count_by_year(&self, year: i32) -> Result<i64, AppError> {
        self.executor
            .query_row(
                "SELECT COUNT(*) FROM opening_balance_snapshots WHERE fiscal_year = ?1",
                params![year],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }
}

impl crate::architecture::Repository for OpeningBalanceRepository<'_> {}
