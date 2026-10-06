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
//!   `run_import_pipeline`): verify_v2_signature → importer.
//!   SEC-056D/SEC-057: no Transport Guard and no per-issuer transport ledger.
//!   Replay protection is exact `package_id` dedup — re-importing an already
//!   imported `package_id` is an idempotent skip.
//!   B1 canonical admin+user apply;
//!   B2 duplicated package_id rejected (data state preserved);
//!   B3 wrong-issuer rejected before the importer;
//!   B4 tampered payload (refreshed hash, stale signature) rejected;
//!   B5 canonical rename of a legacy admin-named unit user preserves row id;
//!   B6 disabled user rejected at the login source; reapply re-enables;
//!   B7 fleet admin disabled after export — the signed snapshot still applies;
//!   B8 foreign payload shape (SyncPackage<TrustPackagePayload>) rejected at
//!      parse — dispatch is type-driven, there is no V2 `kind` field;
//!   B9 failed import does not mark the package imported; the same-package
//!      valid retry succeeds.
//! Section C — authorization via `authorize_command`.

#[allow(dead_code)]
mod common;

use chrono::Utc;
use std::path::Path;

use tempfile::TempDir;
use uuid::Uuid;

use grpc_lib::application::authz::Action;
use grpc_lib::application::services::{
    identity_authentication_policy::IdentityAuthenticationPolicy, FinalizeWilayaProvisionResult,
    IdentityProvisioningService, IdentitySignedExportService, SettingsService,
    SyncPackageIdentityVerificationService, UnitService, UserAccountSyncService,
    BOOTSTRAP_PASSWORD,
};
use grpc_lib::application::sync::{
    ImportedPackageRegistry, PackageId, SyncPackage, SyncPackageMetadata,
    SYNC_PACKAGE_SCHEMA_VERSION,
};
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
use grpc_lib::infrastructure::identity::{AdminKeyProvider, NodeKeyStore};
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::node_identity_provider::NodeIdentityProvider as _;
use grpc_lib::infrastructure::security::SettingsNodeIdentityProvider;
use grpc_lib::infrastructure::security::{Argon2PasswordHashProvider, Ed25519SigningProvider};
use grpc_lib::infrastructure::sync::packages::canonical_json::{
    canonical_bytes_for_integrity, canonical_bytes_for_signature,
};
use grpc_lib::infrastructure::sync::packages::integrity::{PackageHasher, Sha256PackageHasher};
use grpc_lib::infrastructure::sync::packages::signing::{Ed25519PackageSigner, PackageSigner};
use grpc_lib::infrastructure::sync::{
    read_identity_access_package_from_file, PackageBuilder, SerdeJsonSyncPackageSerializer,
};
use grpc_lib::models::{
    CreateUnitRequest, IdentityAccessPayload, UserRole, WilayaNodeConfiguration,
};
use grpc_lib::repositories::executor::DbExecutor;
use grpc_lib::repositories::RepositoryProvider;

/// Issuer signing key used to seed certificates and sign packages.
const ISSUER_SECRET: [u8; 32] = [42u8; 32];
/// A DIFFERENT key: used to prove that signature validity ≠ identity binding.
const OTHER_SECRET: [u8; 32] = [7u8; 32];

const FLEET_PASSWORD: &str = "FleetPass123";
const UNIT_PASSWORD: &str = "UnitPass123";
/// Operator passphrase protecting the portable `.adminkey` (never the fleet
/// password — SEC-013 Phase 3 invariant I1).
const ADMIN_PASSPHRASE: &str = "correct horse battery staple";

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
    payload: IdentityAccessPayload,
) -> SyncPackage<IdentityAccessPayload> {
    let signer = Ed25519PackageSigner::new(secret);
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at: Utc::now(),
            source_node_id: "wilaya-a".to_string(),
            issuer_identity_id: Some(issuer_id),
            package_id: PackageId(pkg_id.to_string()),
            signature_version: Some(SIGNATURE_VERSION_ED25519),
            signing_key_id: Some(signer.public_key_hex()),
            integrity_hash: None,
            signature: None,
            export_mode: None,
            target_node_id: None,
        },
        payload,
    }
}

