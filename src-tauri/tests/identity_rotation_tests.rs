//! B7 credential rotation integration tests (RFC §3.3 / §3.4.2 / §3.12).
//!
//! RFC 2026-08-04-node-identity-trust §3.3 / §3.4.2 / §3.12 / ADR-0038.
//!
//! Exercises `IdentityRotationCoordinator` end-to-end:
//! - WILAYA Rotate: stage-only `begin` (active key untouched) → Root signs →
//!   `finalize` writes the rotation Trust Package (signed by the OLD key,
//!   BEFORE secret promotion) then promotes + installs; replay = ZERO writes;
//! - WILAYA Re-Issue: new `credential_id` + generation 1;
//! - UNIT Rotate: UNIT `begin` → WILAYA `sign_unit_rotation` (Issuer Local State)
//!   → UNIT `finalize` (issuer resolved via `issuer_identity_id`, ACTIVE+WILAYA);
//! - fail-closed: no ACTIVE identity to rotate; tampered signature; no staged key.
//!
//! The test authority Root uses the RFC 8032 §7.1 TEST 1 keypair — exactly the
//! `#[cfg(debug_assertions)]` development Root fallback in `root_public_key.rs`.

#[allow(dead_code)]
mod common;

use tempfile::TempDir;

use grpc_lib::application::services::{
    FinalizeUnitProvisionResult, FinalizeWilayaProvisionResult, IdentityProvisioningService,
    IdentityRotationCoordinator, IdentityTrustAnchorService, RotationFinalizeOutcome,
    RotationOperation,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    Ed25519CertificateSignature, IdentityCertificate, IdentitySigner, IdentityStorePort,
    SubjectType,
};
use grpc_lib::infrastructure::identity::NodeKeyStore;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::Ed25519SigningProvider;
use grpc_lib::infrastructure::sync::packages::canonical_json::canonical_bytes_for_signature;
use grpc_lib::infrastructure::sync::packages::signing::{Ed25519PackageVerifier, PackageVerifier};
use grpc_lib::infrastructure::sync::read_trust_package_from_file;
use grpc_lib::models::{NodeType, Settings};
use grpc_lib::repositories::RepositoryProvider;

/// RFC 8032 §7.1 TEST 1 secret — the matching public key IS the debug-mode
/// development Root fallback (root_public_key.rs). Never a production key.
const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
/// Fixed timestamp for deterministic persistence.
const FIXED_NOW: &str = "2026-08-04T00:00:00Z";
/// A fixed local unit used by the UNIT node and mirrored on the WILAYA node.
const UNIT_ID: &str = "4e1c8f3a-9b2d-4c6e-8f0a-1b2c3d4e5f60";
const UNIT_CODE: &str = "1601";
const UNIT_NAME: &str = "وحدة حماية مدنية 01";
const UNIT_WILAYA: &str = "16";

/// A fresh node directory: real DB + node key store.
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

fn sign_with_root(cert: &IdentityCertificate) -> IdentityCertificate {
    let signature = root_signer().sign_certificate(cert).expect("root signed");
    let mut signed = cert.clone();
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig wrap"));
    signed
}

fn active_wilaya(node: &Node) -> IdentityCertificate {
    node.db
        .executor()
        .identity_store()
        .get_active_by_subject_type(SubjectType::Wilaya)
        .expect("query")
        .expect("ACTIVE WILAYA present")
}

fn active_unit(node: &Node) -> IdentityCertificate {
    node.db
        .executor()
        .identity_store()
        .get_active_by_subject(SubjectType::Unit, &unit_uuid())
        .expect("query")
        .expect("ACTIVE UNIT present")
}

fn unit_uuid() -> uuid::Uuid {
    uuid::Uuid::parse_str(UNIT_ID).expect("unit id uuid")
}

fn wilaya_settings() -> Settings {
    Settings {
        id: 1,
        node_type: NodeType::Wilaya,
        unit_name: None,
        unit_code: None,
        current_year: 2026,
        wilaya_code: Some(UNIT_WILAYA.to_string()),
        wilaya_name: Some("Algiers".to_string()),
        configured: true,
    }
}

