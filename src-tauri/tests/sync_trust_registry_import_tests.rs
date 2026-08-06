//! B4 S5 integration tests: trust/registry package imports.
//!
//! RFC 2026-08-04-node-identity-trust §3.4 / §3.9 / §3.10.
//!
//! Exercises the full import-pipeline sequence against a real SQLite database
//! and real crypto, mirroring `run_import_pipeline` ordering:
//!   verify_v2_signature → Transport Guard → importer → ledger advance.
//! Also covers authorization (Wilaya-admin only, Unit nodes are not trust
//! distributors) and the audit mappings for the two new commands.

#[allow(dead_code)]
mod common;

use chrono::Utc;
use std::path::Path;
use uuid::Uuid;

use grpc_lib::application::authz::Action;
use grpc_lib::application::services::SyncPackageIdentityVerificationService;
use grpc_lib::application::sync::{PackageId, SchemaVersion, SyncPackage, SyncPackageMetadata};
use grpc_lib::application::sync_integrity::transport_guard::{TransportGuard, TransportVerdict};
use grpc_lib::application::usecases::sync::import_registry_package::{
    execute as apply_registry_package, ImportRegistryPackageInput, RegistryPackagePayload,
    UnitFleetEntry,
};
use grpc_lib::application::usecases::sync::import_trust_package::{
    execute as apply_trust_package, ImportTrustPackageInput, TrustPackagePayload,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::audit::{
    audit_action_to_event_type, AuditAction, AuditEventType, EntityType,
};
use grpc_lib::domain::identity::{
    CredentialStatus, Ed25519CertificateSignature, IdentityCertificate, IdentitySigner,
    IdentityStorePort, SIGNATURE_VERSION_ED25519, SubjectType,
};
use grpc_lib::errors::{AppError, BusinessLogicError, ValidationError};
use grpc_lib::infrastructure::db::sync_import::SqliteImportedPackageRegistry;
use grpc_lib::infrastructure::security::{Ed25519SigningProvider, AgeFileEncryptionProvider};
use grpc_lib::infrastructure::sync::packages::canonical_json::{
    canonical_bytes_for_integrity, canonical_bytes_for_signature,
};
use grpc_lib::infrastructure::sync::packages::integrity::{
    PackageHasher, Sha256PackageHasher,
};
use grpc_lib::infrastructure::sync::packages::signing::{Ed25519PackageSigner, PackageSigner};
use grpc_lib::infrastructure::sync::{
    read_registry_package_from_file, read_trust_package_from_file, PackageBuilder,
    SerdeJsonSyncPackageSerializer,
};
use grpc_lib::models::UserRole;
use grpc_lib::repositories::executor::DbExecutor;
use grpc_lib::repositories::RegistrySnapshotsRepository;
use grpc_lib::repositories::RepositoryProvider;
use grpc_lib::commands::authorize_command;

/// Issuer signing key used to seed certificates and sign packages.
const ISSUER_SECRET: [u8; 32] = [42u8; 32];
/// A DIFFERENT key: used to prove that signature validity ≠ identity binding.
const OTHER_SECRET: [u8; 32] = [7u8; 32];

fn make_executor(db: &Database) -> DbExecutor<'_> {
    db.executor()
}

fn seed_issuer(db: &Database, identity_id: Uuid, secret: [u8; 32]) {
    let certificate = IdentityCertificate {
        identity_id,
        subject_type: SubjectType::Wilaya,
        subject_id: identity_id,
        issuer_identity_id: None,
        credential_id: Uuid::new_v4(),
        generation: 1,
        status: CredentialStatus::Active,
        public_key: Ed25519SigningProvider::new(secret).public_key(),
        algorithm_version: SIGNATURE_VERSION_ED25519,
        not_after: None,
        package_sequence: Some(1),
        signature: None,
    };
    IdentityStorePort::upsert(
        &make_executor(db).identity_store(),
        &certificate,
        &Utc::now().to_rfc3339(),
    )
    .expect("seed issuer");
}

