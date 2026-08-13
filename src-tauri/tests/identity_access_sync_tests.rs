//! B8 (③) integration tests: Identity & Access Synchronization.
//!
//! RFC 2026-08-04-node-identity-trust §3.4/§3.10 / ADR-0040.
//!
//! Section A — producer (WILAYA) side, service-level export:
//!   A1 signed V2 `identity_access` package round-trip;
//!   A2 fail-closed: fleet admin password unset blocks export;
//!   A3 fail-closed: disabled fleet admin blocks export;
//!   A4 fleet-identical admin hash / per-unit node-bound user hash.
//! Section B — consumer (UNIT) side, import pipeline (mirror of
//!   `run_import_pipeline`): verify_v2_signature → Transport Guard → importer
//!   → ledger advance.
//!   B1 canonical admin+user apply + ledger advance;
//!   B2 replay rejected, ledger not consumed;
//!   B3 out-of-order rejected;
//!   B4 wrong-issuer rejected before the importer;
//!   B5 tampered payload (refreshed hash, stale signature) rejected;
//!   B6 canonical rename of a legacy admin-named unit user preserves row id;
//!   B7 disabled user rejected at the login source; reapply re-enables;
//!   B8 fleet admin disabled after export — the signed snapshot still applies;
//!   B9 foreign payload shape (SyncPackage<TrustPackagePayload>) rejected at
//!      parse — dispatch is type-driven, there is no V2 `kind` field;
//!   B10 failed import does not consume the ledger; the same-sequence valid
//!      retry succeeds.
//! Section C — authorization via `authorize_command`.

#[allow(dead_code)]
mod common;

use chrono::Utc;
use std::path::Path;

use tempfile::TempDir;
use uuid::Uuid;

use grpc_lib::application::authz::Action;
use grpc_lib::application::services::{
    FinalizeWilayaProvisionResult, IdentityProvisioningService, IdentitySignedExportService,
    SyncPackageIdentityVerificationService, UnitService, UserAccountSyncService,
};
use grpc_lib::application::sync::{PackageId, SchemaVersion, SyncPackage, SyncPackageMetadata};
use grpc_lib::application::sync_integrity::transport_guard::{TransportGuard, TransportVerdict};
use grpc_lib::application::usecases::sync::import_identity_access_package::{
    execute as apply_identity_access_package, ImportIdentityAccessPackageInput,
    ImportIdentityAccessPackageOutcome, IDENTITY_ACCESS_PACKAGE_KIND,
};
use grpc_lib::application::usecases::sync::import_trust_package::TrustPackagePayload;
use grpc_lib::commands::{authorize_command, AppState};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    CredentialStatus, Ed25519CertificateSignature, IdentityCertificate, IdentitySigner,
    IdentityStorePort, SubjectType, SIGNATURE_VERSION_ED25519,
};
use grpc_lib::domain::security::PasswordHashPort;
use grpc_lib::errors::{AppError, BusinessLogicError, ValidationError};
use grpc_lib::infrastructure::db::sync_import::SqliteImportedPackageRegistry;
use grpc_lib::infrastructure::identity::NodeKeyStore;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::{Argon2PasswordHashProvider, Ed25519SigningProvider};
use grpc_lib::infrastructure::sync::packages::canonical_json::{
    canonical_bytes_for_integrity, canonical_bytes_for_signature,
};
use grpc_lib::infrastructure::sync::packages::integrity::{PackageHasher, Sha256PackageHasher};
use grpc_lib::infrastructure::sync::packages::signing::{Ed25519PackageSigner, PackageSigner};
use grpc_lib::infrastructure::sync::{
    read_identity_access_package_from_file, PackageBuilder, SerdeJsonSyncPackageSerializer,
};
use grpc_lib::models::{CreateUnitRequest, IdentityAccessPayload, UserRole};
use grpc_lib::repositories::executor::DbExecutor;
use grpc_lib::repositories::RepositoryProvider;