fn trust_package(
    pkg_id: &str,
    issuer_id: Uuid,
    secret: [u8; 32],
) -> SyncPackage<TrustPackagePayload> {
    let signer = Ed25519PackageSigner::new(secret);
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at: Utc::now(),
            source_node_id: "wilaya-a".to_string(),
            issuer_identity_id: Some(issuer_id),
            package_id: PackageId(pkg_id.to_string()),
            signature_version: Some(SIGNATURE_VERSION_ED25519),
            signing_key_id: Some(signer.public_key_hex()),
            integrity_hash: None,
            signature: None,
            export_mode: None,
            target_node_id: None,
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
/// verification runs BEFORE package_id dedup, then the identity_access importer.
/// SEC-056D/SEC-057: no Transport Guard and no per-issuer transport ledger —
/// replay protection is exact `package_id` dedup through the registry.
fn run_identity_access_pipeline(
    db: &mut Database,
    imported_by: &str,
    package: SyncPackage<IdentityAccessPayload>,
) -> Result<ImportIdentityAccessPackageOutcome, AppError> {
    let source_node_id =
        Some(package.metadata.source_node_id.trim().to_string()).filter(|s| !s.is_empty());
    let issuer_identity_id = package.metadata.issuer_identity_id.map(|u| u.to_string());

    let (outcome, _buf) = db.with_event_persistence(|ctx| {
        let executor = ctx.executor();
        let registry = SqliteImportedPackageRegistry::new(
            executor,
            "identity_access",
            source_node_id.as_deref(),
            imported_by,
            issuer_identity_id.as_deref(),
        );

        SyncPackageIdentityVerificationService::verify_v2_signature(executor, &package)?;

        let package_id = package.metadata.package_id.clone();
        if registry.has_imported(&package_id)? {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::DuplicateSyncPackage {
                    package_id: package_id.0.clone(),
                },
            ));
        }

        let password_port = Argon2PasswordHashProvider;
        let input = ImportIdentityAccessPackageInput {
            package,
            imported_by: imported_by.to_string(),
        };
        apply_identity_access_package(executor, &registry, &password_port, input)
    })?;
    Ok(outcome)
}

// ─────────────────────────────────────────────────────────────────────────────
// Section A — Producer export (service-level)
// ─────────────────────────────────────────────────────────────────────────────

fn create_unit(db: &Database, code: &str) {
    let port = Argon2PasswordHashProvider;
    UnitService::new(make_executor(db), &port)
        .create_unit(
            &CreateUnitRequest {
                code: code.to_string(),
                name: format!("Unit {}", code),
            },
            "WILAYA-1",
        )
        .expect("unit created");
}

