//! SEC-031 / SEC-032 — canonical regression suite for the consolidated sync
//! baseline under SEC-057 (Transport Sequence removed).
//!
//! Stack: `SEC-055` (consolidated migration 001) / `SEC-056D` (V2 Ed25519
//! identity-bound signing) / `SEC-057` (transport sequences retired).
//!
//! What is proven here:
//!
//! - the consolidated baseline contains NO transport-sequence machinery:
//!   the former unified producer stream `transport_export_sequence`, the
//!   frozen consumer ledger `sync_issuer_sequence`, and the fragmented
//!   006/009/010 producer tables are ABSENT on a fresh install;
//! - `applied_sync_packages` keeps the issuer identity + exact `package_id`
//!   dedup and carries NO `package_sequence` column;
//! - producer semantics: every export emits a DISTINCT signed V2 package
//!   (unique `package_id`) — cross-kind and cross-target exports are order
//!   independent, and a failed export writes nothing (fail-closed);
//! - `.unit` bootstrap is a standalone signed artifact with no trace in any
//!   ledger;
//! - repository defense in depth: re-recording an already-applied exact
//!   `package_id` is refused (the only remaining replay guard, SEC-057).

#[allow(dead_code)]
mod common;

use tempfile::TempDir;
use uuid::Uuid;

use grpc_lib::application::services::{
    IdentityProvisioningService, IdentitySignedExportService, SettingsService,
    SyncPackageIdentityVerificationService, UserAccountSyncService,
};
use grpc_lib::application::usecases::exports::types::ProductsExportDataset;
use grpc_lib::db::{run_migrations, ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    Ed25519CertificateSignature, IdentityCertificate, IdentitySigner, SubjectType,
    SIGNATURE_VERSION_ED25519,
};
use grpc_lib::infrastructure::identity::NodeKeyStore;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::{Argon2PasswordHashProvider, Ed25519SigningProvider};
use grpc_lib::infrastructure::sync::{
    read_admin_access_package_from_file, read_products_package_from_file,
};
use grpc_lib::models::{AdminAccessPayload, WilayaNodeConfiguration};
use grpc_lib::repositories::executor::DbExecutor;
use grpc_lib::repositories::SyncAppliedPackagesRepository;

const FIXED_NOW: &str = "2026-08-04T00:00:00Z";
const FLEET_PASSWORD: &str = "FleetPass123";

/// RFC 8032 §7.1 TEST 1 secret — matches the debug-mode Root fallback.
const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn make_executor(db: &Database) -> DbExecutor<'_> {
    db.executor()
}

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
        grpc_lib::application::services::FinalizeWilayaProvisionResult::Provisioned(cert) => cert,
        _ => panic!("first finalize must provision"),
    }
}

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

fn admin_payload(db: &Database) -> AdminAccessPayload {
    let port = Argon2PasswordHashProvider;
    UserAccountSyncService::new(make_executor(db), &port)
        .export_admin_access()
        .expect("admin payload")
}

// ─────────────────────────────────────────────────────────────────────────────
// Consolidated final-schema baseline — transport-sequence machinery absent
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn consolidated_baseline_removes_transport_sequence_tables() {
    let db = ConnectionFactory::new_for_test().expect("db");
    run_migrations(db.get_connection()).expect("migrations");

    let conn = db.get_connection();
    let version: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .expect("schema version");
    assert_eq!(
        version, 1,
        "the consolidated baseline is migration 1 only (004/011 folded in)"
    );

    // SEC-057 retires EVERY transport-sequence object from the baseline.
    for retired in [
        "transport_export_sequence",
        "sync_issuer_sequence",
        "sync_issuer_sequence_state",
        "identity_access_export_sequence",
        "admin_access_export_sequence",
    ] {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [retired],
                |r| r.get(0),
            )
            .expect("retired table probe");
        assert_eq!(
            n, 0,
            "{retired} must not exist on a fresh install (SEC-057)"
        );
    }

    // The applied-package ledger keeps exact package_id dedup (B4) plus the
    // issuer identity — the transport `package_sequence` column is gone.
    let app_cols: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('applied_sync_packages')")
        .expect("pragma")
        .query_map([], |r| r.get(0))
        .expect("map")
        .collect::<Result<_, _>>()
        .expect("cols");
    assert_eq!(
        app_cols.iter().filter(|c| *c == "package_sequence").count(),
        0,
        "applied_sync_packages.package_sequence must NOT exist (SEC-057)"
    );
    assert_eq!(
        app_cols
            .iter()
            .filter(|c| *c == "issuer_identity_id")
            .count(),
        1,
        "issuer_identity_id retained"
    );
    assert_eq!(
        app_cols.iter().filter(|c| *c == "package_id").count(),
        1,
        "package_id is the exact dedup key"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Producer semantics — real service, distinct exact packages
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fresh_unit_cross_kind_exports_admin_then_products() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    let issuer = wilaya_cert.identity_id;

    // admin_access first, then products — order has no sequence to occupy.
    let p1 = dir.path().join("a1.sync");
    service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            "UNIT-A",
            &p1,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("admin first");
    let p2 = dir.path().join("a2.sync");
    service
        .export_v2_package(
            ProductsExportDataset {
                product_rows: vec![],
            },
            "wilaya-test-node",
            "products",
            None,
            None,
            &p2,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("products second");

    // Both artifacts are distinct, issuer-bound, V2 signed packages.
    let admin = read_admin_access_package_from_file(&p1, &crypto).expect("read admin");
    let products = read_products_package_from_file(&p2, &crypto).expect("read products");
    assert_eq!(admin.metadata.issuer_identity_id, Some(issuer));
    assert_eq!(products.metadata.issuer_identity_id, Some(issuer));
    assert_eq!(
        admin.metadata.signature_version,
        Some(SIGNATURE_VERSION_ED25519)
    );
    assert_eq!(
        products.metadata.signature_version,
        Some(SIGNATURE_VERSION_ED25519)
    );
    assert_ne!(
        admin.metadata.package_id, products.metadata.package_id,
        "cross-kind exports are distinct packages"
    );
    SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), &admin)
        .expect("admin V2 signature verifies against issuer cert");
    SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), &products)
        .expect("products V2 signature verifies against issuer cert");
}

