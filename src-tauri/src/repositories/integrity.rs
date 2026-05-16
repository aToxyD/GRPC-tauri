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