/// Full offline WILAYA bootstrap (Root-issued). Returns the ACTIVE WILAYA cert.
fn bootstrap_wilaya(node: &mut Node) -> IdentityCertificate {
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    let request = provisioning
        .generate_wilaya_request(uuid::Uuid::new_v4(), &node.node_key_store)
        .expect("csr generated");
    let signed = sign_with_root(&request);
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

/// Seed a `units` row + mark settings as a configured UNIT node.
fn make_unit_node(node: &mut Node) {
    node.db
        .executor()
        .units()
        .upsert_raw_unit(UNIT_ID, UNIT_CODE, UNIT_NAME, UNIT_WILAYA, FIXED_NOW)
        .expect("unit seeded");
    node.db
        .executor()
        .settings()
        .update_unit_node_settings(UNIT_NAME, UNIT_WILAYA)
        .expect("settings set to UNIT");
}

/// ACTIVE WILAYA cert in the Identity Store (issuer identity for UNIT signing).
fn sign_unit(wilaya_node: &mut Node, csr: &IdentityCertificate) -> IdentityCertificate {
    IdentityProvisioningService::new(&mut wilaya_node.db)
        .sign_unit_identity_request(csr, &wilaya_node.node_key_store)
        .expect("unit csr signed")
}

/// A fully-provisioned WILAYA node that also holds the shared unit row.
fn fresh_wilaya_with_unit() -> Node {
    let mut wilaya = fresh_node();
    bootstrap_wilaya(&mut wilaya);
    wilaya
        .db
        .executor()
        .units()
        .upsert_raw_unit(UNIT_ID, UNIT_CODE, UNIT_NAME, UNIT_WILAYA, FIXED_NOW)
        .expect("unit mirrored on wilaya");
    wilaya
}

/// A fully provisioned UNIT node (ACTIVE UNIT cert, WILAYA anchor installed).
struct ProvisionedUnit {
    unit: Node,
    wilaya: Node,
}

fn provisioned_unit() -> ProvisionedUnit {
    let mut unit = fresh_node();
    make_unit_node(&mut unit);

    let provisioning = IdentityProvisioningService::new(&mut unit.db);
    let subject_id = provisioning
        .resolve_local_unit_subject_id()
        .expect("local unit subject resolved");
    let csr = provisioning
        .generate_identity_request(SubjectType::Unit, subject_id, &unit.node_key_store)
        .expect("unit csr");

    let mut wilaya = fresh_wilaya_with_unit();
    let wilaya_cert = active_wilaya(&wilaya);
    let signed_unit_cert = sign_unit(&mut wilaya, &csr);

    let mut service = IdentityTrustAnchorService::new(unit.db.executor());
    service
        .install_wilaya_certificate(&wilaya_cert, FIXED_NOW)
        .expect("anchor installed");
    match IdentityProvisioningService::new(&mut unit.db)
        .finalize_unit_provision(&signed_unit_cert, &unit.node_key_store, FIXED_NOW)
        .expect("unit finalized")
    {
        FinalizeUnitProvisionResult::Provisioned(_) => {}
        FinalizeUnitProvisionResult::AlreadyProvisioned(_) => {
            panic!("first finalize must provision")
        }
    }

    ProvisionedUnit { unit, wilaya }
}

// ---------------------------------------------------------------------------
// WILAYA rotation — begin
// ---------------------------------------------------------------------------

#[test]
fn wilaya_begin_stages_key_without_touching_active() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    let before = active_wilaya(&node);

    let plan = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect("begin rotation");

    // The active certificate is untouched by `begin`.
    assert!(active_wilaya(&node).is_identical_to(&before));
    // The staged secret derives the CSR public key (R5, pre-finalize).
    let pending = node
        .node_key_store
        .read_pending()
        .expect("pending read")
        .expect("pending key staged");
    assert_eq!(
        plan.certificate.public_key,
        Ed25519SigningProvider::new(pending).public_key()
    );
    // Rotate: same identity + credential, generation + 1, unsigned.
    assert_eq!(plan.operation, RotationOperation::Rotate);
    assert_eq!(plan.certificate.identity_id, before.identity_id);
    assert_eq!(plan.certificate.credential_id, before.credential_id);
    assert_eq!(plan.certificate.generation, before.generation + 1);
    assert_eq!(plan.certificate.signature, None);
    assert_eq!(plan.certificate.subject_type, SubjectType::Wilaya);
}