/// Issuer signing key used to seed certificates and sign packages.
const ISSUER_SECRET: [u8; 32] = [42u8; 32];
/// A DIFFERENT key: used to prove that signature validity ≠ identity binding.
const OTHER_SECRET: [u8; 32] = [7u8; 32];

const FLEET_PASSWORD: &str = "FleetPass123";
const UNIT_PASSWORD: &str = "UnitPass123";

/// RFC 8032 §7.1 TEST 1 secret — the matching public key IS the debug-mode
/// development Root fallback (root_public_key.rs). Never a production key.
const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
/// Fixed timestamp for deterministic persistence.
const FIXED_NOW: &str = "2026-08-04T00:00:00Z";

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

fn identity_access_package(
    pkg_id: &str,
    issuer_id: Uuid,
    secret: [u8; 32],
    sequence: u64,
    payload: IdentityAccessPayload,
) -> SyncPackage<IdentityAccessPayload> {
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
        payload,
    }
}

fn trust_package(
    pkg_id: &str,
    issuer_id: Uuid,
    secret: [u8; 32],
    sequence: u64,
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
            certificates: vec![],
            revocations: vec![],
        },
    }
}

fn payload(unit_code: &str, admin_enabled: bool, user_enabled: bool) -> IdentityAccessPayload {
    let port = Argon2PasswordHashProvider;
    IdentityAccessPayload {
        unit_code: unit_code.to_string(),
        admin_password_hash: port.hash_admin(FLEET_PASSWORD).expect("admin hash"),
        admin_enabled,
        user_password_hash: port.hash_node(UNIT_PASSWORD, unit_code).expect("user hash"),
        user_enabled,
    }
}

