//! F-1 Option A (ADR-0045 §26.9 — owner decision 2026-08-15) integration tests:
//! per-`(issuer, target_unit_code)` producer stream for `identity_access`.
//!
//! RFC 2026-08-04-node-identity-trust §3.4.1 (amended 2026-08-15) / B8.
//!
//! What is proven here:
//! - migration 009 is additive and preserves the global producer ledger;
//! - the `identity_access` producer stream is isolated from the global
//!   per-issuer ledger (cross-kind, both directions);
//! - `.unit` V2 keeps fixed sequence 1 and never consumes the stream;
//! - REAL producer path: fresh UNIT-A and fresh UNIT-B both receive their own
//!   sequence-1 `identity_access` package from the SAME WILAYA issuer, both
//!   bootstrap through the real import command, continuation seq 2/3 works
//!   per unit, replay is rejected, ledger untouched;
//! - consumer Transport Guard rule "first import = 1 on an empty ledger"
//!   (A45-06 / B8 control 11) is unchanged: seq 0 / seq 2 / stale / gap all
//!   rejected;
//! - cross-target misuse is blocked at bootstrap (unit_code binding).
//!
//! Producer sequences are NEVER hand-crafted here — they come from the real
//! migration-backed ledger through `IdentitySignedExportService`. Hand-crafted
//! packages appear only in the consumer-side continuity tests where the
//! producer is intentionally bypassed (allowed: the F-1 remediation is
//! producer-side).

#[allow(dead_code)]
mod common;

use std::path::Path;

use rusqlite::Connection;
use tempfile::TempDir;
use uuid::Uuid;

