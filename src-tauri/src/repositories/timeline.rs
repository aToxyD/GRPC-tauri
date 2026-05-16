use crate::errors::AppResult;
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

pub struct TimelineRepository<'a> {
    executor: DbExecutor<'a>,
}

pub struct TimelineAuditRow {
    pub timestamp: String,
    pub action: String,
    pub username: Option<String>,
    pub entity_id: Option<String>,
    pub new_value: Option<String>,
}

pub struct TimelineIntegrityRow {
    pub timestamp: String,
    pub verification_type: String,
    pub details: Option<String>,
}

pub struct TimelineFindingRow {
    pub emitted_at: String,
    pub message: String,
    pub category: String,
    pub code: String,
}

pub struct TimelineExportRow {
    pub generated_at: String,
    pub fiscal_year: i32,
    pub generated_by: String,
    pub export_hash: String,
}

pub struct TimelineImportEventRow {
    pub occurred_at: String,
    pub package_kind: String,
    pub event_type: String,
    pub source_node_id: Option<String>,
}

impl<'a> TimelineRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn fetch_audit_lifecycle(&self) -> AppResult<Vec<TimelineAuditRow>> {
        self.executor
            .query_all(
                r#"
            SELECT timestamp, action, username, entity_id, new_value
            FROM audit_log
            WHERE action IN ('FiscalYearClosed','FiscalYearOpened')
            ORDER BY timestamp DESC
            "#,
                [],
                |r| {
                    Ok(TimelineAuditRow {
                        timestamp: r.get(0)?,
                        action: r.get(1)?,
                        username: r.get(2)?,
                        entity_id: r.get(3)?,
                        new_value: r.get(4)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn fetch_imports_audit(&self) -> AppResult<Vec<TimelineAuditRow>> {
        self.executor
            .query_all(
                r#"
            SELECT timestamp, action, username, NULL, new_value
            FROM audit_log
            WHERE action LIKE 'Import%'
            ORDER BY timestamp DESC
            "#,
                [],
                |r| {
                    Ok(TimelineAuditRow {
                        timestamp: r.get(0)?,
                        action: r.get(1)?,
                        username: r.get(2)?,
                        entity_id: None,
                        new_value: r.get(4)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn fetch_import_events(&self, limit: i64) -> AppResult<Vec<TimelineImportEventRow>> {
        self.executor
            .query_all(
                r#"
            SELECT occurred_at, package_kind, event_type, source_node_id
            FROM import_audit_events
            ORDER BY occurred_at DESC
            LIMIT ?1
            "#,
                params![limit],
                |r| {
                    Ok(TimelineImportEventRow {
                        occurred_at: r.get(0)?,
                        package_kind: r.get(1)?,
                        event_type: r.get(2)?,
                        source_node_id: r.get(3)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn fetch_integrity_failures(&self, limit: i64) -> AppResult<Vec<TimelineIntegrityRow>> {
        self.executor
            .query_all(
                r#"
            SELECT attempted_at, verification_type, details
            FROM integrity_verification_attempts
            WHERE outcome = 'FAIL'
            ORDER BY attempted_at DESC
            LIMIT ?1
            "#,
                params![limit],
                |r| {
                    Ok(TimelineIntegrityRow {
                        timestamp: r.get(0)?,
                        verification_type: r.get(1)?,
                        details: r.get(2)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn fetch_backups_audit(&self, limit: i64) -> AppResult<Vec<TimelineAuditRow>> {
        self.executor
            .query_all(
                r#"
            SELECT timestamp, action, username, NULL, NULL
            FROM audit_log
            WHERE action IN ('CreateBackup','RestoreBackup','BackupCreated','BackupRestored')
            ORDER BY timestamp DESC
            LIMIT ?1
            "#,
                params![limit],
                |r| {
                    Ok(TimelineAuditRow {
                        timestamp: r.get(0)?,
                        action: r.get(1)?,
                        username: r.get(2)?,
                        entity_id: None,
                        new_value: None,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn fetch_critical_findings(&self, limit: i64) -> AppResult<Vec<TimelineFindingRow>> {
        self.executor
            .query_all(
                r#"
            SELECT emitted_at, message, category, code
            FROM operational_findings_log
            WHERE severity = 'CRITICAL'
            ORDER BY emitted_at DESC
            LIMIT ?1
            "#,
                params![limit],
                |r| {
                    Ok(TimelineFindingRow {
                        emitted_at: r.get(0)?,
                        message: r.get(1)?,
                        category: r.get(2)?,
                        code: r.get(3)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn fetch_exports(&self, limit: i64) -> AppResult<Vec<TimelineExportRow>> {
        self.executor
            .query_all(
                r#"
            SELECT generated_at, fiscal_year, generated_by, export_hash
            FROM fiscal_export_snapshots
            ORDER BY generated_at DESC
            LIMIT ?1
            "#,
                params![limit],
                |r| {
                    Ok(TimelineExportRow {
                        generated_at: r.get(0)?,
                        fiscal_year: r.get(1)?,
                        generated_by: r.get(2)?,
                        export_hash: r.get(3)?,
                    })
                },
            )
            .map_err(Into::into)
    }
}