fn write_encrypted<T: serde::Serialize>(package: &SyncPackage<T>, secret: [u8; 32], path: &Path) {
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

/// Mirror of `run_import_pipeline`'s B8 sequence (RFC §3.4/§3.10): V2 signature
/// verification runs BEFORE the Transport Guard, then the identity_access
/// importer, then the per-issuer ledger is advanced — all in one transaction.
fn run_identity_access_pipeline(
    db: &mut Database,
    imported_by: &str,
    package: SyncPackage<IdentityAccessPayload>,
) -> Result<ImportIdentityAccessPackageOutcome, AppError> {
    let source_node_id =
        Some(package.metadata.source_node_id.trim().to_string()).filter(|s| !s.is_empty());
    let package_sequence = package.metadata.package_sequence;
    let issuer_identity_id = package.metadata.issuer_identity_id.map(|u| u.to_string());

    let (outcome, _buf) = db.with_event_persistence(|ctx| {
        let executor = ctx.executor();
        let registry = SqliteImportedPackageRegistry::new(
            executor,
            "identity_access",
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

        let password_port = Argon2PasswordHashProvider;
        let input = ImportIdentityAccessPackageInput {
            package,
            imported_by: imported_by.to_string(),
        };
        let out = apply_identity_access_package(executor, &registry, &password_port, input)?;

        if let Some(issuer) = issuer_identity_id.as_deref() {
            if let Some(sequence) = package_sequence {
                SyncPackageIdentityVerificationService::advance_issuer_sequence(
                    executor, issuer, sequence,
                )?;
            }
        }

        Ok(out)
    })?;
    Ok(outcome)
}

fn last_applied(db: &Database, issuer: &str) -> Option<u64> {
    SyncPackageIdentityVerificationService::last_applied_sequence(make_executor(db), issuer)
        .expect("read ledger")
}

// ─────────────────────────────────────────────────────────────────────────────
// Section A — Producer export (service-level)
// ─────────────────────────────────────────────────────────────────────────────

fn create_unit(db: &Database, code: &str, username: &str) {
    let port = Argon2PasswordHashProvider;
    UnitService::new(make_executor(db), &port)
        .create_unit(
            &CreateUnitRequest {
                code: code.to_string(),
                name: format!("Unit {}", code),
                username: username.to_string(),
                password: UNIT_PASSWORD.to_string(),
            },
            "WILAYA-1",
        )
        .expect("unit created");
}

fn set_fleet_password(db: &Database) {
    let port = Argon2PasswordHashProvider;
    UserAccountSyncService::new(make_executor(db), &port)
        .set_fleet_admin_password(FLEET_PASSWORD)
        .expect("fleet password set");
}

/// A fresh node directory: real DB + node key store (WILAYA provisioning).
struct Node {
    _dir: TempDir,
    db: Database,
    node_key_store: NodeKeyStore,
}

fn fresh_node() -> Node {
    let dir = TempDir::new().expect("temp dir");
    let db = ConnectionFactory::new_for_test().expect("db");
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
    Node {
        _dir: dir,
        db,
        node_key_store,
    }
}

fn root_signer() -> Ed25519SigningProvider {
    Ed25519SigningProvider::new(TEST_ROOT_SECRET)
}

/// Full offline WILAYA bootstrap (Root-issued). Returns the ACTIVE WILAYA cert.
fn bootstrap_wilaya(node: &mut Node) -> IdentityCertificate {
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    let request = provisioning
        .generate_wilaya_request(Uuid::new_v4(), &node.node_key_store)
        .expect("csr generated");
    let signature = root_signer()
        .sign_certificate(&request)
        .expect("root signed request");
    let mut signed = request;
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig wrap"));
    match provisioning
        .finalize_wilaya_provision(&signed, &node.node_key_store, FIXED_NOW)
        .expect("wilaya finalized")
    {
        FinalizeWilayaProvisionResult::Provisioned(cert) => cert,
        FinalizeWilayaProvisionResult::AlreadyProvisioned(_) => {
            panic!("first finalize must provision")
        }
    }
}

#[test]
fn export_identity_access_round_trips_as_signed_v2_package() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    create_unit(&node.db, "UNIT-9", "unit9user");
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("identity_access.sync");

    let port = Argon2PasswordHashProvider;
    let exported = UserAccountSyncService::new(node.db.executor(), &port)
        .export("UNIT-9")
        .expect("export payload");

    let sequence = IdentitySignedExportService::new(&node.db, &node.node_key_store)
        .export_v2_package(
            exported.clone(),
            "wilaya-test-node",
            IDENTITY_ACCESS_PACKAGE_KIND,
            &path,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("signed V2 export");
    assert_eq!(sequence, 1);

    let pkg = read_identity_access_package_from_file(&path, &crypto)
        .expect("read back identity access package");
    let meta = &pkg.metadata;
    assert_eq!(meta.signature_version, Some(SIGNATURE_VERSION_ED25519));
    assert_eq!(meta.issuer_identity_id, Some(wilaya_cert.identity_id));
    assert_eq!(meta.package_sequence, Some(1));
    assert_eq!(
        meta.signing_key_id.as_deref(),
        Some(hex::encode(&wilaya_cert.public_key).as_str()),
        "signing_key_id must match the ACTIVE certificate public key (R5)"
    );
    assert!(meta.integrity_hash.is_some(), "integrity hash must be set");
    assert!(meta.signature.is_some(), "Ed25519 signature must be set");
    assert_eq!(meta.source_node_id, "wilaya-test-node");

    // Payload survives the round-trip verbatim.
    assert_eq!(pkg.payload, exported);

    // Signature verifies against the issuer cert in the identity store.
    SyncPackageIdentityVerificationService::verify_v2_signature(node.db.executor(), &pkg)
        .expect("signature verifies against issuer cert");

    // Carried hashes are genuine: fleet admin (admin domain) + node-bound user.
    assert!(port
        .verify_admin(FLEET_PASSWORD, &pkg.payload.admin_password_hash)
        .expect("verify admin"));
    assert!(port
        .verify_node(UNIT_PASSWORD, "UNIT-9", &pkg.payload.user_password_hash)
        .expect("verify user"));
}

#[test]
fn export_fails_closed_when_fleet_admin_password_unset() {
    let db = ConnectionFactory::new_for_test().expect("db");
    create_unit(&db, "UNIT-9", "unit9user");

    db.executor()
        .users()
        .change_password(
            &db.executor()
                .users()
                .get_user_by_username_raw("admin")
                .unwrap()
                .unwrap()
                .id,
            "",
            "2024-01-01T00:00:00Z",
        )
        .expect("clear admin password");

    let port = Argon2PasswordHashProvider;
    let err = UserAccountSyncService::new(make_executor(&db), &port)
        .export("UNIT-9")
        .expect_err("export must fail closed");
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
    ));
}

