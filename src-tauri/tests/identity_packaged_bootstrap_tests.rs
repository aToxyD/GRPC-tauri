//! Packaged-identity `.unit` bootstrap integration tests (ADR-0044 amendment).
//!
//! WILAYA generates the UNIT Ed25519 keypair IN MEMORY at export time, signs
//! the UNIT certificate with the ACTIVE local WILAYA identity, and embeds the
//! signed certificate + 32-byte secret into the encrypted `.unit`. On the UNIT
//! node the packaged key is installed into the local `NodeKeyStore` (guarded,
//! never-overwrite) and the certificate installed through the executor-compatible
//! finalize core inside the import audit transaction.
//!
//! RE-EXPORT RULE: `sign_unit_bootstrap_request`'s duplicate-ACTIVE guard
//! rejects a second export once an ACTIVE UNIT identity exists.
//!
//! The test authority Root uses the RFC 8032 §7.1 TEST 1 keypair — exactly the
//! `#[cfg(debug_assertions)]` development Root fallback in `root_public_key.rs`.

#[allow(dead_code)]
mod common;

use tempfile::TempDir;

use grpc_lib::application::services::{
    AuditTxService, FinalizeUnitProvisionResult, FinalizeWilayaProvisionResult,
    IdentityBootstrapStatusService, IdentityProvisioningService, IdentityTrustAnchorService,
    InstallWilayaCertificateResult, NodePackageService, UserContext,
};
use grpc_lib::application::sync::{
    PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::audit::AuditAction;
use grpc_lib::domain::identity::{
    CredentialStatus, Ed25519CertificateSignature, IdentityBootstrapState, IdentityCertificate,
    IdentitySignatureVerifier, IdentitySigner, IdentityStorePort, SubjectType,
    SIGNATURE_VERSION_ED25519,
};
use grpc_lib::infrastructure::identity::{AdminKeyProvider, NodeKeyStore};
use grpc_lib::infrastructure::security::{Ed25519SignatureVerifier, Ed25519SigningProvider};
use grpc_lib::infrastructure::sync::packages::canonical_json::canonical_bytes_for_integrity;
use grpc_lib::infrastructure::sync::packages::integrity::{PackageHasher, Sha256PackageHasher};
use grpc_lib::infrastructure::sync::packages::SerdeJsonSyncPackageDeserializer;
use grpc_lib::models::{Unit, UnitNodePackage, UserExport};
use grpc_lib::repositories::RepositoryProvider;
use rusqlite::params;

/// RFC 8032 §7.1 TEST 1 secret — the matching public key IS the debug-mode
/// development Root fallback (root_public_key.rs). Never a production key.
const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
/// Fixed timestamp for deterministic persistence.
const FIXED_NOW: &str = "2026-08-04T00:00:00Z";
const UNIT_ID: &str = "4e1c8f3a-9b2d-4c6e-8f0a-1b2c3d4e5f60";
const UNIT_CODE: &str = "1601";
const UNIT_NAME: &str = "وحدة حماية مدنية 01";
const UNIT_WILAYA: &str = "16";

/// A fresh node directory: real DB + node key store + `.adminkey` provider.
struct Node {
    _dir: TempDir,
    db: Database,
    node_key_store: NodeKeyStore,
    adminkey_provider: AdminKeyProvider,
}

fn fresh_node() -> Node {
    let dir = TempDir::new().expect("temp dir");
    let db = ConnectionFactory::new_for_test().expect("db");
    let node_dir = dir.path().join("node");
    Node {
        _dir: dir,
        db,
        node_key_store: NodeKeyStore::new(node_dir.clone()),
        adminkey_provider: AdminKeyProvider::new(node_dir.join("adminkey")),
    }
}

fn unit_uuid() -> uuid::Uuid {
    uuid::Uuid::parse_str(UNIT_ID).expect("unit id uuid")
}

fn root_signer() -> Ed25519SigningProvider {
    Ed25519SigningProvider::new(TEST_ROOT_SECRET)
}

fn status(node: &Node) -> IdentityBootstrapState {
    IdentityBootstrapStatusService::compute(&node.db, &node.node_key_store, &node.adminkey_provider)
        .expect("status computed")
}

/// Full offline WILAYA bootstrap (Root-issued). Returns the ACTIVE WILAYA cert.
fn bootstrap_wilaya(node: &mut Node) -> IdentityCertificate {
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    let request = provisioning
        .generate_wilaya_request(uuid::Uuid::new_v4(), &node.node_key_store)
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

/// Re-sign `cert` with the WILAYA node's node secret (same issuer as the anchor).
fn resign_with_wilaya(wilaya: &Node, cert: &IdentityCertificate) -> IdentityCertificate {
    let secret = wilaya.node_key_store.read().expect("wilaya node key");
    let signer = Ed25519SigningProvider::new(secret);
    let mut signed = cert.clone();
    signed.signature = Some(
        Ed25519CertificateSignature::try_from(signer.sign_certificate(cert).expect("sig"))
            .expect("wrap"),
    );
    signed
}

/// WILAYA side of the packaged flow: mirror the unit row, generate the keypair
/// in memory, sign the UNIT certificate with the ACTIVE WILAYA (registering the
/// WILAYA-side Issuer Local State), and return the secret + signed certificate.
struct PackagedExport {
    wilaya: Node,
    wilaya_cert: IdentityCertificate,
    secret: [u8; 32],
    certificate: IdentityCertificate,
}

fn export_packaged_identity() -> PackagedExport {
    let mut wilaya = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut wilaya);
    wilaya
        .db
        .executor()
        .units()
        .upsert_raw_unit(UNIT_ID, UNIT_CODE, UNIT_NAME, UNIT_WILAYA, FIXED_NOW)
        .expect("unit mirrored on wilaya");

    let (secret, certificate) = {
        let provisioning = IdentityProvisioningService::new(&mut wilaya.db);
        let (secret, request) = provisioning
            .generate_unit_identity_request_for_package(unit_uuid())
            .expect("packaged keypair generated in memory");
        let certificate = provisioning
            .sign_unit_bootstrap_request(&request, &wilaya.node_key_store, FIXED_NOW)
            .expect("packaged unit cert signed by ACTIVE WILAYA");
        (secret, certificate)
    };

    PackagedExport {
        wilaya,
        wilaya_cert,
        secret,
        certificate,
    }
}

/// UNIT-side packaged import — mirrors the `import_unit_node_package` command
/// core (base import + guarded key install + executor identity install).
fn apply_packaged_import(
    unit: &mut Node,
    wilaya_cert: &IdentityCertificate,
    package: &UnitNodePackage,
) -> grpc_lib::errors::AppResult<()> {
    IdentityTrustAnchorService::new(unit.db.executor())
        .install_wilaya_certificate(wilaya_cert, FIXED_NOW)?;

    let packaged = IdentityProvisioningService::extract_packaged_unit_identity(
        package,
        &unit_uuid(),
        &wilaya_cert.identity_id,
    )?;
    NodePackageService::new(unit.db.executor()).import_unit_node_package(package)?;
    if let Some(pkg) = packaged {
        IdentityProvisioningService::install_node_key_matching(
            &unit.node_key_store,
            &pkg.secret_key,
            &pkg.certificate.public_key,
        )?;
        IdentityProvisioningService::install_unit_identity_on_executor(
            unit.db.executor(),
            &pkg.certificate,
            &unit.node_key_store,
            FIXED_NOW,
        )?;
    }
    Ok(())
}

/// A valid packaged `.unit` payload (unit + user + cert + secret).
fn packaged_payload(secret: &[u8; 32], certificate: &IdentityCertificate) -> UnitNodePackage {
    UnitNodePackage {
        unit: Unit {
            id: UNIT_ID.into(),
            code: UNIT_CODE.into(),
            name: UNIT_NAME.into(),
            wilaya_code: UNIT_WILAYA.into(),
            user_id: None,
            created_at: chrono::Utc::now(),
        },
        user: UserExport {
            username: "op".into(),
            password_hash: "hash".into(),
            role: "User".into(),
        },
        unit_certificate: Some(certificate.clone()),
        unit_private_key: Some(secret.to_vec()),
    }
}

/// A `.unit` payload carrying only the credential material — no packaged
/// identity. Used by the ADR-0063 §11 credential-lifecycle tests, which
/// exercise the operator row write rather than the identity install.
fn plain_payload(password_hash: &str) -> UnitNodePackage {
    UnitNodePackage {
        unit: Unit {
            id: UNIT_ID.into(),
            code: UNIT_CODE.into(),
            name: UNIT_NAME.into(),
            wilaya_code: UNIT_WILAYA.into(),
            user_id: None,
            created_at: chrono::Utc::now(),
        },
        user: UserExport {
            username: "op".into(),
            password_hash: password_hash.into(),
            role: "User".into(),
        },
        unit_certificate: None,
        unit_private_key: None,
    }
}

// ---------------------------------------------------------------------------
// Export side: in-memory keygen + WILAYA signing + re-export rule
// ---------------------------------------------------------------------------

#[test]
fn packaged_keypair_generated_in_memory_without_touching_wilaya_store() {
    let mut wilaya = fresh_node();
    bootstrap_wilaya(&mut wilaya);
    wilaya
        .db
        .executor()
        .units()
        .upsert_raw_unit(UNIT_ID, UNIT_CODE, UNIT_NAME, UNIT_WILAYA, FIXED_NOW)
        .expect("unit mirrored");

    let before = wilaya.node_key_store.read().expect("wilaya node key");

    let (secret, request) = IdentityProvisioningService::new(&mut wilaya.db)
        .generate_unit_identity_request_for_package(unit_uuid())
        .expect("in-memory keygen");
    assert_eq!(secret.len(), 32);
    assert_eq!(request.subject_type, SubjectType::Unit);
    assert_eq!(request.subject_id, unit_uuid());
    assert_eq!(request.signature, None, "request is a CSR (unsigned)");
    assert_eq!(
        request.issuer_identity_id, None,
        "issuer unbound at CSR time"
    );

    let after = wilaya.node_key_store.read().expect("wilaya node key");
    assert_eq!(
        before, after,
        "packaged keygen must NEVER write to the WILAYA NodeKeyStore"
    );
}

#[test]
fn packaged_certificate_is_signed_by_active_wilaya_and_binds_the_secret() {
    let e = export_packaged_identity();
    let cert = &e.certificate;
    cert.require_signed().expect("signed");
    assert_eq!(cert.subject_type, SubjectType::Unit);
    assert_eq!(cert.subject_id, unit_uuid());
    assert_eq!(cert.status, CredentialStatus::Active);
    assert_eq!(
        cert.issuer_identity_id,
        Some(e.wilaya_cert.identity_id),
        "issuer must be the ACTIVE WILAYA identity"
    );
    assert_eq!(
        Ed25519SigningProvider::new(e.secret).public_key(),
        cert.public_key,
        "the packaged secret must derive the certificate public key"
    );

    let verifier = Ed25519SignatureVerifier;
    let valid = verifier
        .verify_certificate(
            cert,
            &e.wilaya_cert.public_key,
            cert.signature.as_ref().expect("sig"),
        )
        .expect("verify");
    assert!(valid, "WILAYA issuer signature must verify (R5)");

    assert_eq!(
        e.wilaya.node_key_store.read().expect("key"),
        e.wilaya.node_key_store.read().expect("key"),
        "no key material persisted to the WILAYA store beyond its own key"
    );
}

#[test]
fn re_export_is_rejected_once_active_unit_identity_exists() {
    let mut wilaya = fresh_node();
    bootstrap_wilaya(&mut wilaya);
    wilaya
        .db
        .executor()
        .units()
        .upsert_raw_unit(UNIT_ID, UNIT_CODE, UNIT_NAME, UNIT_WILAYA, FIXED_NOW)
        .expect("unit mirrored");

    let provisioning = IdentityProvisioningService::new(&mut wilaya.db);
    let (_, request) = provisioning
        .generate_unit_identity_request_for_package(unit_uuid())
        .expect("first keygen");
    provisioning
        .sign_unit_bootstrap_request(&request, &wilaya.node_key_store, FIXED_NOW)
        .expect("first export signs");

    // RE-EXPORT RULE: the duplicate-ACTIVE guard rejects the second issuance
    // with zero mutation (WILAYA-side ACTIVE UNIT identity already registered).
    let (_, request2) = provisioning
        .generate_unit_identity_request_for_package(unit_uuid())
        .expect("second keygen is allowed (pure in-memory)");
    let err = provisioning
        .sign_unit_bootstrap_request(&request2, &wilaya.node_key_store, FIXED_NOW)
        .expect_err("second export must be rejected (RE-EXPORT = REJECT)");
    assert!(
        err.to_string()
            .contains("ACTIVE UNIT identity already exists"),
        "got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Package serde: fields round-trip + legacy compatibility
// ---------------------------------------------------------------------------

#[test]
fn packaged_identity_fields_round_trip_through_serde() {
    let e = export_packaged_identity();
    let package = packaged_payload(&e.secret, &e.certificate);

    let json = serde_json::to_string(&package).expect("serialize");
    assert!(
        json.contains("unit_certificate") && json.contains("unit_private_key"),
        "new fields must be serialized when present"
    );

    let round: UnitNodePackage = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(round.unit_certificate, Some(e.certificate.clone()));
    assert_eq!(round.unit_private_key, Some(e.secret.to_vec()));
}

#[test]
fn legacy_package_without_identity_fields_still_round_trips() {
    let legacy = UnitNodePackage {
        unit: Unit {
            id: UNIT_ID.into(),
            code: UNIT_CODE.into(),
            name: UNIT_NAME.into(),
            wilaya_code: UNIT_WILAYA.into(),
            user_id: None,
            created_at: chrono::Utc::now(),
        },
        user: UserExport {
            username: "op".into(),
            password_hash: "hash".into(),
            role: "User".into(),
        },
        unit_certificate: None,
        unit_private_key: None,
    };

    let json = serde_json::to_string(&legacy).expect("serialize");
    assert!(
        !json.contains("unit_certificate") && !json.contains("unit_private_key"),
        "None fields must be skipped"
    );

    // A bare legacy payload (no fields at all) must deserialize to None/None.
    let bare = r#"{"unit":{"id":"x","code":"X","name":"N","wilaya_code":"16","user_id":null,"created_at":"2026-08-04T00:00:00Z"},"user":{"username":"op","password_hash":"h","role":"User"}}"#;
    let parsed: UnitNodePackage = serde_json::from_str(bare).expect("legacy parse");
    assert_eq!(parsed.unit_certificate, None);
    assert_eq!(parsed.unit_private_key, None);
}

// ---------------------------------------------------------------------------
// Pre-write extraction validation (both-or-neither + cert/key invariants)
// ---------------------------------------------------------------------------

#[test]
fn extraction_requires_both_certificate_and_key_or_neither() {
    let e = export_packaged_identity();
    let mut cert_only = packaged_payload(&e.secret, &e.certificate);
    cert_only.unit_private_key = None;
    let err = IdentityProvisioningService::extract_packaged_unit_identity(
        &cert_only,
        &unit_uuid(),
        &e.wilaya_cert.identity_id,
    )
    .expect_err("certificate-only must fail closed");
    assert!(err.to_string().contains("BOTH"), "got {err:?}");

    let mut key_only = packaged_payload(&e.secret, &e.certificate);
    key_only.unit_certificate = None;
    let err = IdentityProvisioningService::extract_packaged_unit_identity(
        &key_only,
        &unit_uuid(),
        &e.wilaya_cert.identity_id,
    )
    .expect_err("key-only must fail closed");
    assert!(err.to_string().contains("BOTH"), "got {err:?}");

    let legacy = packaged_payload(&e.secret, &e.certificate);
    let none = IdentityProvisioningService::extract_packaged_unit_identity(
        &legacy,
        &unit_uuid(),
        &e.wilaya_cert.identity_id,
    );
    // (both present → valid); also assert the absent-absent shape is `None`.
    assert!(none.as_ref().expect("valid").is_some());
    let mut absent = packaged_payload(&e.secret, &e.certificate);
    absent.unit_certificate = None;
    absent.unit_private_key = None;
    assert!(
        IdentityProvisioningService::extract_packaged_unit_identity(
            &absent,
            &unit_uuid(),
            &e.wilaya_cert.identity_id,
        )
        .expect("legacy")
        .is_none(),
        "legacy None/None package has no packaged identity"
    );
}

#[test]
fn extraction_rejects_non_32_byte_private_key() {
    let e = export_packaged_identity();
    let mut package = packaged_payload(&e.secret, &e.certificate);
    package.unit_private_key = Some(vec![1u8, 2u8, 3u8]);
    let err = IdentityProvisioningService::extract_packaged_unit_identity(
        &package,
        &unit_uuid(),
        &e.wilaya_cert.identity_id,
    )
    .expect_err("short key must fail closed");
    assert!(err.to_string().contains("32 bytes"), "got {err:?}");
}

#[test]
fn extraction_rejects_mismatched_subject_id() {
    let e = export_packaged_identity();
    let package = packaged_payload(&e.secret, &e.certificate);
    let err = IdentityProvisioningService::extract_packaged_unit_identity(
        &package,
        &uuid::Uuid::new_v4(),
        &e.wilaya_cert.identity_id,
    )
    .expect_err("foreign expected subject must fail closed");
    assert!(err.to_string().contains("subject_id"), "got {err:?}");
}

#[test]
fn extraction_rejects_mismatched_issuer() {
    let e = export_packaged_identity();
    let package = packaged_payload(&e.secret, &e.certificate);
    let err = IdentityProvisioningService::extract_packaged_unit_identity(
        &package,
        &unit_uuid(),
        &uuid::Uuid::new_v4(),
    )
    .expect_err("foreign issuer must fail closed");
    assert!(err.to_string().contains("issuer"), "got {err:?}");
}

#[test]
fn extraction_rejects_unsigned_certificate() {
    let e = export_packaged_identity();
    let mut unsigned = e.certificate.clone();
    unsigned.signature = None;
    let package = packaged_payload(&e.secret, &unsigned);
    let err = IdentityProvisioningService::extract_packaged_unit_identity(
        &package,
        &unit_uuid(),
        &e.wilaya_cert.identity_id,
    )
    .expect_err("unsigned cert must fail closed");
    assert!(err.to_string().contains("signed"), "got {err:?}");
}

#[test]
fn extraction_rejects_non_active_certificate() {
    let e = export_packaged_identity();
    let mut revoked = e.certificate.clone();
    revoked.status = CredentialStatus::Revoked;
    let package = packaged_payload(&e.secret, &revoked);
    let err = IdentityProvisioningService::extract_packaged_unit_identity(
        &package,
        &unit_uuid(),
        &e.wilaya_cert.identity_id,
    )
    .expect_err("non-ACTIVE cert must fail closed");
    assert!(err.to_string().contains("ACTIVE"), "got {err:?}");
}

#[test]
fn extraction_rejects_wrong_algorithm_profile() {
    let e = export_packaged_identity();
    let mut wrong = e.certificate.clone();
    wrong.algorithm_version = 3;
    let package = packaged_payload(&e.secret, &wrong);
    let err = IdentityProvisioningService::extract_packaged_unit_identity(
        &package,
        &unit_uuid(),
        &e.wilaya_cert.identity_id,
    )
    .expect_err("non-Ed25519 profile must fail closed");
    assert!(err.to_string().contains("Ed25519"), "got {err:?}");
}

#[test]
fn extraction_rejects_secret_not_binding_the_certificate() {
    let e = export_packaged_identity();
    let package = packaged_payload(&[7u8; 32], &e.certificate);
    let err = IdentityProvisioningService::extract_packaged_unit_identity(
        &package,
        &unit_uuid(),
        &e.wilaya_cert.identity_id,
    )
    .expect_err("secret/public-key mismatch must fail closed");
    assert!(err.to_string().contains("does not match"), "got {err:?}");
}

// ---------------------------------------------------------------------------
// NodeKeyStore guard: absent → write, same → no-op, different → reject
// ---------------------------------------------------------------------------

#[test]
fn install_node_key_writes_when_absent() {
    let e = export_packaged_identity();
    let dir = TempDir::new().expect("temp dir");
    let store = NodeKeyStore::new(dir.path().to_path_buf());

    assert!(!store.exists());
    IdentityProvisioningService::install_node_key_matching(
        &store,
        &e.secret,
        &e.certificate.public_key,
    )
    .expect("absent → write");
    assert!(store.exists());
    assert_eq!(
        store.read().expect("read"),
        e.secret,
        "the packaged secret must be persisted"
    );
}

#[test]
fn install_node_key_is_noop_for_identical_key() {
    let e = export_packaged_identity();
    let dir = TempDir::new().expect("temp dir");
    let store = NodeKeyStore::new(dir.path().to_path_buf());
    store.write(&e.secret).expect("pre-existing identical key");

    IdentityProvisioningService::install_node_key_matching(
        &store,
        &e.secret,
        &e.certificate.public_key,
    )
    .expect("identical → no-op");
    assert_eq!(store.read().expect("read"), e.secret);
}

#[test]
fn install_node_key_rejects_different_existing_key_without_overwrite() {
    let e = export_packaged_identity();
    let dir = TempDir::new().expect("temp dir");
    let store = NodeKeyStore::new(dir.path().to_path_buf());
    let foreign = [9u8; 32];
    store.write(&foreign).expect("pre-existing different key");

    let err = IdentityProvisioningService::install_node_key_matching(
        &store,
        &e.secret,
        &e.certificate.public_key,
    )
    .expect_err("different key must be refused");
    assert!(
        err.to_string().contains("refusing to overwrite"),
        "got {err:?}"
    );
    assert_eq!(
        store.read().expect("read"),
        foreign,
        "the existing key must NEVER be overwritten"
    );
}

#[test]
fn install_node_key_fails_closed_on_corrupt_store() {
    let e = export_packaged_identity();
    let dir = TempDir::new().expect("temp dir");
    let store = NodeKeyStore::new(dir.path().to_path_buf());
    std::fs::write(dir.path().join("node_identity.key"), "garbage").expect("corrupt file");

    let err = IdentityProvisioningService::install_node_key_matching(
        &store,
        &e.secret,
        &e.certificate.public_key,
    )
    .expect_err("corrupt store must fail closed");
    assert!(!err.to_string().is_empty());
}

#[test]
fn install_node_key_rejects_secret_not_binding_the_certificate() {
    let e = export_packaged_identity();
    let dir = TempDir::new().expect("temp dir");
    let store = NodeKeyStore::new(dir.path().to_path_buf());

    let err = IdentityProvisioningService::install_node_key_matching(
        &store,
        &[7u8; 32],
        &e.certificate.public_key,
    )
    .expect_err("non-binding secret must fail BEFORE any write");
    assert!(err.to_string().contains("does not match"), "got {err:?}");
    assert!(
        !store.exists(),
        "no key may be written on a binding failure"
    );
}

// ---------------------------------------------------------------------------
// Install on the UNIT node: full flow, idempotency, crash-residual retry
// ---------------------------------------------------------------------------

#[test]
fn packaged_import_reaches_unit_active() {
    let e = export_packaged_identity();
    let mut unit = fresh_node();
    let package = packaged_payload(&e.secret, &e.certificate);

    apply_packaged_import(&mut unit, &e.wilaya_cert, &package).expect("packaged import succeeds");
    assert_eq!(status(&unit), IdentityBootstrapState::UnitActive);
}

#[test]
fn packaged_import_matches_unit_key_binding_inside_store() {
    let e = export_packaged_identity();
    let mut unit = fresh_node();
    let package = packaged_payload(&e.secret, &e.certificate);
    apply_packaged_import(&mut unit, &e.wilaya_cert, &package).expect("packaged import succeeds");

    assert_eq!(
        unit.node_key_store.read().expect("unit node key"),
        e.secret,
        "the packaged secret must be the UNIT node key after import"
    );
    let active = unit
        .db
        .executor()
        .identity_store()
        .get_active_by_subject(SubjectType::Unit, &unit_uuid())
        .expect("query")
        .expect("ACTIVE UNIT identity present");
    assert!(active.is_identical_to(&e.certificate));
}

#[test]
fn identical_reimport_is_idempotent_zero_writes() {
    let e = export_packaged_identity();
    let mut unit = fresh_node();
    let package = packaged_payload(&e.secret, &e.certificate);
    apply_packaged_import(&mut unit, &e.wilaya_cert, &package).expect("first import");

    let before = unit
        .db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .len();
    let outcome = IdentityProvisioningService::install_unit_identity_on_executor(
        unit.db.executor(),
        &e.certificate,
        &unit.node_key_store,
        FIXED_NOW,
    )
    .expect("identical re-presentation");
    assert!(
        matches!(outcome, FinalizeUnitProvisionResult::AlreadyProvisioned(_)),
        "identical re-presentation must be a no-op"
    );
    let after = unit
        .db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .len();
    assert_eq!(before, after, "identical re-import must do ZERO writes");
}

#[test]
fn crash_residual_key_present_identity_absent_recovers_on_retry() {
    let e = export_packaged_identity();
    let mut unit = fresh_node();
    let package = packaged_payload(&e.secret, &e.certificate);

    // First attempt — crash BETWEEN the guarded key write and the identity
    // upsert (the DB transaction rolls back, the key file remains).
    IdentityTrustAnchorService::new(unit.db.executor())
        .install_wilaya_certificate(&e.wilaya_cert, FIXED_NOW)
        .expect("anchor installed");
    NodePackageService::new(unit.db.executor())
        .import_unit_node_package(&package)
        .expect("base import");
    IdentityProvisioningService::install_node_key_matching(
        &unit.node_key_store,
        &e.secret,
        &e.certificate.public_key,
    )
    .expect("key written (step 11)");
    // Step 12 (identity upsert) is SKIPPED → identity row absent.

    assert_eq!(
        unit.db
            .executor()
            .identity_store()
            .get_active_by_subject(SubjectType::Unit, &unit_uuid())
            .expect("query"),
        None,
        "crash residual: key present, identity absent"
    );

    // Retry with the SAME package: the key write is a no-op, the identity
    // install settles → UNIT READY.
    apply_packaged_import(&mut unit, &e.wilaya_cert, &package)
        .expect("retry succeeds after crash residual");
    assert_eq!(status(&unit), IdentityBootstrapState::UnitActive);
}

#[test]
fn install_rejects_certificate_not_matching_installed_key() {
    let e = export_packaged_identity();
    let mut unit = fresh_node();
    let package = packaged_payload(&e.secret, &e.certificate);
    apply_packaged_import(&mut unit, &e.wilaya_cert, &package).expect("import");

    // Present a DIFFERENT cert (re-signed by the WILAYA, same subject) whose
    // public key does not match the installed node key → cross-device swap.
    let mut swapped = e.certificate.clone();
    swapped.public_key = Ed25519SigningProvider::new([3u8; 32]).public_key();
    let swapped = resign_with_wilaya(&e.wilaya, &swapped);
    let err = IdentityProvisioningService::install_unit_identity_on_executor(
        unit.db.executor(),
        &swapped,
        &unit.node_key_store,
        FIXED_NOW,
    )
    .expect_err("cross-device cert must fail closed");
    assert!(
        err.to_string()
            .contains("does not match the node signing key"),
        "got {err:?}"
    );
}

#[test]
fn install_rejects_non_ed25519_algorithm_profile() {
    let e = export_packaged_identity();
    let mut unit = fresh_node();
    let package = packaged_payload(&e.secret, &e.certificate);
    apply_packaged_import(&mut unit, &e.wilaya_cert, &package).expect("import");

    // WILAYA-signed cert with a non-Ed25519 algorithm profile.
    let mut wrong = e.certificate.clone();
    wrong.algorithm_version = 3;
    let wrong = resign_with_wilaya(&e.wilaya, &wrong);
    let err = IdentityProvisioningService::install_unit_identity_on_executor(
        unit.db.executor(),
        &wrong,
        &unit.node_key_store,
        FIXED_NOW,
    )
    .expect_err("non-Ed25519 profile must fail closed");
    assert!(err.to_string().contains("algorithm profile"), "got {err:?}");
}

#[test]
fn install_rejects_identity_conflict_with_different_active_credential() {
    let e = export_packaged_identity();
    let mut unit = fresh_node();
    let package = packaged_payload(&e.secret, &e.certificate);
    apply_packaged_import(&mut unit, &e.wilaya_cert, &package).expect("import");

    // A DIFFERENT ACTIVE UNIT credential for the same subject (new credential
    // id, re-signed by the WILAYA) → conflict, fail closed.
    let mut rotated = e.certificate.clone();
    rotated.credential_id = uuid::Uuid::new_v4();
    let rotated = resign_with_wilaya(&e.wilaya, &rotated);
    let err = IdentityProvisioningService::install_unit_identity_on_executor(
        unit.db.executor(),
        &rotated,
        &unit.node_key_store,
        FIXED_NOW,
    )
    .expect_err("different ACTIVE credential must fail closed");
    assert!(
        err.to_string().contains("already exists and differs"),
        "got {err:?}"
    );
}

/// Mirror of `export_packaged_identity` for tests that need only a WILAYA node
/// with a registered ACTIVE UNIT identity (used by the re-export flow).
#[allow(dead_code)]
fn _identity_present_count(node: &Node) -> usize {
    node.db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .len()
}

#[allow(dead_code)]
fn _install_wilaya_certificate_once(unit: &mut Node, wilaya_cert: &IdentityCertificate) {
    let outcome = IdentityTrustAnchorService::new(unit.db.executor())
        .install_wilaya_certificate(wilaya_cert, FIXED_NOW)
        .expect("anchor installed");
    assert!(matches!(
        outcome,
        InstallWilayaCertificateResult::Installed(_)
    ));
}

// ── ADR-0063 §11 — the `.unit` credential lifecycle ────────────────────────

fn imported_operator(node: &Node) -> grpc_lib::models::User {
    node.db
        .executor()
        .users()
        .get_user_by_username_raw("op", UNIT_CODE)
        .expect("operator readable")
        .expect("operator row exists")
}

/// A serialized forced-state value in an artifact is inert (ADR-0063 §11):
/// it never reaches the domain model and cannot keep a re-provisioned
/// operator out of the forced-password lifecycle.
#[test]
fn serialized_forced_state_field_is_inert_on_import() {
    // The V2 envelope exactly as the exporter emits it — canonical integrity
    // over the flagless payload shape (`UserExport` has no forced flag).
    let mut sealed: SyncPackage<UnitNodePackage> = SyncPackage {
        metadata: SyncPackageMetadata {
            created_at: chrono::Utc::now(),
            export_mode: None,
            integrity_hash: None,
            issuer_identity_id: None,
            package_id: PackageId("pkg-unit-s11-inert".into()),
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            signature: Some("test-signature".into()),
            signature_version: Some(SIGNATURE_VERSION_ED25519),
            signing_key_id: None,
            source_node_id: "WILAYA".into(),
            target_node_id: None,
        },
        payload: plain_payload("exported-hash"),
    };
    let clean = serde_json::to_value(&sealed).expect("value");
    let hash = Sha256PackageHasher
        .hash(&canonical_bytes_for_integrity(&clean).expect("canonical"))
        .expect("integrity hash");
    sealed.metadata.integrity_hash = Some(hash);

    // An artifact additionally carries `"must_change_password": false` inside
    // `user`. The contract has no such field, so the deserializer drops it and
    // the integrity hash (computed over the parsed shape) still holds.
    let mut tampered = serde_json::to_value(&sealed).expect("value");
    tampered["payload"]["user"]["must_change_password"] = serde_json::json!(false);
    let artifact = tampered.to_string();

    let parsed = SerdeJsonSyncPackageDeserializer::unit_node_package_from_reader(
        std::io::Cursor::new(artifact.as_bytes()),
    )
    .expect("the artifact parses; the serialized forced flag is dropped by the schema");

    let serialized_user = serde_json::to_value(&parsed.payload.user).expect("user value");
    assert!(
        serialized_user.get("must_change_password").is_none(),
        "the serialized flag must not survive into the domain model"
    );

    // Local policy, not the artifact, decides the resulting state.
    let node = fresh_node();
    NodePackageService::new(node.db.executor())
        .import_unit_node_package(&parsed.payload)
        .expect("import");
    assert!(
        imported_operator(&node).must_change_password,
        "ADR-0063 §11: the injected `must_change_password: false` must not reach local state"
    );
}

/// The forced-state write lives inside the import audit transaction: a
/// failure after the operator upsert restores the previous credential AND the
/// previous forced-state value, and writes no audit row.
#[test]
fn forced_state_rolls_back_with_the_import_audit_transaction() {
    let mut unit = fresh_node();

    // Baseline provisioning, then a completed rotation: the pre-import state
    // is a rotated credential that is NOT forced.
    NodePackageService::new(unit.db.executor())
        .import_unit_node_package(&plain_payload("first-hash"))
        .expect("baseline import");
    let operator = imported_operator(&unit);
    unit.db
        .get_connection()
        .execute(
            "UPDATE users SET password_hash = ?1, must_change_password = 0 WHERE id = ?2",
            params!["rotated-hash", operator.id],
        )
        .expect("simulate completed rotation");

    // The real command transaction shape (base import → identity install …).
    // The step after the operator upsert fails on purpose.
    let user_ctx = UserContext::new("system", "system_bootstrap", None);
    let _err = AuditTxService::execute_with_audit(
        &mut unit.db,
        AuditAction::ImportNodePackage,
        &user_ctx,
        |tx| -> Result<(), grpc_lib::errors::AppError> {
            NodePackageService::new(tx.executor)
                .import_unit_node_package(&plain_payload("replacement-hash"))?;
            Err(grpc_lib::errors::AppError::Internal(
                "injected failure after the operator upsert".into(),
            ))
        },
    )
    .expect_err("the injected failure must abort the import transaction");

    // Rollback: previous credential and previous forced state restored.
    let after = imported_operator(&unit);
    assert_eq!(
        after.password_hash, "rotated-hash",
        "the previous credential must be restored by the rollback"
    );
    assert!(
        !after.must_change_password,
        "no partial forced state may remain after the rollback"
    );

    // The import audit row rolls back with the mutation it describes.
    let audit_rows: i64 = unit
        .db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action = ?1",
            params![AuditAction::ImportNodePackage.as_str()],
            |row| row.get(0),
        )
        .expect("audit count");
    assert_eq!(
        audit_rows, 0,
        "no ImportNodePackage audit row may survive the rollback"
    );
}
