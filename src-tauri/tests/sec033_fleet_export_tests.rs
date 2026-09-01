//! SEC-033 — fleet-level WILAYA exports (`export_products_fleet` /
//! `export_admin_access_fleet`).
//!
//! Proves that the backend — never a renderer selection — enumerates the
//! authoritative UNIT target set and emits ONE signed V2 artifact per target:
//! - multi-target emission suffixes `-<unit_code>` artifacts (trust-rotation
//!   naming convention); single-target keeps the operator-requested path;
//! - every target gets its own distinct, verifiable package (SEC-057: no
//!   transport sequence; package identity + business-key correctness);
//! - zero registered UNITs fail closed BEFORE any artifact is written;
//! - an unsafe authoritative code aborts enumeration before any emission.
//!
//! The consumer side is frozen and covered by `sec031_unified_stream_tests`.

#[allow(dead_code)]
mod common;

use std::path::Path;

use tempfile::TempDir;

use grpc_lib::application::services::{
    export_admin_access_fleet, export_products_fleet, FinalizeWilayaProvisionResult,
    IdentityProvisioningService, UserAccountSyncService,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{Ed25519CertificateSignature, IdentitySigner, SubjectType};
use grpc_lib::infrastructure::identity::NodeKeyStore;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::{Argon2PasswordHashProvider, Ed25519SigningProvider};
use grpc_lib::infrastructure::sync::{
    read_admin_access_package_from_file, read_products_package_from_file,
};
use grpc_lib::repositories::RepositoryProvider;

/// RFC 8032 §7.1 TEST 1 secret — matches the debug-mode development Root
/// fallback (root_public_key.rs). Never a production key.
const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
const FIXED_NOW: &str = "2026-08-04T00:00:00Z";
const FLEET_PASSWORD: &str = "FleetAdminPw2026";

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

/// Full offline WILAYA bootstrap (Root-issued), as in
/// `sync_v2_producer_export_tests`.
fn bootstrap_wilaya(node: &mut Node) {
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
        FinalizeWilayaProvisionResult::Provisioned(_) => {}
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

fn products_fleet(node: &mut Node, crypto: &AgeFileEncryptionProvider, base: &Path) {
    export_products_fleet(
        &mut node.db,
        &node.node_key_store,
        crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        base,
    )
    .expect("fleet products export succeeds");
}

fn admin_fleet(node: &mut Node, crypto: &AgeFileEncryptionProvider, base: &Path) {
    export_admin_access_fleet(
        &mut node.db,
        &node.node_key_store,
        &Argon2PasswordHashProvider,
        crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        base,
    )
    .expect("fleet admin_access export succeeds");
}

// ---------------------------------------------------------------------------
// Products: one artifact per authoritative UNIT, per-target streams
// ---------------------------------------------------------------------------

#[test]
fn products_fleet_emits_one_suffixed_artifact_per_registered_unit() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    mark_settings_wilaya(&node);
    seed_units(&node, &["U-1", "U-2"]);

    let out_dir = TempDir::new().expect("out dir");
    let base = out_dir.path().join("catalog.sync");

    let crypto = AgeFileEncryptionProvider::new();
    let outcome = {
        // Re-bind through the same helper used by tests below so assertions
        // inspect the outcome struct itself.
        export_products_fleet(
            &mut node.db,
            &node.node_key_store,
            &crypto,
            "wilaya-test-node",
            SubjectType::Wilaya,
            &base,
        )
        .expect("fleet export")
    };
    assert_eq!(outcome.targets.len(), 2);
    assert_eq!(outcome.artifact_paths.len(), 2);

    // Multi-target naming: `-<unit_code>` siblings; the requested base path
    // must NOT be created.
    assert!(!base.exists(), "requested base path must stay untouched");
    for path in &outcome.artifact_paths {
        assert!(path.exists(), "missing artifact {}", path.display());
    }
    let names: Vec<String> = outcome
        .artifact_paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert!(names.contains(&"catalog-U-1.sync".to_string()), "{names:?}");
    assert!(names.contains(&"catalog-U-2.sync".to_string()), "{names:?}");

    // Each target gets its OWN distinct, verifiable package (SEC-057):
    // distinct package_id, but a shared canonical issuer identity.
    let pkg1 =
        read_products_package_from_file(&outcome.artifact_paths[0], &crypto).expect("read U pkg");
    let pkg2 =
        read_products_package_from_file(&outcome.artifact_paths[1], &crypto).expect("read U pkg");
    assert_eq!(
        pkg1.metadata.issuer_identity_id, pkg2.metadata.issuer_identity_id,
        "same issuer across targets"
    );
    assert_ne!(
        pkg1.metadata.package_id, pkg2.metadata.package_id,
        "distinct packages per target"
    );
}

#[test]
fn products_fleet_re_export_produces_readable_per_target_artifacts() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    mark_settings_wilaya(&node);
    seed_units(&node, &["U-A", "U-B"]);

    let out_dir = TempDir::new().expect("out dir");
    let base = out_dir.path().join("catalog.sync");
    let crypto = AgeFileEncryptionProvider::new();

    products_fleet(&mut node, &crypto, &base);
    products_fleet(&mut node, &crypto, &base);

    let p_a1 = out_dir.path().join("catalog-U-A.sync");
    let p_b1 = out_dir.path().join("catalog-U-B.sync");
    let second_a = read_products_package_from_file(&p_a1, &crypto).expect("read back U-A stream");
    let second_b = read_products_package_from_file(&p_b1, &crypto).expect("read back U-B stream");
    assert_ne!(second_a.metadata.package_id, second_b.metadata.package_id);
}