#[test]
fn export_fails_closed_when_fleet_admin_disabled() {
    let db = ConnectionFactory::new_for_test().expect("db");
    create_unit(&db, "UNIT-9", "unit9user");
    set_fleet_password(&db);

    let port = Argon2PasswordHashProvider;
    UserAccountSyncService::new(make_executor(&db), &port)
        .set_account_status("admin", false)
        .expect("disable admin");

    let err = UserAccountSyncService::new(make_executor(&db), &port)
        .export("UNIT-9")
        .expect_err("export must fail closed");
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
    ));
}

#[test]
fn export_carries_fleet_identical_admin_and_node_bound_user_hashes() {
    let db = ConnectionFactory::new_for_test().expect("db");
    create_unit(&db, "UNIT-A", "usera");
    create_unit(&db, "UNIT-B", "userb");
    set_fleet_password(&db);

    let port = Argon2PasswordHashProvider;
    let service = UserAccountSyncService::new(make_executor(&db), &port);
    let a = service.export("UNIT-A").expect("export A");
    let b = service.export("UNIT-B").expect("export B");

    // Fleet-wide admin: identical hash in both packages, admin-domain derived.
    assert_eq!(a.admin_password_hash, b.admin_password_hash);
    assert!(port
        .verify_admin(FLEET_PASSWORD, &a.admin_password_hash)
        .expect("verify admin A"));
    assert!(port
        .verify_admin(FLEET_PASSWORD, &b.admin_password_hash)
        .expect("verify admin B"));

    // Unit-bound user: different hashes, each node-bound to its unit code.
    assert_ne!(a.user_password_hash, b.user_password_hash);
    assert!(port
        .verify_node(UNIT_PASSWORD, "UNIT-A", &a.user_password_hash)
        .expect("verify user A against its own node"));
    assert!(!port
        .verify_node(UNIT_PASSWORD, "UNIT-B", &a.user_password_hash)
        .expect("user A must not verify on another node"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Section B — Consumer import pipeline
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn import_happy_path_applies_canonical_accounts_and_advances_ledger() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    let package = sign_v2_package(
        identity_access_package(
            "ia-pkg-1",
            issuer_id,
            ISSUER_SECRET,
            1,
            payload("UNIT-9", true, true),
        ),
        ISSUER_SECRET,
    );

    let outcome = run_identity_access_pipeline(&mut db, "admin", package)
        .expect("identity access import succeeds");
    assert!(outcome.admin_updated);
    assert!(outcome.user_updated);
    assert!(!outcome.user_renamed);
    assert_eq!(outcome.package_id, "ia-pkg-1");

    let port = Argon2PasswordHashProvider;
    let admin = db
        .executor()
        .users()
        .get_user_by_username("admin")
        .expect("read admin")
        .expect("admin present");
    assert_eq!(admin.role, UserRole::Admin);
    assert_eq!(admin.node_id, "UNIT-9");
    assert!(port
        .verify_admin(FLEET_PASSWORD, &admin.password_hash)
        .expect("verify admin"));

    let user = db
        .executor()
        .users()
        .get_user_by_username("user")
        .expect("read user")
        .expect("canonical user present");
    assert_eq!(user.role, UserRole::User);
    assert_eq!(user.node_id, "UNIT-9");
    assert!(port
        .verify_node(UNIT_PASSWORD, "UNIT-9", &user.password_hash)
        .expect("verify user"));

    assert_eq!(last_applied(&db, &issuer_id.to_string()), Some(1));
}

#[test]
fn import_replay_sequence_is_rejected_and_ledger_not_consumed() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    let first = sign_v2_package(
        identity_access_package(
            "ia-pkg-a",
            issuer_id,
            ISSUER_SECRET,
            1,
            payload("UNIT-9", true, true),
        ),
        ISSUER_SECRET,
    );
    run_identity_access_pipeline(&mut db, "admin", first).expect("first import ok");
    assert_eq!(last_applied(&db, &issuer_id.to_string()), Some(1));

    // Same issuer, NEW package id, but sequence 1 again → Transport Guard Replay.
    let replay = sign_v2_package(
        identity_access_package(
            "ia-pkg-b",
            issuer_id,
            ISSUER_SECRET,
            1,
            payload("UNIT-9", true, true),
        ),
        ISSUER_SECRET,
    );
    let err = run_identity_access_pipeline(&mut db, "admin", replay)
        .expect_err("replay must be rejected");
    match err {
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. }) => {}
        e => panic!("expected OperationNotPermitted, got {e:?}"),
    }
    assert_eq!(last_applied(&db, &issuer_id.to_string()), Some(1));
}

