use crate::errors::AppResult;
use crate::models::ImportAuditEvent;
use crate::repositories::executor::DbExecutor;

pub struct ImportAuditEventsRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> ImportAuditEventsRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn insert_event(
        &self,
        event_type: &str,
        package_id: &str,
        package_kind: &str,
        source_node_id: Option<&str>,
        reason_code: Option<&str>,
    ) -> AppResult<()> {
        self.executor.execute(
            r#"INSERT INTO import_audit_events
               (event_type, package_id, package_kind, source_node_id, reason_code, occurred_at)
               VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))"#,
            rusqlite::params![
                event_type,
                package_id,
                package_kind,
                source_node_id,
                reason_code,
            ],
        )?;
        Ok(())
    }

    pub fn list_events(&self, limit: usize) -> AppResult<Vec<ImportAuditEvent>> {
        self.executor.query_map(
            r#"SELECT id, event_type, package_id, package_kind, source_node_id, reason_code, occurred_at
               FROM import_audit_events
               ORDER BY occurred_at DESC
               LIMIT ?1"#,
            rusqlite::params![limit as i64],
            |row| {
                Ok(ImportAuditEvent {
                    id: row.get(0)?,
                    event_type: row.get(1)?,
                    package_id: row.get(2)?,
                    package_kind: row.get(3)?,
                    source_node_id: row.get(4)?,
                    reason_code: row.get(5)?,
                    occurred_at: row.get(6)?,
                })
            },
        ).map_err(Into::into)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn record_reproducibility_metadata(
        &self,
        package_id: &str,
        package_kind: &str,
        imported_at: &str,
        imported_by: &str,
        validation_state: &str,
        source_integrity_state: Option<&str>,
        rejected_records_count: i64,
    ) -> AppResult<i64> {
        self.executor.execute(
            r#"
            INSERT INTO import_reproducibility_metadata
                (package_id, package_kind, imported_at, imported_by,
                 validation_state, source_integrity_state, rejected_records_count)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            rusqlite::params![
                package_id,
                package_kind,
                imported_at,
                imported_by,
                validation_state,
                source_integrity_state,
                rejected_records_count,
            ],
        )?;
        Ok(self.executor.last_insert_rowid())
    }
}
