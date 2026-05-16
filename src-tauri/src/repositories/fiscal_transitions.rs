use super::DbExecutor;
use rusqlite::{params, Result as SqliteResult};

/// Repository for tracking applied fiscal transitions (replay protection).
pub struct FiscalTransitionRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalTransitionRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Check if a fiscal transition ID has already been applied.
    pub fn is_applied(&self, transition_id: &str) -> SqliteResult<bool> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM applied_fiscal_transitions WHERE fiscal_transition_id = ?1",
            params![transition_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Record a fiscal transition application.
    pub fn record_application(
        &self,
        transition_id: &str,
        closed_year: i32,
        opened_year: i32,
        applied_by: &str,
    ) -> SqliteResult<()> {
        self.executor.execute(
            "INSERT INTO applied_fiscal_transitions (fiscal_transition_id, closed_year, opened_year, applied_at, applied_by) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![transition_id, closed_year, opened_year, chrono::Utc::now().to_rfc3339(), applied_by],
        )?;
        Ok(())
    }
}