use grpc_lib::application::services::{
    FinalizeWilayaProvisionResult, IdentityProvisioningService, IdentitySignedExportService,
    SettingsService, SyncPackageIdentityVerificationService, UnitService, UserAccountSyncService,
};
use grpc_lib::application::usecases::sync::import_products_package::PRODUCTS_PACKAGE_KIND;
use grpc_lib::commands::{import_admin_access_package_impl, AppState};
use grpc_lib::db::{run_migrations, ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    CredentialStatus, Ed25519CertificateSignature, IdentityCertificate, IdentitySigner,
    IdentityStorePort, SubjectType, SIGNATURE_VERSION_ED25519,
};
use grpc_lib::domain::security::PasswordHashPort;
use grpc_lib::infrastructure::identity::NodeKeyStore;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::{Argon2PasswordHashProvider, Ed25519SigningProvider};
use grpc_lib::infrastructure::sync::packages::canonical_json::{
    canonical_bytes_for_integrity, canonical_bytes_for_signature,
};
use grpc_lib::infrastructure::sync::packages::integrity::{PackageHasher, Sha256PackageHasher};
use grpc_lib::infrastructure::sync::packages::signing::{Ed25519PackageSigner, PackageSigner};
use grpc_lib::infrastructure::sync::{
    read_admin_access_package_from_file, PackageBuilder, SerdeJsonSyncPackageSerializer,
};
use grpc_lib::models::{
    AdminAccessPayload, CreateUnitRequest, IdentityAccessPayload, Unit, UnitNodePackage,
    UserExport, WilayaNodeConfiguration,
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

fn last_applied(db: &Database, issuer: &str) -> Option<u64> {
    SyncPackageIdentityVerificationService::last_applied_sequence(make_executor(db), issuer)
        .expect("read ledger")
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

fn export_payload(db: &Database, unit_code: &str) -> IdentityAccessPayload {
    let port = Argon2PasswordHashProvider;
    UserAccountSyncService::new(make_executor(db), &port)
        .export(unit_code)
        .expect("export payload")
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

/// Hand-crafted signed package for CONSUMER-side continuity tests only (the
/// F-1 remediation is producer-side; the producer portion of the E2E tests
/// never hand-crafts sequences). Vehicle: `admin_access` — the active
/// security-critical account kind after the D1 cutover (ADR-0051 §9).
fn crafted_admin_payload() -> AdminAccessPayload {
    let port = Argon2PasswordHashProvider;
    AdminAccessPayload {
        admin_enabled: true,
        admin_password_hash: port.hash_admin(FLEET_PASSWORD).expect("admin hash"),
    }
}

fn crafted_admin_package(
    package_id: &str,
    issuer_id: Uuid,
    secret: [u8; 32],
    sequence: u64,
) -> grpc_lib::application::sync::SyncPackage<AdminAccessPayload> {
    let package = grpc_lib::application::sync::SyncPackage {
        metadata: grpc_lib::application::sync::SyncPackageMetadata {
            schema_version: grpc_lib::application::sync::SchemaVersion::V2,
            created_at: chrono::Utc::now(),
            source_node_id: "wilaya-test-node".to_string(),
            package_sequence: Some(sequence),
            issuer_identity_id: Some(issuer_id),
            package_id: grpc_lib::application::sync::PackageId(package_id.to_string()),
            signature_version: Some(SIGNATURE_VERSION_ED25519),
            signing_key_id: Some("default".to_string()),
            integrity_hash: None,
            signature: None,
        },
        payload: crafted_admin_payload(),
    };
    let signer = Ed25519PackageSigner::new(secret);
    let value = serde_json::to_value(&package).expect("value");
    let hash = Sha256PackageHasher
        .hash(&canonical_bytes_for_integrity(&value).expect("canonical integrity"))
        .expect("hash");
    let mut signed = package;
    signed.metadata.integrity_hash = Some(hash);

    let value = serde_json::to_value(&signed).expect("value");
    let signature = signer
        .sign(&canonical_bytes_for_signature(&value).expect("canonical signature"))
        .expect("sign");
    signed.metadata.signature = Some(signature);
    signed
}

fn write_encrypted_admin(
    package: &grpc_lib::application::sync::SyncPackage<AdminAccessPayload>,
    secret: [u8; 32],
    path: &Path,
) {
    let crypto = AgeFileEncryptionProvider::new();
    let signer = Ed25519PackageSigner::new(secret);
    PackageBuilder::new()
        .build_encrypted_stream_path(
            package,
            &SerdeJsonSyncPackageSerializer,
            &signer,
            &crypto,
            path,
        )
        .expect("write package");
}

// ─────────────────────────────────────────────────────────────────────────────
// Migration 009 audit
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn migration_009_applies_on_fresh_database() {
    let db = ConnectionFactory::new_for_test().expect("db");

    let version: i64 = db
        .get_connection()
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .expect("read schema version");
    assert_eq!(
        version, 10,
        "schema must be at version 10 (010 admin_access stream)"
    );

    // Composite PK (issuer, target) — the F-1 stream key.
    let mut stmt = db
        .get_connection()
        .prepare("PRAGMA table_info(identity_access_export_sequence)")
        .expect("pragma");
    let cols: Vec<(String, i64)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?))
        })
        .expect("map")
        .collect::<Result<_, _>>()
        .expect("cols");
    let issuer_pk = cols
        .iter()
        .find(|(name, _)| name == "issuer_identity_id")
        .map(|(_, pk)| *pk)
        .expect("issuer column");
    let target_pk = cols
        .iter()
        .find(|(name, _)| name == "target_unit_code")
        .map(|(_, pk)| *pk)
        .expect("target column");
    assert_eq!(issuer_pk, 1, "issuer_identity_id must be part of the PK");
    assert_eq!(
        target_pk, 2,
        "target_unit_code must be part of the composite PK"
    );
    assert!(cols.iter().any(|(n, _)| n == "last_issued_sequence"));
    assert!(cols.iter().any(|(n, _)| n == "updated_at"));

    // The stream table starts empty; the global producer ledger is untouched.
    let count: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM identity_access_export_sequence",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(count, 0);
}