#[test]
fn fresh_unit_cross_kind_exports_products_then_admin() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    let issuer = wilaya_cert.identity_id;

    // products first, then admin_access — the reverse order also succeeds
    // (order independence; there is no stream to keep contiguous).
    let p1 = dir.path().join("b1.sync");
    service
        .export_v2_package(
            ProductsExportDataset {
                product_rows: vec![],
            },
            "wilaya-test-node",
            "products",
            None,
            None,
            &p1,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("products first");
    let p2 = dir.path().join("b2.sync");
    service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            "UNIT-B",
            &p2,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("admin second");

    let products = read_products_package_from_file(&p1, &crypto).expect("read products");
    let admin = read_admin_access_package_from_file(&p2, &crypto).expect("read admin");
    assert_eq!(products.metadata.issuer_identity_id, Some(issuer));
    assert_eq!(admin.metadata.issuer_identity_id, Some(issuer));
    assert_ne!(
        products.metadata.package_id, admin.metadata.package_id,
        "cross-kind exports are distinct packages"
    );
    SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), &products)
        .expect("products V2 signature verifies against issuer cert");
    SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), &admin)
        .expect("admin V2 signature verifies against issuer cert");
}

#[test]
fn interleaved_kinds_and_targets_are_exact_distinct_packages() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    let issuer = wilaya_cert.identity_id;

    let export_admin = |target: &str, path: &std::path::Path| {
        service
            .export_v2_admin_access_package(
                admin_payload(&node.db),
                "wilaya-test-node",
                target,
                path,
                SubjectType::Wilaya,
                &crypto,
            )
            .expect("admin export")
    };
    let export_products = |_target: &str, path: &std::path::Path| {
        service
            .export_v2_package(
                ProductsExportDataset {
                    product_rows: vec![],
                },
                "wilaya-test-node",
                "products",
                None,
                None,
                path,
                SubjectType::Wilaya,
                &crypto,
            )
            .expect("products export")
    };

    let dir_ref = dir.path();
    // Interleaved kinds against UNIT-A, plus a second target UNIT-C.
    export_admin("UNIT-A", &dir_ref.join("i1.sync"));
    export_products("UNIT-A", &dir_ref.join("i2.sync"));
    export_admin("UNIT-A", &dir_ref.join("i3.sync"));
    export_products("UNIT-A", &dir_ref.join("i4.sync"));
    export_products("UNIT-C", &dir_ref.join("c1.sync"));

    // Every artifact is a unique exact-package sharing the same issuer.
    let i1 = read_admin_access_package_from_file(&dir_ref.join("i1.sync"), &crypto).expect("i1");
    let i2 = read_products_package_from_file(&dir_ref.join("i2.sync"), &crypto).expect("i2");
    let i3 = read_admin_access_package_from_file(&dir_ref.join("i3.sync"), &crypto).expect("i3");
    let i4 = read_products_package_from_file(&dir_ref.join("i4.sync"), &crypto).expect("i4");
    let c1 = read_products_package_from_file(&dir_ref.join("c1.sync"), &crypto).expect("c1");

    let mut ids: Vec<&str> = vec![
        i1.metadata.package_id.0.as_str(),
        i2.metadata.package_id.0.as_str(),
        i3.metadata.package_id.0.as_str(),
        i4.metadata.package_id.0.as_str(),
        c1.metadata.package_id.0.as_str(),
    ];
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        ids.len(),
        5,
        "all five exports are distinct exact packages (kind- and target-blind)"
    );
    for meta in [
        &i1.metadata,
        &i2.metadata,
        &i3.metadata,
        &i4.metadata,
        &c1.metadata,
    ] {
        assert_eq!(meta.issuer_identity_id, Some(issuer));
        assert_eq!(meta.signature_version, Some(SIGNATURE_VERSION_ED25519));
    }
    SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), &i1)
        .expect("i1 V2 signature verifies");
    SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), &i2)
        .expect("i2 V2 signature verifies");
    SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), &i3)
        .expect("i3 V2 signature verifies");
    SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), &i4)
        .expect("i4 V2 signature verifies");
    SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), &c1)
        .expect("c1 V2 signature verifies");
}