/// A distributable UNIT certificate issued by `issuer_id` — signed over its
/// canonical bytes with the issuer's key (ADR-0039 §5 single signing path).
fn unit_certificate(identity_id: Uuid, issuer_id: Uuid) -> IdentityCertificate {
    let mut cert = IdentityCertificate {
        identity_id,
        subject_type: SubjectType::Unit,
        subject_id: identity_id,
        issuer_identity_id: Some(issuer_id),
        credential_id: Uuid::new_v4(),
        generation: 1,
        status: CredentialStatus::Active,
        public_key: Ed25519SigningProvider::new(OTHER_SECRET).public_key(),
        algorithm_version: SIGNATURE_VERSION_ED25519,
        not_after: None,
        package_sequence: Some(1),
        signature: None,
    };
    let raw = Ed25519SigningProvider::new(ISSUER_SECRET)
        .sign_certificate(&cert)
        .expect("sign certificate");
    let sig_bytes: [u8; 64] = raw.as_slice().try_into().expect("64-byte signature");
    cert.signature = Some(Ed25519CertificateSignature::from_bytes(sig_bytes));
    cert
}

/// Build a `signature_version = 2` package the same way `PackageBuilder` does:
/// Pass A (integrity hash over canonical bytes) then Pass B (Ed25519 signature
/// over canonical bytes with the hash present).
fn sign_v2_package<T: serde::Serialize>(
    mut package: SyncPackage<T>,
    secret: [u8; 32],
) -> SyncPackage<T> {
    let signer = Ed25519PackageSigner::new(secret);
    let value = serde_json::to_value(&package).expect("value");
    let hash = Sha256PackageHasher
        .hash(&canonical_bytes_for_integrity(&value).expect("canonical integrity"))
        .expect("hash");
    package.metadata.integrity_hash = Some(hash);

    let value = serde_json::to_value(&package).expect("value");
    let signature = signer
        .sign(&canonical_bytes_for_signature(&value).expect("canonical signature"))
        .expect("sign");
    package.metadata.signature = Some(signature);
    package
}

fn trust_package(
    pkg_id: &str,
    issuer_id: Uuid,
    secret: [u8; 32],
    sequence: u64,
    certificates: Vec<IdentityCertificate>,
) -> SyncPackage<TrustPackagePayload> {
    let signer = Ed25519PackageSigner::new(secret);
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SchemaVersion::V2,
            created_at: Utc::now(),
            source_node_id: "wilaya-a".to_string(),
            package_sequence: Some(sequence),
            issuer_identity_id: Some(issuer_id),
            package_id: PackageId(pkg_id.to_string()),
            signature_version: Some(SIGNATURE_VERSION_ED25519),
            signing_key_id: Some(signer.public_key_hex()),
            integrity_hash: None,
            signature: None,
        },
        payload: TrustPackagePayload {
            certificates,
            revocations: vec![],
        },
    }
}

fn registry_package(
    pkg_id: &str,
    issuer_id: Uuid,
    sequence: u64,
    snapshot_version: u64,
) -> SyncPackage<RegistryPackagePayload> {
    let signer = Ed25519PackageSigner::new(ISSUER_SECRET);
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SchemaVersion::V2,
            created_at: Utc::now(),
            source_node_id: "wilaya-a".to_string(),
            package_sequence: Some(sequence),
            issuer_identity_id: Some(issuer_id),
            package_id: PackageId(pkg_id.to_string()),
            signature_version: Some(SIGNATURE_VERSION_ED25519),
            signing_key_id: Some(signer.public_key_hex()),
            integrity_hash: None,
            signature: None,
        },
        payload: RegistryPackagePayload {
            snapshot_version,
            wilaya_identity_id: issuer_id,
            units: vec![
                UnitFleetEntry {
                    unit_id: Uuid::new_v4(),
                    code: "U16A".into(),
                    name: "Alpha".into(),
                    identity_id: Uuid::new_v4(),
                },
                UnitFleetEntry {
                    unit_id: Uuid::new_v4(),
                    code: "U16B".into(),
                    name: "Beta".into(),
                    identity_id: Uuid::new_v4(),
                },
            ],
        },
    }
}

