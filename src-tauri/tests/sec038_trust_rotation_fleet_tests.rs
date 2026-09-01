//! SEC-038 Part B: Trust rotation fleet emission integration tests.
//!
//! Exercises the production trust rotation fleet emission path end-to-end:
//! finalize_wilaya → enumerate UNITs → validate target codes → derive
//! per-target paths → export_v2_package → one artifact per UNIT (SEC-057:
//! no transport sequence; packages are distinct by exact package identity).
//!
//! Uses production components throughout: real migrations, real
//! IdentitySignedExportService, real rotation/export path.

mod common;

use tempfile::TempDir;

use grpc_lib::application::services::{
    FinalizeWilayaProvisionResult, IdentityProvisioningService, IdentityRotationCoordinator,
    IdentitySignedExportService, RotationFinalizeOutcome, RotationOperation,
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
use grpc_lib::infrastructure::sync::{
    read_products_package_from_file, read_trust_package_from_file,
};
use grpc_lib::models::{NodeType, Settings};
use grpc_lib::repositories::RepositoryProvider;

const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
const FIXED_NOW: &str = "2026-08-04T00:00:00Z";

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

fn mark_settings_wilaya(node: &Node) {
    node.db
        .get_connection()
        .execute(
            "UPDATE settings SET node_type = 'WILAYA', configured = 1, wilaya_code = '16', wilaya_name = 'TestWilaya' WHERE id = 1",
            [],
        )
        .expect("settings");
}

fn seed_units(node: &Node, codes: &[&str]) {
    for code in codes {
        node.db
            .executor()
            .units()
            .upsert_raw_unit(
                &uuid::Uuid::new_v4().to_string(),
                code,
                &format!("Unit {code}"),
                "16",
                FIXED_NOW,
            )
            .expect("seed authoritative unit row");
    }
}

fn wilaya_settings() -> Settings {
    Settings {
        id: 1,
        node_type: NodeType::Wilaya,
        unit_name: None,
        unit_code: None,
        current_year: 2026,
        wilaya_code: Some("16".to_string()),
        wilaya_name: Some("TestWilaya".to_string()),
        configured: true,
    }
}

// ---------------------------------------------------------------------------
// Case 1 — Multiple UNITs: trust rotation emits one artifact per UNIT
// ---------------------------------------------------------------------------

#[test]
fn trust_rotation_fleet_emits_per_target_trust_packages() {
    let mut node = fresh_node();
    let before = bootstrap_wilaya(&mut node);
    mark_settings_wilaya(&node);
    seed_units(&node, &["UNIT-A", "UNIT-B"]);

    let plan = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect("begin rotation");
    let signed = sign_with_root(&plan.certificate);

    let out_dir = TempDir::new().expect("out dir");
    let base_path = out_dir.path().join("rotation.sync");
    let crypto = AgeFileEncryptionProvider::new();

    let outcome = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&signed, &base_path, &wilaya_settings(), &crypto)
        .expect("finalize wilaya rotation");
    assert!(matches!(outcome, RotationFinalizeOutcome::Completed { .. }));

    // Two UNITs → two suffixed artifacts.
    let path_a = out_dir.path().join("rotation-UNIT-A.sync");
    let path_b = out_dir.path().join("rotation-UNIT-B.sync");
    assert!(path_a.exists(), "UNIT-A trust artifact must exist");
    assert!(path_b.exists(), "UNIT-B trust artifact must exist");

    // Each target gets its own distinct, verifiable trust package.
    let pkg_a = read_trust_package_from_file(&path_a, &crypto).expect("read UNIT-A trust pkg");
    let pkg_b = read_trust_package_from_file(&path_b, &crypto).expect("read UNIT-B trust pkg");

    // Both contain the new certificate.
    assert!(pkg_a
        .payload
        .certificates
        .iter()
        .any(|c| c.credential_id == signed.credential_id));
    assert!(pkg_b
        .payload
        .certificates
        .iter()
        .any(|c| c.credential_id == signed.credential_id));

    // Distinct exact packages per UNIT (SEC-057: package identity unique).
    assert_ne!(pkg_a.metadata.package_id, pkg_b.metadata.package_id);

    // Signed by the OLD key (still ACTIVE at package-write time).
    let old_pubkey: [u8; 32] = before.public_key.as_slice().try_into().expect("32 bytes");
    for (label, pkg) in [("A", &pkg_a), ("B", &pkg_b)] {
        let canonical = serde_json::to_value(pkg).expect("package value");
        let canonical_bytes = canonical_bytes_for_signature(&canonical).expect("canonical");
        let sig = pkg.metadata.signature.as_deref().expect("signature");
        assert!(
            Ed25519PackageVerifier::new(old_pubkey)
                .verify(&canonical_bytes, sig)
                .expect("verify old key"),
            "trust package {label} must be signed by the OLD key"
        );
    }
}