#[test]
fn failed_export_writes_nothing_and_retry_succeeds() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);

    // Unwritable directory → fail-closed, no artifact written.
    let bad = dir.path().join("missing").join("f1.sync");
    assert!(
        service
            .export_v2_package(
                ProductsExportDataset {
                    product_rows: vec![]
                },
                "wilaya-test-node",
                "products",
                None,
                None,
                &bad,
                SubjectType::Wilaya,
                &crypto,
            )
            .is_err(),
        "unwritable path must fail"
    );
    assert!(!bad.exists(), "a failed export must write nothing");

    // The retry onto a valid path succeeds — no sequence to burn or reuse.
    let good = dir.path().join("f1.sync");
    service
        .export_v2_package(
            ProductsExportDataset {
                product_rows: vec![],
            },
            "wilaya-test-node",
            "products",
            None,
            None,
            &good,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("retry succeeds");
    let pkg = read_products_package_from_file(&good, &crypto).expect("read retry artifact");
    assert_eq!(
        pkg.metadata.issuer_identity_id,
        Some(wilaya_cert.identity_id)
    );
    assert!(pkg.metadata.signature.is_some());
}

#[test]
fn unit_bootstrap_is_a_standalone_signed_artifact() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    set_fleet_password(&node.db);

    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    let crypto = AgeFileEncryptionProvider::new();

    // `.unit` V2 bootstrap: a standalone signed artifact with no ledger trace.
    let unit_pkg_path = dir.path().join("bootstrap.unit");
    service
        .export_v2_bootstrap_package(
            serde_json::json!({
                "unit_code": "UNIT-A",
                "unit_name": "Unit A",
                "operator_username": "op",
            }),
            "wilaya-test-node",
            &unit_pkg_path,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect(".unit export");
    assert!(unit_pkg_path.exists());

    // The next pipeline kind on that target is still a distinct signed package.
    let admin_path = dir.path().join("after-unit.sync");
    service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            "UNIT-A",
            &admin_path,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("admin after .unit");
    let admin = read_admin_access_package_from_file(&admin_path, &crypto).expect("read admin");
    assert_eq!(
        admin.metadata.issuer_identity_id,
        Some(wilaya_cert.identity_id)
    );
    assert!(admin.metadata.signature.is_some());
    SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), &admin)
        .expect("V2 signature verifies against issuer cert");
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository-level defense in depth — exact package_id replay guard
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn applied_package_ledger_refuses_exact_package_replay() {
    // SEC-057: the exact `package_id` is the ONLY dedup key — no transport
    // sequence is recorded or compared. Re-recording an applied package_id is
    // refused fail-closed; a fresh package_id is accepted.
    let db = ConnectionFactory::new_for_test().expect("db");
    run_migrations(db.get_connection()).expect("migrations");
    let repo = SyncAppliedPackagesRepository::new(make_executor(&db));

    assert!(
        repo.insert_if_new("pkg-1", "products", Some("w-1"), "admin", Some("iss-1"))
            .expect("first insert"),
        "first recording of a package_id must succeed"
    );
    assert!(repo.has_imported("pkg-1").expect("has pkg-1"));
    assert!(
        !repo
            .insert_if_new("pkg-1", "products", Some("w-1"), "admin", Some("iss-1"))
            .expect("exact replay probe"),
        "exact package_id re-import must be refused"
    );
    assert!(
        repo.insert_if_new("pkg-2", "products", Some("w-1"), "admin", Some("iss-1"))
            .expect("fresh package id"),
        "a fresh package_id is accepted"
    );
    assert!(repo.has_imported("pkg-2").expect("has pkg-2"));
}
