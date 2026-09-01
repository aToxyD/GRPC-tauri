//! F-1 Option A suite (ADR-0045 §26.9) under SEC-057 — Transport Sequence
//! removed. Successor semantics: stack `ADR-0053`/`SEC-055`/`SEC-056D`/`SEC-057`.
//!
//! RFC 2026-08-04-node-identity-trust §3.4.1 / ADR-0038.
//!
//! What is proven here:
//! - the consolidated baseline (migration 001, SEC-055, SEC-056D/SEC-057)
//!   contains NO transport-sequence machinery: the former unified producer
//!   stream `transport_export_sequence` and the frozen consumer ledger
//!   `sync_issuer_sequence` are ABSENT on a fresh install, as are the
//!   fragmented producer tables (006/009/010 — pre-ADR-0053 artifacts are
//!   void);
//! - packages are unique by exact `package_id` only: the producer emits a
//!   fresh `package_id` per export and never hand-crafts sequences;
//! - producer exports are fail-closed: a write into an unwritable target
//!   leaves nothing behind and the retry onto a valid path succeeds;
//! - `.unit` V2 bootstrap (A44-08) is a standalone signed artifact with no
//!   trace in any ledger;
//! - REAL producer path: fresh UNIT-A and UNIT-B each receive their OWN
//!   signed artifacts from the SAME WILAYA issuer, bootstrap through the real
//!   import command, and continuation works per unit;
//! - re-importing an already-applied exact package is IDEMPOTENT (the single
//!   canonical fleet admin is preserved) — there is no transport sequence to
//!   enforce replay rejection anymore (SEC-057);
//! - Ed25519 V2 signatures verify against the issuer certificate on every
//!   artifact read back from disk in the E2E path.

#[allow(dead_code)]
mod common;

use std::path::PathBuf;

use tempfile::TempDir;
use uuid::Uuid;

use grpc_lib::application::services::{
    FinalizeWilayaProvisionResult, IdentityProvisioningService, IdentitySignedExportService,
    SettingsService, SyncPackageIdentityVerificationService, UnitService, UserAccountSyncService,
};
use grpc_lib::application::usecases::sync::import_products_package::PRODUCTS_PACKAGE_KIND;
use grpc_lib::commands::{import_admin_access_package_impl, AppState};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    CredentialStatus, Ed25519CertificateSignature, IdentityCertificate, IdentitySigner,
    IdentityStorePort, SubjectType, SIGNATURE_VERSION_ED25519,
};
use grpc_lib::infrastructure::identity::NodeKeyStore;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::{Argon2PasswordHashProvider, Ed25519SigningProvider};
use grpc_lib::infrastructure::sync::read_admin_access_package_from_file;
use grpc_lib::models::{
    AdminAccessPayload, CreateUnitRequest, UnitNodePackage, UserExport, WilayaNodeConfiguration,
};
use grpc_lib::repositories::executor::DbExecutor;
use grpc_lib::repositories::RepositoryProvider;

const FIXED_NOW: &str = "2026-08-04T00:00:00Z";
const FLEET_PASSWORD: &str = "FleetPass123";
const UNIT_PASSWORD: &str = "UnitPass123";

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

