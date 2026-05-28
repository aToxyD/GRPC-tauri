use crate::errors::AppResult;
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

pub struct IntegrityRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> IntegrityRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Rows: (unit_id, product_id, fifo_qty, stock_qty)
    pub fn fetch_fifo_inventory_mismatches(&self) -> AppResult<Vec<(String, String, f64, f64)>> {
        Ok(self.executor.query_all(
            r#"SELECT f.unit_id, f.product_id,
                      SUM(f.qty_remaining) as fifo_qty,
                      COALESCE(s.quantity, 0.0) as stock_qty
               FROM fifo_stock_layers f
               LEFT JOIN inventory_stocks s ON f.product_id = s.product_id
               WHERE f.qty_remaining > 0
               GROUP BY f.unit_id, f.product_id
               HAVING ABS(fifo_qty - COALESCE(s.quantity, 0.0)) > 1e-6"#,
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, f64>(2)?,
                    r.get::<_, f64>(3)?,
                ))
            },
        )?)
    }

    /// Rows: (id, unit_id, product_id, qty_remaining)
    pub fn fetch_negative_layers(&self) -> AppResult<Vec<(String, String, String, f64)>> {
        Ok(self.executor.query_all(
            r#"SELECT id, unit_id, product_id, qty_remaining
               FROM fifo_stock_layers
               WHERE qty_remaining < 0"#,
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, f64>(3)?,
                ))
            },
        )?)
    }

    /// Rows: (consumption_id, movement_id)
    pub fn fetch_orphan_consumptions(&self) -> AppResult<Vec<(String, String)>> {
        Ok(self.executor.query_all(
            r#"SELECT ilc.id, ilc.movement_id
               FROM inventory_layer_consumptions ilc
               LEFT JOIN stock_movements sm ON ilc.movement_id = sm.id
               WHERE sm.id IS NULL"#,
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?)
    }

    /// Rows: (id, quantity, unit_cost, total_cost)
    pub fn fetch_invalid_consumption_costs(&self) -> AppResult<Vec<(String, f64, f64, f64)>> {
        Ok(self.executor.query_all(
            r#"SELECT id, quantity, unit_cost, total_cost
               FROM inventory_layer_consumptions
               WHERE ABS(quantity * unit_cost - total_cost) > 1e-6"#,
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, f64>(2)?,
                    r.get::<_, f64>(3)?,
                ))
            },
        )?)
    }

    /// Rows: (report_id, recorded_total, computed_total)
    pub fn fetch_daily_report_total_mismatches(&self) -> AppResult<Vec<(String, f64, f64)>> {
        Ok(self.executor.query_all(
            r#"SELECT dr.id, dr.total_daily_cost,
                      COALESCE(SUM(drm.total_meal_cost), 0.0) as computed_total
               FROM daily_reports dr
               JOIN daily_report_meals drm ON dr.id = drm.daily_report_id
               GROUP BY dr.id
               HAVING ABS(dr.total_daily_cost - SUM(drm.total_meal_cost)) > 1e-6"#,
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, f64>(2)?,
                ))
            },
        )?)
    }

    /// Rows: (current_year, fiscal_year_status)
    pub fn fetch_invalid_current_fiscal_years(&self) -> AppResult<Vec<(i32, Option<String>)>> {
        Ok(self.executor.query_all(
            r#"SELECT s.current_year, fys.status
               FROM settings s
               LEFT JOIN fiscal_year_status fys ON s.current_year = fys.year
               WHERE fys.status IS NULL OR fys.status != 'open'"#,
            [],
            |r| Ok((r.get::<_, i32>(0)?, r.get::<_, Option<String>>(1)?)),
        )?)
    }

    pub fn record_attempt(
        &self,
        attempted_at: &str,
        verification_type: &str,
        outcome: &str,
        details: Option<&str>,
    ) -> AppResult<()> {
        self.executor.execute(
            r#"
            INSERT INTO integrity_verification_attempts
                (attempted_at, verification_type, outcome, details)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![attempted_at, verification_type, outcome, details],
        )?;
        Ok(())
    }

    pub fn count_failed_attempts(&self, verification_type: &str) -> AppResult<i64> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM integrity_verification_attempts WHERE outcome = 'FAIL' AND verification_type = ?1",
            params![verification_type],
            |r| r.get(0),
        )?;
        Ok(count)
    }

    pub fn count_orphan_opening_balances(&self) -> AppResult<i64> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(DISTINCT fiscal_year) FROM opening_balance_snapshots
             WHERE fiscal_year NOT IN (SELECT year FROM fiscal_year_status)",
            [],
            |r| r.get(0),
        )?;
        Ok(count)
    }

    pub fn count_recent_failures(&self) -> AppResult<i64> {
        self.executor
            .query_row(
                "SELECT COUNT(*) FROM integrity_verification_attempts
                 WHERE outcome = 'FAIL'
                   AND attempted_at >= datetime('now', '-90 days')",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub fn count_post_closure_movements(&self) -> AppResult<i64> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM stock_movements sm
             JOIN fiscal_year_status fys ON fys.year = sm.fiscal_year
             WHERE fys.status='closed' AND fys.closed_at IS NOT NULL
               AND sm.timestamp > fys.closed_at",
            [],
            |r| r.get(0),
        )?;
        Ok(count)
    }
}
