//! B8 First-Import tests (ADR-0044 / ADR-0045, RFC 2026-08-04 §3.12).
//!
//! Section A — `B8FirstImportPredicatesService::evaluate` (unit):
//!   A1 all predicates hold;
//!   A2 anchor missing fails closed;
//!   A3 issuer ≠ anchor fails closed;
//!   A4 payload unit_code ≠ local fails closed;
//!   A5 an existing Admin fails closed (self-termination).
//! Section B — `verify_unit_v2_acceptance` (`.unit` V2 gate, A44-01/07/08):
//!   B1 anchor-first: no anchor → reject;
//!   B2 anchor == issuer binding;
//!   B3 fixed bootstrap sequence 1 (A44-08);
//!   B4 one-time: non-empty ledger → reject;
//!   B5 all conditions → accept.
//! Section C — D1 cutover (ADR-0051 §9): legacy `identity_access` imports
//!   fail closed at the kind boundary before any mutation:
//!   C1 a fully valid legacy bootstrap artifact is still rejected;
//!   C2 rejection is unconditional and kind-scoped (fires before trust checks);
//!   C3 repeated attempts leave zero partial state;
//!   C4 rejection applies on every node type;
//!   C5 the legacy kind is never reinterpreted as `admin_access`.
//! Section D — `.unit` V2 producer: fixed sequence 1, V2/Ed25519, ledger
//!   untouched (next export still allocates 1).

#[allow(dead_code)]
mod common;

use chrono::Utc;
use std::path::Path;

use tempfile::TempDir;
use uuid::Uuid;

use grpc_lib::application::services::{
    B8FirstImportPredicatesService, FinalizeWilayaProvisionResult, IdentityProvisioningService,
    IdentitySignedExportService,
};
use grpc_lib::application::sync::{PackageId, SchemaVersion, SyncPackage, SyncPackageMetadata};
use grpc_lib::commands::{import_identity_access_package_impl, AppState};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    CredentialStatus, Ed25519CertificateSignature, IdentityCertificate, IdentitySigner,
    IdentityStorePort, SubjectType, SIGNATURE_VERSION_ED25519,
};
use grpc_lib::errors::AppError;
use grpc_lib::infrastructure::identity::NodeKeyStore;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::Ed25519SigningProvider;
use grpc_lib::infrastructure::sync::packages::canonical_json::{
    canonical_bytes_for_integrity, canonical_bytes_for_signature,
};
use grpc_lib::infrastructure::sync::packages::integrity::{PackageHasher, Sha256PackageHasher};
use grpc_lib::infrastructure::sync::packages::signing::{Ed25519PackageSigner, PackageSigner};
use grpc_lib::infrastructure::sync::{
    read_unit_node_package_from_file,
    PackageBuilder, SerdeJsonSyncPackageSerializer,
};
use grpc_lib::models::{IdentityAccessPayload, Unit, UnitNodePackage, UserExport};
use grpc_lib::repositories::executor::DbExecutor;
use grpc_lib::repositories::RepositoryProvider;

const ISSUER_SECRET: [u8; 32] = [42u8; 32];
const FIXED_NOW: &str = "2026-08-04T00:00:00Z";

/// RFC 8032 §7.1 TEST 1 secret — matches the debug-mode Root fallback.
const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
    0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
    0x7f, 0x60,
];

fn make_executor(db: &Database) -> DbExecutor<'_> {
    db.executor()
}

/// Seed an ACTIVE WILAYA certificate in the identity store — on a UNIT this
/// row IS the locally installed trust anchor (ADR-0044 anchor-first model).
fn seed_anchor(db: &Database, identity_id: Uuid, secret: [u8; 32]) {
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
    .expect("seed anchor");
}

fn identity_access_package(
    package_id: &str,
    issuer_id: Uuid,
    sequence: u64,
    payload: IdentityAccessPayload,
) -> SyncPackage<IdentityAccessPayload> {
    SyncPackage {
        metadata: SyncPackageMetadata {
            created_at: Utc::now(),
            integrity_hash: None,
            package_sequence: Some(sequence),
            issuer_identity_id: Some(issuer_id),
            package_id: PackageId(package_id.to_string()),
            schema_version: SchemaVersion::V2,
            signature: None,
            signature_version: Some(SIGNATURE_VERSION_ED25519),
            signing_key_id: Some("default".to_string()),
            source_node_id: "wilaya-a".to_string(),
        },
        payload,
    }
}