#[test]
fn products_fleet_single_target_keeps_requested_path() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    mark_settings_wilaya(&node);
    seed_units(&node, &["SOLO-9"]);

    let out_dir = TempDir::new().expect("out dir");
    let base = out_dir.path().join("catalog.sync");
    let crypto = AgeFileEncryptionProvider::new();

    products_fleet(&mut node, &crypto, &base);

    // Single-target emission keeps the exact operator-requested path.
    assert!(base.exists(), "single target uses the requested path as-is");
    let _ = read_products_package_from_file(&base, &crypto).expect("read back");
}

#[test]
fn products_fleet_zero_registered_units_fail_closed_without_artifacts() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    mark_settings_wilaya(&node);
    // No units seeded.

    let out_dir = TempDir::new().expect("out dir");
    let base = out_dir.path().join("catalog.sync");
    let crypto = AgeFileEncryptionProvider::new();

    let err = export_products_fleet(
        &mut node.db,
        &node.node_key_store,
        &crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        &base,
    )
    .expect_err("zero targets must fail closed");
    assert!(!err.to_string().is_empty());
    assert!(!base.exists());

    // Nothing was written by the failed export: a later export with a
    // registered unit succeeds and produces a readable artifact.
    seed_units(&node, &["U-LATE"]);
    let late = out_dir.path().join("late.sync");
    products_fleet(&mut node, &crypto, &late);
    // Single registered target keeps the requested path.
    let _ = read_products_package_from_file(&out_dir.path().join("late.sync"), &crypto)
        .expect("read back");
}

#[test]
fn products_fleet_unsafe_authoritative_code_aborts_before_emission() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    mark_settings_wilaya(&node);
    seed_units(&node, &["U-SAFE"]);
    seed_units(&node, &["../escape"]);

    let out_dir = TempDir::new().expect("out dir");
    let base = out_dir.path().join("catalog.sync");
    let crypto = AgeFileEncryptionProvider::new();

    let err = export_products_fleet(
        &mut node.db,
        &node.node_key_store,
        &crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        &base,
    );
    assert!(err.is_err(), "unsafe authoritative code must fail closed");
    assert!(
        !out_dir.path().join("catalog-U-SAFE.sync").exists(),
        "no artifact may be written once enumeration fails"
    );
}

// ---------------------------------------------------------------------------
// Admin access: fleet payload, one distinct package per target
// ---------------------------------------------------------------------------

#[test]
fn admin_access_fleet_emits_per_target_distinct_packages() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    mark_settings_wilaya(&node);
    seed_units(&node, &["UNIT-9", "UNIT-A"]);

    // Fleet credential must be set or the exporter fails closed.
    UserAccountSyncService::new(node.db.executor(), &Argon2PasswordHashProvider)
        .set_fleet_admin_password(FLEET_PASSWORD)
        .expect("fleet password set");

    let out_dir = TempDir::new().expect("out dir");
    let base = out_dir.path().join("grpc-admin-access.sync");
    let crypto = AgeFileEncryptionProvider::new();

    admin_fleet(&mut node, &crypto, &base);

    let p9 = out_dir.path().join("grpc-admin-access-UNIT-9.sync");
    let pa = out_dir.path().join("grpc-admin-access-UNIT-A.sync");
    let pkg9 = read_admin_access_package_from_file(&p9, &crypto).expect("read UNIT-9 pkg");
    let pkga = read_admin_access_package_from_file(&pa, &crypto).expect("read UNIT-A pkg");

    assert_ne!(pkg9.metadata.package_id, pkga.metadata.package_id);

    // A second fleet export still produces readable per-target artifacts.
    admin_fleet(&mut node, &crypto, &base);
    let _pkg9_again = read_admin_access_package_from_file(&p9, &crypto).expect("read UNIT-9 again");
    let _pkga_again = read_admin_access_package_from_file(&pa, &crypto).expect("read UNIT-A again");
}

#[test]
fn admin_access_fleet_single_target_keeps_requested_path() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    mark_settings_wilaya(&node);
    seed_units(&node, &["UNIT-ONLY"]);

    UserAccountSyncService::new(node.db.executor(), &Argon2PasswordHashProvider)
        .set_fleet_admin_password(FLEET_PASSWORD)
        .expect("fleet password set");

    let out_dir = TempDir::new().expect("out dir");
    let base = out_dir.path().join("grpc-admin-access.sync");
    let crypto = AgeFileEncryptionProvider::new();

    admin_fleet(&mut node, &crypto, &base);
    assert!(base.exists());
    let _ = read_admin_access_package_from_file(&base, &crypto).expect("read back");
}
