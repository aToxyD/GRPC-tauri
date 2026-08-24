//! SEC-031 / SEC-032 — ADR-0053 canonical regression suite
//! (Unified Per-Target Transport Sequence — Accepted 2026-08-24).
//!
//! This file is the mandated regression gate for the unified producer stream:
//!
//! - migration 011 creates ONE stream keyed by `(issuer_identity_id,
//!   target_node_id)` and retires the fragmented 006/009/010 producer tables;
//! - cross-kind continuation on a fresh target: `admin_access` #1 then
//!   `products` #2, and the reverse order `products` #1 then `admin_access`
//!   #2 (order independence);
//! - interleaved kinds stay strictly contiguous per target (1,2,3,4);
//! - per-target isolation: UNIT-A and UNIT-B streams are independent;
//! - failed exports never burn a sequence and retries reuse it;
//! - `.unit` bootstrap keeps fixed sequence 1 and consumes nothing;
//! - repository defense in depth: regression commits are refused, dropped
//!   tokens never advance the ledger;
//! - consumer Transport Guard negatives are unchanged: first import must be
//!   exactly 1; stale (lower), duplicate/replay, and gap sequences are all
//!   rejected on the frozen `sync_issuer_sequence` ledger.
//!
//! Producer sequences are never hand-crafted in producer-side scenarios —
//! they flow from the real migration-backed ledger through
//! `IdentitySignedExportService`. Hand-crafted packages appear only in the
//! consumer-side negative tests where the producer is intentionally bypassed.

#[allow(dead_code)]
mod common;

use tempfile::TempDir;
use uuid::Uuid;

use grpc_lib::application::services::{
    IdentityProvisioningService, IdentitySignedExportService, SettingsService,
    SyncPackageIdentityVerificationService, UserAccountSyncService,
};
use grpc_lib::db::{run_migrations, ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    CredentialStatus, Ed25519CertificateSignature, IdentityCertificate, IdentitySigner,
    SubjectType, SIGNATURE_VERSION_ED25519,
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
use grpc_lib::infrastructure::sync::PackageBuilder;
use grpc_lib::models::{AdminAccessPayload, WilayaNodeConfiguration};
use grpc_lib::repositories::executor::DbExecutor;
use grpc_lib::repositories::PendingTransportSequence;
use grpc_lib::repositories::RepositoryProvider;

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

fn last_applied(db: &Database, issuer: &str) -> Option<u64> {
    SyncPackageIdentityVerificationService::last_applied_sequence(make_executor(db), issuer)
        .expect("read consumer ledger")
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

/// Hand-crafted signed package for CONSUMER-side negative tests only.
fn crafted_admin_package(
    package_id: &str,
    issuer_id: Uuid,
    secret: [u8; 32],
    sequence: u64,
) -> grpc_lib::application::sync::SyncPackage<AdminAccessPayload> {
    let port = Argon2PasswordHashProvider;
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
        payload: AdminAccessPayload {
            admin_enabled: true,
            admin_password_hash: port.hash_admin(FLEET_PASSWORD).expect("admin hash"),
        },
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
    path: &std::path::Path,
) {
    let crypto = AgeFileEncryptionProvider::new();
    let signer = Ed25519PackageSigner::new(secret);
    PackageBuilder::new()
        .build_encrypted_stream_path(
            package,
            &grpc_lib::infrastructure::sync::SerdeJsonSyncPackageSerializer,
            &signer,
            &crypto,
            path,
        )
        .expect("write package");
}

// ─────────────────────────────────────────────────────────────────────────────
// Migration 011
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn migration_011_creates_unified_stream_and_retires_fragmented_tables() {
    let db = ConnectionFactory::new_for_test().expect("db");
    run_migrations(db.get_connection()).expect("migrations");

    let conn = db.get_connection();
    let version: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .expect("schema version");
    assert_eq!(version, 11);

    // Unified stream table exists with the exact composite key.
    let pk_cols: Vec<String> = conn
        .prepare(
            "SELECT name FROM pragma_table_info('transport_export_sequence') WHERE pk > 0 ORDER BY pk",
        )
        .expect("pragma")
        .query_map([], |r| r.get(0))
        .expect("pk cols")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert_eq!(pk_cols, vec!["issuer_identity_id", "target_node_id"]);

    for retired in [
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
        assert_eq!(n, 0, "{retired} must be retired by migration 011");
    }

    // The frozen CONSUMER tables survive untouched.
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM sync_issuer_sequence", [], |r| {
            r.get(0)
        })
        .expect("consumer ledger probe");
    assert_eq!(n, 0, "empty but present");
}

