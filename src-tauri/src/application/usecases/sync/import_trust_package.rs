//! Apply decrypted Trust Package (certificates + revocations) to the Identity Store.
//!
//! RFC 2026-08-04 §3.9 (B4): kind = `trust`. The Trust Package is the ONLY
//! channel for trust distribution (no side channel, §3.4.4). Certificates are
//! gated by the Credential Guard `(credential_id, generation)` (§3.4.2); the
//! Transport Guard lives in `run_import_pipeline` (§3.4.1).

use serde::{Deserialize, Serialize};

use crate::application::sync::ImportedPackageRegistry;
use crate::application::sync::SyncPackage;
use crate::application::sync_integrity::credential_guard::{CredentialGuard, CredentialVerdict};
use crate::domain::identity::{CredentialStatus, IdentityCertificate, IdentityStorePort};
use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
use crate::repositories::{DbExecutor, IdentityStoreRepository};

pub const TRUST_PACKAGE_KIND: &str = "trust";

/// A certificate revocation directive targeting a persisted identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateRevocation {
    pub identity_id: uuid::Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Payload of a Trust Package: certificate distributions + revocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustPackagePayload {
    pub certificates: Vec<IdentityCertificate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revocations: Vec<CertificateRevocation>,
}

#[derive(Debug, Clone)]
pub struct ImportTrustPackageInput {
    pub package: SyncPackage<TrustPackagePayload>,
    pub imported_by: String,
}

#[derive(Debug, Clone)]
pub struct ImportTrustPackageOutcome {
    pub certificate_count: usize,
    pub revocation_count: usize,
    pub package_id: String,
}