fn payload(unit_code: &str, admin_enabled: bool, user_enabled: bool) -> IdentityAccessPayload {
    IdentityAccessPayload {
        unit_code: unit_code.to_string(),
        admin_enabled,
        admin_password_hash: "fleet-admin-hash".to_string(),
        user_enabled,
        user_password_hash: "unit-bound-hash".to_string(),
    }
}

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

fn write_encrypted<T: serde::Serialize>(
    package: &SyncPackage<T>,
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

fn unit_state(unit_code: &str) -> AppState {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        // A fresh UNIT node: no accounts exist yet (the test fixture's
        // seeded legacy admin is removed — B6-A test support only).
        db.get_connection()
            .execute("DELETE FROM users", [])
            .expect("clear seeded users");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'UNIT', configured = 1, unit_name = 'unit-scope-id', wilaya_code = '16', wilaya_name = NULL WHERE id = 1",
                [],
            )
            .expect("settings");
        // `settings.unit_code` is derived from the local `units` row
        // (SettingsService::get_settings).
        db.get_connection()
            .execute(
                "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES ('unit-9', ?1, 'Unit 9', '16', ?2)",
                [unit_code, FIXED_NOW],
            )
            .expect("local unit");
    }
    state
}

fn wilaya_state() -> AppState {
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

fn set_session(state: &AppState, role: &str) {
    let mut session = common::create_test_session("u1", "bob", role);
    session.user_role = grpc_lib::models::UserRole::from(role.to_string());
    common::insert_test_user(state, "u1", "bob", role);
    *state.current_session.lock().expect("session mutex") = Some(session);
}

fn count_active_admins(db: &Database) -> i64 {
    make_executor(db)
        .users()
        .count_active_admins()
        .expect("count admins")
}

// ── Section A: B8 first-import predicates ────────────────────────────────

#[test]
fn a1_all_predicates_hold() {
    let db = ConnectionFactory::new_for_test().expect("db");
    db.get_connection()
        .execute("DELETE FROM users", [])
        .expect("clear seeded users");
    let issuer_id = Uuid::new_v4();
    seed_anchor(&db, issuer_id, ISSUER_SECRET);

    let verdict = B8FirstImportPredicatesService::evaluate(
        &make_executor(&db),
        "UNIT-9",
        Some(&issuer_id.to_string()),
        Some("UNIT-9"),
    )
    .expect("evaluate");
    assert!(verdict.anchor_installed);
    assert!(verdict.anchor_is_issuer);
    assert!(verdict.unit_code_matches);
    assert!(verdict.no_active_admin);
    assert!(verdict.all_hold());
}

#[test]
fn a2_missing_anchor_fails_closed() {
    let db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();

    let verdict = B8FirstImportPredicatesService::evaluate(
        &make_executor(&db),
        "UNIT-9",
        Some(&issuer_id.to_string()),
        Some("UNIT-9"),
    )
    .expect("evaluate");
    assert!(!verdict.anchor_installed);
    assert!(!verdict.all_hold());
}

#[test]
fn a3_issuer_not_anchor_fails_closed() {
    let db = ConnectionFactory::new_for_test().expect("db");
    let anchor_id = Uuid::new_v4();
    seed_anchor(&db, anchor_id, ISSUER_SECRET);
    let other_issuer = Uuid::new_v4();

    let verdict = B8FirstImportPredicatesService::evaluate(
        &make_executor(&db),
        "UNIT-9",
        Some(&other_issuer.to_string()),
        Some("UNIT-9"),
    )
    .expect("evaluate");
    assert!(verdict.anchor_installed);
    assert!(!verdict.anchor_is_issuer);
    assert!(!verdict.all_hold());
}

#[test]
fn a4_unit_code_mismatch_fails_closed() {
    let db = ConnectionFactory::new_for_test().expect("db");
    db.get_connection()
        .execute("DELETE FROM users", [])
        .expect("clear seeded users");
    let issuer_id = Uuid::new_v4();
    seed_anchor(&db, issuer_id, ISSUER_SECRET);

    let verdict = B8FirstImportPredicatesService::evaluate(
        &make_executor(&db),
        "UNIT-FOREIGN",
        Some(&issuer_id.to_string()),
        Some("UNIT-9"),
    )
    .expect("evaluate");
    assert!(verdict.anchor_installed);
    assert!(verdict.anchor_is_issuer);
    assert!(!verdict.unit_code_matches);
    assert!(!verdict.all_hold());
}

#[test]
fn a5_existing_admin_fails_closed() {
    let db = ConnectionFactory::new_for_test().expect("db");
    db.get_connection()
        .execute("DELETE FROM users", [])
        .expect("clear seeded users");
    let issuer_id = Uuid::new_v4();
    seed_anchor(&db, issuer_id, ISSUER_SECRET);
    make_executor(&db)
        .users()
        .upsert_raw_user(
            "u-admin",
            "admin",
            "hash",
            "Admin",
            "UNIT-9",
            &Utc::now().to_rfc3339(),
        )
        .expect("insert admin");

    let verdict = B8FirstImportPredicatesService::evaluate(
        &make_executor(&db),
        "UNIT-9",
        Some(&issuer_id.to_string()),
        Some("UNIT-9"),
    )
    .expect("evaluate");
    assert!(!verdict.no_active_admin);
    assert!(!verdict.all_hold());
}

// ── Section B: `.unit` V2 acceptance gate (ADR-0044) ─────────────────────

#[test]
fn b1_unit_v2_rejected_without_anchor() {
    let db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    let err = B8FirstImportPredicatesService::verify_unit_v2_acceptance(
        &make_executor(&db),
        &issuer_id.to_string(),
        Some(1),
    )
    .expect_err("anchor-first: must reject without an anchor");
    assert!(matches!(err, AppError::Validation(_)));
}

#[test]
fn b2_unit_v2_rejected_when_issuer_not_anchor() {
    let db = ConnectionFactory::new_for_test().expect("db");
    let anchor_id = Uuid::new_v4();
    seed_anchor(&db, anchor_id, ISSUER_SECRET);
    let other = Uuid::new_v4();
    let err = B8FirstImportPredicatesService::verify_unit_v2_acceptance(
        &make_executor(&db),
        &other.to_string(),
        Some(1),
    )
    .expect_err("anchor==issuer must be enforced");
    assert!(matches!(err, AppError::Validation(_)));
}

#[test]
fn b3_unit_v2_rejected_when_sequence_not_one() {
    let db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_anchor(&db, issuer_id, ISSUER_SECRET);
    for seq in [None, Some(0), Some(2), Some(5)] {
        let err = B8FirstImportPredicatesService::verify_unit_v2_acceptance(
            &make_executor(&db),
            &issuer_id.to_string(),
            seq,
        )
        .expect_err("A44-08: first .unit V2 must carry sequence 1");
        assert!(matches!(err, AppError::Validation(_)));
    }
}

#[test]
fn b4_unit_v2_rejected_when_ledger_not_empty() {
    let db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_anchor(&db, issuer_id, ISSUER_SECRET);
    make_executor(&db)
        .sync_applied_packages()
        .record_issuer_sequence(&issuer_id.to_string(), 1)
        .expect("advance ledger");

    let err = B8FirstImportPredicatesService::verify_unit_v2_acceptance(
        &make_executor(&db),
        &issuer_id.to_string(),
        Some(1),
    )
    .expect_err("one-time bootstrap: non-empty ledger must reject");
    assert!(matches!(err, AppError::Validation(_)));
}

#[test]
fn b5_unit_v2_acceptance_succeeds_on_fresh_anchor_first_node() {
    let db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_anchor(&db, issuer_id, ISSUER_SECRET);

    B8FirstImportPredicatesService::verify_unit_v2_acceptance(
        &make_executor(&db),
        &issuer_id.to_string(),
        Some(1),
    )
    .expect("fresh node + installed anchor + seq 1 must be accepted");
}

// ── Section C: D1 cutover — legacy `identity_access` imports fail closed ──
//
// ADR-0051 §9 / Decision D1 (ratified 2026-08-22): at cutover, legacy
// `identity_access` IMPORT is unconditionally REJECTED at the package-kind
// boundary BEFORE any account mutation. The historical bootstrap flow these
// tests used to exercise is intentionally dead in production; each test now
// proves the cutover contract itself:
//   C1 a fully valid bootstrap artifact is still rejected;
//   C2 rejection is unconditional (no trust material changes nothing);
//   C3 repeated attempts leave zero partial state;
//   C4 rejection applies on every node type;
//   C5 the legacy kind is never reinterpreted as `admin_access`.

const D1_IMPORT_REJECTION: &str = "مرفوض مغلقًا";

fn last_applied(db: &Database, issuer: &str) -> Option<u64> {
    grpc_lib::application::services::SyncPackageIdentityVerificationService::last_applied_sequence(
        make_executor(db),
        issuer,
    )
    .expect("read ledger")
}

#[test]
fn c1_d1_rejects_even_a_fully_valid_bootstrap_artifact() {
    let state = unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        seed_anchor(db, issuer_id, ISSUER_SECRET);
    }
    set_session(&state, "User");

    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("bootstrap.sync");
    let package = sign_v2_package(
        identity_access_package("b8-c1", issuer_id, 1, payload("UNIT-9", true, true)),
        ISSUER_SECRET,
    );
    write_encrypted(&package, ISSUER_SECRET, &path);

    let err = import_identity_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("D1: even a valid legacy bootstrap package must be rejected");
    assert!(
        err.contains(D1_IMPORT_REJECTION),
        "rejection must cite the D1 cutover; got: {err}"
    );

    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    assert_eq!(count_active_admins(db), 0, "no admin may be created");
    assert_eq!(
        last_applied(db, &issuer_id.to_string()),
        None,
        "the ledger must not advance on the rejected import"
    );
}

