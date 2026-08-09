//! Apply decrypted Registry Package (fleet-state snapshot) to the local node.
//!
//! RFC 2026-08-04 §3.9 (B4): kind = `registry`. Registry snapshots are
//! persisted verbatim for auditability (P4) and deterministic replay. Applying
//! fleet state to live unit records is out of scope (unit management carries
//! its own authz node-type guard).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::sync::ImportedPackageRegistry;
use crate::application::sync::SyncPackage;
use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
use crate::repositories::{DbExecutor, RegistrySnapshotsRepository};

pub const REGISTRY_PACKAGE_KIND: &str = "registry";

/// One fleet entry: a registered unit bound to its node identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitFleetEntry {
    pub unit_id: Uuid,
    pub code: String,
    pub name: String,
    pub identity_id: Uuid,
}

/// Fleet-state snapshot carried by a Registry Package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPackagePayload {
    pub snapshot_version: u64,
    pub wilaya_identity_id: Uuid,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<UnitFleetEntry>,
}

#[derive(Debug, Clone)]
pub struct ImportRegistryPackageInput {
    pub package: SyncPackage<RegistryPackagePayload>,
    pub imported_by: String,
}

#[derive(Debug, Clone)]
pub struct ImportRegistryPackageOutcome {
    pub snapshot_version: u64,
    pub unit_count: usize,
    pub package_id: String,
}

pub fn execute(
    executor: DbExecutor<'_>,
    registry: &impl ImportedPackageRegistry,
    input: ImportRegistryPackageInput,
) -> AppResult<ImportRegistryPackageOutcome> {
    if input.package.metadata.source_node_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "source_node_id".into(),
            message: "حزمة سجل بلا مصدر".into(),
        }));
    }
    if input.package.payload.snapshot_version == 0 {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "snapshot_version".into(),
            message: "إصدار لقطة السجل يجب أن يبدأ من 1".into(),
        }));
    }

    let package_id = input.package.metadata.package_id.clone();
    if registry.has_imported(&package_id)? {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::DuplicateSyncPackage {
                package_id: package_id.0.clone(),
            },
        ));
    }

    let payload_json = serde_json::to_string(&input.package.payload)?;
    RegistrySnapshotsRepository::new(executor).insert_snapshot(
        &package_id.0,
        input.package.payload.snapshot_version,
        &input.package.payload.wilaya_identity_id.to_string(),
        &payload_json,
        &chrono::Utc::now().to_rfc3339(),
        &input.imported_by,
    )?;
    registry.mark_imported(&package_id)?;

    Ok(ImportRegistryPackageOutcome {
        snapshot_version: input.package.payload.snapshot_version,
        unit_count: input.package.payload.units.len(),
        package_id: package_id.0.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::sync::{
        PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
    };
    use crate::db::{ConnectionFactory, Database};
    use crate::errors::{AppError, BusinessLogicError};
    use crate::infrastructure::db::sync_import::SqliteImportedPackageRegistry;
    use crate::repositories::{DbExecutor, RegistrySnapshotsRepository};
    use uuid::Uuid;

    fn make_executor(db: &Database) -> DbExecutor<'_> {
        db.executor()
    }

    fn registry_package(
        pkg_id: &str,
        snapshot_version: u64,
    ) -> SyncPackage<RegistryPackagePayload> {
        SyncPackage {
            metadata: SyncPackageMetadata {
                schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
                created_at: chrono::Utc::now(),
                source_node_id: "w-node".to_string(),
                package_sequence: Some(1),
                issuer_identity_id: Some(Uuid::from_u128(
                    0x0000_0000_0000_0000_0000_0000_0000_0001,
                )),
                package_id: PackageId(pkg_id.to_string()),
                signature_version: Some(2),
                signing_key_id: None,
                integrity_hash: None,
                signature: None,
            },
            payload: RegistryPackagePayload {
                snapshot_version,
                wilaya_identity_id: Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0001),
                units: vec![UnitFleetEntry {
                    unit_id: Uuid::new_v4(),
                    code: "U16A".into(),
                    name: "Alpha".into(),
                    identity_id: Uuid::new_v4(),
                }],
            },
        }
    }

    fn registry<'a>(db: &'a Database) -> SqliteImportedPackageRegistry<'a> {
        SqliteImportedPackageRegistry::new(
            make_executor(db),
            REGISTRY_PACKAGE_KIND,
            Some("w-node"),
            "admin",
            Some(1),
            Some("issuer-a"),
        )
    }

    #[test]
    fn snapshot_is_persisted_and_auditable() {
        let db = ConnectionFactory::new_for_test().unwrap();

        let outcome = execute(
            make_executor(&db),
            &registry(&db),
            ImportRegistryPackageInput {
                package: registry_package("pkg-reg-1", 1),
                imported_by: "admin".into(),
            },
        )
        .unwrap();

        assert_eq!(outcome.snapshot_version, 1);
        assert_eq!(outcome.unit_count, 1);

        let latest = RegistrySnapshotsRepository::new(make_executor(&db))
            .latest_snapshot()
            .unwrap()
            .expect("snapshot present");
        assert_eq!(latest.package_id, "pkg-reg-1");
        assert_eq!(latest.imported_by, "admin");
    }

    #[test]
    fn duplicate_package_id_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let input = ImportRegistryPackageInput {
            package: registry_package("pkg-reg-dup", 1),
            imported_by: "admin".into(),
        };
        execute(make_executor(&db), &registry(&db), input.clone()).unwrap();

        let err = execute(make_executor(&db), &registry(&db), input).unwrap_err();
        assert!(matches!(
            err,
            AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage { .. })
        ));
    }

    #[test]
    fn zero_snapshot_version_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportRegistryPackageInput {
                package: registry_package("pkg-reg-zero", 0),
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(err, AppError::Validation(_)),
            "expected validation error, got {err:?}"
        );
    }
}