#[test]
fn import_out_of_order_sequence_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    let package = sign_v2_package(
        identity_access_package(
            "ia-pkg-c",
            issuer_id,
            ISSUER_SECRET,
            3,
            payload("UNIT-9", true, true),
        ),
        ISSUER_SECRET,
    );
    let err = run_identity_access_pipeline(&mut db, "admin", package)
        .expect_err("out-of-order must be rejected");
    match err {
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. }) => {}
        e => panic!("expected OperationNotPermitted, got {e:?}"),
    }
    assert_eq!(last_applied(&db, &issuer_id.to_string()), None);
}

#[test]
fn import_wrong_issuer_is_rejected_before_importer() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    // Package VALIDLY signed — but with OTHER_SECRET, whose key does not match
    // the issuer certificate's key. Signature validity ≠ identity binding.
    let package = sign_v2_package(
        identity_access_package(
            "ia-pkg-wrong-issuer",
            issuer_id,
            OTHER_SECRET,
            1,
            payload("UNIT-9", true, true),
        ),
        OTHER_SECRET,
    );

    let err = run_identity_access_pipeline(&mut db, "admin", package)
        .expect_err("wrong issuer must be rejected before the importer");
    match err {
        AppError::Validation(ValidationError::InvalidFormat { field, .. }) => {
            assert_eq!(field, "signature");
        }
        e => panic!("expected signature validation error, got {e:?}"),
    }

    // The importer never ran: the seeded `admin` is untouched (still WILAYA
    // node-bound, not the canonical UNIT-9 sync row), ledger untouched.
    let admin = db
        .executor()
        .users()
        .get_user_by_username("admin")
        .expect("read")
        .expect("seeded admin present");
    assert_eq!(admin.node_id, "WILAYA", "importer must not have run");
    assert_eq!(last_applied(&db, &issuer_id.to_string()), None);
}

#[test]
fn import_tampered_payload_with_refreshed_hash_is_rejected_by_signature() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    let mut package = sign_v2_package(
        identity_access_package(
            "ia-pkg-tamper",
            issuer_id,
            ISSUER_SECRET,
            1,
            payload("UNIT-9", true, true),
        ),
        ISSUER_SECRET,
    );

    // Attacker swaps the payload and recomputes the SHA-256 integrity hash
    // (public knowledge) — but cannot forge the Ed25519 signature.
    package.payload = payload("UNIT-9", true, false);
    let value = serde_json::to_value(&package).expect("value");
    let hash = Sha256PackageHasher
        .hash(&canonical_bytes_for_integrity(&value).expect("canonical"))
        .expect("hash");
    package.metadata.integrity_hash = Some(hash);
    // Signature left stale.

    let err = run_identity_access_pipeline(&mut db, "admin", package)
        .expect_err("tampered package must be rejected");
    match err {
        AppError::Validation(ValidationError::InvalidFormat { field, .. }) => {
            assert_eq!(field, "signature");
        }
        e => panic!("expected signature validation error, got {e:?}"),
    }
    assert_eq!(last_applied(&db, &issuer_id.to_string()), None);
}