#[test]
fn c2_d1_rejection_is_unconditional_and_kind_scoped() {
    // No anchor installed: the D1 boundary fires BEFORE trust evaluation,
    // so the error is identical to the anchored case — the legacy kind is
    // rejected by its KIND, not by its content or trust context.
    let state = unit_state("UNIT-9");
    set_session(&state, "User");

    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("bootstrap.sync");
    let issuer_id = Uuid::new_v4();
    let package = sign_v2_package(
        identity_access_package("b8-c2", issuer_id, 1, payload("UNIT-9", true, true)),
        ISSUER_SECRET,
    );
    write_encrypted(&package, ISSUER_SECRET, &path);

    let err = import_identity_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("no anchor/issuer trust material must reject");
    assert!(
        err.contains(D1_IMPORT_REJECTION),
        "rejection must be the D1 cutover, not a downstream trust failure; got: {err}"
    );

    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    assert_eq!(count_active_admins(db), 0, "no admin must be created");
}

#[test]
fn c3_d1_rejection_leaves_zero_partial_state_across_attempts() {
    let state = unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        seed_anchor(db, issuer_id, ISSUER_SECRET);
    }
    set_session(&state, "User");

    let dir = TempDir::new().expect("temp dir");
    for (name, id, seq) in [("first", "b8-c3a", 1u64), ("second", "b8-c3b", 2u64)] {
        let path = dir.path().join(format!("{name}.sync"));
        let package = sign_v2_package(
            identity_access_package(id, issuer_id, seq, payload("UNIT-9", true, true)),
            ISSUER_SECRET,
        );
        write_encrypted(&package, ISSUER_SECRET, &path);
        let err = import_identity_access_package_impl(&state, path.to_string_lossy().into_owned())
            .expect_err("every legacy attempt must reject under D1");
        assert!(err.contains(D1_IMPORT_REJECTION), "got: {err}");
    }

    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    assert_eq!(count_active_admins(db), 0, "admin count must stay 0");
    assert_eq!(
        last_applied(db, &issuer_id.to_string()),
        None,
        "ledger must stay empty across rejected attempts"
    );
}