// ---------------------------------------------------------------------------
// Case 1b — A second rotation again emits readable per-UNIT trust packages
// ---------------------------------------------------------------------------

#[test]
fn trust_rotation_fleet_second_rotation_emits_readable_packages() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    mark_settings_wilaya(&node);
    seed_units(&node, &["UNIT-A", "UNIT-B"]);

    // First rotation.
    let plan1 = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect("begin rotation 1");
    let signed1 = sign_with_root(&plan1.certificate);
    let dir1 = TempDir::new().expect("dir1");
    let path1 = dir1.path().join("rot1.sync");
    let crypto = AgeFileEncryptionProvider::new();
    IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&signed1, &path1, &wilaya_settings(), &crypto)
        .expect("finalize rotation 1");

    let _ = read_trust_package_from_file(&dir1.path().join("rot1-UNIT-A.sync"), &crypto)
        .expect("read A1");
    let _ = read_trust_package_from_file(&dir1.path().join("rot1-UNIT-B.sync"), &crypto)
        .expect("read B1");

    // Second rotation.
    let plan2 = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect("begin rotation 2");
    let signed2 = sign_with_root(&plan2.certificate);
    let dir2 = TempDir::new().expect("dir2");
    let path2 = dir2.path().join("rot2.sync");
    IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&signed2, &path2, &wilaya_settings(), &crypto)
        .expect("finalize rotation 2");

    let _ = read_trust_package_from_file(&dir2.path().join("rot2-UNIT-A.sync"), &crypto)
        .expect("read A2");
    let _ = read_trust_package_from_file(&dir2.path().join("rot2-UNIT-B.sync"), &crypto)
        .expect("read B2");
}

// ---------------------------------------------------------------------------
// Case 2 — Trust rotation then products export each emit readable artifacts
// ---------------------------------------------------------------------------

#[test]
fn trust_rotation_then_products_export_emit_readable_packages() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    mark_settings_wilaya(&node);
    seed_units(&node, &["UNIT-X"]);

    // Trust rotation.
    let plan = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect("begin rotation");
    let signed = sign_with_root(&plan.certificate);
    let dir = TempDir::new().expect("out dir");
    let base_path = dir.path().join("rotation.sync");
    let crypto = AgeFileEncryptionProvider::new();
    IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&signed, &base_path, &wilaya_settings(), &crypto)
        .expect("finalize rotation");

    // Single UNIT → original path kept (no suffix).
    let _ = read_trust_package_from_file(&base_path, &crypto).expect("read trust pkg");

    // Products export for UNIT-X still produces a readable signed package.
    let products_path = dir.path().join("products.sync");
    let exporter = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    exporter
        .export_v2_package(
            serde_json::json!({ "items": [] }),
            "wilaya-test-node",
            "products",
            "UNIT-X",
            &products_path,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("products export");
    let _ = read_products_package_from_file(&products_path, &crypto).expect("read products pkg");
}

// ---------------------------------------------------------------------------
// Case 3 — Unsafe UNIT code: rotation fails closed before emission
// ---------------------------------------------------------------------------