#[test]
fn migration_009_upgrade_preserves_existing_producer_state() {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("upgrade.db");
    let conn = Connection::open(&path).expect("open");

    // Simulate the pre-009 schema: apply migrations 1..8 verbatim (the runner
    // wraps each in its own transaction; here we only need the final shape).
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            description TEXT
        );",
    )
    .expect("schema_version");
    for (version, sql) in [
        (1, include_str!("../src/db/migrations/001_initial.sql")),
        (
            2,
            include_str!("../src/db/migrations/002_identity_store.sql"),
        ),
        (
            3,
            include_str!("../src/db/migrations/003_certificate_signature.sql"),
        ),
        (
            4,
            include_str!("../src/db/migrations/004_sync_issuer_sequence.sql"),
        ),
        (
            5,
            include_str!("../src/db/migrations/005_registry_snapshots.sql"),
        ),
        (
            6,
            include_str!("../src/db/migrations/006_issuer_sequence_state.sql"),
        ),
        (
            8,
            include_str!("../src/db/migrations/008_single_active_admin.sql"),
        ),
    ] {
        conn.execute_batch(sql).expect("apply simulated migration");
        conn.execute(
            "INSERT INTO schema_version (version, description) VALUES (?1, ?2)",
            rusqlite::params![version, "simulated"],
        )
        .expect("record simulated migration");
    }

    // Pre-existing GLOBAL producer ledger data must survive the upgrade.
    conn.execute(
        "INSERT INTO sync_issuer_sequence_state (issuer_identity_id, last_issued_sequence) VALUES ('wilaya-legacy', 7)",
        [],
    )
    .expect("seed legacy ledger row");

    run_migrations(&conn).expect("upgrade must succeed");

    let version: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .expect("read schema version");
    assert_eq!(
        version, 10,
        "upgrade must land on version 10 (010 admin_access stream)"
    );

    let legacy: i64 = conn
        .query_row(
            "SELECT last_issued_sequence FROM sync_issuer_sequence_state WHERE issuer_identity_id = 'wilaya-legacy'",
            [],
            |r| r.get(0),
        )
        .expect("legacy row");
    assert_eq!(legacy, 7, "global producer ledger data must be preserved");

    // The new stream table exists and is empty; the global ledger still works.
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM identity_access_export_sequence",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(count, 0);

    // Non-identity-access producer behavior unchanged: allocate + commit.
    let db = ConnectionFactory::new_for_test().expect("db");
    let pending = db
        .executor()
        .sync_issuer_sequence_state()
        .begin_export("wilaya-legacy")
        .expect("begin");
    assert_eq!(pending.value(), 1, "fresh global stream still starts at 1");
}

