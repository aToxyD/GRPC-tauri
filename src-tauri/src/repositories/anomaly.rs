use crate::errors::AppResult;
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

pub struct AnomalyRepository<'a> {
    executor: DbExecutor<'a>,
}

pub struct InventoryValueJump {
    pub fiscal_year: i32,
    pub curr_value: f64,
    pub prev_value: f64,
    pub curr_date: String,
    pub prev_date: String,
}

impl<'a> AnomalyRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn fetch_outbound_stats(
        &self,
        current_start: &str,
        baseline_start: &str,
        baseline_end: &str,
    ) -> AppResult<Vec<(String, f64, f64)>> {
        self.executor
            .query_all(
                r#"
            SELECT
                sm.product_id,
                COALESCE(SUM(CASE
                    WHEN sm.movement_type = 'OUT' AND sm.timestamp >= ?1
                    THEN sm.quantity ELSE 0 END), 0) AS current_qty,
                COALESCE(SUM(CASE
                    WHEN sm.movement_type = 'OUT'
                      AND sm.timestamp >= ?2
                      AND sm.timestamp <  ?3
                    THEN sm.quantity ELSE 0 END), 0) AS baseline_qty
            FROM stock_movements sm
            WHERE sm.timestamp >= ?2
            GROUP BY sm.product_id
            "#,
                params![current_start, baseline_start, baseline_end],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(Into::into)
    }

    pub fn fetch_inventory_value_jump_pairs(&self) -> AppResult<Vec<InventoryValueJump>> {
        self.executor
            .query_all(
                r#"
            WITH ranked AS (
                SELECT
                    fiscal_year,
                    snapshot_date,
                    total_inventory_value,
                    ROW_NUMBER() OVER (PARTITION BY fiscal_year ORDER BY id DESC) AS rn
                FROM fiscal_operational_snapshots
            )
            SELECT
                a.fiscal_year,
                a.total_inventory_value AS curr_value,
                b.total_inventory_value AS prev_value,
                a.snapshot_date         AS curr_date,
                b.snapshot_date         AS prev_date
            FROM ranked a
            JOIN ranked b
              ON a.fiscal_year = b.fiscal_year AND a.rn = 1 AND b.rn = 2
            "#,
                [],
                |r| {
                    Ok(InventoryValueJump {
                        fiscal_year: r.get(0)?,
                        curr_value: r.get(1)?,
                        prev_value: r.get(2)?,
                        curr_date: r.get(3)?,
                        prev_date: r.get(4)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn count_critical_findings(&self) -> AppResult<i64> {
        self.executor
            .query_row(
                "SELECT COUNT(*) FROM operational_findings_log WHERE severity = 'CRITICAL'",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub fn count_active_anomalies(&self) -> AppResult<i64> {
        self.executor
            .query_row(
                "SELECT COUNT(*) FROM operational_findings_log WHERE severity IN ('WARNING','CRITICAL')",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub fn count_critical_without_recommendation(&self) -> AppResult<i64> {
        self.executor
            .query_row(
                "SELECT COUNT(*) FROM operational_findings_log
                 WHERE severity = 'CRITICAL'
                   AND (recommendation IS NULL OR TRIM(recommendation) = '')",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub fn count_recent_reports(&self, window_start: &str) -> AppResult<i64> {
        self.executor
            .query_row(
                "SELECT COUNT(*) FROM daily_reports WHERE created_at >= ?1 AND deleted = 0",
                params![window_start],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub fn count_failed_import_events(&self, window_start: &str) -> AppResult<i64> {
        self.executor
            .query_row(
                r#"
            SELECT COUNT(*) FROM import_audit_events
            WHERE event_type IN ('REJECTED','FAILED','DUPLICATE','STALE')
              AND occurred_at >= ?1
            "#,
                params![window_start],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub fn count_stale_import_conflicts(&self, window_start: &str) -> AppResult<i64> {
        self.executor.query_row(
            "SELECT COUNT(*) FROM sync_conflicts WHERE conflict_type = 'STALE_IMPORT' AND created_at >= ?1",
            params![window_start],
            |r| r.get(0),
        ).map_err(Into::into)
    }

    pub fn count_recent_failed_integrity_attempts(&self, window_start: &str) -> AppResult<i64> {
        self.executor.query_row(
            "SELECT COUNT(*) FROM integrity_verification_attempts WHERE outcome = 'FAIL' AND attempted_at >= ?1",
            params![window_start],
            |r| r.get(0),
        ).map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn log_finding(
        &self,
        emitted_at: &str,
        severity: &str,
        category: &str,
        code: &str,
        message: &str,
        recommendation: &str,
        context_json: Option<String>,
    ) -> AppResult<()> {
        self.executor.execute(
            r#"
            INSERT INTO operational_findings_log
                (emitted_at, severity, category, code, message, recommendation, context_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                emitted_at,
                severity,
                category,
                code,
                message,
                recommendation,
                context_json,
            ],
        )?;
        Ok(())
    }
}