#[test]
fn wilaya_begin_reissue_derives_new_credential() {
    let mut node = fresh_node();
    let before = bootstrap_wilaya(&mut node);

    let plan = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::ReIssue)
        .expect("begin re-issue");

    assert_eq!(plan.operation, RotationOperation::ReIssue);
    assert_eq!(plan.certificate.identity_id, before.identity_id);
    assert_ne!(plan.certificate.credential_id, before.credential_id);
    assert_eq!(plan.certificate.generation, 1);
}

#[test]
fn begin_without_active_identity_fails_and_leaves_no_pending() {
    let mut node = fresh_node();
    let err = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect_err("must reject without ACTIVE WILAYA");
    assert!(err.to_string().contains("ACTIVE"), "got {err:?}");
    assert!(node
        .node_key_store
        .read_pending()
        .expect("pending read")
        .is_none());
}

// ---------------------------------------------------------------------------
// WILAYA rotation — finalize + trust package
// ---------------------------------------------------------------------------

#[test]
fn wilaya_full_rotation_emits_old_key_signed_trust_package_and_promotes() {
    let mut node = fresh_node();
    let before = bootstrap_wilaya(&mut node);
    let old_secret = node.node_key_store.read().expect("old secret");

    let plan = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect("begin rotation");
    let pending_secret = node
        .node_key_store
        .read_pending()
        .expect("pending read")
        .expect("pending key staged");
    let signed = sign_with_root(&plan.certificate);

    let dir = TempDir::new().expect("temp dir");
    let package_path = dir.path().join("rotation.sync");
    let crypto = AgeFileEncryptionProvider::new();

    let outcome = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&signed, &package_path, &wilaya_settings(), &crypto)
        .expect("finalize wilaya rotation");
    let RotationFinalizeOutcome::Completed {
        certificate,
        operation,
    } = outcome
    else {
        panic!("first finalize must complete")
    };
    assert_eq!(operation, RotationOperation::Rotate);
    assert!(certificate.is_identical_to(&signed));

    // Secret promoted: pending gone, ACTIVE key is now the staged secret and no
    // longer the old key.
    assert!(node
        .node_key_store
        .read_pending()
        .expect("pending read")
        .is_none());
    assert_eq!(
        node.node_key_store.read().expect("active key"),
        pending_secret,
        "promote_pending must install the staged secret"
    );
    assert_ne!(
        node.node_key_store.read().expect("active key"),
        old_secret,
        "the rotated node MUST NOT keep the old secret"
    );
    let new_active = active_wilaya(&node);
    assert!(new_active.is_identical_to(&signed));

    // Trust package exists, carries the new certificate, and is sequence 1.
    let package = read_trust_package_from_file(&package_path, &crypto).expect("read trust package");
    assert_eq!(package.metadata.package_sequence, Some(1));
    assert!(package
        .payload
        .certificates
        .iter()
        .any(|c| c.credential_id == signed.credential_id));

    // Signed by the OLD key (still ACTIVE at package-write time): verifies
    // against the old certificate and does NOT verify against the new one.
    let canonical = serde_json::to_value(&package).expect("package value");
    let canonical_bytes = canonical_bytes_for_signature(&canonical).expect("canonical");
    let signature = package.metadata.signature.as_deref().expect("signature");
    let old_pubkey: [u8; 32] = before.public_key.as_slice().try_into().expect("32 bytes");
    let new_pubkey: [u8; 32] = signed.public_key.as_slice().try_into().expect("32 bytes");
    assert!(Ed25519PackageVerifier::new(old_pubkey)
        .verify(&canonical_bytes, signature)
        .expect("verify old key"));
    assert!(!Ed25519PackageVerifier::new(new_pubkey)
        .verify(&canonical_bytes, signature)
        .expect("verify new key"));

    // Before the promotion the WILAYA identity was intact; after it, the node
    // still holds exactly one ACTIVE WILAYA row.
    assert!(active_wilaya(&node).is_identical_to(&signed));
}