#[test]
fn import_renames_legacy_admin_named_unit_user_preserving_row_identity() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    // Pre-sync UNIT node whose local unit-bound user is named "admin"
    // (legacy `.unit` import) — it shadows the seeded admin row.
    let port = Argon2PasswordHashProvider;
    let user_hash = port.hash_node(UNIT_PASSWORD, "UNIT-9").expect("user hash");
    db.executor()
        .users()
        .upsert_user(
            "legacy-admin-id",
            "admin",
            &user_hash,
            UserRole::User,
            "UNIT-9",
            "2024-01-01T00:00:00Z",
        )
        .expect("legacy admin-named user upserted");
    let pre_apply_id = db
        .executor()
        .users()
        .get_user_by_node_id("UNIT-9")
        .expect("read")
        .expect("unit-bound user present")
        .id;

    let package = sign_v2_package(
        identity_access_package(
            "ia-pkg-rename",
            issuer_id,
            ISSUER_SECRET,
            1,
            payload("UNIT-9", true, true),
        ),
        ISSUER_SECRET,
    );
    let outcome = run_identity_access_pipeline(&mut db, "admin", package).expect("import succeeds");
    assert!(
        outcome.user_renamed,
        "legacy admin-named user must be renamed"
    );

    let admin = db
        .executor()
        .users()
        .get_user_by_username("admin")
        .expect("read")
        .expect("canonical admin present");
    assert_eq!(admin.role, UserRole::Admin);
    assert_eq!(admin.node_id, "UNIT-9");

    let user = db
        .executor()
        .users()
        .get_user_by_username("user")
        .expect("read")
        .expect("canonical user present");
    assert_eq!(user.role, UserRole::User);
    assert_eq!(user.node_id, "UNIT-9");
    assert_eq!(user.id, pre_apply_id, "rename must keep the row identity");
}

#[test]
fn import_disabled_user_rejected_at_source_and_reapply_reenables() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    // Package 1: unit user disabled.
    let disabled = sign_v2_package(
        identity_access_package(
            "ia-pkg-disabled",
            issuer_id,
            ISSUER_SECRET,
            1,
            payload("UNIT-9", true, false),
        ),
        ISSUER_SECRET,
    );
    run_identity_access_pipeline(&mut db, "admin", disabled).expect("apply disabled");
    assert!(
        db.executor()
            .users()
            .get_user_by_username("user")
            .expect("read")
            .is_none(),
        "disabled unit user must be rejected at the login source"
    );

    // Package 2: re-enabled by the Wilaya (new sequence, new snapshot).
    let enabled = sign_v2_package(
        identity_access_package(
            "ia-pkg-enabled",
            issuer_id,
            ISSUER_SECRET,
            2,
            payload("UNIT-9", true, true),
        ),
        ISSUER_SECRET,
    );
    run_identity_access_pipeline(&mut db, "admin", enabled).expect("reapply enabled");
    assert!(
        db.executor()
            .users()
            .get_user_by_username("user")
            .expect("read")
            .is_some(),
        "re-enabled unit user must authenticate at source"
    );
    assert_eq!(last_applied(&db, &issuer_id.to_string()), Some(2));
}