/// SEC-029: producer-side tests must mirror the production lifecycle — the
/// `create_unit` IPC command requires `settings.wilaya_code`, which only
/// `configure_wilaya` sets. Service-level tests bypassing the command layer
/// must perform the same real configuration step instead of relying on the
/// UNCONFIGURED parse fallback that previously masked node scope.
fn configure_producer_as_wilaya(db: &Database) {
    SettingsService::new(make_executor(db))
        .configure_wilaya(&WilayaNodeConfiguration::new(
            "16".into(),
            "TestWilaya".into(),
        ))
        .expect("producer configured as WILAYA");
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
    configure_producer_as_wilaya(&node.db);
    create_unit(&node.db, "UNIT-9");
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("identity_access.sync");

    let port = Argon2PasswordHashProvider;
    let exported = UserAccountSyncService::new(node.db.executor(), &port)
        .export("UNIT-9")
        .expect("export payload");

    IdentitySignedExportService::new(&node.db, &node.node_key_store)
        .export_v2_package(
            exported.clone(),
            "wilaya-test-node",
            IDENTITY_ACCESS_PACKAGE_KIND,
            None,
            None,
            &path,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("signed V2 export");

    let pkg = read_identity_access_package_from_file(&path, &crypto)
        .expect("read back identity access package");
    let meta = &pkg.metadata;
    assert_eq!(meta.signature_version, Some(SIGNATURE_VERSION_ED25519));
    assert_eq!(meta.issuer_identity_id, Some(wilaya_cert.identity_id));
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
    // ADR-0063: the created operator's credential is the server-side
    // bootstrap value, not a caller-supplied password.
    assert!(port
        .verify_node(
            BOOTSTRAP_PASSWORD,
            "UNIT-9",
            &pkg.payload.user_password_hash
        )
        .expect("verify user"));
}

#[test]
fn export_fails_closed_when_fleet_admin_password_unset() {
    let db = ConnectionFactory::new_for_test().expect("db");
    configure_producer_as_wilaya(&db);
    create_unit(&db, "UNIT-9");

    db.executor()
        .users()
        .change_password(
            &db.executor()
                .users()
                .get_user_by_username_raw("admin", "WILAYA")
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
    configure_producer_as_wilaya(&db);
    create_unit(&db, "UNIT-9");
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
    configure_producer_as_wilaya(&db);
    create_unit(&db, "UNIT-A");
    create_unit(&db, "UNIT-B");
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
        .verify_node(BOOTSTRAP_PASSWORD, "UNIT-A", &a.user_password_hash)
        .expect("verify user A against its own node"));
    assert!(!port
        .verify_node(BOOTSTRAP_PASSWORD, "UNIT-B", &a.user_password_hash)
        .expect("user A must not verify on another node"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Section B — Consumer import pipeline
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn import_happy_path_applies_canonical_accounts() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    let package = sign_v2_package(
        identity_access_package(
            "ia-pkg-1",
            issuer_id,
            ISSUER_SECRET,
            payload("UNIT-9", true, true),
        ),
        ISSUER_SECRET,
    );

    let outcome = run_identity_access_pipeline(&mut db, "admin", package)
        .expect("identity access import succeeds");
    assert!(outcome.admin_updated);
    assert!(outcome.user_updated);
    assert_eq!(outcome.package_id, "ia-pkg-1");

    let port = Argon2PasswordHashProvider;
    let admin = db
        .executor()
        .users()
        .get_user_by_username("admin", "UNIT-9")
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
        .get_user_by_username("user", "UNIT-9")
        .expect("read user")
        .expect("canonical user present");
    assert_eq!(user.role, UserRole::User);
    assert_eq!(user.node_id, "UNIT-9");
    // The imported package carries its own hash, so the operator credential is
    // the one delivered by the package — not the creation bootstrap value.
    assert!(port
        .verify_node(UNIT_PASSWORD, "UNIT-9", &user.password_hash)
        .expect("verify user"));
}

#[test]
fn import_duplicate_package_id_is_rejected_and_state_preserved() {
    // SEC-056D/SEC-057: replay protection is exact `package_id` dedup. There is
    // no per-issuer sequence, so a NEW package id from the same issuer is
    // accepted; re-presenting the SAME package id is a rejected idempotent skip.
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    let package = sign_v2_package(
        identity_access_package(
            "ia-pkg-dedup",
            issuer_id,
            ISSUER_SECRET,
            payload("UNIT-9", true, true),
        ),
        ISSUER_SECRET,
    );
    run_identity_access_pipeline(&mut db, "admin", package.clone()).expect("first import ok");

    // Same package id again → the registry dedup rejects the re-import.
    let err = run_identity_access_pipeline(&mut db, "admin", package)
        .expect_err("duplicate package_id must be rejected");
    match err {
        AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage { package_id }) => {
            assert_eq!(package_id, "ia-pkg-dedup");
        }
        e => panic!("expected DuplicateSyncPackage, got {e:?}"),
    }

    // The rejected re-import consumed nothing: exactly one canonical admin row.
    let admins = db
        .executor()
        .users()
        .get_user_by_username("admin", "UNIT-9")
        .expect("read")
        .expect("canonical admin present");
    assert_eq!(admins.role, UserRole::Admin);
    assert_eq!(admins.node_id, "UNIT-9");
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
    // node-bound, not the canonical UNIT-9 sync row), and the package was never
    // registered as imported.
    let admin = db
        .executor()
        .users()
        .get_user_by_username("admin", "WILAYA")
        .expect("read")
        .expect("seeded admin present");
    assert_eq!(admin.node_id, "WILAYA", "importer must not have run");
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
}

#[test]
fn import_preserves_canonical_operator_row_without_rename() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    // ADR-0052: the operator row is canonically named `user` from creation;
    // import must upsert it in place — no rename path exists.
    create_unit(&db, "UNIT-9");
    let pre_apply_id = db
        .executor()
        .users()
        .get_user_by_node_id("UNIT-9")
        .expect("read")
        .expect("unit-bound user present")
        .id;

    let package = sign_v2_package(
        identity_access_package(
            "ia-pkg-canonical",
            issuer_id,
            ISSUER_SECRET,
            payload("UNIT-9", true, true),
        ),
        ISSUER_SECRET,
    );
    let outcome = run_identity_access_pipeline(&mut db, "admin", package).expect("import succeeds");
    assert!(outcome.admin_updated);
    assert!(outcome.user_updated);

    let admin = db
        .executor()
        .users()
        .get_user_by_username("admin", "UNIT-9")
        .expect("read")
        .expect("canonical admin present");
    assert_eq!(admin.role, UserRole::Admin);
    assert_eq!(admin.node_id, "UNIT-9");

    let user = db
        .executor()
        .users()
        .get_user_by_username("user", "UNIT-9")
        .expect("read")
        .expect("canonical user present");
    assert_eq!(user.role, UserRole::User);
    assert_eq!(user.node_id, "UNIT-9");
    assert_eq!(
        user.id, pre_apply_id,
        "operator row identity must be preserved without any rename"
    );
}

/// SEC-029 regression: the canonical UNIT security scope is the unit CODE.
///
/// SEC-028 proved the defect empirically: accounts bind to
/// `users.node_id = unit.code` while `Settings::get_unit_id()` returned
/// `unit_name`, so the production login scope missed every UNIT account
/// before `verify_node` could run. This test pins the full invariant chain:
/// `unit.code == users.node_id == Settings.unit_code ==
/// NodeIdentityProvider::current_node_id()`.
#[test]
fn sec029_unit_security_scope_is_unit_code_not_display_name() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    // .unit provisioning on the UNIT node: canonical operator row plus the
    // settings display-name write performed by NodePackageService during
    // import (`update_unit_node_settings` sets node_type='UNIT').
    create_unit(&db, "UNIT-S29");
    SettingsService::new(make_executor(&db))
        .update_unit_node_settings("وحدة العرض", "UNIT-S29")
        .expect("display-name settings write mirrors .unit import");

    // admin_access applies the canonical admin + user rows (real hashes).
    let package = sign_v2_package(
        identity_access_package(
            "ia-pkg-s29",
            issuer_id,
            ISSUER_SECRET,
            payload("UNIT-S29", true, true),
        ),
        ISSUER_SECRET,
    );
    run_identity_access_pipeline(&mut db, "admin", package).expect("admin_access applies");

    // A — provider scope == unit.code, never the display name.
    let port = Argon2PasswordHashProvider;
    let scope = SettingsNodeIdentityProvider::new(make_executor(&db))
        .current_node_id()
        .expect("node scope resolves");
    assert_eq!(
        scope, "UNIT-S29",
        "UNIT security scope must be the unit code"
    );
    assert_ne!(scope, "وحدة العرض", "unit_name is display metadata only");

    let policy_ok =
        IdentityAuthenticationPolicy::password_login_allowed(&db, "user", &scope).expect("policy");
    assert!(policy_ok, "operator password-login gate must be open");

    // B/C — production authentication sequence of `commands/auth.rs` under
    // the RESOLVED scope: scoped lookup then verify_node/verify_admin.
    let user = db
        .executor()
        .users()
        .get_user_by_username("user", &scope)
        .expect("read")
        .expect("operator account found under canonical scope");
    assert_eq!(user.node_id, "UNIT-S29");
    assert!(
        port.verify_node(UNIT_PASSWORD, &user.node_id, &user.password_hash)
            .expect("verify operator"),
        "B: UNIT operator login must succeed under the canonical scope"
    );

    let admin = db
        .executor()
        .users()
        .get_user_by_username("admin", &scope)
        .expect("read")
        .expect("canonical admin found under canonical scope");
    assert!(
        port.verify_admin(FLEET_PASSWORD, &admin.password_hash)
            .expect("verify admin"),
        "C: UNIT admin login must succeed under the canonical scope"
    );

    // D — credential domains stay separate.
    assert!(
        !port
            .verify_node(FLEET_PASSWORD, &user.node_id, &user.password_hash)
            .expect("cross verify"),
        "D: user + fleet password must be rejected"
    );
    assert!(
        !port
            .verify_admin(UNIT_PASSWORD, &admin.password_hash)
            .expect("cross verify"),
        "D: admin + operator password must be rejected"
    );
    assert!(
        !port
            .verify_node("WrongPass999!", &user.node_id, &user.password_hash)
            .expect("wrong-pwd verify"),
        "D: wrong operator password must be rejected"
    );
    assert!(
        db.executor()
            .users()
            .get_user_by_username("ghost", &scope)
            .expect("read")
            .is_none(),
        "D: unknown username must not resolve"
    );

    // E — WILAYA isolation: no UNIT-authenticated `user` identity at WILAYA.
    assert!(
        db.executor()
            .users()
            .get_user_by_username("user", "WILAYA")
            .expect("read")
            .is_none(),
        "E: UNIT operator must not be a WILAYA-scoped identity"
    );
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
            payload("UNIT-9", true, false),
        ),
        ISSUER_SECRET,
    );
    run_identity_access_pipeline(&mut db, "admin", disabled).expect("apply disabled");
    assert!(
        db.executor()
            .users()
            .get_user_by_username("user", "UNIT-9")
            .expect("read")
            .is_none(),
        "disabled unit user must be rejected at the login source"
    );

    // Package 2: re-enabled by the Wilaya (new package id, new snapshot).
    let enabled = sign_v2_package(
        identity_access_package(
            "ia-pkg-enabled",
            issuer_id,
            ISSUER_SECRET,
            payload("UNIT-9", true, true),
        ),
        ISSUER_SECRET,
    );
    run_identity_access_pipeline(&mut db, "admin", enabled).expect("reapply enabled");
    assert!(
        db.executor()
            .users()
            .get_user_by_username("user", "UNIT-9")
            .expect("read")
            .is_some(),
        "re-enabled unit user must authenticate at source"
    );
}