#[test]
fn wilaya_finalize_replay_is_zero_write() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);

    let plan = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect("begin rotation");
    let signed = sign_with_root(&plan.certificate);

    let dir = TempDir::new().expect("temp dir");
    let package_path = dir.path().join("rotation.sync");
    let crypto = AgeFileEncryptionProvider::new();

    let first = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&signed, &package_path, &wilaya_settings(), &crypto)
        .expect("first finalize");
    assert!(matches!(first, RotationFinalizeOutcome::Completed { .. }));

    // Re-presenting the identical signed certificate: ZERO writes, and the
    // (already promoted) node has no pending key to consume.
    let before_rows = node
        .db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .len();
    let replay = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&signed, &package_path, &wilaya_settings(), &crypto)
        .expect("replay is accepted");
    assert!(
        matches!(replay, RotationFinalizeOutcome::AlreadyCompleted { .. }),
        "identical re-presentation must be a no-op, got {replay:?}"
    );
    let after_rows = node
        .db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .len();
    assert_eq!(before_rows, after_rows, "replay must perform ZERO writes");
}

#[test]
fn wilaya_finalize_rejects_tampered_signature() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);

    let plan = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect("begin rotation");
    // Signed by an IMPOSTER, not the pinned Authority Root.
    let imposter = Ed25519SigningProvider::new([7u8; 32]);
    let signature = imposter
        .sign_certificate(&plan.certificate)
        .expect("imposter signed");
    let mut signed = plan.certificate;
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig"));

    let dir = TempDir::new().expect("temp dir");
    let package_path = dir.path().join("rotation.sync");
    let crypto = AgeFileEncryptionProvider::new();

    let err = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&signed, &package_path, &wilaya_settings(), &crypto)
        .expect_err("imposter signature must fail closed");
    assert!(err.to_string().contains("pinned issuer"), "got {err:?}");
    // Nothing was promoted; the staged key is still there for a retry.
    assert!(node
        .node_key_store
        .read_pending()
        .expect("pending read")
        .is_some());
}

