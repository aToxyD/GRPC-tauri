//! Audit repository (SQL-only).
//!
//! Clean Architecture rules enforced:
//! - Repository contains ONLY SQL + row mapping.
//! - No loops/iterators/conditionals with business meaning.
//! - No hashing, pagination building, export, integrity verification, or stats calculations.

use crate::domain::audit::{
    AuditEntryDbRow, AuditEventRow, AuditQuery, DailyOperationCountRow, OperationCountRow,
    UserActivitySummaryRow,
};
use crate::domain::audit_chain::AuditChainVerifyRow;
use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

pub struct AuditRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> AuditRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Audit log insertion (SQL only)
    // ─────────────────────────────────────────────────────────────────────────

    pub fn insert_audit_log(
        &self,
        entry: &crate::domain::audit::NewAuditEntry,
    ) -> Result<(), AppError> {
        self.executor.execute(
            r#"INSERT INTO audit_log
               (id, user_id, username, action, entity_type, entity_id, entity_name,
                old_value, new_value, session_id, timestamp, status, error_message, metadata,
                previous_hash, entry_hash,
                event_type, actor_id, target_type, target_id, fiscal_year,
                before_snapshot, after_snapshot, node_id, details)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                       ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)"#,
            params![
                &entry.id,
                &entry.user_id,
                &entry.username,
                &entry.action,
                &entry.entity_type,
                &entry.entity_id,
                &entry.entity_name,
                &entry.old_value,
                &entry.new_value,
                &entry.session_id,
                &entry.timestamp,
                &entry.status,
                &entry.error_message,
                &entry.metadata,
                &entry.previous_hash,
                &entry.entry_hash,
                &entry.event_type,
                &entry.actor_id,
                &entry.target_type,
                &entry.target_id,
                &entry.fiscal_year,
                &entry.before_snapshot,
                &entry.after_snapshot,
                &entry.node_id,
                &entry.details,
            ],
        )?;
        Ok(())
    }

    pub fn fetch_latest_entry_hash(&self) -> Result<Option<String>, AppError> {
        Ok(self.executor.query_row_optional(
            "SELECT entry_hash FROM audit_log WHERE entry_hash IS NOT NULL ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn verify_audit_chain_streaming(
        &self,
    ) -> Result<crate::domain::audit_chain::VerificationSummary, AppError> {
        let mut stmt = self.executor.prepare(
            r#"SELECT previous_hash, entry_hash,
                      id, user_id, username, action, entity_type, entity_id, entity_name,
                      old_value, new_value, session_id, timestamp, status, error_message, metadata
               FROM audit_log
               ORDER BY rowid ASC"#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(AuditChainVerifyRow {
                previous_hash: row.get(0)?,
                entry_hash: row.get(1)?,
                entry: crate::domain::audit::NewAuditEntry {
                    id: row.get(2)?,
                    user_id: row.get(3)?,
                    username: row.get(4)?,
                    action: row.get(5)?,
                    entity_type: row.get(6)?,
                    entity_id: row.get(7)?,
                    entity_name: row.get(8)?,
                    old_value: row.get(9)?,
                    new_value: row.get(10)?,
                    session_id: row.get(11)?,
                    timestamp: row.get(12)?,
                    status: row.get(13)?,
                    error_message: row.get(14)?,
                    metadata: row.get(15)?,
                    previous_hash: None,
                    entry_hash: None,
                    event_type: None,
                    actor_id: None,
                    target_type: None,
                    target_id: None,
                    fiscal_year: None,
                    before_snapshot: None,
                    after_snapshot: None,
                    node_id: None,
                    details: None,
                },
            })
        })?;

        let mapped_iter = rows.map(|res| res.map_err(AppError::from));
        crate::domain::audit_chain::verify_chain_stream(mapped_iter)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Audit log query (SQL only; optional parameters)
    // ─────────────────────────────────────────────────────────────────────────

    pub fn count_entries(&self, q: &AuditQuery) -> Result<i64, AppError> {
        let total: i64 = self.executor.query_row(
            r#"SELECT COUNT(*)
               FROM audit_log
               WHERE (?1 IS NULL OR user_id = ?1)
                 AND (?2 IS NULL OR action = ?2)
                 AND (?3 IS NULL OR entity_type = ?3)
                 AND (?4 IS NULL OR timestamp >= ?4)
                 AND (?5 IS NULL OR timestamp <= ?5)
                 AND (?6 IS NULL OR status = ?6)
                 AND (?7 IS NULL OR (
                      entity_name LIKE ?7 OR username LIKE ?7 OR action LIKE ?7
                 ))"#,
            params![
                q.user_id.as_deref(),
                q.action.as_deref(),
                q.entity_type.as_deref(),
                q.start_timestamp.as_deref(),
                q.end_timestamp.as_deref(),
                q.status.as_deref(),
                q.search_like.as_deref(),
            ],
            |row| row.get(0),
        )?;
        Ok(total)
    }

    pub fn fetch_entries(&self, q: &AuditQuery) -> Result<Vec<AuditEntryDbRow>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT id, user_id, username, action, entity_type, entity_id, entity_name,
                       old_value, new_value, session_id, timestamp, status, error_message, metadata,
                       previous_hash, entry_hash
               FROM audit_log
               WHERE (?1 IS NULL OR user_id = ?1)
                 AND (?2 IS NULL OR action = ?2)
                 AND (?3 IS NULL OR entity_type = ?3)
                 AND (?4 IS NULL OR timestamp >= ?4)
                 AND (?5 IS NULL OR timestamp <= ?5)
                 AND (?6 IS NULL OR status = ?6)
                 AND (?7 IS NULL OR (
                      entity_name LIKE ?7 OR username LIKE ?7 OR action LIKE ?7
                 ))
               ORDER BY timestamp DESC
               LIMIT ?8 OFFSET ?9"#,
            params![
                q.user_id.as_deref(),
                q.action.as_deref(),
                q.entity_type.as_deref(),
                q.start_timestamp.as_deref(),
                q.end_timestamp.as_deref(),
                q.status.as_deref(),
                q.search_like.as_deref(),
                q.limit,
                q.offset,
            ],
            |row| {
                Ok(AuditEntryDbRow {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    username: row.get(2)?,
                    action: row.get(3)?,
                    entity_type: row.get(4)?,
                    entity_id: row.get(5)?,
                    entity_name: row.get(6)?,
                    old_value_json: row.get(7)?,
                    new_value_json: row.get(8)?,
                    session_id: row.get(9)?,
                    timestamp: row.get(10)?,
                    status: row.get(11)?,
                    error_message: row.get(12)?,
                    metadata_json: row.get(13)?,
                    previous_hash: row.get(14)?,
                    entry_hash: row.get(15)?,
                })
            },
        )?)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Audit event rows (supports both legacy and structured columns)
    // ─────────────────────────────────────────────────────────────────────────

    /// Fetch rows with ALL columns (legacy + structured) for the projection layer.
    /// Returns AuditEventRow which can be converted via to_audit_event().
    pub fn fetch_event_rows(&self, q: &AuditQuery) -> Result<Vec<AuditEventRow>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT id, user_id, username, action, entity_type, entity_id, entity_name,
                      old_value, new_value, session_id, timestamp, status, error_message, metadata,
                      previous_hash, entry_hash,
                      event_type, actor_id, target_type, target_id, fiscal_year,
                      before_snapshot, after_snapshot, node_id, details
               FROM audit_log
               WHERE (?1 IS NULL OR user_id = ?1)
                 AND (?2 IS NULL OR action = ?2)
                 AND (?3 IS NULL OR entity_type = ?3)
                 AND (?4 IS NULL OR timestamp >= ?4)
                 AND (?5 IS NULL OR timestamp <= ?5)
                 AND (?6 IS NULL OR status = ?6)
                 AND (?7 IS NULL OR (
                      entity_name LIKE ?7 OR username LIKE ?7 OR action LIKE ?7
                 ))
               ORDER BY timestamp DESC, id ASC
               LIMIT ?8 OFFSET ?9"#,
            params![
                q.user_id.as_deref(),
                q.action.as_deref(),
                q.entity_type.as_deref(),
                q.start_timestamp.as_deref(),
                q.end_timestamp.as_deref(),
                q.status.as_deref(),
                q.search_like.as_deref(),
                q.limit,
                q.offset,
            ],
            |row| {
                Ok(AuditEventRow {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    username: row.get(2)?,
                    action: row.get(3)?,
                    entity_type: row.get(4)?,
                    entity_id: row.get(5)?,
                    entity_name: row.get(6)?,
                    old_value_json: row.get(7)?,
                    new_value_json: row.get(8)?,
                    session_id: row.get(9)?,
                    timestamp: row.get(10)?,
                    status: row.get(11)?,
                    error_message: row.get(12)?,
                    metadata_json: row.get(13)?,
                    previous_hash: row.get(14)?,
                    entry_hash: row.get(15)?,
                    event_type: row.get(16)?,
                    actor_id: row.get(17)?,
                    target_type: row.get(18)?,
                    target_id: row.get(19)?,
                    fiscal_year: row.get(20)?,
                    before_snapshot_json: row.get(21)?,
                    after_snapshot_json: row.get(22)?,
                    node_id: row.get(23)?,
                    details_json: row.get(24)?,
                })
            },
        )?)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Keyset pagination primitives (deterministic, no OFFSET)
    // ─────────────────────────────────────────────────────────────────────────

    /// Keyset-paginated fetch of audit event rows.
    /// Uses (timestamp, id) as the pagination key for stable, deterministic ordering.
    ///
    /// - `keyset_timestamp`: the `timestamp` of the last row from the previous page (exclusive)
    /// - `keyset_id`: the `id` of the last row from the previous page (tiebreaker, exclusive)
    /// - `limit`: maximum number of rows to return
    /// - Returns: results strictly after the keyset, ordered by (timestamp ASC, id ASC)
    ///
    /// Passing `None` for both keyset values fetches the first page.
    pub fn fetch_event_rows_keyset(
        &self,
        q: &AuditQuery,
        keyset_timestamp: Option<&str>,
        keyset_id: Option<&str>,
    ) -> Result<Vec<AuditEventRow>, AppError> {
        let limit = q.limit;
        Ok(self.executor.query_all(
            r#"SELECT id, user_id, username, action, entity_type, entity_id, entity_name,
                      old_value, new_value, session_id, timestamp, status, error_message, metadata,
                      previous_hash, entry_hash,
                      event_type, actor_id, target_type, target_id, fiscal_year,
                      before_snapshot, after_snapshot, node_id, details
               FROM audit_log
               WHERE (?1 IS NULL OR user_id = ?1)
                 AND (?2 IS NULL OR action = ?2)
                 AND (?3 IS NULL OR entity_type = ?3)
                 AND (?4 IS NULL OR timestamp >= ?4)
                 AND (?5 IS NULL OR timestamp <= ?5)
                 AND (?6 IS NULL OR status = ?6)
                 AND (?7 IS NULL OR (
                      entity_name LIKE ?7 OR username LIKE ?7 OR action LIKE ?7
                 ))
                 AND (?8 IS NULL OR ?9 IS NULL OR
                      (timestamp > ?8 OR (timestamp = ?8 AND id > ?9)))
               ORDER BY timestamp ASC, id ASC
               LIMIT ?10"#,
            params![
                q.user_id.as_deref(),
                q.action.as_deref(),
                q.entity_type.as_deref(),
                q.start_timestamp.as_deref(),
                q.end_timestamp.as_deref(),
                q.status.as_deref(),
                q.search_like.as_deref(),
                keyset_timestamp,
                keyset_id,
                limit,
            ],
            |row| {
                Ok(AuditEventRow {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    username: row.get(2)?,
                    action: row.get(3)?,
                    entity_type: row.get(4)?,
                    entity_id: row.get(5)?,
                    entity_name: row.get(6)?,
                    old_value_json: row.get(7)?,
                    new_value_json: row.get(8)?,
                    session_id: row.get(9)?,
                    timestamp: row.get(10)?,
                    status: row.get(11)?,
                    error_message: row.get(12)?,
                    metadata_json: row.get(13)?,
                    previous_hash: row.get(14)?,
                    entry_hash: row.get(15)?,
                    event_type: row.get(16)?,
                    actor_id: row.get(17)?,
                    target_type: row.get(18)?,
                    target_id: row.get(19)?,
                    fiscal_year: row.get(20)?,
                    before_snapshot_json: row.get(21)?,
                    after_snapshot_json: row.get(22)?,
                    node_id: row.get(23)?,
                    details_json: row.get(24)?,
                })
            },
        )?)
    }

    /// Count total entries matching a keyset query (same WHERE clause without keyset + pagination).
    pub fn count_entries_keyset(&self, q: &AuditQuery) -> Result<i64, AppError> {
        let total: i64 = self.executor.query_row(
            r#"SELECT COUNT(*)
               FROM audit_log
               WHERE (?1 IS NULL OR user_id = ?1)
                 AND (?2 IS NULL OR action = ?2)
                 AND (?3 IS NULL OR entity_type = ?3)
                 AND (?4 IS NULL OR timestamp >= ?4)
                 AND (?5 IS NULL OR timestamp <= ?5)
                 AND (?6 IS NULL OR status = ?6)
                 AND (?7 IS NULL OR (
                      entity_name LIKE ?7 OR username LIKE ?7 OR action LIKE ?7
                 ))"#,
            params![
                q.user_id.as_deref(),
                q.action.as_deref(),
                q.entity_type.as_deref(),
                q.start_timestamp.as_deref(),
                q.end_timestamp.as_deref(),
                q.status.as_deref(),
                q.search_like.as_deref(),
            ],
            |row| row.get(0),
        )?;
        Ok(total)
    }

    /// Memory-aware Audit Log Scan (bounded peak RAM)
    pub fn fetch_entries_iter<F>(&self, q: &AuditQuery, consumer: F) -> Result<(), AppError>
    where
        F: FnMut(AuditEntryDbRow) -> Result<(), String>,
    {
        self.executor
            .query_iter(
                r#"SELECT id, user_id, username, action, entity_type, entity_id, entity_name,
                       old_value, new_value, session_id, timestamp, status, error_message, metadata,
                       previous_hash, entry_hash
               FROM audit_log
               WHERE (?1 IS NULL OR user_id = ?1)
                 AND (?2 IS NULL OR action = ?2)
                 AND (?3 IS NULL OR entity_type = ?3)
                 AND (?4 IS NULL OR timestamp >= ?4)
                 AND (?5 IS NULL OR timestamp <= ?5)
                 AND (?6 IS NULL OR status = ?6)
                 AND (?7 IS NULL OR (
                      entity_name LIKE ?7 OR username LIKE ?7 OR action LIKE ?7
                 ))
               ORDER BY timestamp ASC"#,
                params![
                    q.user_id.as_deref(),
                    q.action.as_deref(),
                    q.entity_type.as_deref(),
                    q.start_timestamp.as_deref(),
                    q.end_timestamp.as_deref(),
                    q.status.as_deref(),
                    q.search_like.as_deref(),
                ],
                |row| {
                    Ok(AuditEntryDbRow {
                        id: row.get(0)?,
                        user_id: row.get(1)?,
                        username: row.get(2)?,
                        action: row.get(3)?,
                        entity_type: row.get(4)?,
                        entity_id: row.get(5)?,
                        entity_name: row.get(6)?,
                        old_value_json: row.get(7)?,
                        new_value_json: row.get(8)?,
                        session_id: row.get(9)?,
                        timestamp: row.get(10)?,
                        status: row.get(11)?,
                        error_message: row.get(12)?,
                        metadata_json: row.get(13)?,
                        previous_hash: row.get(14)?,
                        entry_hash: row.get(15)?,
                    })
                },
                consumer,
            )
            .map_err(AppError::from)
    }

    pub fn fetch_user_activity_since(
        &self,
        user_id: &str,
        since_timestamp: &str,
    ) -> Result<Vec<AuditEntryDbRow>, AppError> {
        let mut out = Vec::new();
        self.executor.query_iter(
            r#"SELECT id, user_id, username, action, entity_type, entity_id, entity_name,
                      old_value, new_value, session_id, timestamp, status, error_message, metadata,
                      previous_hash, entry_hash
               FROM audit_log
               WHERE user_id = ?1 AND timestamp >= ?2
               ORDER BY timestamp DESC"#,
            params![user_id, since_timestamp],
            |row| {
                Ok(AuditEntryDbRow {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    username: row.get(2)?,
                    action: row.get(3)?,
                    entity_type: row.get(4)?,
                    entity_id: row.get(5)?,
                    entity_name: row.get(6)?,
                    old_value_json: row.get(7)?,
                    new_value_json: row.get(8)?,
                    session_id: row.get(9)?,
                    timestamp: row.get(10)?,
                    status: row.get(11)?,
                    error_message: row.get(12)?,
                    metadata_json: row.get(13)?,
                    previous_hash: row.get(14)?,
                    entry_hash: row.get(15)?,
                })
            },
            |row| {
                out.push(row);
                // Safety: cap the results to prevent memory bomb if since_timestamp is very old
                if out.len() > 1000 {
                    return Err("Too many results".to_string());
                }
                Ok(())
            },
        )?;
        Ok(out)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Statistics (SQL aggregation only; calculations belong to service)
    // ─────────────────────────────────────────────────────────────────────────

    pub fn count_by_action(&self, action: &str) -> Result<i64, AppError> {
        Ok(self
            .executor
            .query_row("SELECT COUNT(*) FROM audit_log WHERE action = ?1", params![action], |row| row.get(0))?)
    }

    pub fn count_total_ops(&self, start_ts: &str, end_ts: &str) -> Result<i64, AppError> {
        Ok(self.executor.query_row(
            "SELECT COUNT(*) FROM audit_log WHERE timestamp >= ?1 AND timestamp <= ?2",
            params![start_ts, end_ts],
            |row| row.get(0),
        )?)
    }

    pub fn count_failed_ops(&self, start_ts: &str, end_ts: &str) -> Result<i64, AppError> {
        Ok(self.executor.query_row(
            "SELECT COUNT(*) FROM audit_log WHERE timestamp >= ?1 AND timestamp <= ?2 AND status = 'Failed'",
            params![start_ts, end_ts],
            |row| row.get(0),
        )?)
    }

    pub fn top_active_users(
        &self,
        start_ts: &str,
        end_ts: &str,
    ) -> Result<Vec<UserActivitySummaryRow>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT user_id, username, COUNT(*) as total,
                      SUM(CASE WHEN status = 'Failed' THEN 1 ELSE 0 END) as failed,
                      MAX(timestamp) as last_activity
               FROM audit_log
               WHERE timestamp >= ?1 AND timestamp <= ?2
               GROUP BY user_id, username
               ORDER BY total DESC
               LIMIT 10"#,
            params![start_ts, end_ts],
            |row| {
                Ok(UserActivitySummaryRow {
                    user_id: row.get(0)?,
                    username: row.get(1)?,
                    total_operations: row.get(2)?,
                    failed_operations: row.get(3)?,
                    last_activity: row.get(4)?,
                })
            },
        )?)
    }

    pub fn ops_by_type(
        &self,
        start_ts: &str,
        end_ts: &str,
    ) -> Result<Vec<OperationCountRow>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT action, COUNT(*) as count
               FROM audit_log
               WHERE timestamp >= ?1 AND timestamp <= ?2
               GROUP BY action
               ORDER BY count DESC"#,
            params![start_ts, end_ts],
            |row| {
                Ok(OperationCountRow {
                    action: row.get(0)?,
                    count: row.get(1)?,
                })
            },
        )?)
    }

    pub fn ops_by_day(
        &self,
        start_ts: &str,
        end_ts: &str,
    ) -> Result<Vec<DailyOperationCountRow>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT DATE(timestamp) as date, COUNT(*) as total,
                      SUM(CASE WHEN status = 'Failed' THEN 1 ELSE 0 END) as failed
               FROM audit_log
               WHERE timestamp >= ?1 AND timestamp <= ?2
               GROUP BY DATE(timestamp)
               ORDER BY date"#,
            params![start_ts, end_ts],
            |row| {
                Ok(DailyOperationCountRow {
                    date: row.get(0)?,
                    total: row.get(1)?,
                    failed: row.get(2)?,
                })
            },
        )?)
    }

    pub fn upsert_daily_summary(
        &self,
        date: &str,
        total_operations: i64,
        failed_operations: i64,
        users_active: i64,
        last_updated: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT OR REPLACE INTO audit_summary
             (date, total_operations, failed_operations, users_active, last_updated)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                date,
                total_operations,
                failed_operations,
                users_active,
                last_updated
            ],
        )?;
        Ok(())
    }

    pub fn count_distinct_users(&self, start_ts: &str, end_ts: &str) -> Result<i64, AppError> {
        Ok(self.executor.query_row(
            "SELECT COUNT(DISTINCT user_id) FROM audit_log WHERE timestamp >= ?1 AND timestamp <= ?2",
            params![start_ts, end_ts],
            |row| row.get(0),
        )?)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Cleanup (SQL only)
    // ─────────────────────────────────────────────────────────────────────────

    pub fn delete_older_than(&self, cutoff_timestamp: &str) -> Result<u64, AppError> {
        Ok(self.executor.execute(
            "DELETE FROM audit_log WHERE timestamp < ?1",
            params![cutoff_timestamp],
        )? as u64)
    }

    /// Find the timestamp of the latest audit entry with the given action string.
    pub fn find_latest_action_timestamp(&self, action: &str) -> Result<Option<String>, AppError> {
        Ok(self.executor.query_row(
            "SELECT MAX(timestamp) FROM audit_log WHERE action = ?1 AND status = 'Success'",
            params![action],
            |row| row.get::<_, Option<String>>(0),
        )?)
    }

    pub fn get_last_backup_timestamp(&self) -> Result<Option<String>, AppError> {
        Ok(self.executor.query_row(
            "SELECT MAX(timestamp) FROM audit_log WHERE action IN ('CreateBackup', 'BackupCreated')",
            [],
            |r| r.get::<_, Option<String>>(0),
        )?)
    }
}