#[test]
fn c4_d1_rejection_applies_on_every_node_type() {
    let state = wilaya_state();
    set_session(&state, "User");

    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("bootstrap.sync");
    let issuer_id = Uuid::new_v4();
    let package = sign_v2_package(
        identity_access_package("b8-c4", issuer_id, 1, payload("UNIT-9", true, true)),
        ISSUER_SECRET,
    );
    write_encrypted(&package, ISSUER_SECRET, &path);

    let err = import_identity_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("D1 applies regardless of node type");
    assert!(
        err.contains(D1_IMPORT_REJECTION),
        "got: {err}"
    );
}

#[test]
fn c5_d1_legacy_kind_never_reinterpreted_as_admin_access() {
    // An Admin session on an anchored UNIT must NOT have its legacy package
    // silently aliased onto the new `admin_access` semantics: no partial
    // application of the Admin portion, no operator mutation.
    let state = unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        seed_anchor(db, issuer_id, ISSUER_SECRET);
    }
    set_session(&state, "Admin");

    let dir = TempDir::new().expect("temp dir");
    let second = dir.path().join("second.sync");
    let package2 = sign_v2_package(
        identity_access_package("b8-c5b", issuer_id, 2, payload("UNIT-9", true, false)),
        ISSUER_SECRET,
    );
    write_encrypted(&package2, ISSUER_SECRET, &second);
    let err = import_identity_access_package_impl(&state, second.to_string_lossy().into_owned())
        .expect_err("Admin session must ALSO hit the D1 wall — no reinterpretation");
    assert!(err.contains(D1_IMPORT_REJECTION), "got: {err}");

    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let canonical_admins: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM users WHERE role = 'Admin' AND username = 'admin' AND deleted = 0",
            [],
            |row| row.get(0),
        )
        .expect("count canonical admins");
    assert_eq!(canonical_admins, 0, "legacy import must not create admin");
}