/// Canonical fleet `admin` rows only — the test-session user `bob`
/// (`insert_test_user` with role=Admin) is NOT a canonical admin and must not
/// count (mirrors b8 c5's counting query).
fn count_canonical_admins(db: &Database) -> i64 {
    db.get_connection()
        .query_row(
            "SELECT COUNT(*) FROM users WHERE role = 'Admin' AND username = 'admin' AND deleted = 0",
            [],
            |row| row.get(0),
        )
        .expect("count canonical admins")
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

fn create_unit(db: &Database, code: &str) {
    let port = Argon2PasswordHashProvider;
    UnitService::new(make_executor(db), &port)
        .create_unit(
            &CreateUnitRequest {
                code: code.to_string(),
                name: format!("Unit {}", code),
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

/// SEC-029: producer-side tests mirror the production lifecycle — the
/// `create_unit` IPC command requires `settings.wilaya_code`, which only
/// `configure_wilaya` sets.
fn configure_producer_as_wilaya(db: &Database) {
    SettingsService::new(make_executor(db))
        .configure_wilaya(&WilayaNodeConfiguration::new(
            "16".into(),
            "TestWilaya".into(),
        ))
        .expect("producer configured as WILAYA");
}

/// Fleet-wide Admin-Only payload (ADR-0051): exactly
/// `{admin_password_hash, admin_enabled}` — no UNIT dimension exists.
fn admin_payload(db: &Database) -> AdminAccessPayload {
    let port = Argon2PasswordHashProvider;
    UserAccountSyncService::new(make_executor(db), &port)
        .export_admin_access()
        .expect("admin payload")
}

/// A fresh UNIT node: no users, UNIT settings, local `units` row carrying the
/// authoritative unit code (mirrors `unit_state` in b8_first_import_tests).
fn unit_state(unit_code: &str) -> AppState {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute("DELETE FROM users", [])
            .expect("clear seeded users");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'UNIT', configured = 1, unit_name = 'unit-scope-id', wilaya_code = '16', wilaya_name = NULL WHERE id = 1",
                [],
            )
            .expect("settings");
        db.get_connection()
            .execute(
                "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1, ?2, ?3, '16', ?4)",
                [Uuid::new_v4().to_string(), unit_code.to_string(), format!("Unit {}", unit_code), FIXED_NOW.to_string()],
            )
            .expect("local unit");
    }
    state
}

/// Seed the REAL WILAYA certificate as the locally installed ACTIVE anchor
/// (identity store), exactly like the offline provisioning ceremony would.
fn seed_anchor_from_cert(db: &Database, cert: &IdentityCertificate) {
    let mut anchor = cert.clone();
    anchor.issuer_identity_id = None;
    anchor.signature = None;
    anchor.status = CredentialStatus::Active;
    IdentityStorePort::upsert(
        &make_executor(db).identity_store(),
        &anchor,
        &chrono::Utc::now().to_rfc3339(),
    )
    .expect("seed anchor");
}

fn set_session(state: &AppState, role: &str) {
    let mut session = common::create_test_session("u1", "bob", role);
    session.user_role = grpc_lib::models::UserRole::from(role.to_string());
    common::insert_test_user(state, "u1", "bob", role);
    *state.current_session.lock().expect("session mutex") = Some(session);
}

// ─────────────────────────────────────────────────────────────────────────────
// Consolidated baseline audit — transport-sequence machinery absent
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn consolidated_baseline_removes_transport_sequence_tables() {
    let db = ConnectionFactory::new_for_test().expect("db");

    let version: i64 = db
        .get_connection()
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .expect("read schema version");
    assert_eq!(
        version, 1,
        "schema must be at version 1 (consolidated fresh-install baseline)"
    );

    // SEC-057: every transport-sequence object is retired from the baseline.
    let conn = db.get_connection();
    for table in [
        "transport_export_sequence",
        "sync_issuer_sequence",
        "sync_issuer_sequence_state",
        "identity_access_export_sequence",
        "admin_access_export_sequence",
    ] {
        let present: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                rusqlite::params![table],
                |r| r.get(0),
            )
            .expect("table presence");
        assert_eq!(
            present, 0,
            "{table} must be absent on a fresh install (SEC-057)"
        );
    }

    // The applied-package ledger keeps exact package_id dedup (B4) plus the
    // issuer identity, and carries NO `package_sequence` column.
    let mut stmt = conn
        .prepare("PRAGMA table_info(applied_sync_packages)")
        .expect("pragma");
    let cols: Vec<String> = stmt
        .query_map([], |row| row.get(1))
        .expect("map")
        .collect::<Result<_, _>>()
        .expect("cols");
    assert!(
        cols.contains(&"package_id".to_string()),
        "package_id is the exact dedup key"
    );
    assert!(
        cols.contains(&"issuer_identity_id".to_string()),
        "issuer_identity_id retained"
    );
    assert!(
        !cols.contains(&"package_sequence".to_string()),
        "applied_sync_packages.package_sequence must NOT exist (SEC-057)"
    );
}

#[test]
fn consolidated_baseline_creates_complete_final_schema() {
    // SEC-049/SEC-055/SEC-056D/SEC-057: a fresh database initialized from the
    // consolidated runner (migration 001 only, 004/011 folded in) must contain
    // the complete required final schema, with the retired fragmented producer
    // tables absent AND the transport-sequence tables removed, while security
    // invariants present.
    let db = ConnectionFactory::new_for_test().expect("db");
    let conn = db.get_connection();

    let version: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .expect("read schema version");
    assert_eq!(version, 1, "consolidated runner must land on version 1");

    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .expect("prepare");
    let tables: std::collections::HashSet<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("map tables")
        .collect::<Result<_, _>>()
        .expect("collect tables");

    for required in [
        "settings",
        "users",
        "units",
        "fiscal_year_status",
        "applied_fiscal_transitions",
        "fiscal_closure_package_registry",
        "products",
        "inventory_stocks",
        "opening_balance_snapshots",
        "stock_movements",
        "fifo_stock_layers",
        "inventory_layer_consumptions",
        "daily_reports",
        "daily_report_meals",
        "daily_report_meal_items",
        "supplier_orders",
        "supplier_order_items",
        "unit_monthly_snapshots",
        "monthly_reports",
        "audit_log",
        "audit_summary",
        "import_audit_events",
        "applied_sync_packages",
        "sync_conflicts",
        "import_reproducibility_metadata",
        "fiscal_export_snapshots",
        "fiscal_operational_snapshots",
        "integrity_verification_attempts",
        "operational_findings_log",
        "operational_sessions",
        "telemetry_events",
        "domain_events",
        "rate_limiter_attempts",
        "identity_store",
        "registry_snapshots",
    ] {
        assert!(
            tables.contains(required),
            "required table {required} missing from fresh consolidated schema"
        );
    }

    for retired in [
        "sync_issuer_sequence",
        "transport_export_sequence",
        "sync_issuer_sequence_state",
        "identity_access_export_sequence",
        "admin_access_export_sequence",
        "reference_price_snapshots",
        "report_generation_metadata",
    ] {
        assert!(
            !tables.contains(retired),
            "retired table {retired} must NOT exist in the fresh schema"
        );
    }

    // security / identity invariants
    let has_index = |name: &str| -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name = ?1",
            rusqlite::params![name],
            |r| r.get::<_, i64>(0),
        )
        .expect("index presence")
            == 1
    };
    assert!(has_index("idx_identity_active_subject"));
    assert!(has_index("idx_identity_single_active_admin"));

    let sig: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('identity_store') WHERE name = 'signature'",
            [],
            |r| r.get(0),
        )
        .expect("signature column presence");
    assert_eq!(sig, 1, "identity_store.signature column must exist");

    let seq: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('identity_store') WHERE name = 'package_sequence'",
            [],
            |r| r.get(0),
        )
        .expect("package_sequence column absence");
    assert_eq!(
        seq, 0,
        "identity_store.package_sequence column must NOT exist (removed in SEC-051)"
    );

    let applied_seq: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('applied_sync_packages') WHERE name = 'package_sequence'",
            [],
            |r| r.get(0),
        )
        .expect("applied package_sequence column absence");
    assert_eq!(
        applied_seq, 0,
        "applied_sync_packages.package_sequence column must NOT exist (removed in SEC-057)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Producer semantics — no sequences, only exact distinct packages
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn producer_exports_distinct_packages_across_kinds_and_targets() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    create_unit(&node.db, "UNIT-A");
    create_unit(&node.db, "UNIT-B");
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);

    // Interleaving admin/products against the same target, plus a second
    // target: each export is a DISTINCT signed package (unique package_id).
    // SEC-057 removes every transport stream — there is nothing to consume.
    service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            "UNIT-A",
            &dir.path().join("adm1.sync"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("export admin 1");
    service
        .export_v2_package(
            serde_json::json!({ "probe": "products-for-a" }),
            "wilaya-test-node",
            PRODUCTS_PACKAGE_KIND,
            "UNIT-A",
            &dir.path().join("prod2.sync"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("export products 2");
    service
        .export_v2_package(
            serde_json::json!({ "probe": "products-for-a-2" }),
            "wilaya-test-node",
            PRODUCTS_PACKAGE_KIND,
            "UNIT-A",
            &dir.path().join("prod3.sync"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("export products 3");
    service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            "UNIT-A",
            &dir.path().join("adm4.sync"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("export admin 4");
    service
        .export_v2_package(
            serde_json::json!({ "probe": "products-for-b" }),
            "wilaya-test-node",
            PRODUCTS_PACKAGE_KIND,
            "UNIT-B",
            &dir.path().join("prodb.sync"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("export products to UNIT-B");

    // The admin_access artifacts are distinct exact packages sharing the
    // issuer; their V2 signatures verify against the issuer certificate.
    let adm1 = read_admin_access_package_from_file(&dir.path().join("adm1.sync"), &crypto)
        .expect("read adm1");
    let adm4 = read_admin_access_package_from_file(&dir.path().join("adm4.sync"), &crypto)
        .expect("read adm4");
    assert_eq!(
        adm1.metadata.issuer_identity_id, adm4.metadata.issuer_identity_id,
        "same issuer across kinds and targets"
    );
    assert_ne!(
        adm1.metadata.package_id, adm4.metadata.package_id,
        "every export is a distinct exact package"
    );
    assert!(adm1.metadata.signature.is_some() && adm4.metadata.signature.is_some());
    for pkg in [&adm1, &adm4] {
        SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), pkg)
            .expect("V2 signature verifies against issuer cert");
    }
}

#[test]
fn failed_export_writes_nothing_and_retry_succeeds() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);

    // An unwritable target directory fails closed BEFORE any artifact exists.
    let bad_path = dir.path().join("missing-dir").join("p1.sync");
    assert!(
        service
            .export_v2_package(
                serde_json::json!({ "probe": "products" }),
                "wilaya-test-node",
                PRODUCTS_PACKAGE_KIND,
                "UNIT-A",
                &bad_path,
                SubjectType::Wilaya,
                &crypto,
            )
            .is_err(),
        "build into a missing directory must fail"
    );
    assert!(
        !bad_path.exists(),
        "no artifact may be written by a failed export"
    );

    // The retry onto a valid path succeeds independently of any ledger.
    let good_path = dir.path().join("p1.sync");
    service
        .export_v2_package(
            serde_json::json!({ "probe": "products" }),
            "wilaya-test-node",
            PRODUCTS_PACKAGE_KIND,
            "UNIT-A",
            &good_path,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("retry export succeeds");
    assert!(good_path.exists());
}

#[test]
fn unit_bootstrap_and_admin_export_are_distinct_artifacts() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    create_unit(&node.db, "UNIT-A");
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);

    // `.unit` V2 (A44-08) is a standalone signed artifact.
    service
        .export_v2_bootstrap_package(
            UnitNodePackage {
                unit: grpc_lib::models::Unit {
                    id: "unit-a".into(),
                    code: "UNIT-A".into(),
                    name: "Unit A".into(),
                    wilaya_code: "16".into(),
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
            },
            "wilaya-test-node",
            &dir.path().join("unit-a.unit"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("bootstrap export");

    // The next admin_access export is a distinct signed package with the
    // same issuer — no sequencing involved (SEC-057).
    service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            "UNIT-A",
            &dir.path().join("a1.sync"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("export admin 1");
    let pkg = read_admin_access_package_from_file(&dir.path().join("a1.sync"), &crypto)
        .expect("read admin package");
    assert_eq!(
        pkg.metadata.issuer_identity_id,
        Some(wilaya_cert.identity_id)
    );
    assert!(pkg.metadata.signature.is_some());
    SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), &pkg)
        .expect("V2 signature verifies");
}

// ─────────────────────────────────────────────────────────────────────────────
// REAL producer per-unit delivery E2E — the SEC-057 closer
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn real_producer_delivery_continuation_and_idempotent_reimport() {
    // SEC-057 semantics: no per-target transport streams exist. Each UNIT
    // receives its OWN signed artifact from the same WILAYA issuer; bootstrap
    // and continuation flow through the real import command; re-importing an
    // already-applied package is IDEMPOTENT (no transport sequence exists to
    // classify it as replay).

    // WILAYA fleet: UNIT-A and UNIT-B provisioned on the WILAYA.
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    create_unit(&node.db, "UNIT-A");
    create_unit(&node.db, "UNIT-B");
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    let issuer = wilaya_cert.identity_id;

    // ── Phase A — each UNIT receives its own bootstrap artifact ────────────
    let a1: PathBuf = dir.path().join("a1.sync");
    service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            "UNIT-A",
            &a1,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("real producer export UNIT-A");
    let b1: PathBuf = dir.path().join("b1.sync");
    service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            "UNIT-B",
            &b1,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("real producer export UNIT-B");

    let pkg = read_admin_access_package_from_file(&a1, &crypto).expect("read UNIT-A artifact");
    assert_eq!(pkg.metadata.issuer_identity_id, Some(issuer));
    assert_eq!(
        pkg.metadata.signature_version,
        Some(SIGNATURE_VERSION_ED25519),
        "SEC-056D: V2 Ed25519 package signature"
    );
    // Fleet-wide account payload: carries NO unit dimension (ADR-0051 §4).
    let serialized = serde_json::to_value(&pkg.payload).expect("payload value");
    assert!(
        serialized.get("unit_code").is_none() && serialized.get("user_password_hash").is_none(),
        "admin_access payload must carry no UNIT/operator material"
    );

    // The artifact verifies against the issuer certificate in the producer DB.
    SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), &pkg)
        .expect("producer-side V2 signature verifies");

    let state_a = unit_state("UNIT-A");
    seed_anchor_from_cert(
        state_a.get_db().expect("lock").as_ref().expect("db"),
        &wilaya_cert,
    );
    set_session(&state_a, "User");

    // The real import command verifies the V2 signature against the anchor.
    import_admin_access_package_impl(&state_a, a1.to_string_lossy().into_owned())
        .expect("UNIT-A bootstrap succeeds from its own artifact");
    {
        let guard = state_a.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        assert_eq!(count_canonical_admins(db), 1, "canonical Admin created");
    }

    // ── Phase B — UNIT-B bootstraps from ITS OWN artifact ──────────────────
    let state_b = unit_state("UNIT-B");
    seed_anchor_from_cert(
        state_b.get_db().expect("lock").as_ref().expect("db"),
        &wilaya_cert,
    );
    set_session(&state_b, "User");

    import_admin_access_package_impl(&state_b, b1.to_string_lossy().into_owned())
        .expect("UNIT-B bootstrap succeeds from its own artifact");
    {
        let guard = state_b.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        assert_eq!(count_canonical_admins(db), 1, "canonical Admin created");
    }

    // ── Continuation — distinct follow-up packages apply on both units ─────
    set_session(&state_a, "Admin");
    let a2: PathBuf = dir.path().join("a2.sync");
    service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            "UNIT-A",
            &a2,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("real producer export UNIT-A continuation");

    let b2: PathBuf = dir.path().join("b2.sync");
    service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            "UNIT-B",
            &b2,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("real producer export UNIT-B continuation");

    let a1_meta = read_admin_access_package_from_file(&a1, &crypto)
        .expect("a1")
        .metadata;
    let a2_meta = read_admin_access_package_from_file(&a2, &crypto)
        .expect("a2")
        .metadata;
    let b1_meta = read_admin_access_package_from_file(&b1, &crypto)
        .expect("b1")
        .metadata;
    let b2_meta = read_admin_access_package_from_file(&b2, &crypto)
        .expect("b2")
        .metadata;
    assert_ne!(
        a1_meta.package_id, a2_meta.package_id,
        "every UNIT-A export is a distinct package_id"
    );
    assert_ne!(
        b1_meta.package_id, b2_meta.package_id,
        "every UNIT-B export is a distinct package_id"
    );
    assert_eq!(a1_meta.issuer_identity_id, Some(issuer));
    assert_eq!(b2_meta.issuer_identity_id, Some(issuer));

    import_admin_access_package_impl(&state_a, a2.to_string_lossy().into_owned())
        .expect("UNIT-A continuation import succeeds (AdminOnly)");
    set_session(&state_b, "Admin");
    import_admin_access_package_impl(&state_b, b2.to_string_lossy().into_owned())
        .expect("UNIT-B applies its own continuation package independently (AdminOnly)");

    {
        let guard_a = state_a.get_db().expect("lock");
        let db_a = guard_a.as_ref().expect("db");
        let guard_b = state_b.get_db().expect("lock");
        let db_b = guard_b.as_ref().expect("db");
        assert_eq!(count_canonical_admins(db_a), 1);
        assert_eq!(count_canonical_admins(db_b), 1);
    }

    // ── Re-import — an already-applied exact package re-applies idempotently ──
    // SEC-057: admin_access has no replay ledger — the single canonical admin
    // UPSERT preserves state instead of rejecting the exact package.
    set_session(&state_a, "Admin");
    import_admin_access_package_impl(&state_a, a1.to_string_lossy().into_owned())
        .expect("re-import of the applied package is idempotent");
    set_session(&state_b, "Admin");
    import_admin_access_package_impl(&state_b, b1.to_string_lossy().into_owned())
        .expect("re-import of the applied package is idempotent");
    {
        let guard_a = state_a.get_db().expect("lock");
        let db_a = guard_a.as_ref().expect("db");
        let guard_b = state_b.get_db().expect("lock");
        let db_b = guard_b.as_ref().expect("db");
        assert_eq!(
            count_canonical_admins(db_a),
            1,
            "Admin count stays one after idempotent re-import"
        );
        assert_eq!(
            count_canonical_admins(db_b),
            1,
            "Admin count stays one after idempotent re-import"
        );
    }
}
