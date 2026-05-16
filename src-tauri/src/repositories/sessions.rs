use crate::errors::AppResult;
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

pub struct SessionRepository<'a> {
    executor: DbExecutor<'a>,
}

pub struct SessionRecordRow {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub end_reason: Option<String>,
    pub critical_operations_count: i64,
    pub integrity_warnings_count: i64,
    pub anomalies_surfaced_count: i64,
}

impl<'a> SessionRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn recover_abandoned(&self, ended_at: &str) -> AppResult<u64> {
        let updated = self.executor.execute(
            r#"
            UPDATE operational_sessions
            SET ended_at = ?1,
                end_reason = 'UNEXPECTED_TERMINATION'
            WHERE ended_at IS NULL
            "#,
            params![ended_at],
        )?;
        Ok(updated as u64)
    }

    pub fn insert_session(
        &self,
        id: &str,
        user_id: &str,
        username: &str,
        started_at: &str,
    ) -> AppResult<()> {
        self.executor.execute(
            r#"
            INSERT INTO operational_sessions
                (id, user_id, username, started_at,
                 critical_operations_count, integrity_warnings_count, anomalies_surfaced_count)
            VALUES (?1, ?2, ?3, ?4, 0, 0, 0)
            "#,
            params![id, user_id, username, started_at],
        )?;
        Ok(())
    }

    pub fn update_session_end(&self, id: &str, ended_at: &str, reason: &str) -> AppResult<u64> {
        let updated = self.executor.execute(
            r#"
            UPDATE operational_sessions
            SET ended_at = ?1, end_reason = ?2
            WHERE id = ?3 AND ended_at IS NULL
            "#,
            params![ended_at, reason, id],
        )?;
        Ok(updated as u64)
    }

    pub fn increment_counter(&self, id: &str, column_name: &str) -> AppResult<()> {
        let sql = format!(
            "UPDATE operational_sessions SET {} = {} + 1 WHERE id = ?1 AND ended_at IS NULL",
            column_name, column_name
        );
        self.executor.execute(&sql, params![id])?;
        Ok(())
    }

    pub fn get_by_id(&self, id: &str) -> AppResult<Option<SessionRecordRow>> {
        self.executor
            .query_row_optional(
                r#"
            SELECT id, user_id, username, started_at, ended_at, end_reason,
                   critical_operations_count, integrity_warnings_count, anomalies_surfaced_count
            FROM operational_sessions WHERE id = ?1
            "#,
                params![id],
                Self::map_row,
            )
            .map_err(Into::into)
    }

    pub fn list_recent(&self, limit: i64) -> AppResult<Vec<SessionRecordRow>> {
        self.executor
            .query_all(
                r#"
            SELECT id, user_id, username, started_at, ended_at, end_reason,
                   critical_operations_count, integrity_warnings_count, anomalies_surfaced_count
            FROM operational_sessions
            ORDER BY started_at DESC
            LIMIT ?1
            "#,
                params![limit],
                Self::map_row,
            )
            .map_err(Into::into)
    }

    fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRecordRow> {
        Ok(SessionRecordRow {
            id: r.get(0)?,
            user_id: r.get(1)?,
            username: r.get(2)?,
            started_at: r.get(3)?,
            ended_at: r.get(4)?,
            end_reason: r.get(5)?,
            critical_operations_count: r.get(6)?,
            integrity_warnings_count: r.get(7)?,
            anomalies_surfaced_count: r.get(8)?,
        })
    }
}