pub fn execute(
    executor: DbExecutor<'_>,
    registry: &impl ImportedPackageRegistry,
    input: ImportTrustPackageInput,
) -> AppResult<ImportTrustPackageOutcome> {
    if input.package.metadata.source_node_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "source_node_id".into(),
            message: "حزمة ثقة بلا مصدر".into(),
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

    let identity_store = IdentityStoreRepository::new(executor);
    let now = chrono::Utc::now().to_rfc3339();

    // Certificates first, then revocations: a revocation may target a
    // certificate introduced in the same package (deterministic order, P1).
    for certificate in &input.package.payload.certificates {
        // New certificates MUST be signed (ADR-0039 §6); legacy migration
        // records carry NULL signatures and are not distributable.
        certificate.require_signed()?;

        let stored = identity_store.max_generation_for_credential(&certificate.credential_id)?;
        match CredentialGuard::check(
            &certificate.credential_id.to_string(),
            certificate.generation,
            stored,
        ) {
            CredentialVerdict::Accept { .. } => {
                identity_store.upsert(certificate, &now)?;
            }
            CredentialVerdict::Replay { .. } => {
                // Already-known state: idempotent no-op.
            }
            CredentialVerdict::Rollback {
                credential_id,
                stored,
                incoming,
            } => {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::OperationNotPermitted {
                        message: format!(
                            "Credential rollback rejected: {credential_id} stored generation {stored} >= incoming {incoming}"
                        ),
                    },
                ));
            }
            CredentialVerdict::RejectZero { credential_id } => {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::OperationNotPermitted {
                        message: format!("Impossible credential generation 0: {credential_id}"),
                    },
                ));
            }
        }
    }

    let mut revocation_count = 0usize;
    for revocation in &input.package.payload.revocations {
        let existing = identity_store.get_by_identity_id(&revocation.identity_id)?.ok_or_else(
            || {
                AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
                    resource: "identity".into(),
                    id: revocation.identity_id.to_string(),
                })
            },
        )?;
        let mut revoked = existing;
        revoked.status = CredentialStatus::Revoked;
        identity_store.upsert(&revoked, &now)?;
        revocation_count += 1;
    }

    let certificate_count = input.package.payload.certificates.len();
    registry.mark_imported(&package_id)?;

    Ok(ImportTrustPackageOutcome {
        certificate_count,
        revocation_count,
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
    use crate::domain::identity::test_signature;
    use crate::domain::identity::{CredentialStatus, SubjectType};
    use crate::errors::{AppError, BusinessLogicError};
    use crate::infrastructure::db::sync_import::SqliteImportedPackageRegistry;
    use crate::repositories::{DbExecutor, IdentityStoreRepository};
    use uuid::Uuid;

    fn make_executor(db: &Database) -> DbExecutor<'_> {
        db.executor()
    }

    fn sample_cert(
        identity_id: Uuid,
        credential_id: Uuid,
        generation: u64,
        subject_id: Uuid,
    ) -> IdentityCertificate {
        IdentityCertificate {
            identity_id,
            subject_type: SubjectType::Unit,
            subject_id,
            issuer_identity_id: Some(Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0001)),
            credential_id,
            generation,
            status: CredentialStatus::Active,
            public_key: vec![7u8; 32],
            algorithm_version: 2,
            not_after: None,
            package_sequence: Some(1),
            signature: Some(test_signature([3u8; 64])),
        }
    }

    fn trust_package(
        pkg_id: &str,
        certificates: Vec<IdentityCertificate>,
        revocations: Vec<CertificateRevocation>,
    ) -> SyncPackage<TrustPackagePayload> {
        SyncPackage {
            metadata: SyncPackageMetadata {
                schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
                created_at: chrono::Utc::now(),
                source_node_id: "w-node".to_string(),
                package_sequence: Some(1),
                issuer_identity_id: Some(Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0001)),
                package_id: PackageId(pkg_id.to_string()),
                signature_version: Some(2),
                signing_key_id: None,
                integrity_hash: None,
                signature: None,
            },
            payload: TrustPackagePayload {
                certificates,
                revocations,
            },
        }
    }

    fn registry<'a>(db: &'a Database) -> SqliteImportedPackageRegistry<'a> {
        SqliteImportedPackageRegistry::new(
            make_executor(db),
            TRUST_PACKAGE_KIND,
            Some("w-node"),
            "admin",
            Some(1),
            Some("issuer-a"),
        )
    }

    #[test]
    fn accepts_new_credential_and_persists() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let identity_id = Uuid::new_v4();
        let credential_id = Uuid::new_v4();
        let subject_id = Uuid::new_v4();

        let package = trust_package(
            "pkg-trust-new",
            vec![sample_cert(identity_id, credential_id, 1, subject_id)],
            vec![],
        );

        let outcome = execute(make_executor(&db), &registry(&db), ImportTrustPackageInput {
            package,
            imported_by: "admin".into(),
        })
        .unwrap();

        assert_eq!(outcome.certificate_count, 1);
        assert_eq!(outcome.revocation_count, 0);

        let store = IdentityStoreRepository::new(make_executor(&db));
        let stored = store
            .get_active_by_subject(SubjectType::Unit, &subject_id)
            .unwrap()
            .expect("certificate persisted");
        assert_eq!(stored.generation, 1);
        assert_eq!(stored.status, CredentialStatus::Active);
    }

    #[test]
    fn rotation_with_higher_generation_is_accepted() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let identity_id = Uuid::new_v4();
        let credential_id = Uuid::new_v4();
        let subject_id = Uuid::new_v4();

        let gen1 = trust_package(
            "pkg-trust-rot-1",
            vec![sample_cert(identity_id, credential_id, 1, subject_id)],
            vec![],
        );
        execute(make_executor(&db), &registry(&db), ImportTrustPackageInput {
            package: gen1,
            imported_by: "admin".into(),
        })
        .unwrap();

        let gen2 = trust_package(
            "pkg-trust-rot-2",
            vec![sample_cert(identity_id, credential_id, 2, subject_id)],
            vec![],
        );
        execute(make_executor(&db), &registry(&db), ImportTrustPackageInput {
            package: gen2,
            imported_by: "admin".into(),
        })
        .unwrap();

        let store = IdentityStoreRepository::new(make_executor(&db));
        let stored = store
            .get_active_by_subject(SubjectType::Unit, &subject_id)
            .unwrap()
            .expect("certificate persisted");
        assert_eq!(stored.generation, 2);
        assert_eq!(stored.public_key, vec![7u8; 32]);
    }

    #[test]
    fn rollback_is_rejected_fail_closed() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let identity_id = Uuid::new_v4();
        let credential_id = Uuid::new_v4();
        let subject_id = Uuid::new_v4();

        let gen2 = trust_package(
            "pkg-trust-rb-2",
            vec![sample_cert(identity_id, credential_id, 2, subject_id)],
            vec![],
        );
        execute(make_executor(&db), &registry(&db), ImportTrustPackageInput {
            package: gen2,
            imported_by: "admin".into(),
        })
        .unwrap();

        // RFC §3.4.2: (X,2) → (X,1) is rejected regardless of transport order.
        let gen1 = trust_package(
            "pkg-trust-rb-1",
            vec![sample_cert(identity_id, credential_id, 1, subject_id)],
            vec![],
        );
        let err = execute(make_executor(&db), &registry(&db), ImportTrustPackageInput {
            package: gen1,
            imported_by: "admin".into(),
        })
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected rollback rejection, got {err:?}"
        );
    }

    #[test]
    fn replay_of_same_generation_is_idempotent_noop() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let identity_id = Uuid::new_v4();
        let credential_id = Uuid::new_v4();
        let subject_id = Uuid::new_v4();

        let first = trust_package(
            "pkg-trust-replay-1",
            vec![sample_cert(identity_id, credential_id, 1, subject_id)],
            vec![],
        );
        execute(make_executor(&db), &registry(&db), ImportTrustPackageInput {
            package: first,
            imported_by: "admin".into(),
        })
        .unwrap();

        let replayed = trust_package(
            "pkg-trust-replay-2",
            vec![sample_cert(identity_id, credential_id, 1, subject_id)],
            vec![],
        );
        let outcome = execute(make_executor(&db), &registry(&db), ImportTrustPackageInput {
            package: replayed,
            imported_by: "admin".into(),
        })
        .unwrap();
        assert_eq!(outcome.certificate_count, 1);
    }

    #[test]
    fn revocation_applies_revoked_status() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let identity_id = Uuid::new_v4();
        let credential_id = Uuid::new_v4();
        let subject_id = Uuid::new_v4();

        let issue = trust_package(
            "pkg-trust-rev-issue",
            vec![sample_cert(identity_id, credential_id, 1, subject_id)],
            vec![],
        );
        execute(make_executor(&db), &registry(&db), ImportTrustPackageInput {
            package: issue,
            imported_by: "admin".into(),
        })
        .unwrap();

        let revoke = trust_package(
            "pkg-trust-rev-apply",
            vec![],
            vec![CertificateRevocation {
                identity_id,
                reason: Some("loss of custody".into()),
            }],
        );
        let outcome = execute(make_executor(&db), &registry(&db), ImportTrustPackageInput {
            package: revoke,
            imported_by: "admin".into(),
        })
        .unwrap();
        assert_eq!(outcome.revocation_count, 1);

        let store = IdentityStoreRepository::new(make_executor(&db));
        let stored = store
            .get_by_identity_id(&identity_id)
            .unwrap()
            .expect("identity present");
        assert_eq!(stored.status, CredentialStatus::Revoked);
    }

    #[test]
    fn revocation_of_unknown_identity_rejects_package() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let package = trust_package(
            "pkg-trust-rev-unknown",
            vec![],
            vec![CertificateRevocation {
                identity_id: Uuid::new_v4(),
                reason: None,
            }],
        );
        let err = execute(make_executor(&db), &registry(&db), ImportTrustPackageInput {
            package,
            imported_by: "admin".into(),
        })
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::ResourceNotFound { .. })
            ),
            "expected resource not found, got {err:?}"
        );
    }

    #[test]
    fn unsigned_certificate_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let mut cert = sample_cert(Uuid::new_v4(), Uuid::new_v4(), 1, Uuid::new_v4());
        cert.signature = None;

        let package = trust_package("pkg-trust-unsigned", vec![cert], vec![]);
        let err = execute(make_executor(&db), &registry(&db), ImportTrustPackageInput {
            package,
            imported_by: "admin".into(),
        })
        .unwrap_err();
        assert!(
            matches!(err, AppError::Internal(_)),
            "expected unsigned-certificate rejection, got {err:?}"
        );
    }

    #[test]
    fn duplicate_package_id_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let package = trust_package(
            "pkg-trust-dup",
            vec![sample_cert(Uuid::new_v4(), Uuid::new_v4(), 1, Uuid::new_v4())],
            vec![],
        );
        let input = ImportTrustPackageInput {
            package: package.clone(),
            imported_by: "admin".into(),
        };
        execute(make_executor(&db), &registry(&db), input.clone()).unwrap();

        let err = execute(make_executor(&db), &registry(&db), input).unwrap_err();
        assert!(matches!(
            err,
            AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage { .. })
        ));
    }
}