#[test]
fn fleet_admin_disabled_after_export_old_snapshot_still_applies() {
    // WILAYA producer: unit exists, fleet password set, payload exported.
    let producer = ConnectionFactory::new_for_test().expect("db");
    configure_producer_as_wilaya(&producer);
    create_unit(&producer, "UNIT-9");
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
        identity_access_package("ia-pkg-snapshot", issuer_id, ISSUER_SECRET, exported),
        ISSUER_SECRET,
    );
    let outcome = run_identity_access_pipeline(&mut consumer, "admin", package)
        .expect("signed snapshot still applies on UNIT");
    assert!(outcome.admin_updated);

    let admin = consumer
        .executor()
        .users()
        .get_user_by_username("admin", "UNIT-9")
        .expect("read")
        .expect("admin present from snapshot");
    assert_eq!(admin.node_id, "UNIT-9");
    assert!(port
        .verify_admin(FLEET_PASSWORD, &admin.password_hash)
        .expect("verify admin from snapshot"));
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
    let package = trust_package("trust-shape", issuer_id, ISSUER_SECRET);
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
fn failed_import_does_not_mark_package_imported_and_same_package_retry_succeeds() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_issuer(&db, issuer_id, ISSUER_SECRET);

    // First attempt: validly signed but with the WRONG issuer key → rejected at
    // signature verification, before the importer and before the registry is
    // told the package was imported.
    let bad = sign_v2_package(
        identity_access_package(
            "ia-pkg-retry",
            issuer_id,
            OTHER_SECRET,
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

    // Retry with the SAME package id, correctly signed → succeeds. The failed
    // attempt never marked the package imported, so the package_id dedup skips
    // nothing here.
    let good = sign_v2_package(
        identity_access_package(
            "ia-pkg-retry",
            issuer_id,
            ISSUER_SECRET,
            payload("UNIT-9", true, true),
        ),
        ISSUER_SECRET,
    );
    let outcome = run_identity_access_pipeline(&mut db, "admin", good)
        .expect("same-package valid retry succeeds");
    assert!(outcome.admin_updated);
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
fn authz_manage_account_sync_requires_wilaya_admin() {
    let action = Action::ManageAccountSync;

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
        AppError::Authorization(grpc_lib::errors::AuthorizationError::InsufficientPermissions) => {}
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

// ─────────────────────────────────────────────────────────────────────────────
// Section D — SEC-013 Phase 3: post-provisioning Admin credential + B8
// lifecycle (end-to-end, no test-only shortcuts).
// ─────────────────────────────────────────────────────────────────────────────

/// Remove the test-factory seeded admin so the node behaves like a real
/// production node (the production `ConnectionFactory::new()` never seeds).
fn remove_seeded_admin(db: &Database) {
    let admin = db
        .executor()
        .users()
        .get_user_by_username_raw("admin", "WILAYA")
        .expect("seed query")
        .expect("seeded admin present");
    db.executor()
        .users()
        .delete_user(&admin.id)
        .expect("seeded admin removed");
}

/// SEC-013 Phase 3 (§16): the full post-provisioning Admin credential + B8
/// lifecycle — fresh WILAYA → first Admin Key ceremony (identity-only admin,
/// empty hash) → B8 export fails closed → fleet admin password initialized
/// via `set_fleet_admin_password` → normal password path opens (ADR-0050) →
/// B8 export succeeds → fresh UNIT B8 import (real WILAYA anchor) → canonical
/// Admin + User rows (fleet hash verbatim) → UNIT admin password login path →
/// UNIT Admin authorization boundary (AdminOnly allowed / WilayaNode denied).
#[test]
fn sec013_phase3_post_provisioning_admin_credential_lifecycle() {
    let mut wilaya = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut wilaya);
    remove_seeded_admin(&wilaya.db);
    configure_producer_as_wilaya(&wilaya.db);

    let dir = TempDir::new().expect("temp dir");
    let adminkey_provider = AdminKeyProvider::new(dir.path().join("adminkey"));
    let port = Argon2PasswordHashProvider;

    // First Admin Key ceremony — the production provisioning path. The admin
    // row is created identity-only: password_hash = "" (never the passphrase).
    IdentityProvisioningService::new(&mut wilaya.db)
        .issue_first_admin_key(
            "admin",
            ADMIN_PASSPHRASE,
            &wilaya.node_key_store,
            &adminkey_provider,
            FIXED_NOW,
        )
        .expect("first admin key issued");

    let admin = wilaya
        .db
        .executor()
        .users()
        .get_user_by_username_raw("admin", "WILAYA")
        .expect("query")
        .expect("admin row");
    assert_eq!(admin.role, UserRole::Admin);
    assert!(
        admin.password_hash.is_empty(),
        "identity-only admin must have an empty password hash"
    );

    create_unit(&wilaya.db, "UNIT-9");

    // B8 export fails closed before the fleet password exists (ADR-0040).
    let err = UserAccountSyncService::new(wilaya.db.executor(), &port)
        .export("UNIT-9")
        .expect_err("export must fail closed before password initialization");
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
    ));

    // Post-provisioning: the WILAYA Admin (Admin Key session) initializes the
    // fleet admin password — the ONLY initializer (ADR-0040). The Admin Key
    // passphrase itself is never written into `password_hash` (I1/I2).
    UserAccountSyncService::new(wilaya.db.executor(), &port)
        .set_fleet_admin_password(FLEET_PASSWORD)
        .expect("fleet admin password set");

    let admin = wilaya
        .db
        .executor()
        .users()
        .get_user_by_username_raw("admin", "WILAYA")
        .expect("query")
        .expect("admin row");
    assert!(!admin.password_hash.is_empty());
    assert!(
        port.verify_admin(FLEET_PASSWORD, &admin.password_hash)
            .expect("verify admin"),
        "fleet hash verifies in the global admin domain"
    );
    assert!(
        IdentityAuthenticationPolicy::password_login_allowed(&wilaya.db, "admin", "WILAYA")
            .expect("policy"),
        "a usable fleet password opens the normal WILAYA password login path (ADR-0050)"
    );

    // B8 export now succeeds: signed + encrypted V2 identity_access package.
    let exported = UserAccountSyncService::new(wilaya.db.executor(), &port)
        .export("UNIT-9")
        .expect("export payload");
    let crypto = AgeFileEncryptionProvider::new();
    let path = dir.path().join("identity_access.sync");
    IdentitySignedExportService::new(&wilaya.db, &wilaya.node_key_store)
        .export_v2_package(
            exported.clone(),
            "wilaya-test-node",
            IDENTITY_ACCESS_PACKAGE_KIND,
            None,
            None,
            &path,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("signed V2 export");

    // Fresh UNIT: anchor-first trust material = the REAL WILAYA certificate
    // (the same certificate that authenticates the package signature).
    let mut unit_db = ConnectionFactory::new_for_test().expect("db");
    remove_seeded_admin(&unit_db);
    let mut anchor = wilaya_cert.clone();
    anchor.package_sequence = Some(1);
    IdentityStorePort::upsert(
        &make_executor(&unit_db).identity_store(),
        &anchor,
        FIXED_NOW,
    )
    .expect("wilaya anchor seeded");

    // B8 import applies the canonical Admin + User rows (fleet hash verbatim).
    let pkg =
        read_identity_access_package_from_file(&path, &crypto).expect("read exported package");
    let outcome =
        run_identity_access_pipeline(&mut unit_db, "admin", pkg).expect("import succeeds");
    assert!(outcome.admin_updated);
    assert!(outcome.user_updated);

    let admin = unit_db
        .executor()
        .users()
        .get_user_by_username_raw("admin", "UNIT-9")
        .expect("query")
        .expect("canonical admin present");
    assert_eq!(admin.role, UserRole::Admin);
    assert_eq!(admin.node_id, "UNIT-9");
    assert_eq!(
        admin.password_hash, exported.admin_password_hash,
        "the imported hash is the exported fleet hash verbatim"
    );
    assert!(port
        .verify_admin(FLEET_PASSWORD, &admin.password_hash)
        .expect("verify admin"));
    assert!(
        IdentityAuthenticationPolicy::password_login_allowed(&unit_db, "admin", "UNIT-9")
            .expect("policy"),
        "UNIT admin password login path is open after B8"
    );

    // UNIT Admin authorization boundary (I7 / I14): AdminOnly actions allowed,
    // WilayaNode actions denied — the local UNIT Admin session never receives
    // Wilaya authority.
    let state = unit_configured_state();
    set_session(&state, "Admin");
    let err = authorize_command(&state, Action::ManageAccountSync, None)
        .expect_err("UNIT Admin: WilayaNode action denied");
    assert!(matches!(
        err,
        AppError::Authorization(grpc_lib::errors::AuthorizationError::InsufficientPermissions)
    ));
}
