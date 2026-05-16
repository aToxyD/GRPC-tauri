//! Sync Conflicts Repository
//!
//! Stores and queries sync conflict records.
//! SQL only — no business logic.

use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

pub struct SyncConflictRepository<'a> {
    executor: DbExecutor<'a>,
}

#[derive(Debug, Clone)]
pub struct SyncConflictRow {
    pub id: String,
    pub package_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub conflict_type: String,
    pub severity: String,
    pub description: String,
    pub suggested_resolution: Option<String>,
    pub resolved: bool,
    pub resolution_note: Option<String>,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct NodeMetricsRow {
    pub node_id: String,
    pub last_sync_timestamp: Option<String>,
    pub packages_received: i64,
    pub packages_rejected: i64,
    pub replay_attempts: i64,
}

impl<'a> SyncConflictRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_conflict(
        &self,
        id: &str,
        package_id: &str,
        source_node_id: &str,
        target_node_id: &str,
        conflict_type: &str,
        severity: &str,
        description: &str,
        suggested_resolution: Option<&str>,
        created_at: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            r#"INSERT INTO sync_conflicts
               (id, package_id, source_node_id, target_node_id, conflict_type, severity,
                description, suggested_resolution, resolved, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)"#,
            params![
                id,
                package_id,
                source_node_id,
                target_node_id,
                conflict_type,
                severity,
                description,
                suggested_resolution,
                created_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_conflicts(&self, unresolved_only: bool) -> Result<Vec<SyncConflictRow>, AppError> {
        let sql = if unresolved_only {
            r#"SELECT id, package_id, source_node_id, target_node_id, conflict_type, severity,
                      description, suggested_resolution, resolved, resolution_note, resolved_by,
                      resolved_at, created_at
               FROM sync_conflicts WHERE resolved = 0
               ORDER BY created_at DESC"#
        } else {
            r#"SELECT id, package_id, source_node_id, target_node_id, conflict_type, severity,
                      description, suggested_resolution, resolved, resolution_note, resolved_by,
                      resolved_at, created_at
               FROM sync_conflicts
               ORDER BY created_at DESC"#
        };

        Ok(self.executor.query_all(sql, [], Self::map_row)?)
    }

    pub fn list_recent(&self, limit: i64) -> Result<Vec<SyncConflictRow>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT id, package_id, source_node_id, target_node_id, conflict_type, severity,
                      description, suggested_resolution, resolved, resolution_note, resolved_by,
                      resolved_at, created_at
               FROM sync_conflicts
               ORDER BY created_at DESC LIMIT ?1"#,
            params![limit],
            Self::map_row,
        )?)
    }

    pub fn resolve_conflict(
        &self,
        id: &str,
        resolved_by: &str,
        note: &str,
        resolved_at: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            r#"UPDATE sync_conflicts
               SET resolved = 1, resolved_by = ?2, resolution_note = ?3, resolved_at = ?4
               WHERE id = ?1"#,
            params![id, resolved_by, note, resolved_at],
        )?;
        Ok(())
    }

    pub fn count_total(&self) -> Result<i64, AppError> {
        Ok(self
            .executor
            .query_row("SELECT COUNT(*) FROM sync_conflicts", [], |row| row.get(0))?)
    }

    pub fn count_unresolved(&self) -> Result<i64, AppError> {
        Ok(self.executor.query_row(
            "SELECT COUNT(*) FROM sync_conflicts WHERE resolved = 0",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn count_by_severity(&self) -> Result<Vec<(String, i64)>, AppError> {
        Ok(self.executor.query_all(
            "SELECT severity, COUNT(*) FROM sync_conflicts GROUP BY severity ORDER BY COUNT(*) DESC",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?)
    }

    pub fn count_by_type(&self) -> Result<Vec<(String, i64)>, AppError> {
        Ok(self.executor.query_all(
            "SELECT conflict_type, COUNT(*) FROM sync_conflicts GROUP BY conflict_type ORDER BY COUNT(*) DESC",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?)
    }

    pub fn count_by_conflict_type(&self, conflict_type: &str) -> Result<i64, AppError> {
        Ok(self.executor.query_row(
            "SELECT COUNT(*) FROM sync_conflicts WHERE conflict_type = ?1",
            params![conflict_type],
            |row| row.get(0),
        )?)
    }

    pub fn get_per_node_metrics(&self) -> Result<Vec<NodeMetricsRow>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT source_node_id,
                      MAX(created_at) as last_sync,
                      COUNT(*) as total,
                      SUM(CASE WHEN resolved = 0 THEN 1 ELSE 0 END) as rejected,
                      SUM(CASE WHEN conflict_type = 'REPLAY_ATTEMPT' THEN 1 ELSE 0 END) as replay
               FROM sync_conflicts
               GROUP BY source_node_id
               ORDER BY last_sync DESC"#,
            [],
            |row| {
                Ok(NodeMetricsRow {
                    node_id: row.get(0)?,
                    last_sync_timestamp: row.get(1)?,
                    packages_received: row.get(2)?,
                    packages_rejected: row.get(3)?,
                    replay_attempts: row.get(4)?,
                })
            },
        )?)
    }

    fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncConflictRow> {
        Ok(SyncConflictRow {
            id: row.get(0)?,
            package_id: row.get(1)?,
            source_node_id: row.get(2)?,
            target_node_id: row.get(3)?,
            conflict_type: row.get(4)?,
            severity: row.get(5)?,
            description: row.get(6)?,
            suggested_resolution: row.get(7)?,
            resolved: row.get::<_, i64>(8)? != 0,
            resolution_note: row.get(9)?,
            resolved_by: row.get(10)?,
            resolved_at: row.get(11)?,
            created_at: row.get(12)?,
        })
    }
}