fn write_encrypted<T: serde::Serialize>(
    package: &SyncPackage<T>,
    secret: [u8; 32],
    path: &Path,
) {
    let signer = Ed25519PackageSigner::new(secret);
    PackageBuilder::new()
        .build_encrypted_stream_path(
            package,
            &SerdeJsonSyncPackageSerializer,
            &signer,
            &AgeFileEncryptionProvider::new(),
            path,
        )
        .expect("build encrypted package");
}

/// Mirror of `run_import_pipeline`'s B4 sequence (RFC §3.4/§3.10): V2 signature
/// verification runs BEFORE the Transport Guard, then the importer, then the
/// per-issuer ledger is advanced — all in one transaction.
fn run_pipeline<T>(
    db: &mut Database,
    kind: &str,
    imported_by: &str,
    package: SyncPackage<T>,
    apply: impl FnOnce(
        DbExecutor<'_>,
        &SqliteImportedPackageRegistry<'_>,
        SyncPackage<T>,
        &str,
    ) -> grpc_lib::errors::AppResult<()>,
) -> Result<(), AppError>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Clone,
{
    let source_node_id = Some(package.metadata.source_node_id.trim().to_string())
        .filter(|s| !s.is_empty());
    let package_sequence = package.metadata.package_sequence;
    let issuer_identity_id = package.metadata.issuer_identity_id.map(|u| u.to_string());

    let (_outcome, _buf) = db.with_event_persistence(|ctx| {
        let executor = ctx.executor();
        let registry = SqliteImportedPackageRegistry::new(
            executor,
            kind,
            source_node_id.as_deref(),
            imported_by,
            package_sequence,
            issuer_identity_id.as_deref(),
        );

        SyncPackageIdentityVerificationService::verify_v2_signature(executor, &package)?;

        if let Some(issuer) = issuer_identity_id.as_deref() {
            let sequence = package_sequence.ok_or_else(|| {
                AppError::Validation(ValidationError::InvalidFormat {
                    field: "package_sequence".into(),
                    message: "حزمة موقّعة بلا رقم تسلسل نقل".into(),
                })
            })?;
            let last_applied =
                SyncPackageIdentityVerificationService::last_applied_sequence(executor, issuer)?;
            match TransportGuard::check(issuer, sequence, last_applied) {
                TransportVerdict::Accept { .. } => {}
                TransportVerdict::OutOfOrder { expected, got, .. } => {
                    return Err(AppError::BusinessLogic(
                        BusinessLogicError::OperationNotPermitted {
                            message: format!("out-of-order expected={expected} got={got}"),
                        },
                    ));
                }
                TransportVerdict::Replay { .. } => {
                    return Err(AppError::BusinessLogic(
                        BusinessLogicError::OperationNotPermitted {
                            message: format!("replay sequence={sequence}"),
                        },
                    ));
                }
            }
        }

        apply(executor, &registry, package, imported_by)?;

        if let Some(issuer) = issuer_identity_id.as_deref() {
            if let Some(sequence) = package_sequence {
                SyncPackageIdentityVerificationService::advance_issuer_sequence(
                    executor, issuer, sequence,
                )?;
            }
        }

        Ok(())
    })?;
    Ok(())
}

fn last_applied(db: &Database, issuer: &str) -> Option<u64> {
    SyncPackageIdentityVerificationService::last_applied_sequence(make_executor(db), issuer)
        .expect("read ledger")
}