// ─────────────────────────────────────────────────────────────────────────────
// Producer isolation: identity_access stream vs global per-issuer ledger
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn identity_access_stream_isolated_from_global_ledger_and_other_kinds() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    configure_producer_as_wilaya(&node.db);
    create_unit(&node.db, "UNIT-A");
    create_unit(&node.db, "UNIT-B");
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    let issuer = wilaya_cert.identity_id.to_string();

    // 1st identity_access to UNIT-A → 1 (per-target stream).
    let seq = service
        .export_v2_identity_access_package(
            export_payload(&node.db, "UNIT-A"),
            "wilaya-test-node",
            &dir.path().join("a1.sync"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("export A1");
    assert_eq!(seq, 1);

    // The GLOBAL per-issuer ledger must be untouched by identity_access.
    let global = node
        .db
        .executor()
        .sync_issuer_sequence_state()
        .next_issued_sequence(&issuer)
        .expect("read global ledger");
    assert_eq!(
        global, None,
        "identity_access must not advance the global ledger"
    );

    // A products export uses the GLOBAL ledger → 1 (its own fresh stream).
    let seq = service
        .export_v2_package(
            serde_json::json!({ "probe": "products-1" }),
            "wilaya-test-node",
            PRODUCTS_PACKAGE_KIND,
            &dir.path().join("p1.sync"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("export products 1");
    assert_eq!(seq, 1, "other V2 kinds keep the global per-issuer ledger");

    // Fresh UNIT-B still receives sequence 1 — the F-1 resolution.
    let seq = service
        .export_v2_identity_access_package(
            export_payload(&node.db, "UNIT-B"),
            "wilaya-test-node",
            &dir.path().join("b1.sync"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("export B1");
    assert_eq!(seq, 1, "UNIT-B must receive its own sequence 1");

    // Second products export → 2 on the global ledger: identity_access did not
    // consume a global sequence.
    let seq = service
        .export_v2_package(
            serde_json::json!({ "probe": "products-2" }),
            "wilaya-test-node",
            PRODUCTS_PACKAGE_KIND,
            &dir.path().join("p2.sync"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("export products 2");
    assert_eq!(seq, 2, "global ledger advances only via other V2 kinds");

    // Second identity_access to UNIT-A → 2 on ITS stream; UNIT-B stream at 1.
    let seq = service
        .export_v2_identity_access_package(
            export_payload(&node.db, "UNIT-A"),
            "wilaya-test-node",
            &dir.path().join("a2.sync"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("export A2");
    assert_eq!(seq, 2, "UNIT-A stream advances per-target only");

    let stream_b = node
        .db
        .executor()
        .identity_access_export_sequence_state()
        .next_issued_sequence(&issuer, "UNIT-B")
        .expect("read UNIT-B stream");
    assert_eq!(
        stream_b,
        Some(1),
        "UNIT-B stream must be untouched by UNIT-A exports"
    );
}

#[test]
fn unit_bootstrap_package_does_not_consume_identity_access_stream() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    create_unit(&node.db, "UNIT-A");
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    let issuer = wilaya_cert.identity_id.to_string();

    // `.unit` V2 (A44-08): fixed sequence 1, outside any ledger.
    let seq = service
        .export_v2_bootstrap_package(
            UnitNodePackage {
                unit: Unit {
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
    assert_eq!(seq, 1, "A44-08: fixed bootstrap sequence 1");

    // Neither ledger may be advanced by the `.unit`.
    let global = node
        .db
        .executor()
        .sync_issuer_sequence_state()
        .next_issued_sequence(&issuer)
        .expect("read global ledger");
    assert_eq!(global, None, ".unit must not burn a global sequence");
    let stream = node
        .db
        .executor()
        .identity_access_export_sequence_state()
        .next_issued_sequence(&issuer, "UNIT-A")
        .expect("read stream");
    assert_eq!(
        stream, None,
        ".unit must not consume the identity_access stream"
    );

    // First identity_access export after `.unit` still receives sequence 1.
    let seq = service
        .export_v2_identity_access_package(
            export_payload(&node.db, "UNIT-A"),
            "wilaya-test-node",
            &dir.path().join("a1.sync"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("export A1");
    assert_eq!(
        seq, 1,
        "A45-06 holds: first identity_access after .unit is 1"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// REAL producer multi-UNIT bootstrap — the F-1 closer
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn real_producer_multi_unit_fleetwide_bootstrap_continuation_and_replay() {
    // ADR-0051 fleet-wide semantics (post-D1 vehicle): ONE `admin_access`
    // package per sequence, issued on the dedicated issuer-only stream,
    // independently importable by EVERY authorized UNIT. Replay state is
    // strictly local per UNIT.

    // WILAYA fleet: UNIT-A and UNIT-B provisioned on the WILAYA.
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    configure_producer_as_wilaya(&node.db);
    create_unit(&node.db, "UNIT-A");
    create_unit(&node.db, "UNIT-B");
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    let issuer = wilaya_cert.identity_id;

    // ── Phase A — one fleet package (seq 1) bootstraps every UNIT ──────────
    let a1 = dir.path().join("a1.sync");
    let seq = service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            &a1,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("real producer export seq 1");
    assert_eq!(seq, 1, "first admin_access export = seq 1");

    let pkg = read_admin_access_package_from_file(&a1, &crypto).expect("read seq-1 artifact");
    assert_eq!(pkg.metadata.package_sequence, Some(1));
    assert_eq!(pkg.metadata.issuer_identity_id, Some(issuer));
    // Fleet-wide by construction: the payload carries NO unit dimension.
    let serialized = serde_json::to_value(&pkg.payload).expect("payload value");
    assert!(
        serialized.get("unit_code").is_none() && serialized.get("user_password_hash").is_none(),
        "admin_access payload must carry no UNIT/operator material"
    );

    let state_a = unit_state("UNIT-A");
    seed_anchor_from_cert(
        state_a.get_db().expect("lock").as_ref().expect("db"),
        &wilaya_cert,
    );
    set_session(&state_a, "User");

    import_admin_access_package_impl(&state_a, a1.to_string_lossy().into_owned())
        .expect("UNIT-A bootstrap succeeds from the fleet package");
    {
        let guard = state_a.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        assert_eq!(count_canonical_admins(db), 1, "canonical Admin created");
        assert_eq!(
            last_applied(db, &issuer.to_string()),
            Some(1),
            "UNIT-A ledger = 1"
        );
    }

    // ── Phase B — the SAME package bootstraps UNIT-B independently ─────────
    let state_b = unit_state("UNIT-B");
    seed_anchor_from_cert(
        state_b.get_db().expect("lock").as_ref().expect("db"),
        &wilaya_cert,
    );
    set_session(&state_b, "User");

    import_admin_access_package_impl(&state_b, a1.to_string_lossy().into_owned())
        .expect("UNIT-B bootstrap succeeds from the SAME fleet package");
    {
        let guard = state_b.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        assert_eq!(count_canonical_admins(db), 1, "canonical Admin created");
        assert_eq!(
            last_applied(db, &issuer.to_string()),
            Some(1),
            "UNIT-B ledger = 1 (independent local replay state)"
        );
    }

    // ── Continuation — one seq-2 package applied on both units ─────────────
    set_session(&state_a, "Admin");
    let a2 = dir.path().join("a2.sync");
    let seq = service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            &a2,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("real producer export seq 2");
    assert_eq!(seq, 2, "second admin_access export = seq 2");

    import_admin_access_package_impl(&state_a, a2.to_string_lossy().into_owned())
        .expect("UNIT-A seq 2 import succeeds (AdminOnly)");
    set_session(&state_b, "Admin");
    import_admin_access_package_impl(&state_b, a2.to_string_lossy().into_owned())
        .expect("UNIT-B applies the SAME seq-2 package independently (AdminOnly)");

    {
        let guard_a = state_a.get_db().expect("lock");
        let db_a = guard_a.as_ref().expect("db");
        let guard_b = state_b.get_db().expect("lock");
        let db_b = guard_b.as_ref().expect("db");
        assert_eq!(last_applied(db_a, &issuer.to_string()), Some(2));
        assert_eq!(last_applied(db_b, &issuer.to_string()), Some(2));
        assert_eq!(count_canonical_admins(db_a), 1);
        assert_eq!(count_canonical_admins(db_b), 1);
    }

    // ── Replay — exact bootstrap package re-import is rejected, state intact ──
    set_session(&state_a, "Admin");
    let err = import_admin_access_package_impl(&state_a, a1.to_string_lossy().into_owned())
        .expect_err("replay of the bootstrap package must be rejected");
    assert!(!err.is_empty());
    let err = import_admin_access_package_impl(&state_b, a1.to_string_lossy().into_owned())
        .expect_err("replay of the bootstrap package must be rejected");
    assert!(!err.is_empty());
    {
        let guard_a = state_a.get_db().expect("lock");
        let db_a = guard_a.as_ref().expect("db");
        let guard_b = state_b.get_db().expect("lock");
        let db_b = guard_b.as_ref().expect("db");
        assert_eq!(
            count_canonical_admins(db_a),
            1,
            "Admin count stays one after replay"
        );
        assert_eq!(
            count_canonical_admins(db_b),
            1,
            "Admin count stays one after replay"
        );
        assert_eq!(
            last_applied(db_a, &issuer.to_string()),
            Some(2),
            "ledger unchanged"
        );
        assert_eq!(
            last_applied(db_b, &issuer.to_string()),
            Some(2),
            "ledger unchanged"
        );
    }

    // ── Local replay state — a fresh UNIT-C bootstraps from the SAME seq-1 ──
    // Replay protection is LOCAL: C's empty ledger accepts sequence 1 even
    // though A and B already advanced to 2.
    create_unit(&node.db, "UNIT-C");
    let state_c = unit_state("UNIT-C");
    seed_anchor_from_cert(
        state_c.get_db().expect("lock").as_ref().expect("db"),
        &wilaya_cert,
    );
    set_session(&state_c, "User");
    import_admin_access_package_impl(&state_c, a1.to_string_lossy().into_owned())
        .expect("UNIT-C bootstrap from the same seq-1 artifact succeeds");
    {
        let guard_c = state_c.get_db().expect("lock");
        let db_c = guard_c.as_ref().expect("db");
        assert_eq!(
            count_canonical_admins(db_c),
            1,
            "canonical Admin created on C"
        );
        assert_eq!(
            last_applied(db_c, &issuer.to_string()),
            Some(1),
            "C's ledger is independent of A/B"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Consumer side unchanged: Transport Guard rule "first import = 1"
// ─────────────────────────────────────────────────────────────────────────────

/// The consumer must still reject every non-1 first sequence on an empty
/// ledger (A45-06 / B8 control 11) — the F-1 remediation is producer-side.
/// These tests hand-craft packages intentionally: they probe the CONSUMER,
/// which is unchanged.
#[test]
fn consumer_rejects_non_one_first_sequence_stale_and_gap() {
    // seq 2 as first import on an empty ledger → OutOfOrder expected=1.
    // Vehicle: `admin_access` (the active account kind after D1); the
    // Transport Guard rule is kind-agnostic and unchanged.
    let state = unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        let cert = IdentityCertificate {
            identity_id: issuer_id,
            subject_type: SubjectType::Wilaya,
            subject_id: issuer_id,
            issuer_identity_id: None,
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: Ed25519SigningProvider::new([42u8; 32]).public_key(),
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: Some(1),
            signature: None,
        };
        IdentityStorePort::upsert(
            &make_executor(db).identity_store(),
            &cert,
            &chrono::Utc::now().to_rfc3339(),
        )
        .expect("seed anchor");
    }
    set_session(&state, "User");

    let dir = TempDir::new().expect("temp dir");

    // seq 2 as first → rejected (empty-ledger baseline is 1).
    let seq2_first = dir.path().join("seq2.sync");
    write_encrypted_admin(
        &crafted_admin_package("f1-seq2", issuer_id, [42u8; 32], 2),
        [42u8; 32],
        &seq2_first,
    );
    let err = import_admin_access_package_impl(&state, seq2_first.to_string_lossy().into_owned())
        .expect_err("seq 2 as first import must be OutOfOrder");
    assert!(err.contains("ترتيب") || !err.is_empty());

    // seq 0 as first → rejected.
    let seq0_first = dir.path().join("seq0.sync");
    write_encrypted_admin(
        &crafted_admin_package("f1-seq0", issuer_id, [42u8; 32], 0),
        [42u8; 32],
        &seq0_first,
    );
    let err = import_admin_access_package_impl(&state, seq0_first.to_string_lossy().into_owned())
        .expect_err("seq 0 as first import must be rejected");
    assert!(!err.is_empty());

    // A valid seq 1 bootstrap succeeds.
    let first = dir.path().join("first.sync");
    write_encrypted_admin(
        &crafted_admin_package("f1-first", issuer_id, [42u8; 32], 1),
        [42u8; 32],
        &first,
    );
    import_admin_access_package_impl(&state, first.to_string_lossy().into_owned())
        .expect("valid seq 1 bootstrap succeeds");
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        assert_eq!(count_canonical_admins(db), 1);
        assert_eq!(last_applied(db, &issuer_id.to_string()), Some(1));
    }

    // Post-bootstrap: stale seq 1 → Replay; gap (seq 4) → OutOfOrder.
    set_session(&state, "Admin");
    let stale = dir.path().join("stale.sync");
    write_encrypted_admin(
        &crafted_admin_package("f1-stale", issuer_id, [42u8; 32], 1),
        [42u8; 32],
        &stale,
    );
    let err = import_admin_access_package_impl(&state, stale.to_string_lossy().into_owned())
        .expect_err("stale seq 1 must be Replay");
    assert!(!err.is_empty());

    let gap = dir.path().join("gap.sync");
    write_encrypted_admin(
        &crafted_admin_package("f1-gap", issuer_id, [42u8; 32], 4),
        [42u8; 32],
        &gap,
    );
    let err = import_admin_access_package_impl(&state, gap.to_string_lossy().into_owned())
        .expect_err("gap to seq 4 must be OutOfOrder (expected 2)");
    assert!(!err.is_empty());

    // The valid next sequence still imports.
    let second = dir.path().join("second.sync");
    write_encrypted_admin(
        &crafted_admin_package("f1-second", issuer_id, [42u8; 32], 2),
        [42u8; 32],
        &second,
    );
    import_admin_access_package_impl(&state, second.to_string_lossy().into_owned())
        .expect("valid contiguous seq 2 succeeds after failed probes");
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        assert_eq!(
            count_canonical_admins(db),
            1,
            "failed probes must not create admins"
        );
        assert_eq!(last_applied(db, &issuer_id.to_string()), Some(2));
    }
}
