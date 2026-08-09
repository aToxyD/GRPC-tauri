//! Licensing derived-view storage (SQL only).
//!
//! ADR-0042 §3 / §6 / §7. Stores the consumer's DERIVED, replaceable view of
//! license state. Import REPLACES any previously held row for a license id
//! (full overwrite, never a merge — ADR-0003 Invariant 2). `artifact_json`
//! keeps the exact canonical envelope bytes received so the FULL verification
//! pipeline (artifact-spec §4 steps 1–6) can be re-run deterministically.

use crate::errors::{AppError, AppResult, ValidationError};

use super::executor::DbExecutor;

/// A stored derived license row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicensingLicenseRow {
    pub license_id: String,
    pub artifact_id: String,
    pub type_key: String,
    pub subject_id: String,
    /// Declared entitlement keys (JSON array), e.g. `["core.sync","core.stock"]`.
    pub entitlements_json: String,
    pub status: String,
    pub contract_version_major: u32,
    pub contract_version_minor: u32,
    /// Exact canonical envelope bytes received at import (for re-verification).
    pub artifact_json: String,
    pub imported_at: String,
    pub last_verified_at: Option<String>,
}

pub struct LicensingLicenseRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> LicensingLicenseRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Full-overwrite insert/replace of the derived view for a license id.
    pub fn upsert(&self, row: &LicensingLicenseRow) -> AppResult<()> {
        validate_license_id(&row.license_id)?;
        self.executor
            .execute(
                r#"INSERT INTO licensing_license (
                       license_id, artifact_id, type_key, subject_id, entitlements_json,
                       status, contract_version_major, contract_version_minor,
                       artifact_json, imported_at, last_verified_at
                   )
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                   ON CONFLICT(license_id) DO UPDATE SET
                       artifact_id = excluded.artifact_id,
                       type_key = excluded.type_key,
                       subject_id = excluded.subject_id,
                       entitlements_json = excluded.entitlements_json,
                       status = excluded.status,
                       contract_version_major = excluded.contract_version_major,
                       contract_version_minor = excluded.contract_version_minor,
                       artifact_json = excluded.artifact_json,
                       imported_at = excluded.imported_at,
                       last_verified_at = excluded.last_verified_at"#,
                rusqlite::params![
                    row.license_id,
                    row.artifact_id,
                    row.type_key,
                    row.subject_id,
                    row.entitlements_json,
                    row.status,
                    row.contract_version_major as i64,
                    row.contract_version_minor as i64,
                    row.artifact_json,
                    row.imported_at,
                    row.last_verified_at,
                ],
            )
            .map_err(AppError::from)?;
        Ok(())
    }

    pub fn get_by_id(&self, license_id: &str) -> AppResult<Option<LicensingLicenseRow>> {
        self.executor
            .query_row_optional(
                "SELECT license_id, artifact_id, type_key, subject_id, entitlements_json, \
                        status, contract_version_major, contract_version_minor, artifact_json, \
                        imported_at, last_verified_at \
                 FROM licensing_license WHERE license_id = ?1",
                rusqlite::params![license_id],
                map_row,
            )
            .map_err(AppError::from)
    }

    pub fn list_all(&self) -> AppResult<Vec<LicensingLicenseRow>> {
        self.executor
            .query_all(
                "SELECT license_id, artifact_id, type_key, subject_id, entitlements_json, \
                        status, contract_version_major, contract_version_minor, artifact_json, \
                        imported_at, last_verified_at \
                 FROM licensing_license ORDER BY imported_at ASC",
                [],
                map_row,
            )
            .map_err(AppError::from)
    }

    /// Licenses whose signed status is Active (enforcement candidates).
    pub fn list_active(&self) -> AppResult<Vec<LicensingLicenseRow>> {
        self.executor
            .query_all(
                "SELECT license_id, artifact_id, type_key, subject_id, entitlements_json, \
                        status, contract_version_major, contract_version_minor, artifact_json, \
                        imported_at, last_verified_at \
                 FROM licensing_license WHERE status = 'active' ORDER BY imported_at ASC",
                [],
                map_row,
            )
            .map_err(AppError::from)
    }

    pub fn count(&self) -> AppResult<u64> {
        let n: i64 = self
            .executor
            .query_row("SELECT COUNT(*) FROM licensing_license", [], |r| r.get(0))
            .map_err(AppError::from)?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    /// Touch the re-verification timestamp after a successful full pipeline run.
    pub fn update_last_verified(&self, license_id: &str, verified_at: &str) -> AppResult<()> {
        self.executor
            .execute(
                "UPDATE licensing_license SET last_verified_at = ?2 WHERE license_id = ?1",
                rusqlite::params![license_id, verified_at],
            )
            .map_err(AppError::from)?;
        Ok(())
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LicensingLicenseRow> {
    Ok(LicensingLicenseRow {
        license_id: row.get(0)?,
        artifact_id: row.get(1)?,
        type_key: row.get(2)?,
        subject_id: row.get(3)?,
        entitlements_json: row.get(4)?,
        status: row.get(5)?,
        contract_version_major: u32::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
        contract_version_minor: u32::try_from(row.get::<_, i64>(7)?).unwrap_or(0),
        artifact_json: row.get(8)?,
        imported_at: row.get(9)?,
        last_verified_at: row.get(10)?,
    })
}

fn validate_license_id(license_id: &str) -> AppResult<()> {
    if license_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "license_id".into(),
            message: "license id is empty".into(),
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ConnectionFactory, Database};

    fn repo(db: &Database) -> LicensingLicenseRepository<'_> {
        LicensingLicenseRepository::new(db.executor())
    }

    fn row(license_id: &str, status: &str) -> LicensingLicenseRow {
        LicensingLicenseRow {
            license_id: license_id.to_string(),
            artifact_id: "a".repeat(64),
            type_key: "production".to_string(),
            subject_id: "subject-1".to_string(),
            entitlements_json: r#"["core.sync","core.stock"]"#.to_string(),
            status: status.to_string(),
            contract_version_major: 1,
            contract_version_minor: 0,
            artifact_json: "{}".to_string(),
            imported_at: "2026-08-09T00:00:00Z".to_string(),
            last_verified_at: None,
        }
    }

    #[test]
    fn upsert_replaces_derived_view_full_overwrite() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let r = repo(&db);
        r.upsert(&row("lic-1", "active")).unwrap();
        let mut revoked = row("lic-1", "revoked");
        revoked.entitlements_json = r#"["core.admin"]"#.to_string();
        r.upsert(&revoked).unwrap();

        let stored = r.get_by_id("lic-1").unwrap().unwrap();
        assert_eq!(stored.status, "revoked");
        assert_eq!(stored.entitlements_json, r#"["core.admin"]"#);
        // Full overwrite: the row count stays at one for the id.
        assert_eq!(r.count().unwrap(), 1);
    }

    #[test]
    fn idempotent_reimport_of_identical_artifact() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let r = repo(&db);
        r.upsert(&row("lic-1", "active")).unwrap();
        r.upsert(&row("lic-1", "active")).unwrap();
        assert_eq!(r.count().unwrap(), 1);
    }

    #[test]
    fn list_active_filters_status() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let r = repo(&db);
        r.upsert(&row("lic-a", "active")).unwrap();
        r.upsert(&row("lic-b", "suspended")).unwrap();
        r.upsert(&row("lic-c", "active")).unwrap();
        let actives = r.list_active().unwrap();
        assert_eq!(actives.len(), 2);
        assert!(actives.iter().all(|l| l.status == "active"));
    }

    #[test]
    fn update_last_verified_touches_only_target() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let r = repo(&db);
        r.upsert(&row("lic-a", "active")).unwrap();
        r.update_last_verified("lic-a", "2026-08-09T12:00:00Z")
            .unwrap();
        assert_eq!(
            r.get_by_id("lic-a")
                .unwrap()
                .unwrap()
                .last_verified_at
                .as_deref(),
            Some("2026-08-09T12:00:00Z")
        );
        assert!(r.get_by_id("lic-b").unwrap().is_none());
    }

    #[test]
    fn rejects_empty_license_id() {
        let db = ConnectionFactory::new_for_test().unwrap();
        assert!(repo(&db).upsert(&row("", "active")).is_err());
    }
}
