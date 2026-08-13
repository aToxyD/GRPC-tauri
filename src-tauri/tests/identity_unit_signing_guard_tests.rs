//! SEC-004 Stage 2 — UNIT bootstrap CSR signing guard (Option A+B).
//!
//! Scope: the WILAYA-side `sign_unit_bootstrap_request` path only. This is the
//! bootstrap *signing* authority (new node join), NOT the rotation path.
//!
//! Covered:
//! - duplicate-issuance guard: no ACTIVE → succeeds; ACTIVE exists → denied;
//!   only-REVOKED / only-SUPERSEDED histories allow re-issuance;
//! - issuer binding: `issuer_identity_id` is always the ACTIVE WILAYA and
//!   caller-supplied claims are ignored (R5);
//! - WILAYA-side registration: a successful sign records the UNIT identity in
//!   the Identity Store as Issuer Local State; failed attempts mutate nothing;
//! - command layer: `sign_unit_identity_request_impl` requires a WILAYA Admin
//!   session, and writes exactly one audit event per successful signing — never
//!   a false success audit on denied requests.
//!
//! The test authority Root uses the RFC 8032 §7.1 TEST 1 keypair — exactly the
//! `#[cfg(debug_assertions)]` development Root fallback in `root_public_key.rs`.

#[allow(dead_code)]
mod common;

use tempfile::TempDir;

use grpc_lib::application::services::{
    AuditService, FinalizeWilayaProvisionResult, IdentityProvisioningService,
};
use grpc_lib::commands::{identity::sign_unit_identity_request_impl, AppState};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::audit::{AuditAction, AuditFilters, AuditStatus};
use grpc_lib::domain::identity::{
    CredentialStatus, Ed25519CertificateSignature, IdentityCertificate, IdentitySignatureVerifier,
    IdentitySigner, IdentityStorePort, SubjectType, SIGNATURE_VERSION_ED25519,
};
use grpc_lib::errors::{AppError, AuthenticationError, BusinessLogicError};
use grpc_lib::infrastructure::identity::NodeKeyStore;
use grpc_lib::infrastructure::security::{Ed25519SignatureVerifier, Ed25519SigningProvider};
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

fn unit_uuid() -> uuid::Uuid {
    uuid::Uuid::parse_str(UNIT_ID).expect("unit id uuid")
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

fn active_wilaya_cert(node: &Node) -> IdentityCertificate {
    node.db
        .executor()
        .identity_store()
        .get_active_by_subject_type(SubjectType::Wilaya)
        .expect("query")
        .expect("ACTIVE WILAYA present")
}

/// Build an unsigned UNIT bootstrap CSR for `subject_id` using a fresh keypair
/// and a caller-chosen credential identity (mimics `begin_unit_provision`).
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
        status: CredentialStatus::Active,
        public_key: signer.public_key().to_vec(),
        algorithm_version: SIGNATURE_VERSION_ED25519,
        not_after: None,
        package_sequence: None,
        signature: None,
    }
}

/// Sign the CSR through the hardened WILAYA bootstrap authority.
fn sign_bootstrap(wilaya: &mut Node, csr: &IdentityCertificate) -> IdentityCertificate {
    IdentityProvisioningService::new(&mut wilaya.db)
        .sign_unit_bootstrap_request(csr, &wilaya.node_key_store, FIXED_NOW)
        .expect("bootstrap sign succeeded")
}

fn stored_active_unit(node: &Node) -> IdentityCertificate {
    node.db
        .executor()
        .identity_store()
        .get_active_by_subject(SubjectType::Unit, &unit_uuid())
        .expect("query")
        .expect("ACTIVE UNIT identity present")
}