#[test]
fn wilaya_finalize_without_staged_key_fails() {
    let mut node = fresh_node();
    let before = bootstrap_wilaya(&mut node);

    let dir = TempDir::new().expect("temp dir");
    let package_path = dir.path().join("rotation.sync");
    let crypto = AgeFileEncryptionProvider::new();

    // A genuinely different (but well-formed, Root-signed) certificate with NO
    // staged key must fail — it is neither an idempotent replay nor installable.
    let mut other = before.clone();
    other.credential_id = uuid::Uuid::new_v4();
    other.generation += 1;
    other.public_key = Ed25519SigningProvider::new([5u8; 32]).public_key();
    let other = sign_with_root(&other);

    let err = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&other, &package_path, &wilaya_settings(), &crypto)
        .expect_err("no staged key must fail");
    assert!(
        err.to_string().contains("staged rotation key"),
        "got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// UNIT rotation — full flow
// ---------------------------------------------------------------------------

#[test]
fn unit_full_rotation_via_wilaya_signing_completes() {
    let mut provisioned = provisioned_unit();
    let unit_before = active_unit(&provisioned.unit);

    // 1. UNIT begin: stage a fresh node key, derive the CSR.
    let plan = IdentityRotationCoordinator::new(
        &mut provisioned.unit.db,
        &provisioned.unit.node_key_store,
    )
    .begin(SubjectType::Unit, RotationOperation::Rotate)
    .expect("unit begin rotation");
    assert_eq!(plan.operation, RotationOperation::Rotate);
    assert_eq!(plan.certificate.subject_type, SubjectType::Unit);
    assert_eq!(plan.certificate.identity_id, unit_before.identity_id);
    assert_eq!(plan.certificate.credential_id, unit_before.credential_id);
    assert_eq!(plan.certificate.generation, unit_before.generation + 1);
    assert_eq!(plan.certificate.signature, None);

    // 2. WILAYA signs the UNIT rotation CSR + records Issuer Local State.
    let signed_unit = IdentityRotationCoordinator::new(
        &mut provisioned.wilaya.db,
        &provisioned.wilaya.node_key_store,
    )
    .sign_unit_rotation(&plan.certificate)
    .expect("sign unit rotation");
    assert_eq!(signed_unit.certificate.subject_type, SubjectType::Unit);
    assert_eq!(signed_unit.operation, RotationOperation::ReIssue);
    assert!(
        signed_unit.certificate.signature.is_some(),
        "WILAYA must sign the UNIT rotation CSR"
    );
    // Issuer Local State: the WILAYA store now holds the UNIT cert (best-effort).
    let wilaya_local = provisioned
        .wilaya
        .db
        .executor()
        .identity_store()
        .get_by_identity_id(&signed_unit.certificate.identity_id)
        .expect("query")
        .expect("WILAYA-side local state recorded");

    // 3. UNIT finalize: issuer resolved via `issuer_identity_id`, promoted.
    let outcome = IdentityRotationCoordinator::new(
        &mut provisioned.unit.db,
        &provisioned.unit.node_key_store,
    )
    .finalize_unit(&signed_unit.certificate)
    .expect("unit finalize rotation");
    let RotationFinalizeOutcome::Completed {
        certificate,
        operation,
    } = outcome
    else {
        panic!("first unit finalize must complete")
    };
    assert_eq!(operation, RotationOperation::Rotate);
    assert!(certificate.is_identical_to(&signed_unit.certificate));

    let new_active = active_unit(&provisioned.unit);
    assert!(new_active.is_identical_to(&signed_unit.certificate));
    assert_eq!(new_active.generation, unit_before.generation + 1);
    assert!(provisioned
        .unit
        .node_key_store
        .read_pending()
        .expect("pending read")
        .is_none());
    assert!(wilaya_local.is_identical_to(&signed_unit.certificate));

    // 4. Replay on the UNIT: ZERO writes.
    let before_rows = provisioned
        .unit
        .db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .len();
    let replay = IdentityRotationCoordinator::new(
        &mut provisioned.unit.db,
        &provisioned.unit.node_key_store,
    )
    .finalize_unit(&signed_unit.certificate)
    .expect("replay accepted");
    assert!(
        matches!(replay, RotationFinalizeOutcome::AlreadyCompleted { .. }),
        "identical re-presentation must be a no-op, got {replay:?}"
    );
    assert_eq!(
        before_rows,
        provisioned
            .unit
            .db
            .executor()
            .identity_store()
            .list_all()
            .expect("list")
            .len(),
        "replay must perform ZERO writes"
    );
}

#[test]
fn unit_finalize_requires_active_wilaya_issuer() {
    let mut provisioned = provisioned_unit();

    let plan = IdentityRotationCoordinator::new(
        &mut provisioned.unit.db,
        &provisioned.unit.node_key_store,
    )
    .begin(SubjectType::Unit, RotationOperation::Rotate)
    .expect("unit begin rotation");

    // Sign with a DIFFERENT key and pretend a foreign identity issued it.
    let mut signed = plan.certificate.clone();
    signed.issuer_identity_id = Some(uuid::Uuid::new_v4());
    let foreign = Ed25519SigningProvider::new([9u8; 32]);
    let signature = foreign
        .sign_certificate(&plan.certificate)
        .expect("foreign signed");
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig"));

    let err = IdentityRotationCoordinator::new(
        &mut provisioned.unit.db,
        &provisioned.unit.node_key_store,
    )
    .finalize_unit(&signed)
    .expect_err("unknown issuer must fail closed");
    assert!(err.to_string().contains("does not exist"), "got {err:?}");
}

#[test]
fn unit_begin_without_active_identity_fails() {
    let mut node = fresh_node();
    let err = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Unit, RotationOperation::Rotate)
        .expect_err("must reject without ACTIVE UNIT");
    assert!(err.to_string().contains("ACTIVE"), "got {err:?}");
    assert!(node
        .node_key_store
        .read_pending()
        .expect("pending read")
        .is_none());
}