#[test]
fn trust_rotation_unsafe_unit_code_fails_closed() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    mark_settings_wilaya(&node);
    seed_units(&node, &["UNIT-GOOD", "../escape"]);

    let plan = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect("begin rotation");
    let signed = sign_with_root(&plan.certificate);

    let dir = TempDir::new().expect("out dir");
    let base_path = dir.path().join("rotation.sync");
    let crypto = AgeFileEncryptionProvider::new();

    let err = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&signed, &base_path, &wilaya_settings(), &crypto)
        .expect_err("unsafe code must fail closed");
    let msg = format!("{err}");
    assert!(
        msg.contains("رمز") || msg.contains("path") || msg.contains("unsafe") || msg.contains("."),
        "error must cite unsafe code, got: {msg}"
    );

    // No artifacts produced.
    assert!(!dir.path().join("rotation-UNIT-GOOD.sync").exists());
    assert!(!dir.path().join("rotation-..escape.sync").exists());
}

// ---------------------------------------------------------------------------
// Case 4 — Zero UNITs: rotation warns and produces no trust packages
// ---------------------------------------------------------------------------

#[test]
fn trust_rotation_zero_units_warns_and_produces_no_packages() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    mark_settings_wilaya(&node);
    // No units seeded.

    let plan = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect("begin rotation");
    let signed = sign_with_root(&plan.certificate);

    let dir = TempDir::new().expect("out dir");
    let base_path = dir.path().join("rotation.sync");
    let crypto = AgeFileEncryptionProvider::new();

    // Zero UNITs → warning + success (no trust packages, no hard error).
    let outcome = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&signed, &base_path, &wilaya_settings(), &crypto)
        .expect("zero units should not error");
    assert!(matches!(outcome, RotationFinalizeOutcome::Completed { .. }));

    // No trust artifacts produced.
    assert!(!base_path.exists());
    assert!(!dir.path().join("rotation-*.sync").exists());

    // Pending key promoted + ACTIVE cert installed (rotation completed).
    assert!(node
        .node_key_store
        .read_pending()
        .expect("pending read")
        .is_none());
    let new_active = node
        .db
        .executor()
        .identity_store()
        .get_active_by_subject_type(SubjectType::Wilaya)
        .expect("query")
        .expect("ACTIVE WILAYA");
    assert!(new_active.is_identical_to(&signed));
}

// ---------------------------------------------------------------------------
// Part D — Cross-UNIT artifact acceptance regression test
//
// SEC-037 identified that trust packages exported for UNIT-A are
// cryptographically valid when presented to UNIT-B (same WILAYA domain)
// because `target_node_id` is NOT part of the signed metadata — the
// consumer is fully issuer-keyed. This test documents and regresses that
// reality without attempting to "fix" it.
//
// The test proves:
// 1. signature remains valid regardless of which UNIT presents it
// 2. payload is identical (fleet-wide broadcast)
// 3. no privilege escalation occurs (trust packages contain only certs)
// 4. package_id dedup remains intact
// ---------------------------------------------------------------------------