#[test]
fn fleet_admin_disabled_after_export_old_snapshot_still_applies() {
    // WILAYA producer: unit exists, fleet password set, payload exported.
    let producer = ConnectionFactory::new_for_test().expect("db");
    create_unit(&producer, "UNIT-9", "unit9user");
    set_fleet_password(&producer);
    let port = Argon2PasswordHashProvider;
    let exported = UserAccountSyncService::new(make_executor(&producer), &port)
        .export("UNIT-9")
        .expect("export payload");
    assert!(exported.admin_enabled);

    // The fleet admin is NOW disabled on the Wilaya (after the export).
    UserAccountSyncService::new(make_executor(&producer), &port)
        .set_account_status("admin", false)
        .expect("disable admin");

    // The previously exported snapshot is a signed V2 package; the UNIT applies
    // it as-of signing time (offline snapshot semantics) — it does NOT re-query
    // the Wilaya and therefore still carries the enabled fleet admin.
    let issuer_id = Uuid::new_v4();
    let mut consumer = ConnectionFactory::new_for_test().expect("db");
    seed_issuer(&consumer, issuer_id, ISSUER_SECRET);
    let package = sign_v2_package(
        identity_access_package("ia-pkg-snapshot", issuer_id, ISSUER_SECRET, 1, exported),
        ISSUER_SECRET,
    );
    let outcome = run_identity_access_pipeline(&mut consumer, "admin", package)
        .expect("signed snapshot still applies on UNIT");
    assert!(outcome.admin_updated);

    let admin = consumer
        .executor()
        .users()
        .get_user_by_username("admin")
        .expect("read")
        .expect("admin present from snapshot");
    assert_eq!(admin.node_id, "UNIT-9");
    assert!(port
        .verify_admin(FLEET_PASSWORD, &admin.password_hash)
        .expect("verify admin from snapshot"));
    assert_eq!(last_applied(&consumer, &issuer_id.to_string()), Some(1));
}

#[test]
fn foreign_payload_shape_is_rejected_at_parse() {
    // A `SyncPackage<TrustPackagePayload>` written by the SAME builder/signer.
    // V2 has no `kind` field — dispatch is type-driven: the typed reader
    // (`read_identity_access_package_from_file`) must reject the foreign shape
    // during JSON deserialization, before any integrity/signature/apply step.
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("trust_as_identity_access.sync");
    let issuer_id = Uuid::new_v4();
    let package = trust_package("trust-shape", issuer_id, ISSUER_SECRET, 1);
    write_encrypted(&package, ISSUER_SECRET, &path);

    let err = read_identity_access_package_from_file(&path, &AgeFileEncryptionProvider::new())
        .expect_err("foreign payload shape must be rejected at parse");
    match err {
        AppError::Validation(ValidationError::InvalidFormat { field, .. }) => {
            assert_eq!(field, "sync_package");
        }
        e => panic!("expected sync_package validation error, got {e:?}"),
    }
}

#[test]
fn failed_import_does_not_consume_ledger_and_same_sequence_retry_succeeds() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    // First attempt: validly signed but with the WRONG issuer key → rejected at
    // signature verification, before the importer and before ledger advance.
    let bad = sign_v2_package(
        identity_access_package(
            "ia-pkg-retry",
            issuer_id,
            OTHER_SECRET,
            1,
            payload("UNIT-9", true, true),
        ),
        OTHER_SECRET,
    );
    let err = run_identity_access_pipeline(&mut db, "admin", bad).expect_err("reject");
    match err {
        AppError::Validation(ValidationError::InvalidFormat { field, .. }) => {
            assert_eq!(field, "signature");
        }
        e => panic!("expected signature validation error, got {e:?}"),
    }
    assert_eq!(last_applied(&db, &issuer_id.to_string()), None);

    // Retry with the SAME package id + sequence 1, correctly signed → succeeds.
    // The failed import consumed nothing, so the transport guard accepts.
    let good = sign_v2_package(
        identity_access_package(
            "ia-pkg-retry",
            issuer_id,
            ISSUER_SECRET,
            1,
            payload("UNIT-9", true, true),
        ),
        ISSUER_SECRET,
    );
    let outcome = run_identity_access_pipeline(&mut db, "admin", good)
        .expect("same-sequence valid retry succeeds");
    assert!(outcome.admin_updated);
    assert_eq!(last_applied(&db, &issuer_id.to_string()), Some(1));
}

