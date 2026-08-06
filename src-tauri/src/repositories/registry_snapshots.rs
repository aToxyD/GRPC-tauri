//! Registry fleet-state snapshot repository (SQL only).
//!
//! RFC 2026-08-04 §3.9 (B4): Registry Packages deliver fleet state to nodes.
//! Snapshots are persisted verbatim for auditability (P4) and deterministic
//! replay; applying fleet state to live unit records is out of scope here.

use crate::errors::AppResult;
use crate::repositories::executor::DbExecutor;
use rusqlite::{params, Row};

/// Persisted fleet-state snapshot row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrySnapshotRow {
    pub package_id: String,
    pub snapshot_version: u64,
    pub wilaya_identity_id: String,
    pub payload_json: String,
    pub imported_at: String,
    pub imported_by: String,
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<RegistrySnapshotRow> {
    let snapshot_version: i64 = row.get(1)?;
    Ok(RegistrySnapshotRow {
        package_id: row.get(0)?,
        snapshot_version: snapshot_version as u64,
        wilaya_identity_id: row.get(2)?,
        payload_json: row.get(3)?,
        imported_at: row.get(4)?,
        imported_by: row.get(5)?,
    })
}

pub struct RegistrySnapshotsRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> RegistrySnapshotsRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Persist an accepted registry package. Returns `Ok(true)` if a new row
    /// was inserted, `Ok(false)` if the package was already recorded.
    pub fn insert_snapshot(
        &self,
        package_id: &str,
        snapshot_version: u64,
        wilaya_identity_id: &str,
        payload_json: &str,
        imported_at: &str,
        imported_by: &str,
    ) -> AppResult<bool> {
        self.executor
            .execute(
                r#"INSERT OR IGNORE INTO registry_snapshots
                   (package_id, snapshot_version, wilaya_identity_id, payload_json, imported_at, imported_by)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
                params![
                    package_id,
                    snapshot_version as i64,
                    wilaya_identity_id,
                    payload_json,
                    imported_at,
                    imported_by,
                ],
            )
            .map_err(crate::errors::AppError::from)?;
        Ok(self.executor.changes() == 1)
    }

    /// Latest persisted snapshot (highest snapshot_version, then latest id).
    pub fn latest_snapshot(&self) -> AppResult<Option<RegistrySnapshotRow>> {
        let row = self.executor.query_row_optional(
            r#"SELECT package_id, snapshot_version, wilaya_identity_id, payload_json, imported_at, imported_by
               FROM registry_snapshots
               ORDER BY snapshot_version DESC, id DESC
               LIMIT 1"#,
            [],
            map_row,
        )?;
        Ok(row)
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
    fn snapshot_roundtrip_and_idempotency() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = RegistrySnapshotsRepository::new(make_executor(&db));

        assert!(repo
            .insert_snapshot(
                "pkg-reg-1",
                1,
                "wilaya-id",
                "{\"fleet\":[]}",
                "2026-08-05T00:00:00Z",
                "admin",
            )
            .unwrap());

        // Same package is idempotent (INSERT OR IGNORE).
        assert!(!repo
            .insert_snapshot(
                "pkg-reg-1",
                1,
                "wilaya-id",
                "{\"fleet\":[]}",
                "2026-08-05T00:00:00Z",
                "admin",
            )
            .unwrap());

        let latest = repo.latest_snapshot().unwrap().expect("snapshot present");
        assert_eq!(latest.package_id, "pkg-reg-1");
        assert_eq!(latest.snapshot_version, 1);
        assert_eq!(latest.wilaya_identity_id, "wilaya-id");
    }

    #[test]
    fn latest_returns_highest_snapshot_version() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = RegistrySnapshotsRepository::new(make_executor(&db));

        repo.insert_snapshot(
            "pkg-v1",
            1,
            "wilaya-id",
            "{\"fleet\":[1]}",
            "2026-08-05T00:00:00Z",
            "admin",
        )
        .unwrap();
        repo.insert_snapshot(
            "pkg-v2",
            2,
            "wilaya-id",
            "{\"fleet\":[1,2]}",
            "2026-08-05T01:00:00Z",
            "admin",
        )
        .unwrap();

        let latest = repo.latest_snapshot().unwrap().unwrap();
        assert_eq!(latest.package_id, "pkg-v2");
        assert_eq!(latest.snapshot_version, 2);
    }

    #[test]
    fn empty_repository_has_no_latest() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = RegistrySnapshotsRepository::new(make_executor(&db));
        assert_eq!(repo.latest_snapshot().unwrap(), None);
    }
}
