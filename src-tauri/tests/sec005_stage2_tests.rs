//! SEC-005 Stage 2 — command-boundary regression tests for the three confirmed
//! findings:
//!
//! - Finding A: `run_fiscal_integrity_scan` and `verify_inventory_integrity` are
//!   writable integrity-scan commands and MUST be authorized at the command
//!   boundary (`Action::ViewSystemHealth`) exactly like their guarded analogues
//!   (`verify_integrity`, `get_advanced_diagnostics_bundle`).
//! - Finding B: `finalize_wilaya_rotation`, `finalize_unit_rotation`, and
//!   `sign_unit_rotation_request` MUST drop the (non-reentrant) DB lock before
//!   `log_rotation_audit` re-locks it. Exercised through the extracted
//!   `_impl` command bodies so the command-level path (including the audit) is
//!   actually executed.
//! - Finding C: `IdentityRotationCoordinator::finalize_wilaya` MUST reject any
//!   WILAYA certificate whose `issuer_identity_id` is `Some(...)` — WILAYA
//!   certificates are issued exclusively by the offline Authority Root — and
//!   the rejection MUST occur before any persistence.
//!
//! The test authority Root uses the RFC 8032 §7.1 TEST 1 keypair — exactly the
//! `#[cfg(debug_assertions)]` development Root fallback in `root_public_key.rs`.

#[allow(dead_code)]
mod common;

use tempfile::TempDir;

