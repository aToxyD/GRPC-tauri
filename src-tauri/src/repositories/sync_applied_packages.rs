//! Records applied sync interchange package identifiers (SQL only).

use crate::errors::{AppError, AppResult};

use super::executor::DbExecutor;

pub struct SyncAppliedPackagesRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> SyncAppliedPackagesRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Returns `Ok(true)` if this was the first time this `package_id` was recorded.
    pub fn insert_if_new(
        &self,
        package_id: &str,
        kind: &str,
        source_node_id: Option<&str>,
        imported_by: &str,
    ) -> AppResult<bool> {
        if package_id.trim().is_empty() {
            return Err(AppError::Validation(
                crate::errors::ValidationError::InvalidFormat {
                    field: "package_id".into(),
                    message: "معرّف الحزمة فارغ".into(),
                },
            ));
        }

        self.executor
            .execute(
                r#"INSERT OR IGNORE INTO applied_sync_packages (package_id, kind, source_node_id, imported_at, imported_by)
                   VALUES (?1, ?2, ?3, datetime('now'), ?4)"#,
                rusqlite::params![
                    package_id,
                    kind,
                    source_node_id,
                    imported_by,
                ],
            )
            .map_err(AppError::from)?;
        Ok(self.executor.changes() == 1)
    }

    pub fn has_imported(&self, package_id: &str) -> AppResult<bool> {
        if package_id.trim().is_empty() {
            return Ok(false);
        }
        let exists = self.executor.query_row(
            "SELECT EXISTS(SELECT 1 FROM applied_sync_packages WHERE package_id = ?1)",
            rusqlite::params![package_id],
            |row| row.get::<_, i64>(0).map(|v| v == 1),
        )?;
        Ok(exists)
    }
}
