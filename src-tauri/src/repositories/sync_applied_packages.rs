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
    ///
    /// Exact-package replay protection (SEC-056D/SEC-057): the `package_id` is
    /// the only dedup key — no transport sequence is recorded or compared.
    pub fn insert_if_new(
        &self,
        package_id: &str,
        kind: &str,
        source_node_id: Option<&str>,
        imported_by: &str,
        issuer_identity_id: Option<&str>,
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
                r#"INSERT OR IGNORE INTO applied_sync_packages (package_id, kind, source_node_id, imported_at, imported_by, issuer_identity_id)
                   VALUES (?1, ?2, ?3, datetime('now'), ?4, ?5)"#,
                rusqlite::params![package_id, kind, source_node_id, imported_by, issuer_identity_id],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ConnectionFactory, Database};

    fn make_executor(db: &Database) -> DbExecutor<'_> {
        db.executor()
    }

    #[test]
    fn insert_tracks_package_id_exact_dedup() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = SyncAppliedPackagesRepository::new(make_executor(&db));

        let first = repo
            .insert_if_new("pkg-1", "trust", Some("w-1"), "admin", Some("issuer-a"))
            .unwrap();
        assert!(first);
        assert!(repo.has_imported("pkg-1").unwrap());

        // Exact-package replay is rejected (same package_id is not re-inserted).
        let replay = repo
            .insert_if_new("pkg-1", "trust", Some("w-1"), "admin", Some("issuer-a"))
            .unwrap();
        assert!(!replay);

        let fresh = repo
            .insert_if_new(
                "pkg-2",
                "daily_report",
                Some("u-1"),
                "admin",
                Some("issuer-a"),
            )
            .unwrap();
        assert!(fresh);
        assert!(repo.has_imported("pkg-2").unwrap());
    }

    #[test]
    fn empty_package_id_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = SyncAppliedPackagesRepository::new(make_executor(&db));
        assert!(repo
            .insert_if_new("", "trust", None, "admin", None)
            .is_err());
    }
}