// ─────────────────────────────────────────────────────────────────────────────
// Trust package import
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn trust_import_happy_path_persists_certificates_and_advances_ledger() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    let cert_identity = Uuid::new_v4();
    let certificate = unit_certificate(cert_identity, issuer_id);

    let package = sign_v2_package(
        trust_package("trust-pkg-1", issuer_id, ISSUER_SECRET, 1, vec![certificate]),
        ISSUER_SECRET,
    );

    run_pipeline(
        &mut db,
        "trust",
        "admin",
        package,
        |executor, registry, package, imported_by| {
            let input = ImportTrustPackageInput {
                package,
                imported_by: imported_by.to_string(),
            };
            let outcome = apply_trust_package(executor, registry, input)?;
            assert_eq!(outcome.certificate_count, 1);
            assert_eq!(outcome.revocation_count, 0);
            assert_eq!(outcome.package_id, "trust-pkg-1");
            Ok(())
        },
    )
    .expect("trust import succeeds");

    let stored = make_executor(&db)
        .identity_store()
        .get_by_identity_id(&cert_identity)
        .expect("read cert")
        .expect("certificate persisted");
    assert_eq!(stored.identity_id, cert_identity);
    assert_eq!(stored.issuer_identity_id, Some(issuer_id));

    assert_eq!(last_applied(&db, &issuer_id.to_string()), Some(1));
}

#[test]
fn trust_import_round_trips_through_encrypted_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("trust.sync");
    let issuer_id = Uuid::new_v4();

    let package = trust_package(
        "trust-file-1",
        issuer_id,
        ISSUER_SECRET,
        1,
        vec![],
    );
    write_encrypted(&package, ISSUER_SECRET, &path);

    let read_back = read_trust_package_from_file(&path, &AgeFileEncryptionProvider::new())
        .expect("read + integrity verify");
    assert_eq!(read_back.metadata.package_id.0, "trust-file-1");
    assert_eq!(
        read_back.metadata.signature_version,
        Some(SIGNATURE_VERSION_ED25519)
    );
}

#[test]
fn trust_replay_sequence_is_rejected_and_ledger_not_consumed() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    let first = sign_v2_package(
        trust_package("trust-pkg-a", issuer_id, ISSUER_SECRET, 1, vec![]),
        ISSUER_SECRET,
    );
    run_pipeline(&mut db, "trust", "admin", first, |executor, registry, package, _| {
        let input = ImportTrustPackageInput {
            package,
            imported_by: "admin".into(),
        };
        let _ = apply_trust_package(executor, registry, input)?;
        Ok(())
    })
    .expect("first import ok");
    assert_eq!(last_applied(&db, &issuer_id.to_string()), Some(1));

    // Same issuer, NEW package id, but sequence 1 again → Transport Guard Replay.
    let replay = sign_v2_package(
        trust_package("trust-pkg-b", issuer_id, ISSUER_SECRET, 1, vec![]),
        ISSUER_SECRET,
    );
    let err = run_pipeline(&mut db, "trust", "admin", replay, |executor, registry, package, _| {
        let input = ImportTrustPackageInput {
            package,
            imported_by: "admin".into(),
        };
        let _ = apply_trust_package(executor, registry, input)?;
        Ok(())
    })
    .expect_err("replay must be rejected");
    match err {
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. }) => {}
        e => panic!("expected OperationNotPermitted, got {e:?}"),
    }
    // Sequence must NOT have advanced past 1.
    assert_eq!(last_applied(&db, &issuer_id.to_string()), Some(1));
}

#[test]
fn trust_out_of_order_sequence_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    // Jump straight to sequence 3 without 1 → OutOfOrder.
    let package = sign_v2_package(
        trust_package("trust-pkg-c", issuer_id, ISSUER_SECRET, 3, vec![]),
        ISSUER_SECRET,
    );
    let err = run_pipeline(&mut db, "trust", "admin", package, |executor, registry, package, _| {
        let input = ImportTrustPackageInput {
            package,
            imported_by: "admin".into(),
        };
        let _ = apply_trust_package(executor, registry, input)?;
        Ok(())
    })
    .expect_err("out-of-order must be rejected");
    match err {
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. }) => {}
        e => panic!("expected OperationNotPermitted, got {e:?}"),
    }
    assert_eq!(last_applied(&db, &issuer_id.to_string()), None);
}