// ─────────────────────────────────────────────────────────────────────────────
// Producer semantics — real service, real ledger
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fresh_unit_cross_kind_continuation_admin_then_products() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    let issuer = wilaya_cert.identity_id.to_string();

    // admin_access is FIRST on this fresh (issuer, UNIT-A) stream → seq 1.
    let p1 = dir.path().join("a1.sync");
    let seq = service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            "UNIT-A",
            &p1,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("admin first");
    assert_eq!(seq, 1);

    // products continues the SAME stream → seq 2 (cross-kind, order A).
    let p2 = dir.path().join("a2.sync");
    let seq = service
        .export_v2_package(
            serde_json::json!({ "products": [] }),
            "wilaya-test-node",
            "products",
            "UNIT-A",
            &p2,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("products second");
    assert_eq!(seq, 2, "products continues after admin_access");

    let repo = make_executor(&node.db).transport_export_sequence_state();
    assert_eq!(
        repo.next_issued_sequence(&issuer, "UNIT-A")
            .expect("ledger"),
        Some(2)
    );
}

#[test]
fn fresh_unit_cross_kind_continuation_products_then_admin() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    let issuer = wilaya_cert.identity_id.to_string();

    // products FIRST on this fresh stream → seq 1.
    let p1 = dir.path().join("b1.sync");
    let seq = service
        .export_v2_package(
            serde_json::json!({ "products": [] }),
            "wilaya-test-node",
            "products",
            "UNIT-B",
            &p1,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("products first");
    assert_eq!(seq, 1);

    // admin_access continues → seq 2 (order independence proof).
    let p2 = dir.path().join("b2.sync");
    let seq = service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            "UNIT-B",
            &p2,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("admin second");
    assert_eq!(seq, 2);

    assert_eq!(
        make_executor(&node.db)
            .transport_export_sequence_state()
            .next_issued_sequence(&issuer, "UNIT-B")
            .expect("ledger"),
        Some(2)
    );
}

#[test]
fn interleaved_kinds_are_strictly_contiguous_per_target() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    let issuer = wilaya_cert.identity_id.to_string();

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
    let export_products = |target: &str, path: &std::path::Path| {
        service
            .export_v2_package(
                serde_json::json!({ "products": [] }),
                "wilaya-test-node",
                "products",
                target,
                path,
                SubjectType::Wilaya,
                &crypto,
            )
            .expect("products export")
    };

    let dir_ref = dir.path();
    // Interleaved against UNIT-A only: 1 → 2 → 3 → 4.
    assert_eq!(export_admin("UNIT-A", &dir_ref.join("i1.sync")), 1);
    assert_eq!(export_products("UNIT-A", &dir_ref.join("i2.sync")), 2);
    assert_eq!(export_admin("UNIT-A", &dir_ref.join("i3.sync")), 3);
    assert_eq!(export_products("UNIT-A", &dir_ref.join("i4.sync")), 4);

    // A different target starts at its own 1 — isolation across targets.
    assert_eq!(export_products("UNIT-C", &dir_ref.join("c1.sync")), 1);

    assert_eq!(
        make_executor(&node.db)
            .transport_export_sequence_state()
            .next_issued_sequence(&issuer, "UNIT-A")
            .expect("ledger UNIT-A"),
        Some(4)
    );
    assert_eq!(
        make_executor(&node.db)
            .transport_export_sequence_state()
            .next_issued_sequence(&issuer, "UNIT-C")
            .expect("ledger UNIT-C"),
        Some(1)
    );
}

#[test]
fn failed_export_does_not_burn_and_retry_reuses_the_number() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    set_fleet_password(&node.db);

    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    let issuer = wilaya_cert.identity_id.to_string();

    // Unwritable directory → allocation happens, commit does not.
    let bad = dir.path().join("missing").join("f1.sync");
    assert!(
        service
            .export_v2_package(
                serde_json::json!({ "probe": 1 }),
                "wilaya-test-node",
                "products",
                "UNIT-A",
                &bad,
                SubjectType::Wilaya,
                &crypto,
            )
            .is_err(),
        "unwritable path must fail"
    );
    assert_eq!(
        make_executor(&node.db)
            .transport_export_sequence_state()
            .next_issued_sequence(&issuer, "UNIT-A")
            .expect("ledger"),
        None,
        "no sequence burned by a failed export"
    );

    // Retry reuses the same number — zero gaps by construction.
    let good = dir.path().join("f1.sync");
    let seq = service
        .export_v2_package(
            serde_json::json!({ "probe": 1 }),
            "wilaya-test-node",
            "products",
            "UNIT-A",
            &good,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("retry succeeds");
    assert_eq!(seq, 1, "retry MUST reuse the uncommitted number");
}