use grpc_lib::application::services::{
    AuditService, FinalizeUnitProvisionResult, FinalizeWilayaProvisionResult,
    IdentityProvisioningService, IdentityRotationCoordinator, IdentityTrustAnchorService,
    RotationFinalizeOutcome, RotationOperation,
};
use grpc_lib::commands::{
    fiscal::run_fiscal_integrity_scan_impl, identity::finalize_unit_rotation_impl,
    identity::finalize_wilaya_rotation_impl, identity::sign_unit_rotation_request_impl,
    inventory::verify_inventory_integrity_impl, AppState,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::audit::{AuditAction, AuditFilters, AuditStatus};
use grpc_lib::domain::identity::{
    Ed25519CertificateSignature, IdentityCertificate, IdentitySigner, IdentityStorePort,
    SubjectType,
};
use grpc_lib::errors::{AppError, AuthenticationError};
use grpc_lib::infrastructure::identity::NodeKeyStore;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::Ed25519SigningProvider;
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
/// A fixed local unit mirrored on the WILAYA node.
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
    let node_dir = dir.path().join("node");
    Node {
        _dir: dir,
        db,
        node_key_store: NodeKeyStore::new(node_dir),
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

/// Full offline WILAYA bootstrap (Root-issued).
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

/// A provisioned WILAYA node holding the shared unit row.
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

/// Build an unsigned UNIT rotation CSR using a fresh keypair.
fn unit_csr(
    subject_id: uuid::Uuid,
    key_seed: [u8; 32],
    credential_id: uuid::Uuid,
    identity_id: uuid::Uuid,
) -> IdentityCertificate {
    let signer = Ed25519SigningProvider::new(key_seed);
    IdentityCertificate {
        identity_id,
        subject_type: SubjectType::Unit,
        subject_id,
        issuer_identity_id: None,
        credential_id,
        generation: 1,
        status: grpc_lib::domain::identity::CredentialStatus::Active,
        public_key: signer.public_key().to_vec(),
        algorithm_version: grpc_lib::domain::identity::SIGNATURE_VERSION_ED25519,
        not_after: None,
        package_sequence: None,
        signature: None,
    }
}

// ---------------------------------------------------------------------------
// Configured-node command states (authz fixtures)
// ---------------------------------------------------------------------------

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

fn set_session(state: &AppState, session: grpc_lib::domain::session::CurrentSession) {
    common::insert_test_user(
        state,
        &session.user_id,
        &session.username,
        &session.user_role.to_string(),
    );
    *state.current_session.lock().expect("session mutex") = Some(session);
}

/// AppState wired as a configured WILAYA node with an ACTIVE WILAYA identity
/// (provisioned through a temp node key store) + a mirrored unit row.
/// The WILAYA Admin session `u1` is installed.
fn wilaya_admin_command_state() -> (AppState, TempDir) {
    let dir = TempDir::new().expect("temp dir");
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
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
    {
        let mut guard = state.get_db().expect("lock");
        let db = guard.as_mut().expect("db");
        db.executor()
            .units()
            .upsert_raw_unit(UNIT_ID, UNIT_CODE, UNIT_NAME, UNIT_WILAYA, FIXED_NOW)
            .expect("unit row");
        let mut provisioning = IdentityProvisioningService::new(db);
        let request = provisioning
            .generate_wilaya_request(uuid::Uuid::new_v4(), &node_key_store)
            .expect("wilaya csr");
        let signed = sign_with_root(&request);
        match provisioning
            .finalize_wilaya_provision(&signed, &node_key_store, FIXED_NOW)
            .expect("finalize")
        {
            FinalizeWilayaProvisionResult::Provisioned(_) => {}
            FinalizeWilayaProvisionResult::AlreadyProvisioned(_) => {
                panic!("first finalize must provision")
            }
        }
    }
    let session = common::create_test_session("u1", "wilaya-admin", "Admin");
    common::insert_test_user(&state, &session.user_id, &session.username, "Admin");
    *state.current_session.lock().expect("session lock") = Some(session);
    (state, dir)
}

fn audit_for_action(state: &AppState, action: &str) -> grpc_lib::domain::audit::AuditLogResponse {
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let filters = AuditFilters {
        action: Some(action.to_string()),
        ..Default::default()
    };
    AuditService::new(db.executor())
        .get_audit_entries(&filters, 0, 100)
        .expect("audit query")
}

fn count_attempts(state: &AppState) -> i64 {
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    db.get_connection()
        .query_row(
            "SELECT COUNT(*) FROM integrity_verification_attempts",
            [],
            |row| row.get(0),
        )
        .expect("attempt count")
}

// ---------------------------------------------------------------------------
// Finding A — writable integrity-scan commands are authorized at the boundary
// ---------------------------------------------------------------------------

#[test]
fn sec005a_fiscal_scan_denied_unauthenticated() {
    let state = wilaya_configured_state();
    let err = run_fiscal_integrity_scan_impl(&state).expect_err("must deny unauthenticated");
    match err {
        AppError::Authentication(AuthenticationError::SessionNotFound) => {}
        other => panic!("expected SessionNotFound, got {other:?}"),
    }
    assert_eq!(count_attempts(&state), 0, "denied scan writes nothing");
}

#[test]
fn sec005a_fiscal_scan_allowed_for_wilaya_admin() {
    let state = wilaya_configured_state();
    let session = common::create_test_session("u1", "wilaya-admin", "Admin");
    set_session(&state, session);

    let report = run_fiscal_integrity_scan_impl(&state).expect("WILAYA Admin allowed");
    assert!(
        report.ok,
        "fresh DB has exactly one seeded open fiscal year → clean scan, got {report:?}"
    );
    assert_eq!(
        count_attempts(&state),
        1,
        "allowed scan records the attempt row"
    );
}

#[test]
fn sec005a_inventory_verify_denied_unauthenticated() {
    let state = wilaya_configured_state();
    let err = verify_inventory_integrity_impl(&state, 2026).expect_err("must deny unauthenticated");
    match err {
        AppError::Authentication(AuthenticationError::SessionNotFound) => {}
        other => panic!("expected SessionNotFound, got {other:?}"),
    }
    assert_eq!(count_attempts(&state), 0, "denied scan writes nothing");
}

#[test]
fn sec005a_inventory_verify_allowed_for_wilaya_admin() {
    let state = wilaya_configured_state();
    let session = common::create_test_session("u1", "wilaya-admin", "Admin");
    set_session(&state, session);

    let report = verify_inventory_integrity_impl(&state, 2026).expect("WILAYA Admin allowed");
    assert_eq!(report.mismatch_count, 0, "empty ledger is consistent");
    assert_eq!(
        count_attempts(&state),
        1,
        "allowed scan records the attempt row"
    );
}

// ---------------------------------------------------------------------------
// Finding B — rotation commands complete (no deadlock) and record the audit
// ---------------------------------------------------------------------------

#[test]
fn sec005b_wilaya_rotation_finalize_command_completes_and_records_audit() {
    let (state, dir) = wilaya_admin_command_state();
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
    let plan = {
        let mut guard = state.get_db().expect("lock");
        let db = guard.as_mut().expect("db");
        IdentityRotationCoordinator::new(db, &node_key_store)
            .begin(SubjectType::Wilaya, RotationOperation::Rotate)
            .expect("begin wilaya rotation")
    };
    let signed = sign_with_root(&plan.certificate);
    let cert_path = dir.path().join("signed-wilaya-rotation.json");
    std::fs::write(
        &cert_path,
        serde_json::to_string_pretty(&signed).expect("json"),
    )
    .expect("write cert file");
    let package_path = dir.path().join("rotation.sync");

    // If the DB lock were still held across `log_rotation_audit`, this call
    // would deadlock on the non-reentrant std Mutex — it must return.
    let outcome = finalize_wilaya_rotation_impl(
        &state,
        cert_path.to_str().expect("utf8").to_string(),
        package_path.to_str().expect("utf8").to_string(),
        &node_key_store,
    )
    .expect("command-level WILAYA rotation completes without hanging");
    assert!(
        matches!(outcome, RotationFinalizeOutcome::Completed { .. }),
        "expected Completed, got {outcome:?}"
    );

    let resp = audit_for_action(&state, "IdentityRotated");
    assert_eq!(resp.total_count, 1, "exactly one rotation audit event");
    assert_eq!(resp.entries[0].user_id, "u1");
    assert_eq!(resp.entries[0].action, AuditAction::IdentityRotated);
    assert_eq!(resp.entries[0].status, AuditStatus::Success);
    assert!(
        resp.entries[0].session_id.is_some(),
        "actor session recorded"
    );
}

#[test]
fn sec005b_unit_rotation_sign_command_completes_and_records_audit() {
    let (state, dir) = wilaya_admin_command_state();
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
    let csr = unit_csr(
        unit_uuid(),
        [3; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );

    let signed = sign_unit_rotation_request_impl(
        &state,
        serde_json::to_string(&csr).expect("csr json"),
        &node_key_store,
    )
    .expect("command-level UNIT rotation signing completes without hanging");
    assert!(
        signed.certificate.signature.is_some(),
        "WILAYA must sign the CSR"
    );
    assert_eq!(signed.operation, RotationOperation::ReIssue);

    let resp = audit_for_action(&state, "IdentityReissued");
    assert_eq!(resp.total_count, 1, "exactly one signing audit event");
    assert_eq!(resp.entries[0].user_id, "u1");
    assert_eq!(resp.entries[0].action, AuditAction::IdentityReissued);
    assert_eq!(resp.entries[0].status, AuditStatus::Success);
}

/// A provisioned UNIT node behind a real `AppState` + the WILAYA signer node.
struct UnitCommandFixture {
    _dir: TempDir,
    _wilaya_dir: TempDir,
    state: AppState,
    unit_nks: NodeKeyStore,
    wilaya_nks: NodeKeyStore,
    wilaya_db: Database,
}

fn unit_command_fixture() -> UnitCommandFixture {
    let dir = TempDir::new().expect("temp dir");
    let unit_nks = NodeKeyStore::new(dir.path().join("unit-node"));
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

    let subject_id;
    let csr;
    {
        let mut guard = state.get_db().expect("lock");
        let db = guard.as_mut().expect("db");
        db.executor()
            .units()
            .upsert_raw_unit(UNIT_ID, UNIT_CODE, UNIT_NAME, UNIT_WILAYA, FIXED_NOW)
            .expect("unit row");
        let provisioning = IdentityProvisioningService::new(db);
        subject_id = provisioning
            .resolve_local_unit_subject_id()
            .expect("local unit subject resolved");
        csr = provisioning
            .generate_identity_request(SubjectType::Unit, subject_id, &unit_nks)
            .expect("unit csr");
    }

    let mut wilaya = fresh_wilaya_with_unit();
    let wilaya_cert = active_wilaya(&wilaya);
    let signed_unit_cert = IdentityProvisioningService::new(&mut wilaya.db)
        .sign_unit_identity_request(&csr, &wilaya.node_key_store)
        .expect("unit csr signed by wilaya");

    {
        let mut guard = state.get_db().expect("lock");
        let db = guard.as_mut().expect("db");
        IdentityTrustAnchorService::new(db.executor())
            .install_wilaya_certificate(&wilaya_cert, FIXED_NOW)
            .expect("anchor installed");
        match IdentityProvisioningService::new(db)
            .finalize_unit_provision(&signed_unit_cert, &unit_nks, FIXED_NOW)
            .expect("unit finalized")
        {
            FinalizeUnitProvisionResult::Provisioned(_) => {}
            FinalizeUnitProvisionResult::AlreadyProvisioned(_) => {
                panic!("first finalize must provision")
            }
        }
    }

    let session = common::create_test_session("u1", "unit-admin", "Admin");
    common::insert_test_user(&state, &session.user_id, &session.username, "Admin");
    *state.current_session.lock().expect("session lock") = Some(session);

    let Node {
        _dir: wilaya_dir,
        db: wilaya_db,
        node_key_store: wilaya_nks,
    } = wilaya;
    UnitCommandFixture {
        _dir: dir,
        _wilaya_dir: wilaya_dir,
        state,
        unit_nks,
        wilaya_nks,
        wilaya_db,
    }
}

#[test]
fn sec005b_unit_rotation_finalize_command_completes_and_records_audit() {
    let mut fix = unit_command_fixture();

    let plan = {
        let mut guard = fix.state.get_db().expect("lock");
        let db = guard.as_mut().expect("db");
        IdentityRotationCoordinator::new(db, &fix.unit_nks)
            .begin(SubjectType::Unit, RotationOperation::Rotate)
            .expect("begin unit rotation")
    };
    let signed_unit = IdentityRotationCoordinator::new(&mut fix.wilaya_db, &fix.wilaya_nks)
        .sign_unit_rotation(&plan.certificate)
        .expect("wilaya signs the unit rotation CSR");
    let cert_path = fix._dir.path().join("signed-unit-rotation.json");
    std::fs::write(
        &cert_path,
        serde_json::to_string_pretty(&signed_unit.certificate).expect("json"),
    )
    .expect("write cert file");

    let outcome = finalize_unit_rotation_impl(
        &fix.state,
        cert_path.to_str().expect("utf8").to_string(),
        &fix.unit_nks,
    )
    .expect("command-level UNIT rotation completes without hanging");
    assert!(
        matches!(outcome, RotationFinalizeOutcome::Completed { .. }),
        "expected Completed, got {outcome:?}"
    );

    let resp = audit_for_action(&fix.state, "IdentityRotated");
    assert_eq!(resp.total_count, 1, "exactly one rotation audit event");
    assert_eq!(resp.entries[0].user_id, "u1");
    assert_eq!(resp.entries[0].action, AuditAction::IdentityRotated);
    assert_eq!(resp.entries[0].status, AuditStatus::Success);
}

// ---------------------------------------------------------------------------
// Finding C — WILAYA rotation certificates are Root-issued only (issuer None)
// ---------------------------------------------------------------------------

#[test]
fn sec005c_wilaya_finalize_rejects_non_root_issuer_before_persistence() {
    let mut node = fresh_node();
    let before = bootstrap_wilaya(&mut node);

    let plan = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect("begin wilaya rotation");
    // Root-signed but carrying a non-Root issuer claim — the R5-broken case the
    // trust anchor path already rejects (identity_trust_anchor_service.rs).
    let mut claimed = plan.certificate.clone();
    claimed.issuer_identity_id = Some(uuid::Uuid::new_v4());
    let signed = sign_with_root(&claimed);

    let dir = TempDir::new().expect("temp dir");
    let package_path = dir.path().join("rotation.sync");
    let crypto = AgeFileEncryptionProvider::new();

    let err = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&signed, &package_path, &wilaya_settings(), &crypto)
        .expect_err("issuer claim must be rejected");
    assert!(
        err.to_string().contains("offline Authority Root"),
        "got {err:?}"
    );

    // Rejection happens BEFORE persistence: identity untouched, staged key
    // unconsumed, no package file written, zero new identity rows.
    assert!(active_wilaya(&node).is_identical_to(&before));
    assert!(
        node.node_key_store
            .read_pending()
            .expect("read pending")
            .is_some(),
        "staged key must remain available for a corrected retry"
    );
    assert!(!package_path.exists(), "no trust package may be written");
    assert_eq!(
        node.db
            .executor()
            .identity_store()
            .list_all()
            .expect("list")
            .len(),
        1,
        "identity store must be unchanged"
    );
}

#[test]
fn sec005c_wilaya_finalize_accepts_root_issued_issuer_none() {
    // Positive control: the Root-issued rotation plan (issuer None) is accepted
    // end-to-end. The exact command-level path is exercised by
    // `sec005b_wilaya_rotation_finalize_command_completes_and_records_audit`.
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);

    let plan = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .begin(SubjectType::Wilaya, RotationOperation::Rotate)
        .expect("begin wilaya rotation");
    assert_eq!(
        plan.certificate.issuer_identity_id, None,
        "WILAYA rotation plans are Root-issued (issuer None)"
    );
    let signed = sign_with_root(&plan.certificate);

    let dir = TempDir::new().expect("temp dir");
    let package_path = dir.path().join("rotation.sync");
    let crypto = AgeFileEncryptionProvider::new();
    let outcome = IdentityRotationCoordinator::new(&mut node.db, &node.node_key_store)
        .finalize_wilaya(&signed, &package_path, &wilaya_settings(), &crypto)
        .expect("Root-issued rotation accepted");
    assert!(matches!(outcome, RotationFinalizeOutcome::Completed { .. }));
}