#[test]
fn trust_wrong_issuer_certificate_is_rejected_before_importer() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    // The Identity Store certificate for `issuer_id` holds ISSUER_SECRET's key.
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    let cert_identity = Uuid::new_v4();
    let certificate = unit_certificate(cert_identity, issuer_id);

    // Package is VALIDLY signed — but with OTHER_SECRET, whose key does not
    // match the issuer's certificate key. Signature validity ≠ identity binding.
    let package = sign_v2_package(
        trust_package(
            "trust-pkg-wrong-issuer",
            issuer_id,
            OTHER_SECRET,
            1,
            vec![certificate],
        ),
        OTHER_SECRET,
    );

    let err = run_pipeline(&mut db, "trust", "admin", package, |executor, registry, package, _| {
        let input = ImportTrustPackageInput {
            package,
            imported_by: "admin".into(),
        };
        let _ = apply_trust_package(executor, registry, input)?;
        Ok(())
    })
    .expect_err("wrong issuer must be rejected before the importer");

    match err {
        AppError::Validation(ValidationError::InvalidFormat { field, .. }) => {
            assert_eq!(field, "signature");
        }
        e => panic!("expected signature validation error, got {e:?}"),
    }

    // The importer never ran: no certificate persisted, ledger untouched.
    assert!(make_executor(&db)
        .identity_store()
        .get_by_identity_id(&cert_identity)
        .expect("read")
        .is_none());
    assert_eq!(last_applied(&db, &issuer_id.to_string()), None);
}

#[test]
fn trust_unknown_issuer_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let unknown_issuer = Uuid::new_v4();
    // No certificate seeded for `unknown_issuer`.

    let package = sign_v2_package(
        trust_package("trust-pkg-unknown", unknown_issuer, ISSUER_SECRET, 1, vec![]),
        ISSUER_SECRET,
    );
    let err = run_pipeline(&mut db, "trust", "admin", package, |executor, registry, package, _| {
        let input = ImportTrustPackageInput {
            package,
            imported_by: "admin".into(),
        };
        let _ = apply_trust_package(executor, registry, input)?;
        Ok(())
    })
    .expect_err("unknown issuer must be rejected");
    match err {
        AppError::Validation(ValidationError::InvalidFormat { field, .. }) => {
            assert_eq!(field, "issuer_identity_id");
        }
        e => panic!("expected issuer validation error, got {e:?}"),
    }
}

#[test]
fn trust_tampered_payload_with_refreshed_hash_is_rejected_by_signature() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    let mut package = sign_v2_package(
        trust_package("trust-pkg-tamper", issuer_id, ISSUER_SECRET, 1, vec![]),
        ISSUER_SECRET,
    );

    // Attacker swaps the payload and recomputes the SHA-256 integrity hash
    // (public knowledge) — but cannot forge the Ed25519 signature.
    let forged_cert_identity = Uuid::new_v4();
    package.payload = TrustPackagePayload {
        certificates: vec![IdentityCertificate {
            identity_id: forged_cert_identity,
            subject_type: SubjectType::Unit,
            subject_id: forged_cert_identity,
            issuer_identity_id: Some(issuer_id),
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: Ed25519SigningProvider::new(OTHER_SECRET).public_key(),
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: Some(1),
            signature: None,
        }],
        revocations: vec![],
    };
    let value = serde_json::to_value(&package).expect("value");
    let hash = Sha256PackageHasher
        .hash(&canonical_bytes_for_integrity(&value).expect("canonical"))
        .expect("hash");
    package.metadata.integrity_hash = Some(hash);
    // Signature left stale.

    let err = run_pipeline(&mut db, "trust", "admin", package, |executor, registry, package, _| {
        let input = ImportTrustPackageInput {
            package,
            imported_by: "admin".into(),
        };
        let _ = apply_trust_package(executor, registry, input)?;
        Ok(())
    })
    .expect_err("tampered package must be rejected");

    match err {
        AppError::Validation(ValidationError::InvalidFormat { field, .. }) => {
            assert_eq!(field, "signature");
        }
        e => panic!("expected signature validation error, got {e:?}"),
    }
    assert!(make_executor(&db)
        .identity_store()
        .get_by_identity_id(&forged_cert_identity)
        .expect("read")
        .is_none());
    assert_eq!(last_applied(&db, &issuer_id.to_string()), None);
}