// ── Section D: `.unit` V2 producer (ADR-0044 A44-07/08) ──────────────────

fn fresh_node() -> (TempDir, Database, NodeKeyStore) {
    let dir = TempDir::new().expect("temp dir");
    let db = ConnectionFactory::new_for_test().expect("db");
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
    (dir, db, node_key_store)
}

fn root_signer() -> Ed25519SigningProvider {
    Ed25519SigningProvider::new(TEST_ROOT_SECRET)
}

fn bootstrap_wilaya(db: &mut Database, node_key_store: &NodeKeyStore) -> IdentityCertificate {
    let mut provisioning = IdentityProvisioningService::new(db);
    let request = provisioning
        .generate_wilaya_request(Uuid::new_v4(), node_key_store)
        .expect("csr generated");
    let signature = root_signer()
        .sign_certificate(&request)
        .expect("root signed request");
    let mut signed = request;
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig wrap"));
    match provisioning
        .finalize_wilaya_provision(&signed, node_key_store, FIXED_NOW)
        .expect("wilaya finalized")
    {
        FinalizeWilayaProvisionResult::Provisioned(cert) => cert,
        FinalizeWilayaProvisionResult::AlreadyProvisioned(_) => {
            panic!("first finalize must provision")
        }
    }
}

#[test]
fn d1_unit_v2_export_is_fixed_sequence_one_and_ledger_untouched() {
    let (_dir, mut db, node_key_store) = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut db, &node_key_store);
    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("unit.unit");

    let dataset = UnitNodePackage {
        unit: Unit {
            id: "unit-9".into(),
            code: "UNIT-9".into(),
            name: "Unit 9".into(),
            wilaya_code: "16".into(),
            user_id: None,
            created_at: Utc::now(),
        },
        user: UserExport {
            username: "op".into(),
            password_hash: "hash".into(),
            role: "User".into(),
        },
        unit_certificate: None,
        unit_private_key: None,
    };

    let sequence = IdentitySignedExportService::new(&db, &node_key_store)
        .export_v2_bootstrap_package(
            dataset.clone(),
            "wilaya-test-node",
            &path,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("bootstrap export");

    assert_eq!(sequence, 1, "A44-08: fixed bootstrap sequence 1");

    let pkg = read_unit_node_package_from_file(&path, &crypto).expect("read .unit back");
    let meta = &pkg.metadata;
    assert_eq!(meta.signature_version, Some(SIGNATURE_VERSION_ED25519));
    assert_eq!(meta.package_sequence, Some(1));
    assert_eq!(meta.issuer_identity_id, Some(wilaya_cert.identity_id));
    assert!(meta.signature.is_some(), "Ed25519 signature must be set");
    assert_eq!(pkg.payload.unit.code, dataset.unit.code);
    assert_eq!(pkg.payload.unit.name, dataset.unit.name);
    assert_eq!(pkg.payload.user.username, dataset.user.username);
    assert_eq!(pkg.payload.user.role, dataset.user.role);

    // The bootstrap artifact never advances the per-issuer ledger: the next
    // identity_access export still allocates sequence 1 (A45-06).
    let issued = db
        .executor()
        .sync_issuer_sequence_state()
        .next_issued_sequence(&wilaya_cert.identity_id.to_string())
        .expect("read ledger");
    assert_eq!(issued, None, ".unit export must not burn ledger sequence");
}
