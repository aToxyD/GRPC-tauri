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
    /// `package_sequence` / `issuer_identity_id` are the B4 transport metadata
    /// (RFC 2026-08-04 §3.4.1); legacy HMAC/V1 packages pass `None`.
    pub fn insert_if_new(
        &self,
        package_id: &str,
        kind: &str,
        source_node_id: Option<&str>,
        imported_by: &str,
        package_sequence: Option<u64>,
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
                r#"INSERT OR IGNORE INTO applied_sync_packages (package_id, kind, source_node_id, imported_at, imported_by, package_sequence, issuer_identity_id)
                   VALUES (?1, ?2, ?3, datetime('now'), ?4, ?5, ?6)"#,
                rusqlite::params![
                    package_id,
                    kind,
                    source_node_id,
                    imported_by,
                    package_sequence.map(|s| s as i64),
                    issuer_identity_id,
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

    /// Last applied transport sequence for a given issuing node identity
    /// (RFC 2026-08-04 §3.4.1 Transport Guard). Returns `None` when no package
    /// from that issuer has been applied yet — the expected first sequence is 1.
    pub fn last_applied_sequence_for_issuer(
        &self,
        issuer_identity_id: &str,
    ) -> AppResult<Option<u64>> {
        if issuer_identity_id.trim().is_empty() {
            return Ok(None);
        }
        let seq: Option<i64> = self.executor.query_row_optional(
            "SELECT last_applied_sequence FROM sync_issuer_sequence WHERE issuer_identity_id = ?1",
            rusqlite::params![issuer_identity_id],
            |row| row.get(0),
        )?;
        Ok(seq.and_then(|s| u64::try_from(s).ok()))
    }

    /// Advance the per-issuer transport sequence ledger after a package has been
    /// applied. Must run inside the same transaction as the import (fail-closed).
    pub fn record_issuer_sequence(
        &self,
        issuer_identity_id: &str,
        package_sequence: u64,
    ) -> AppResult<()> {
        if issuer_identity_id.trim().is_empty() {
            return Err(AppError::Validation(
                crate::errors::ValidationError::InvalidFormat {
                    field: "issuer_identity_id".into(),
                    message: "معرّف هوية المُصدِر فارغ".into(),
                },
            ));
        }
        self.executor
            .execute(
                r#"INSERT INTO sync_issuer_sequence (issuer_identity_id, last_applied_sequence, updated_at)
                   VALUES (?1, ?2, datetime('now'))
                   ON CONFLICT(issuer_identity_id) DO UPDATE SET
                     last_applied_sequence = excluded.last_applied_sequence,
                     updated_at = datetime('now')"#,
                rusqlite::params![issuer_identity_id, package_sequence as i64],
            )
            .map_err(AppError::from)?;
        Ok(())
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
    fn insert_with_transport_columns_roundtrips() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = SyncAppliedPackagesRepository::new(make_executor(&db));

        let first = repo
            .insert_if_new("pkg-1", "trust", Some("w-1"), "admin", Some(7), Some("issuer-a"))
            .unwrap();
        assert!(first);
        assert!(repo.has_imported("pkg-1").unwrap());

        // Legacy V1 package: transport columns stay NULL.
        let legacy = repo
            .insert_if_new("pkg-2", "daily_report", Some("u-1"), "admin", None, None)
            .unwrap();
        assert!(legacy);
        assert!(repo.has_imported("pkg-2").unwrap());
    }

    #[test]
    fn issuer_sequence_ledger_is_per_issuer_and_monotonic() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = SyncAppliedPackagesRepository::new(make_executor(&db));

        assert_eq!(repo.last_applied_sequence_for_issuer("issuer-a").unwrap(), None);

        repo.record_issuer_sequence("issuer-a", 1).unwrap();
        repo.record_issuer_sequence("issuer-a", 2).unwrap();
        repo.record_issuer_sequence("issuer-b", 1).unwrap();

        assert_eq!(repo.last_applied_sequence_for_issuer("issuer-a").unwrap(), Some(2));
        assert_eq!(repo.last_applied_sequence_for_issuer("issuer-b").unwrap(), Some(1));
        assert_eq!(repo.last_applied_sequence_for_issuer("issuer-c").unwrap(), None);
    }

    #[test]
    fn empty_issuer_id_is_treated_as_absent() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = SyncAppliedPackagesRepository::new(make_executor(&db));

        assert_eq!(repo.last_applied_sequence_for_issuer("").unwrap(), None);
        assert!(repo.record_issuer_sequence("", 1).is_err());
    }
}