// ─────────────────────────────────────────────────────────────────────────────
// Registry package import
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn registry_import_happy_path_persists_snapshot_verbatim_and_advances_ledger() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    let package = sign_v2_package(
        registry_package("registry-pkg-1", issuer_id, 1, 1),
        ISSUER_SECRET,
    );
    run_pipeline(
        &mut db,
        "registry",
        "admin",
        package,
        |executor, registry, package, imported_by| {
            let input = ImportRegistryPackageInput {
                package,
                imported_by: imported_by.to_string(),
            };
            let outcome = apply_registry_package(executor, registry, input)?;
            assert_eq!(outcome.snapshot_version, 1);
            assert_eq!(outcome.unit_count, 2);
            assert_eq!(outcome.package_id, "registry-pkg-1");
            Ok(())
        },
    )
    .expect("registry import succeeds");

    let latest = RegistrySnapshotsRepository::new(make_executor(&db))
        .latest_snapshot()
        .expect("read")
        .expect("snapshot persisted");
    assert_eq!(latest.package_id, "registry-pkg-1");
    assert_eq!(latest.snapshot_version, 1);
    let payload: RegistryPackagePayload = serde_json::from_str(&latest.payload_json).expect("json");
    assert_eq!(payload.units.len(), 2);

    assert_eq!(last_applied(&db, &issuer_id.to_string()), Some(1));
}

#[test]
fn registry_import_round_trips_through_encrypted_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("registry.sync");
    let issuer_id = Uuid::new_v4();

    let package = registry_package("registry-file-1", issuer_id, 1, 2);
    write_encrypted(&package, ISSUER_SECRET, &path);

    let read_back =
        read_registry_package_from_file(&path, &AgeFileEncryptionProvider::new())
            .expect("read + integrity verify");
    assert_eq!(read_back.metadata.package_id.0, "registry-file-1");
    assert_eq!(read_back.payload.snapshot_version, 2);
}

#[test]
fn registry_duplicate_package_id_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    let first = sign_v2_package(
        registry_package("registry-pkg-dup", issuer_id, 1, 1),
        ISSUER_SECRET,
    );
    run_pipeline(&mut db, "registry", "admin", first, |executor, registry, package, _| {
        let input = ImportRegistryPackageInput {
            package,
            imported_by: "admin".into(),
        };
        let _ = apply_registry_package(executor, registry, input)?;
        Ok(())
    })
    .expect("first import ok");

    // Same package id again (sequence 2 would be fine transport-wise, but the
    // package registry rejects the duplicate package id).
    let dup = sign_v2_package(
        registry_package("registry-pkg-dup", issuer_id, 2, 2),
        ISSUER_SECRET,
    );
    let err = run_pipeline(&mut db, "registry", "admin", dup, |executor, registry, package, _| {
        let input = ImportRegistryPackageInput {
            package,
            imported_by: "admin".into(),
        };
        let _ = apply_registry_package(executor, registry, input)?;
        Ok(())
    })
    .expect_err("duplicate package must be rejected");
    match err {
        AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage { package_id }) => {
            assert_eq!(package_id, "registry-pkg-dup");
        }
        e => panic!("expected DuplicateSyncPackage, got {e:?}"),
    }
    assert_eq!(last_applied(&db, &issuer_id.to_string()), Some(1));
}