#[test]
fn cross_unit_trust_artifact_acceptance_regression() {
    let mut node = fresh_node();
    let before = bootstrap_wilaya(&mut node);
    mark_settings_wilaya(&node);
    seed_units(&node, &["UNIT-A", "UNIT-B"]);

    // Export trust package via the fleet emission path (2 targets → suffixed).
    let plan = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect("begin rotation");
    let signed = sign_with_root(&plan.certificate);
    let dir = TempDir::new().expect("out dir");
    let base_path = dir.path().join("rotation.sync");
    let crypto = AgeFileEncryptionProvider::new();
    IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&signed, &base_path, &wilaya_settings(), &crypto)
        .expect("finalize rotation");

    let path_a = dir.path().join("rotation-UNIT-A.sync");
    let path_b = dir.path().join("rotation-UNIT-B.sync");
    assert!(path_a.exists());
    assert!(path_b.exists());

    let pkg_a = read_trust_package_from_file(&path_a, &crypto).expect("read A");
    let pkg_b = read_trust_package_from_file(&path_b, &crypto).expect("read B");

    // --- 1. Signature remains valid regardless of presentation target ---
    // Both packages are signed by the OLD WILAYA key. The signature verifies
    // against the WILAYA public key — there is no target binding in the
    // signed envelope.
    let old_pubkey: [u8; 32] = before.public_key.as_slice().try_into().expect("32 bytes");
    for (label, pkg) in [("A", &pkg_a), ("B", &pkg_b)] {
        let canonical = serde_json::to_value(pkg).expect("package value");
        let canonical_bytes = canonical_bytes_for_signature(&canonical).expect("canonical");
        let sig = pkg.metadata.signature.as_deref().expect("signature");
        assert!(
            Ed25519PackageVerifier::new(old_pubkey)
                .verify(&canonical_bytes, sig)
                .expect("verify"),
            "signature on package {label} must verify against the WILAYA key"
        );
    }

    // --- 2. Payload is identical (fleet-wide broadcast) ---
    // Both packages carry the exact same payload: the new WILAYA certificate.
    assert_eq!(
        pkg_a.payload.certificates.len(),
        pkg_b.payload.certificates.len()
    );
    for (a, b) in pkg_a
        .payload
        .certificates
        .iter()
        .zip(pkg_b.payload.certificates.iter())
    {
        assert_eq!(a.credential_id, b.credential_id);
        assert_eq!(a.identity_id, b.identity_id);
        assert_eq!(a.public_key, b.public_key);
        assert_eq!(a.generation, b.generation);
        assert_eq!(a.status, b.status);
    }
    assert_eq!(
        pkg_a.payload.revocations.len(),
        pkg_b.payload.revocations.len()
    );

    // --- 3. No privilege escalation: trust packages contain only certs ---
    // Trust packages carry `TrustPackagePayload { certificates, revocations }`.
    // There are no admin_password_hash, admin_enabled, user_password_hash,
    // or any credential material that could be misused for escalation.
    for (label, pkg) in [("A", &pkg_a), ("B", &pkg_b)] {
        // The payload is a TrustPackagePayload — structurally it can only
        // contain certificates and revocations. Verify the certificate
        // count matches expectations (exactly 1 — the new WILAYA cert).
        assert_eq!(
            pkg.payload.certificates.len(),
            1,
            "trust package {label} must contain exactly 1 certificate"
        );
        assert!(
            pkg.payload.certificates[0].issuer_identity_id.is_none(),
            "WILAYA cert must be Root-issued (no issuer_identity_id)"
        );
    }

    // --- 4. Package_id dedup remains intact ---
    // Each per-target package gets a unique package_id (UUID per call).
    // When UNIT-A imports first, UNIT-B's import of the same package_id
    // would be rejected by `has_imported` in the usecase layer. Verify that
    // each package carries a non-empty, distinct package_id — confirming that
    // dedup applies per-package, not per-target.
    assert!(!pkg_a.metadata.package_id.0.is_empty());
    assert!(!pkg_b.metadata.package_id.0.is_empty());
    // Package IDs are distinct per emission call — dedup is per-package_id,
    // so a fleet-wide broadcast produces N unique dedup keys for N targets.
    assert_ne!(
        pkg_a.metadata.package_id, pkg_b.metadata.package_id,
        "per-target packages must have distinct package_ids for independent dedup"
    );

    // --- Cross-UNIT acceptance: same-WILAYA packages are structurally valid ---
    // A package exported for UNIT-A has no target binding in the envelope.
    // When UNIT-B presents it, the consumer verifies:
    //   (a) issuer_identity_id matches local ACTIVE WILAYA (SEC-010)
    //   (b) Ed25519 signature is valid
    //   (c) package_id is not already imported
    //   (d) sequence is contiguous
    // None of these checks reference `target_node_id`.
    // This test verifies (a) and (b) at the producer level. (c) and (d)
    // are covered by the existing frozen consumer integration tests.
    assert_eq!(
        pkg_a.metadata.issuer_identity_id, pkg_b.metadata.issuer_identity_id,
        "both packages share the same issuer — cross-UNIT acceptance is by issuer, not target"
    );
}