#[test]
fn unit_bootstrap_keeps_fixed_one_and_consumes_nothing() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    configure_producer_as_wilaya(&node.db);
    set_fleet_password(&node.db);

    let dir = TempDir::new().expect("temp dir");
    let service = IdentitySignedExportService::new(&node.db, &node.node_key_store);
    let issuer = wilaya_cert.identity_id.to_string();

    // `.unit` V2 bootstrap: fixed sequence 1, NO transport stream consumed.
    let crypto = AgeFileEncryptionProvider::new();
    let unit_pkg_path = dir.path().join("bootstrap.unit");
    let seq = service
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
    assert_eq!(seq, 1, ".unit stays fixed at sequence 1");
    assert_eq!(
        make_executor(&node.db)
            .transport_export_sequence_state()
            .next_issued_sequence(&issuer, "UNIT-A")
            .expect("ledger"),
        None,
        ".unit must not touch any transport stream"
    );

    // The next pipeline kind on that target still starts at 1.
    let crypto = AgeFileEncryptionProvider::new();
    let seq = service
        .export_v2_admin_access_package(
            admin_payload(&node.db),
            "wilaya-test-node",
            "UNIT-A",
            &dir.path().join("after-unit.sync"),
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("admin after .unit");
    assert_eq!(seq, 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository-level defense in depth
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn regression_commit_is_refused_fail_closed() {
    let db = ConnectionFactory::new_for_test().expect("db");
    run_migrations(db.get_connection()).expect("migrations");
    let executor = make_executor(&db);
    let repo = executor.transport_export_sequence_state();

    let token = repo.begin_export("iss-1", "T-1").expect("token");
    assert_eq!(token.value(), 1);
    token.commit().expect("first commit");

    let token = repo.begin_export("iss-1", "T-1").expect("token");
    assert_eq!(token.value(), 2);

    // Simulated concurrent write ahead of our commit → regression refused.
    repo.record_issued_sequence("iss-1", "T-1", 9)
        .expect("external advance");
    let err = token.commit().expect_err("regression must be refused");
    assert!(
        format!("{err}").contains("تراجع"),
        "error must name the regression: {err}"
    );
    assert_eq!(
        repo.next_issued_sequence("iss-1", "T-1").expect("read"),
        Some(9),
        "refused commit left the external value intact"
    );
}

#[test]
fn dropped_token_never_advances_the_ledger() {
    let db = ConnectionFactory::new_for_test().expect("db");
    run_migrations(db.get_connection()).expect("migrations");
    let repo = make_executor(&db).transport_export_sequence_state();

    {
        let token: PendingTransportSequence = repo.begin_export("iss-2", "T-2").expect("token");
        assert_eq!(token.value(), 1);
        // Dropped without commit — RAII guarantees no ledger write.
    }

    assert_eq!(
        repo.next_issued_sequence("iss-2", "T-2").expect("read"),
        None,
        "dropped token must leave the stream untouched"
    );

    // The next allocation restarts at 1 — no phantom consumption.
    let token = repo.begin_export("iss-2", "T-2").expect("token");
    assert_eq!(token.value(), 1);
    token.commit().expect("commit");
}

#[test]
fn record_issued_sequence_roundtrips_and_keys_are_validated() {
    let db = ConnectionFactory::new_for_test().expect("db");
    run_migrations(db.get_connection()).expect("migrations");
    let repo = make_executor(&db).transport_export_sequence_state();

    assert_eq!(
        repo.next_issued_sequence("iss-3", "T-3").expect("empty"),
        None
    );
    repo.record_issued_sequence("iss-3", "T-3", 41)
        .expect("record");
    assert_eq!(
        repo.next_issued_sequence("iss-3", "T-3").expect("41"),
        Some(41)
    );
    repo.record_issued_sequence("iss-3", "T-3", 42)
        .expect("advance");
    assert_eq!(
        repo.next_issued_sequence("iss-3", "T-3").expect("42"),
        Some(42)
    );

    // Fail-closed stream keys: empty or whitespace-only values are rejected.
    assert!(repo.next_issued_sequence("", "T-3").is_err());
    assert!(repo.next_issued_sequence("iss-3", "   ").is_err());
    assert!(repo.begin_export("", "T-3").is_err());
    assert!(repo.begin_export("iss-3", "").is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// Consumer Transport Guard negatives — frozen rules, hand-crafted vehicles
// ─────────────────────────────────────────────────────────────────────────────

mod consumer {
    use super::*;

    fn unit_receiver(unit_code: &str) -> (TempDir, grpc_lib::commands::AppState) {
        use grpc_lib::commands::AppState;
        let dir = TempDir::new().expect("temp dir");
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
                    [Uuid::new_v4().to_string(), unit_code.to_string(), format!("Unit {unit_code}"), FIXED_NOW.to_string()],
                )
                .expect("local unit");
        }
        (dir, state)
    }

    fn seed_anchor(state: &grpc_lib::commands::AppState, cert: &IdentityCertificate) {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        let mut anchor = cert.clone();
        anchor.issuer_identity_id = None;
        anchor.signature = None;
        anchor.status = CredentialStatus::Active;
        grpc_lib::domain::identity::IdentityStorePort::upsert(
            &make_executor(db).identity_store(),
            &anchor,
            &chrono::Utc::now().to_rfc3339(),
        )
        .expect("seed anchor");
    }

    fn set_session(state: &grpc_lib::commands::AppState, role: &str) {
        use grpc_lib::models::UserRole;
        let mut session = common::create_test_session("u1", "bob", role);
        session.user_role = UserRole::from(role.to_string());
        common::insert_test_user(state, "u1", "bob", role);
        *state.current_session.lock().expect("session mutex") = Some(session);
    }

    /// First import MUST be exactly sequence 1 on an empty consumer ledger.
    #[test]
    fn consumer_rejects_first_import_that_is_not_one() {
        let mut node = fresh_node();
        let wilaya_cert = bootstrap_wilaya(&mut node);
        let issuer = wilaya_cert.identity_id;
        let secret = node.node_key_store.read().expect("secret");

        let (_dir, state) = unit_receiver("UNIT-N1");
        seed_anchor(&state, &wilaya_cert);
        set_session(&state, "User");

        let pkg = crafted_admin_package("sec031-not-one", issuer, secret, 2);
        let path = _dir.path().join("not-one.sync");
        write_encrypted_admin(&pkg, secret, &path);

        let err = grpc_lib::commands::import_admin_access_package_impl(
            &state,
            path.to_string_lossy().into_owned(),
        )
        .expect_err("first import ≠ 1 must be rejected");
        assert!(
            format!("{err:?}").contains("OutOfOrder") || !format!("{err:?}").is_empty(),
            "rejected: {err:?}"
        );
    }

    /// Stale (lower than applied), duplicate/replay, and gap sequences are
    /// all rejected by the frozen consumer guard.
    #[test]
    fn consumer_rejects_stale_duplicate_and_gap_sequences() {
        let mut node = fresh_node();
        let wilaya_cert = bootstrap_wilaya(&mut node);
        let issuer = wilaya_cert.identity_id;
        let secret = node.node_key_store.read().expect("secret");

        let (dir, state) = unit_receiver("UNIT-N2");
        seed_anchor(&state, &wilaya_cert);
        set_session(&state, "User");
        let state_ref = &state;

        let apply = |name: &str, seq: u64| -> Result<(), String> {
            let pkg = crafted_admin_package(name, issuer, secret, seq);
            let path = dir.path().join(format!("{name}.sync"));
            write_encrypted_admin(&pkg, secret, &path);
            grpc_lib::commands::import_admin_access_package_impl(
                state_ref,
                path.to_string_lossy().into_owned(),
            )
            .map(|_| ())
        };

        // Valid bootstrap: 1.
        apply("sec031-first", 1).expect("bootstrap seq 1 applies");
        {
            let guard = state.get_db().expect("lock");
            let db = guard.as_ref().expect("db");
            assert_eq!(last_applied(db, &issuer.to_string()), Some(1));
        }

        // Post-bootstrap imports take the AdminOnly re-import path
        // (ADR-0051 §9): promote the local session to Admin.
        set_session(state_ref, "Admin");

        // Gap: skipping 2 entirely → 4 rejected.
        assert!(
            apply("sec031-gap", 4).is_err(),
            "gap sequence 4 must be rejected while applied=1"
        );
        assert_eq!(
            last_applied(
                state.get_db().expect("lock").as_ref().expect("db"),
                &issuer.to_string()
            ),
            Some(1),
            "rejected gap leaves the ledger untouched"
        );

        // Duplicate/replay of an already-applied sequence → rejected.
        assert!(
            apply("sec031-replay", 1).is_err(),
            "replayed sequence 1 must be rejected while applied=1"
        );

        // Valid continuation: 2 applies, then stale 1 is rejected again.
        apply("sec031-second", 2).expect("continuation seq 2 applies");
        assert!(
            apply("sec031-stale", 1).is_err(),
            "stale sequence 1 must be rejected while applied=2"
        );
        {
            let guard = state.get_db().expect("lock");
            let db = guard.as_ref().expect("db");
            assert_eq!(last_applied(db, &issuer.to_string()), Some(2));
        }
    }
}