// ─────────────────────────────────────────────────────────────────────────────
// Authorization: Wilaya-admin only (Unit nodes are not trust distributors)
// ─────────────────────────────────────────────────────────────────────────────

fn wilaya_configured_state() -> grpc_lib::commands::AppState {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = grpc_lib::commands::AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'WILAYA', configured = 1, wilaya_code = '16', wilaya_name = 'TestWilaya' WHERE id = 1",
                [],
            )
            .expect("settings");
    }
    state
}

fn unit_configured_state() -> grpc_lib::commands::AppState {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = grpc_lib::commands::AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'UNIT', configured = 1, unit_name = 'unit-scope-id', wilaya_code = '16', wilaya_name = NULL WHERE id = 1",
                [],
            )
            .expect("settings");
    }
    state
}

fn set_session(state: &grpc_lib::commands::AppState, role: &str) {
    let mut session = common::create_test_session("u1", "bob", role);
    session.user_role = UserRole::from(role.to_string());
    *state.current_session.lock().expect("session mutex") = Some(session);
}

#[test]
fn authz_wilaya_admin_allowed_for_trust_and_registry_imports() {
    for action in [Action::ImportTrustPackage, Action::ImportRegistryPackage] {
        let state = wilaya_configured_state();
        set_session(&state, "Admin");
        let (session, _settings) =
            authorize_command(&state, action, None).expect("wilaya admin allowed");
        assert_eq!(session.username, "bob");
    }
}

#[test]
fn authz_non_admin_denied_on_wilaya() {
    for action in [Action::ImportTrustPackage, Action::ImportRegistryPackage] {
        let state = wilaya_configured_state();
        set_session(&state, "User");
        let err = authorize_command(&state, action, None).expect_err("deny");
        match err {
            AppError::Authorization(grpc_lib::errors::AuthorizationError::RequiresAdmin) => {}
            e => panic!("expected RequiresAdmin, got {e:?}"),
        }
    }
}

#[test]
fn authz_unit_node_admin_denied_trust_import() {
    // Unit nodes are NOT trust distributors — part of the trust chain model
    // (Root → WILAYA → UNIT), not a transient policy.
    for action in [Action::ImportTrustPackage, Action::ImportRegistryPackage] {
        let state = unit_configured_state();
        set_session(&state, "Admin");
        let err = authorize_command(&state, action, None).expect_err("deny");
        match err {
            AppError::Authorization(
                grpc_lib::errors::AuthorizationError::RequiresWilayaNode,
            ) => {}
            e => panic!("expected RequiresWilayaNode, got {e:?}"),
        }
    }
}

#[test]
fn authz_missing_session_denied() {
    let state = wilaya_configured_state();
    let err = authorize_command(&state, Action::ImportTrustPackage, None).expect_err("deny");
    match err {
        AppError::Authentication(grpc_lib::errors::AuthenticationError::SessionNotFound) => {}
        e => panic!("expected SessionNotFound, got {e:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Audit mappings for the two new actions
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn audit_trust_and_registry_actions_round_trip() {
    for (action, name) in [
        (AuditAction::ImportTrustPackage, "ImportTrustPackage"),
        (AuditAction::ImportRegistryPackage, "ImportRegistryPackage"),
    ] {
        assert_eq!(action.as_str(), name);
        assert_eq!(AuditAction::parse(name), Some(action.clone()));
        assert_eq!(AuditAction::parse(&name.to_lowercase()), None);
        assert!(!action.display_arabic().is_empty());
    }
}

#[test]
fn audit_trust_and_registry_map_to_system_entity_and_sync_event() {
    for action in [AuditAction::ImportTrustPackage, AuditAction::ImportRegistryPackage] {
        assert_eq!(action.default_entity_type(), EntityType::System);
        assert_eq!(audit_action_to_event_type(&action), AuditEventType::SyncEvent);
    }
}