// ─────────────────────────────────────────────────────────────────────────────
// Section C — Authorization
// ─────────────────────────────────────────────────────────────────────────────

fn wilaya_configured_state() -> AppState {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
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

fn unit_configured_state() -> AppState {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
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

fn set_session(state: &AppState, role: &str) {
    let mut session = common::create_test_session("u1", "bob", role);
    session.user_role = UserRole::from(role.to_string());
    common::insert_test_user(state, "u1", "bob", role);
    *state.current_session.lock().expect("session mutex") = Some(session);
}

#[test]
fn authz_manage_and_export_require_wilaya_admin() {
    for action in [
        Action::ManageAccountSync,
        Action::ExportIdentityAccessPackage,
    ] {
        // WILAYA + Admin → allowed.
        let state = wilaya_configured_state();
        set_session(&state, "Admin");
        let (session, _settings) =
            authorize_command(&state, action, None).expect("wilaya admin allowed");
        assert_eq!(session.username, "bob");

        // WILAYA + User → RequiresAdmin.
        let state = wilaya_configured_state();
        set_session(&state, "User");
        let err = authorize_command(&state, action, None).expect_err("deny user");
        match err {
            AppError::Authorization(grpc_lib::errors::AuthorizationError::RequiresAdmin) => {}
            e => panic!("expected RequiresAdmin, got {e:?}"),
        }

        // UNIT + Admin → InsufficientPermissions (account authority is Wilaya-only).
        let state = unit_configured_state();
        set_session(&state, "Admin");
        let err = authorize_command(&state, action, None).expect_err("deny unit");
        match err {
            AppError::Authorization(
                grpc_lib::errors::AuthorizationError::InsufficientPermissions,
            ) => {}
            e => panic!("expected InsufficientPermissions, got {e:?}"),
        }

        // No session → SessionNotFound.
        let state = wilaya_configured_state();
        let err = authorize_command(&state, action, None).expect_err("deny no session");
        match err {
            AppError::Authentication(grpc_lib::errors::AuthenticationError::SessionNotFound) => {}
            e => panic!("expected SessionNotFound, got {e:?}"),
        }
    }
}

#[test]
fn authz_import_is_unit_only() {
    let action = Action::ImportIdentityAccessPackage;

    // UNIT + Admin → allowed (one-way Wilaya→UNIT apply, SEC-003-06-b).
    let state = unit_configured_state();
    set_session(&state, "Admin");
    let (session, _settings) =
        authorize_command(&state, action, None).expect("unit admin allowed");
    assert_eq!(session.username, "bob");

    // UNIT + User → RequiresAdmin (credential-overwrite authority is Admin-only).
    let state = unit_configured_state();
    set_session(&state, "User");
    let err = authorize_command(&state, action, None).expect_err("deny unit user");
    match err {
        AppError::Authorization(grpc_lib::errors::AuthorizationError::RequiresAdmin) => {}
        e => panic!("expected RequiresAdmin, got {e:?}"),
    }

    // WILAYA + Admin → InsufficientPermissions (no reverse path).
    let state = wilaya_configured_state();
    set_session(&state, "Admin");
    let err = authorize_command(&state, action, None).expect_err("deny wilaya");
    match err {
        AppError::Authorization(grpc_lib::errors::AuthorizationError::InsufficientPermissions) => {}
        e => panic!("expected InsufficientPermissions, got {e:?}"),
    }

    // No session → SessionNotFound.
    let state = unit_configured_state();
    let err = authorize_command(&state, action, None).expect_err("deny no session");
    match err {
        AppError::Authentication(grpc_lib::errors::AuthenticationError::SessionNotFound) => {}
        e => panic!("expected SessionNotFound, got {e:?}"),
    }
}