fn is_operation_not_permitted(err: AppError, needle: &str) {
    match err {
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { message }) => {
            assert!(
                message.contains(needle),
                "message '{message}' should contain '{needle}'"
            );
        }
        other => panic!("expected OperationNotPermitted, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Duplicate-issuance guard: no ACTIVE UNIT identity → signed + registered
// ---------------------------------------------------------------------------

#[test]
fn bootstrap_sign_succeeds_without_active_unit_identity() {
    let mut wilaya = fresh_wilaya_with_unit();
    let wilaya_cert = active_wilaya_cert(&wilaya);
    let csr = unit_csr(
        unit_uuid(),
        [1; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );

    let signed = sign_bootstrap(&mut wilaya, &csr);

    // Issuer bound to the ACTIVE WILAYA (R5), never caller-controlled.
    assert_eq!(
        signed.issuer_identity_id,
        Some(wilaya_cert.identity_id)
    );
    // Signed under the WILAYA key and verifiable.
    let signature = signed.signature.as_ref().expect("signature present");
    assert!(Ed25519SignatureVerifier
        .verify_certificate(&signed, &wilaya_cert.public_key, signature)
        .expect("verify"));
    // WILAYA-side Issuer Local State registered.
    let stored = stored_active_unit(&wilaya);
    assert_eq!(stored.identity_id, signed.identity_id);
    assert_eq!(stored.credential_id, csr.credential_id);
    assert_eq!(stored.status, CredentialStatus::Active);
}

// ---------------------------------------------------------------------------
// Duplicate-issuance guard: ACTIVE exists → denied, nothing mutated
// ---------------------------------------------------------------------------

#[test]
fn bootstrap_sign_denied_when_active_unit_identity_exists() {
    let mut wilaya = fresh_wilaya_with_unit();
    let csr1 = unit_csr(
        unit_uuid(),
        [1; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );
    let first = sign_bootstrap(&mut wilaya, &csr1);

    let csr2 = unit_csr(
        unit_uuid(),
        [2; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );
    let err = IdentityProvisioningService::new(&mut wilaya.db)
        .sign_unit_bootstrap_request(&csr2, &wilaya.node_key_store, FIXED_NOW)
        .expect_err("duplicate issuance denied");
    is_operation_not_permitted(err, "duplicate bootstrap issuance is denied");

    // No mutation: the store still holds the FIRST certificate as ACTIVE and
    // the second credential was never registered.
    let stored = stored_active_unit(&wilaya);
    assert_eq!(stored.identity_id, first.identity_id);
    assert_eq!(stored.credential_id, csr1.credential_id);
    assert!(wilaya
        .db
        .executor()
        .identity_store()
        .max_generation_for_credential(&csr2.credential_id)
        .expect("query")
        .is_none());
}

// ---------------------------------------------------------------------------
// Duplicate-issuance guard: lifecycle transitions allow re-issuance
// ---------------------------------------------------------------------------

#[test]
fn bootstrap_sign_allowed_when_only_revoked_identity_exists() {
    let mut wilaya = fresh_wilaya_with_unit();
    let csr1 = unit_csr(
        unit_uuid(),
        [1; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );
    sign_bootstrap(&mut wilaya, &csr1);

    // Revoke the stored ACTIVE cert (rotation-style lifecycle transition).
    let mut revoked = stored_active_unit(&wilaya);
    revoked.status = CredentialStatus::Revoked;
    wilaya
        .db
        .executor()
        .identity_store()
        .upsert(&revoked, FIXED_NOW)
        .expect("mark revoked");

    let csr2 = unit_csr(
        unit_uuid(),
        [2; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );
    let second = sign_bootstrap(&mut wilaya, &csr2);

    assert_eq!(second.credential_id, csr2.credential_id);
    assert_eq!(stored_active_unit(&wilaya).credential_id, csr2.credential_id);
}

#[test]
fn bootstrap_sign_allowed_when_only_superseded_identity_exists() {
    let mut wilaya = fresh_wilaya_with_unit();
    let csr1 = unit_csr(
        unit_uuid(),
        [1; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );
    sign_bootstrap(&mut wilaya, &csr1);

    let mut superseded = stored_active_unit(&wilaya);
    superseded.status = CredentialStatus::Superseded;
    wilaya
        .db
        .executor()
        .identity_store()
        .upsert(&superseded, FIXED_NOW)
        .expect("mark superseded");

    let csr2 = unit_csr(
        unit_uuid(),
        [2; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );
    let second = sign_bootstrap(&mut wilaya, &csr2);

    assert_eq!(second.credential_id, csr2.credential_id);
    assert_eq!(stored_active_unit(&wilaya).credential_id, csr2.credential_id);
}

// ---------------------------------------------------------------------------
// Issuer binding: caller-controlled issuer claims are ignored (R5)
// ---------------------------------------------------------------------------

#[test]
fn bootstrap_sign_binds_issuer_to_active_wilaya_ignoring_caller_claims() {
    let mut wilaya = fresh_wilaya_with_unit();
    let wilaya_cert = active_wilaya_cert(&wilaya);

    let mut csr = unit_csr(
        unit_uuid(),
        [1; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );
    csr.issuer_identity_id = Some(uuid::Uuid::nil());

    let signed = sign_bootstrap(&mut wilaya, &csr);

    assert_eq!(
        signed.issuer_identity_id,
        Some(wilaya_cert.identity_id),
        "issuer must be the ACTIVE WILAYA, never the caller-supplied claim"
    );
    let signature = signed.signature.as_ref().expect("signature present");
    assert!(Ed25519SignatureVerifier
        .verify_certificate(&signed, &wilaya_cert.public_key, signature)
        .expect("verify under WILAYA key"));
}

// ---------------------------------------------------------------------------
// Command layer: WILAYA Admin only + audit records
// ---------------------------------------------------------------------------

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
        let signature = root_signer()
            .sign_certificate(&request)
            .expect("root signature");
        let mut signed = request;
        signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("wrap"));
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

fn audit_for(state: &AppState) -> grpc_lib::domain::audit::AuditLogResponse {
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let filters = AuditFilters {
        action: Some("SignUnitIdentityRequest".to_string()),
        ..Default::default()
    };
    AuditService::new(db.executor())
        .get_audit_entries(&filters, 0, 100)
        .expect("audit query")
}

#[test]
fn command_wilaya_admin_signs_unit_and_records_audit() {
    let (state, dir) = wilaya_admin_command_state();
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
    let wilaya_cert = {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.executor()
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)
            .expect("query")
            .expect("ACTIVE WILAYA present")
    };
    let csr = unit_csr(
        unit_uuid(),
        [1; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );

    let certificate = sign_unit_identity_request_impl(
        &state,
        serde_json::to_string(&csr).expect("csr json"),
        &node_key_store,
    )
    .expect("WILAYA Admin signs");

    assert_eq!(
        certificate.issuer_identity_id,
        Some(wilaya_cert.identity_id)
    );

    let resp = audit_for(&state);
    assert_eq!(resp.total_count, 1);
    let entry = &resp.entries[0];
    assert_eq!(entry.user_id, "u1");
    assert_eq!(entry.username, "wilaya-admin");
    assert_eq!(entry.action, AuditAction::SignUnitIdentityRequest);
    assert_eq!(entry.entity_type, grpc_lib::domain::audit::EntityType::System);
    assert_eq!(entry.entity_id, Some(certificate.identity_id.to_string()));
    assert_eq!(entry.entity_name.as_deref(), Some(UNIT_ID));
    assert_eq!(entry.status, AuditStatus::Success);
    assert_eq!(
        entry
            .metadata
            .as_ref()
            .and_then(|m| m.get("issuer_identity_id"))
            .and_then(|v| v.as_str()),
        Some(wilaya_cert.identity_id.to_string().as_str())
    );
    assert!(entry.session_id.is_some(), "actor session recorded");
}

#[test]
fn command_denied_unauthenticated_records_no_audit() {
    let (state, dir) = wilaya_admin_command_state();
    *state.current_session.lock().expect("session lock") = None;
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
    let csr = unit_csr(
        unit_uuid(),
        [1; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );

    let err = sign_unit_identity_request_impl(
        &state,
        serde_json::to_string(&csr).expect("csr json"),
        &node_key_store,
    )
    .expect_err("unauthenticated denied");
    match err {
        AppError::Authentication(AuthenticationError::SessionNotFound) => {}
        other => panic!("expected SessionNotFound, got {other:?}"),
    }

    assert_eq!(audit_for(&state).total_count, 0);
}

#[test]
fn command_denied_duplicate_records_no_false_success_audit() {
    let (state, dir) = wilaya_admin_command_state();
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
    let csr1 = unit_csr(
        unit_uuid(),
        [1; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );
    sign_unit_identity_request_impl(
        &state,
        serde_json::to_string(&csr1).expect("csr json"),
        &node_key_store,
    )
    .expect("first signing succeeds");

    let csr2 = unit_csr(
        unit_uuid(),
        [2; 32],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
    );
    let err = sign_unit_identity_request_impl(
        &state,
        serde_json::to_string(&csr2).expect("csr json"),
        &node_key_store,
    )
    .expect_err("duplicate denied");
    is_operation_not_permitted(err, "duplicate bootstrap issuance is denied");

    let resp = audit_for(&state);
    assert_eq!(resp.total_count, 1, "no false success audit on the denied attempt");
    assert_eq!(resp.entries[0].status, AuditStatus::Success);
}
