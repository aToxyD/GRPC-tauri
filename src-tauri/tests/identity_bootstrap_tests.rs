//! Identity bootstrap B5 integration tests.
//!
//! RFC 2026-08-04-node-identity-trust §3.6–3.7 / ADR-0038 / ADR-0039.
//!
//! Covers the offline Root bootstrap flow ("Root signs fully offline"):
//! - derived state machine transitions (Uninitialized → … → Ready);
//! - one-way transition guards (reverse / out-of-order rejected);
//! - idempotent `finalize_wilaya_provision` (identical re-import = zero writes);
//! - fail-closed verification matrix (wrong Root sig, pubkey swap, issuer, subject);
//! - `complete_with_passphrase` Challenge–Response E2E + replay protection;
//! - additive legacy window (pre-existing hashed admin keeps password access);
//! - identity-only admin (empty hash) and the production seeding gate.
//!
//! The test authority Root uses the RFC 8032 §7.1 TEST 1 keypair, which is
//! exactly the `#[cfg(debug_assertions)]` development Root fallback in
//! `root_public_key.rs` — so `finalize` resolves the correct Root public key
//! without any process-environment mutation (parallel-safe).

#[allow(dead_code)]
mod common;

use std::sync::{Arc, Mutex};

use tempfile::TempDir;

use grpc_lib::application::services::{
    AuditService, FinalizeWilayaProvisionResult, IdentityAuthenticationPolicy,
    IdentityBootstrapStatusService, IdentityChallengeService, IdentityProvisioningService,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::audit::{AuditFilters, AuditStatus};
use grpc_lib::domain::identity::{
    AdminKeyFile, ChallengeMessage, CredentialStatus, Ed25519CertificateSignature,
    IdentityBootstrapState, IdentityCertificate, IdentityChallengeState, IdentitySigner,
    IdentityStorePort, SubjectType, ADMINKEY_FORMAT_VERSION, IDENTITY_ALGORITHM_PROFILE_ED25519,
};
use grpc_lib::domain::security::PasswordHashPort;
use grpc_lib::errors::AuthenticationError;
use grpc_lib::infrastructure::identity::{AdminKeyProvider, NodeKeyStore};
use grpc_lib::infrastructure::security::{
    Argon2PasswordHashProvider, Ed25519SignatureVerifier, Ed25519SigningProvider,
};
use grpc_lib::models::UserRole;
use grpc_lib::repositories::identity_store::IdentityStoreRepository;
use grpc_lib::repositories::RepositoryProvider;

/// RFC 8032 §7.1 TEST 1 secret — the matching public key IS the debug-mode
/// development Root fallback (root_public_key.rs). Never a production key.
const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
/// Operator passphrase protecting the portable `.adminkey`.
const ADMIN_PASSPHRASE: &str = "correct horse battery staple";
const BOOTSTRAP_USERNAME: &str = "admin";
/// Fixed timestamp for deterministic persistence.
const FIXED_NOW: &str = "2026-08-04T00:00:00Z";

/// A fresh node directory: real DB + node key store + `.adminkey` provider.
struct Node {
    _dir: TempDir,
    db: Database,
    node_key_store: NodeKeyStore,
    adminkey_provider: AdminKeyProvider,
}

fn fresh_node() -> Node {
    let dir = TempDir::new().expect("temp dir");
    let db = ConnectionFactory::new_for_test().expect("db");
    let node_dir = dir.path().join("node");
    Node {
        _dir: dir,
        db,
        node_key_store: NodeKeyStore::new(node_dir.clone()),
        adminkey_provider: AdminKeyProvider::new(node_dir.join("adminkey")),
    }
}

fn status(node: &Node) -> IdentityBootstrapState {
    IdentityBootstrapStatusService::compute(&node.db, &node.node_key_store, &node.adminkey_provider)
        .expect("status computed")
}

fn root_signer() -> Ed25519SigningProvider {
    Ed25519SigningProvider::new(TEST_ROOT_SECRET)
}

/// Full offline flow: request (node keygen + CSR), offline Root signs, finalize.
fn bootstrap_wilaya(node: &mut Node) -> IdentityCertificate {
    bootstrap_wilaya_using(&mut node.db, &node.node_key_store)
}

/// `bootstrap_wilaya` core, parameterized over the store so the SEC-002-R
/// command harness can provision through the SAME global paths the command
/// resolves (`default_data_dir()`).
fn bootstrap_wilaya_using(
    db: &mut Database,
    node_key_store: &NodeKeyStore,
) -> IdentityCertificate {
    let mut provisioning = IdentityProvisioningService::new(db);
    let request = provisioning
        .generate_wilaya_request(uuid::Uuid::new_v4(), node_key_store)
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

/// Remove the seeded admin so the node behaves like a fresh B5 fleet node.
fn remove_seeded_admin(node: &mut Node) {
    remove_seeded_admin_using(&mut node.db)
}

fn remove_seeded_admin_using(db: &mut Database) {
    let admin = db
        .executor()
        .users()
        .get_user_by_username(BOOTSTRAP_USERNAME)
        .expect("seed query")
        .expect("seeded admin present");
    db.executor()
        .users()
        .delete_user(&admin.id)
        .expect("seeded admin removed");
}

fn challenge_service(
    node: &Node,
) -> (Arc<Mutex<IdentityChallengeState>>, IdentityChallengeService) {
    let state = Arc::new(Mutex::new(IdentityChallengeState::default()));
    let service = IdentityChallengeService::new(
        state.clone(),
        node.adminkey_provider.clone(),
        Arc::new(Ed25519SignatureVerifier),
        Arc::new(Argon2PasswordHashProvider),
        Arc::new(Mutex::new(
            grpc_lib::domain::rate_limiter::RateLimiter::new(),
        )),
    );
    (state, service)
}

fn begin_challenge(service: &IdentityChallengeService, node: &Node) -> ChallengeMessage {
    service
        .begin(&IdentityStoreRepository::new(node.db.executor()))
        .expect("challenge issued")
}

// ---------------------------------------------------------------------------
// Derived state machine
// ---------------------------------------------------------------------------

#[test]
fn fresh_node_is_uninitialized_then_waiting_for_root_certificate() {
    let mut node = fresh_node();
    assert_eq!(status(&node), IdentityBootstrapState::Uninitialized);

    let provisioning = IdentityProvisioningService::new(&mut node.db);
    provisioning
        .generate_wilaya_request(uuid::Uuid::new_v4(), &node.node_key_store)
        .expect("csr generated");

    assert_eq!(
        status(&node),
        IdentityBootstrapState::WaitingForRootCertificate
    );
}

#[test]
fn full_bootstrap_sequence_reaches_ready_on_identity_only_admin() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);

    let wilaya = bootstrap_wilaya(&mut node);
    assert_eq!(wilaya.subject_type, SubjectType::Wilaya);
    assert_eq!(status(&node), IdentityBootstrapState::WilayaActive);

    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    let admin_cert = provisioning
        .issue_first_admin_key(
            BOOTSTRAP_USERNAME,
            ADMIN_PASSPHRASE,
            &node.node_key_store,
            &node.adminkey_provider,
            FIXED_NOW,
        )
        .expect("admin issued");
    assert_eq!(admin_cert.subject_type, SubjectType::Admin);
    assert_eq!(admin_cert.status, CredentialStatus::Active);

    // The admin user was created identity-only (empty hash) → Ready.
    assert_eq!(status(&node), IdentityBootstrapState::Ready);

    // E2E: complete_with_passphrase establishes a session reusing the challenge id.
    let (_, service) = challenge_service(&node);
    let challenge = begin_challenge(&service, &node);
    let established = service
        .complete_with_passphrase(&mut node.db, &challenge.session_id, ADMIN_PASSPHRASE)
        .expect("challenge login succeeds");
    assert_eq!(established.session.username, BOOTSTRAP_USERNAME);
    assert_eq!(
        established.session.session_id,
        challenge.session_id.to_string()
    );
}

#[test]
fn additive_window_keeps_legacy_password_until_hash_cleared() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);

    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    provisioning
        .issue_first_admin_key(
            BOOTSTRAP_USERNAME,
            ADMIN_PASSPHRASE,
            &node.node_key_store,
            &node.adminkey_provider,
            FIXED_NOW,
        )
        .expect("admin issued");

    // Legacy hashed admin was linked WITHOUT clearing the hash → AdminProvisioned.
    assert_eq!(status(&node), IdentityBootstrapState::AdminProvisioned);

    // Legacy password path still authenticates (additive window). B8: the
    // seeded admin is the fleet-wide `admin`, so its hash verifies in the
    // global admin domain.
    let admin = node
        .db
        .executor()
        .users()
        .get_user_by_username(BOOTSTRAP_USERNAME)
        .expect("query")
        .expect("admin present");
    assert!(!admin.password_hash.is_empty());
    let port = Argon2PasswordHashProvider;
    assert!(port
        .verify_admin("admin", &admin.password_hash)
        .expect("verify ok"));

    // Clearing the hash (identity cutover, B6) flips the state to Ready.
    node.db
        .executor()
        .users()
        .change_password(&admin.id, "", FIXED_NOW)
        .expect("hash cleared");
    assert_eq!(status(&node), IdentityBootstrapState::Ready);
}

// ---------------------------------------------------------------------------
// One-way transition guards
// ---------------------------------------------------------------------------

#[test]
fn issue_first_admin_before_wilaya_is_rejected() {
    let mut node = fresh_node();
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    let err = provisioning
        .issue_first_admin_key(
            BOOTSTRAP_USERNAME,
            ADMIN_PASSPHRASE,
            &node.node_key_store,
            &node.adminkey_provider,
            FIXED_NOW,
        )
        .expect_err("must reject without ACTIVE WILAYA");
    assert!(err.to_string().contains("ACTIVE WILAYA"), "got {err:?}");
}

#[test]
fn begin_wilaya_request_is_single_shot() {
    let mut node = fresh_node();
    let provisioning = IdentityProvisioningService::new(&mut node.db);
    provisioning
        .generate_wilaya_request(uuid::Uuid::new_v4(), &node.node_key_store)
        .expect("first request");
    let err = provisioning
        .generate_wilaya_request(uuid::Uuid::new_v4(), &node.node_key_store)
        .expect_err("second request must be rejected");
    assert!(err.to_string().contains("already"), "got {err:?}");
}

#[test]
fn finalize_without_prior_begin_is_rejected() {
    let mut node = fresh_node();
    let cert = IdentityCertificate {
        identity_id: uuid::Uuid::new_v4(),
        subject_type: SubjectType::Wilaya,
        subject_id: uuid::Uuid::new_v4(),
        issuer_identity_id: None,
        credential_id: uuid::Uuid::new_v4(),
        generation: 1,
        status: CredentialStatus::Active,
        public_key: vec![7u8; 32],
        algorithm_version: 2,
        not_after: None,
        package_sequence: None,
        signature: Some(Ed25519CertificateSignature::from_bytes([0u8; 64])),
    };
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    // No node key exists → the node-secret read fails closed.
    assert!(provisioning
        .finalize_wilaya_provision(&cert, &node.node_key_store, FIXED_NOW)
        .is_err());
}

#[test]
fn duplicate_admin_for_same_subject_is_rejected() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);

    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    let first = provisioning
        .issue_first_admin_key(
            BOOTSTRAP_USERNAME,
            ADMIN_PASSPHRASE,
            &node.node_key_store,
            &node.adminkey_provider,
            FIXED_NOW,
        )
        .expect("first admin");
    let err = provisioning
        .issue_first_admin_key(
            BOOTSTRAP_USERNAME,
            ADMIN_PASSPHRASE,
            &node.node_key_store,
            &node.adminkey_provider,
            FIXED_NOW,
        )
        .expect_err("second admin for the same subject must be rejected");
    assert!(err.to_string().contains("already exists"), "got {err:?}");
    assert_eq!(first.credential_id, first.credential_id);
}

// ---------------------------------------------------------------------------
// Idempotent finalize + fail-closed matrix
// ---------------------------------------------------------------------------

#[test]
fn finalize_is_idempotent_for_identical_certificate() {
    let mut node = fresh_node();
    let request = {
        let provisioning = IdentityProvisioningService::new(&mut node.db);
        provisioning
            .generate_wilaya_request(uuid::Uuid::new_v4(), &node.node_key_store)
            .expect("csr")
    };
    let signature = root_signer()
        .sign_certificate(&request)
        .expect("root signed");
    let mut signed = request.clone();
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig"));

    let first = {
        let mut provisioning = IdentityProvisioningService::new(&mut node.db);
        provisioning
            .finalize_wilaya_provision(&signed, &node.node_key_store, FIXED_NOW)
            .expect("first finalize")
    };
    match first {
        FinalizeWilayaProvisionResult::Provisioned(_) => {}
        FinalizeWilayaProvisionResult::AlreadyProvisioned(_) => {
            panic!("first finalize must provision")
        }
    }

    let before = node
        .db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .len();
    let outcome = {
        let mut provisioning = IdentityProvisioningService::new(&mut node.db);
        provisioning
            .finalize_wilaya_provision(&signed, &node.node_key_store, FIXED_NOW)
            .expect("re-finalize identical")
    };
    let FinalizeWilayaProvisionResult::AlreadyProvisioned(stored) = outcome else {
        panic!("identical re-import must be a no-op");
    };
    assert!(stored.is_identical_to(&signed));
    let after = node
        .db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .len();
    assert_eq!(
        before, after,
        "identical re-import must perform ZERO writes"
    );
}

#[test]
fn finalize_rejects_different_wilaya_certificate_for_provisioned_node() {
    let mut node = fresh_node();
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);

    let request = provisioning
        .generate_wilaya_request(uuid::Uuid::new_v4(), &node.node_key_store)
        .expect("csr");
    let signature = root_signer()
        .sign_certificate(&request)
        .expect("root signed");
    let mut signed = request;
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig"));
    provisioning
        .finalize_wilaya_provision(&signed, &node.node_key_store, FIXED_NOW)
        .expect("first finalize");

    // A DIFFERENT certificate (new identity id) for the already-provisioned node.
    let mut other = signed.clone();
    other.identity_id = uuid::Uuid::new_v4();
    other.credential_id = uuid::Uuid::new_v4();
    let signature = root_signer().sign_certificate(&other).expect("root signed");
    other.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig"));

    let err = provisioning
        .finalize_wilaya_provision(&other, &node.node_key_store, FIXED_NOW)
        .expect_err("different WILAYA cert must fail closed");
    assert!(err.to_string().contains("already exists"), "got {err:?}");
}

#[test]
fn finalize_rejects_wrong_root_signature() {
    let mut node = fresh_node();
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    let request = provisioning
        .generate_wilaya_request(uuid::Uuid::new_v4(), &node.node_key_store)
        .expect("csr");

    // Signed by a different Root (NOT the pinned/development Root).
    let imposter = Ed25519SigningProvider::new([7u8; 32]);
    let signature = imposter
        .sign_certificate(&request)
        .expect("imposter signed");
    let mut signed = request;
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig"));

    let err = provisioning
        .finalize_wilaya_provision(&signed, &node.node_key_store, FIXED_NOW)
        .expect_err("imposter signature must fail closed");
    assert!(
        err.to_string().contains("not valid for the Authority Root"),
        "got {err:?}"
    );
}

#[test]
fn finalize_rejects_certificate_public_key_that_does_not_match_node_key() {
    let mut node = fresh_node();
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    let mut request = provisioning
        .generate_wilaya_request(uuid::Uuid::new_v4(), &node.node_key_store)
        .expect("csr");

    // Swap the public key so the certificate is bound to a different device.
    let foreign = Ed25519SigningProvider::new([9u8; 32]);
    request.public_key = foreign.public_key();
    let signature = root_signer()
        .sign_certificate(&request)
        .expect("root signed");
    let mut signed = request;
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig"));

    let err = provisioning
        .finalize_wilaya_provision(&signed, &node.node_key_store, FIXED_NOW)
        .expect_err("cross-device public key must fail closed");
    assert!(
        err.to_string()
            .contains("does not match the node signing key"),
        "got {err:?}"
    );
}

#[test]
fn finalize_rejects_node_issuer_and_non_wilaya_subject() {
    let mut node = fresh_node();
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    let request = provisioning
        .generate_wilaya_request(uuid::Uuid::new_v4(), &node.node_key_store)
        .expect("csr");

    let mut with_issuer = request.clone();
    with_issuer.issuer_identity_id = Some(uuid::Uuid::new_v4());
    let signature = root_signer()
        .sign_certificate(&with_issuer)
        .expect("root signed");
    let mut signed = with_issuer;
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig"));
    let err = provisioning
        .finalize_wilaya_provision(&signed, &node.node_key_store, FIXED_NOW)
        .expect_err("node-issued WILAYA cert must fail closed");
    assert!(
        err.to_string().contains("offline Authority Root"),
        "got {err:?}"
    );

    let mut as_admin = request.clone();
    as_admin.subject_type = SubjectType::Admin;
    let signature = root_signer()
        .sign_certificate(&as_admin)
        .expect("root signed");
    let mut signed = as_admin;
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig"));
    let err = provisioning
        .finalize_wilaya_provision(&signed, &node.node_key_store, FIXED_NOW)
        .expect_err("non-WILAYA subject must fail closed");
    assert!(
        err.to_string().contains("WILAYA certificate"),
        "got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Challenge–Response fail-closed behavior
// ---------------------------------------------------------------------------

#[test]
fn challenge_replay_via_passphrase_is_rejected() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    provisioning
        .issue_first_admin_key(
            BOOTSTRAP_USERNAME,
            ADMIN_PASSPHRASE,
            &node.node_key_store,
            &node.adminkey_provider,
            FIXED_NOW,
        )
        .expect("admin issued");

    let (_, service) = challenge_service(&node);
    let challenge = begin_challenge(&service, &node);
    service
        .complete_with_passphrase(&mut node.db, &challenge.session_id, ADMIN_PASSPHRASE)
        .expect("first completion succeeds");

    let replay = service
        .complete_with_passphrase(&mut node.db, &challenge.session_id, ADMIN_PASSPHRASE)
        .expect_err("replayed completion must be rejected");
    assert!(
        matches!(
            replay,
            grpc_lib::errors::AppError::Authentication(
                AuthenticationError::InvalidCredentials { .. }
            )
        ),
        "got {replay:?}"
    );
}

#[test]
fn challenge_wrong_passphrase_fails_closed() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    provisioning
        .issue_first_admin_key(
            BOOTSTRAP_USERNAME,
            ADMIN_PASSPHRASE,
            &node.node_key_store,
            &node.adminkey_provider,
            FIXED_NOW,
        )
        .expect("admin issued");

    let (_, service) = challenge_service(&node);
    let challenge = begin_challenge(&service, &node);
    assert!(service
        .complete_with_passphrase(&mut node.db, &challenge.session_id, "wrong passphrase")
        .is_err());

    // The challenge was consumed by the failed attempt (atomic replay guard).
    assert!(service
        .complete_with_passphrase(&mut node.db, &challenge.session_id, ADMIN_PASSPHRASE)
        .is_err());
}

#[test]
fn identity_only_admin_has_empty_hash_and_no_password_path() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    provisioning
        .issue_first_admin_key(
            BOOTSTRAP_USERNAME,
            ADMIN_PASSPHRASE,
            &node.node_key_store,
            &node.adminkey_provider,
            FIXED_NOW,
        )
        .expect("admin issued");

    let admin = node
        .db
        .executor()
        .users()
        .get_user_by_username(BOOTSTRAP_USERNAME)
        .expect("query")
        .expect("admin present");
    assert!(
        admin.password_hash.is_empty(),
        "identity-only admin MUST have an empty password hash"
    );
    // The password verifier fails closed on empty hash material (no parse).
    let port = Argon2PasswordHashProvider;
    assert!(port
        .verify_node("anything", &admin.node_id, &admin.password_hash)
        .is_err());
}

// ---------------------------------------------------------------------------
// B6-A/B6-B: password login gate (IdentityAuthenticationPolicy)
// ---------------------------------------------------------------------------

#[test]
fn password_login_gate_is_sole_security_fact() {
    // No identity material → the password path stays open.
    let node = fresh_node();
    assert!(
        IdentityAuthenticationPolicy::password_login_allowed(&node.db, &node.adminkey_provider)
            .expect("policy"),
        "unprovisioned node must keep the password path"
    );

    // Full bootstrap → ACTIVE ADMIN identity (cert + `.adminkey`) → password closed.
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);
    IdentityProvisioningService::new(&mut node.db)
        .issue_first_admin_key(
            BOOTSTRAP_USERNAME,
            ADMIN_PASSPHRASE,
            &node.node_key_store,
            &node.adminkey_provider,
            FIXED_NOW,
        )
        .expect("admin issued");
    assert!(
        IdentityAuthenticationPolicy::has_active_admin_identity(&node.db, &node.adminkey_provider,)
            .expect("fact"),
        "ACTIVE ADMIN cert + `.adminkey` must be detected"
    );
    assert!(
        !IdentityAuthenticationPolicy::password_login_allowed(&node.db, &node.adminkey_provider)
            .expect("policy"),
        "ACTIVE ADMIN identity must close the password path"
    );

    // Fail-safe: ACTIVE ADMIN cert WITHOUT the `.adminkey` file must NOT lock
    // the operator out — the password path stays open (B6-A refinement 1).
    std::fs::remove_file(node.adminkey_provider.file_path()).expect("remove adminkey");
    assert!(
        !IdentityAuthenticationPolicy::has_active_admin_identity(
            &node.db,
            &node.adminkey_provider,
        )
        .expect("fact"),
        "missing `.adminkey` must disable the identity fact"
    );
    assert!(
        IdentityAuthenticationPolicy::password_login_allowed(&node.db, &node.adminkey_provider)
            .expect("policy"),
        "missing `.adminkey` must keep the password path open (no lockout)"
    );
}

// ---------------------------------------------------------------------------
// SEC-002 — PRE-AUTH First Admin Provisioning hardening
//
// SEC002-01..12: one-time backend gate (A), canonical first-admin identity (B),
// crash-consistent provisioning + recovery (C), and secret-free audit (D).
// ---------------------------------------------------------------------------

fn active_admin_count(node: &Node) -> usize {
    node.db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .iter()
        .filter(|c| c.subject_type == SubjectType::Admin && c.status == CredentialStatus::Active)
        .count()
}

fn raw_admin_cert(subject_id: uuid::Uuid) -> IdentityCertificate {
    IdentityCertificate {
        identity_id: uuid::Uuid::new_v4(),
        subject_type: SubjectType::Admin,
        subject_id,
        issuer_identity_id: None,
        credential_id: uuid::Uuid::new_v4(),
        generation: 1,
        status: CredentialStatus::Active,
        public_key: vec![7u8; 32],
        algorithm_version: 2,
        not_after: None,
        package_sequence: None,
        signature: None,
    }
}

fn root_signed_cert(cert: &mut IdentityCertificate) {
    let signature = root_signer()
        .sign_certificate(&cert.clone())
        .expect("root signed");
    cert.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig wrap"));
}

/// Write a `.adminkey` embedding `cert`, as if it were orphaned by a crash.
fn write_stray_adminkey(node: &Node, cert: IdentityCertificate) {
    let encrypted = node
        .adminkey_provider
        .encrypt_private_key(&[9u8; 32], ADMIN_PASSPHRASE)
        .expect("stray key encrypted");
    node.adminkey_provider
        .write(&AdminKeyFile {
            format_version: ADMINKEY_FORMAT_VERSION,
            algorithm_version: IDENTITY_ALGORITHM_PROFILE_ED25519,
            certificate: cert,
            encrypted_private_key: encrypted,
        })
        .expect("stray adminkey written");
}

/// E2E: the admin authenticates via Challenge–Response using the `.adminkey`.
fn verify_admin_usable(node: &mut Node) {
    let (_, service) = challenge_service(node);
    let challenge = begin_challenge(&service, node);
    service
        .complete_with_passphrase(&mut node.db, &challenge.session_id, ADMIN_PASSPHRASE)
        .expect("admin challenge authentication must succeed");
}

fn issue_admin(
    node: &mut Node,
    username: &str,
) -> Result<IdentityCertificate, grpc_lib::errors::AppError> {
    issue_admin_using(
        &mut node.db,
        &node.node_key_store,
        &node.adminkey_provider,
        username,
    )
}

/// `issue_admin` core, parameterized over the stores so the SEC-002-R command
/// harness can issue through the SAME global paths the command resolves.
fn issue_admin_using(
    db: &mut Database,
    node_key_store: &NodeKeyStore,
    adminkey_provider: &AdminKeyProvider,
    username: &str,
) -> Result<IdentityCertificate, grpc_lib::errors::AppError> {
    IdentityProvisioningService::new(db).issue_first_admin_key(
        username,
        ADMIN_PASSPHRASE,
        node_key_store,
        adminkey_provider,
        FIXED_NOW,
    )
}

#[test]
fn sec002_01_admin_issuance_requires_active_wilaya() {
    let mut node = fresh_node();
    let err = issue_admin(&mut node, BOOTSTRAP_USERNAME).expect_err("no ACTIVE WILAYA");
    assert!(err.to_string().contains("ACTIVE WILAYA"), "got {err:?}");
}

#[test]
fn sec002_02_first_admin_succeeds_and_is_usable() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);

    let cert = issue_admin(&mut node, BOOTSTRAP_USERNAME).expect("first admin issued");
    assert_eq!(cert.subject_type, SubjectType::Admin);
    assert_eq!(cert.status, CredentialStatus::Active);
    assert_eq!(active_admin_count(&node), 1);
    assert!(node.adminkey_provider.exists());
    assert_eq!(status(&node), IdentityBootstrapState::Ready);
    verify_admin_usable(&mut node);
}

#[test]
fn sec002_03_second_admin_attempt_is_rejected() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);
    issue_admin(&mut node, BOOTSTRAP_USERNAME).expect("first admin");

    let err = issue_admin(&mut node, BOOTSTRAP_USERNAME).expect_err("second must be rejected");
    assert!(err.to_string().contains("already exists"), "got {err:?}");
    assert_eq!(active_admin_count(&node), 1);
}

#[test]
fn sec002_04_different_username_is_rejected() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);
    issue_admin(&mut node, BOOTSTRAP_USERNAME).expect("first admin");

    let err = issue_admin(&mut node, "boss").expect_err("different username must be rejected");
    assert!(err.to_string().contains("canonical"), "got {err:?}");
    assert_eq!(active_admin_count(&node), 1);
}

#[test]
fn sec002_05_database_level_single_active_admin_invariant() {
    // Independent of the service gate: the DB itself must reject a second
    // ACTIVE ADMIN row (migration 008), so even a future insertion path cannot
    // break the one-time invariant. True cross-process concurrency is
    // impossible — the single-writer Mutex serializes every writer.
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    let admin = node
        .db
        .executor()
        .users()
        .get_user_by_username(BOOTSTRAP_USERNAME)
        .expect("query")
        .expect("seeded admin");
    let subject_id = uuid::Uuid::parse_str(&admin.id).expect("uuid");

    node.db
        .executor()
        .identity_store()
        .upsert(&raw_admin_cert(subject_id), FIXED_NOW)
        .expect("first ACTIVE ADMIN inserted");
    node.db
        .executor()
        .identity_store()
        .upsert(&raw_admin_cert(subject_id), FIXED_NOW)
        .expect_err("DB must reject a second ACTIVE ADMIN");
    assert_eq!(active_admin_count(&node), 1);
}

#[test]
fn sec002_06_non_canonical_username_is_rejected() {
    // The canonical check fires BEFORE the WILAYA gate, so a non-canonical name
    // is rejected even on a fully unprovisioned node.
    let mut node = fresh_node();
    let err = issue_admin(&mut node, "boss").expect_err("non-canonical must be rejected");
    assert!(err.to_string().contains("canonical"), "got {err:?}");
    assert_eq!(active_admin_count(&node), 0);
}

#[test]
fn sec002_07_retry_after_success_is_rejected_and_store_unchanged() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);
    let first = issue_admin(&mut node, BOOTSTRAP_USERNAME).expect("first admin");

    let err = issue_admin(&mut node, BOOTSTRAP_USERNAME).expect_err("retry must be rejected");
    assert!(err.to_string().contains("already exists"), "got {err:?}");
    let active = node
        .db
        .executor()
        .identity_store()
        .get_active_by_subject_type(SubjectType::Admin)
        .expect("query")
        .expect("admin present");
    assert_eq!(
        active.identity_id, first.identity_id,
        "retry must not replace the active admin"
    );
}

#[test]
fn sec002_08_crash_partial_state_is_detectable_and_recoverable() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);

    // SEC-002-C crash window: the `.adminkey` was written but the DB
    // transaction never committed — no certificate, no ACTIVE ADMIN.
    let mut stray = raw_admin_cert(uuid::Uuid::new_v4());
    root_signed_cert(&mut stray);
    write_stray_adminkey(&node, stray);
    assert!(node.adminkey_provider.exists());
    assert_eq!(
        status(&node),
        IdentityBootstrapState::WilayaActive,
        "partial provisioning state must remain detectable"
    );

    let cert = issue_admin(&mut node, BOOTSTRAP_USERNAME).expect("retry after crash succeeds");
    assert_eq!(cert.status, CredentialStatus::Active);
    assert_eq!(active_admin_count(&node), 1);
    verify_admin_usable(&mut node);
}

#[test]
fn sec002_09_legacy_cert_without_adminkey_is_recoverable() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    let admin = node
        .db
        .executor()
        .users()
        .get_user_by_username(BOOTSTRAP_USERNAME)
        .expect("query")
        .expect("seeded admin");
    let subject_id = uuid::Uuid::parse_str(&admin.id).expect("uuid");

    // Legacy crash state: ACTIVE ADMIN certificate, `.adminkey` missing.
    // Detectable as AdminProvisioned (never Ready) — never a permanent lockout.
    let dead = raw_admin_cert(subject_id);
    node.db
        .executor()
        .identity_store()
        .upsert(&dead, FIXED_NOW)
        .expect("dead admin inserted");
    assert_eq!(status(&node), IdentityBootstrapState::AdminProvisioned);
    assert!(!node.adminkey_provider.exists());

    let cert = issue_admin(&mut node, BOOTSTRAP_USERNAME).expect("recovery succeeds");
    assert_eq!(active_admin_count(&node), 1, "supersede keeps at most one ACTIVE ADMIN");
    assert_ne!(cert.identity_id, dead.identity_id);

    let dead_row = node
        .db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .into_iter()
        .find(|c| c.credential_id == dead.credential_id)
        .expect("dead credential row preserved");
    assert_eq!(dead_row.status, CredentialStatus::Superseded);

    assert!(node.adminkey_provider.exists());
    verify_admin_usable(&mut node);
}

#[test]
fn sec002_09_mismatched_adminkey_is_recoverable() {
    // Crash mid-recovery: the store still holds the OLD ACTIVE certificate but
    // the on-disk `.adminkey` embeds a DIFFERENT certificate. The ceremony must
    // detect the mismatch and recover (not treat it as a usable admin).
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    let admin = node
        .db
        .executor()
        .users()
        .get_user_by_username(BOOTSTRAP_USERNAME)
        .expect("query")
        .expect("seeded admin");
    let subject_id = uuid::Uuid::parse_str(&admin.id).expect("uuid");

    let dead = raw_admin_cert(subject_id);
    node.db
        .executor()
        .identity_store()
        .upsert(&dead, FIXED_NOW)
        .expect("dead admin inserted");
    let mut stray = raw_admin_cert(subject_id);
    root_signed_cert(&mut stray);
    write_stray_adminkey(&node, stray);

    let cert = issue_admin(&mut node, BOOTSTRAP_USERNAME).expect("mismatch is recoverable");
    assert_eq!(active_admin_count(&node), 1);
    assert_ne!(cert.identity_id, dead.identity_id);
    let on_disk = node
        .adminkey_provider
        .read_if_exists()
        .expect("read")
        .expect("file present");
    assert_eq!(
        on_disk.certificate.identity_id, cert.identity_id,
        "recovered `.adminkey` must embed the ACTIVE certificate"
    );
    verify_admin_usable(&mut node);
}

#[test]
fn sec002_10_orphan_user_cannot_mint_extra_admin() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);
    issue_admin(&mut node, BOOTSTRAP_USERNAME).expect("first admin");

    // An orphan users row (no certificate) cannot mint an extra ACTIVE ADMIN.
    node.db
        .executor()
        .users()
        .upsert_user(
            &uuid::Uuid::new_v4().to_string(),
            "boss",
            "",
            UserRole::Admin,
            "WILAYA",
            FIXED_NOW,
        )
        .expect("orphan user inserted");

    let err = issue_admin(&mut node, "boss").expect_err("orphan user must be rejected");
    assert!(err.to_string().contains("canonical"), "got {err:?}");
    assert_eq!(active_admin_count(&node), 1);
}

#[test]
fn sec002_11_existing_admin_remains_usable() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);
    issue_admin(&mut node, BOOTSTRAP_USERNAME).expect("first admin");

    verify_admin_usable(&mut node);

    let err = issue_admin(&mut node, BOOTSTRAP_USERNAME).expect_err("usable admin blocks ceremony");
    assert!(err.to_string().contains("already exists"), "got {err:?}");
}

#[test]
fn sec002_12_no_secrets_in_errors_or_audit() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);
    issue_admin(&mut node, BOOTSTRAP_USERNAME).expect("first admin");

    // Rejected attempts must not leak the passphrase or key material.
    for attempt in [BOOTSTRAP_USERNAME, "boss"] {
        let err = issue_admin(&mut node, attempt).expect_err("must be rejected");
        assert!(
            !err.to_string().contains(ADMIN_PASSPHRASE),
            "error leaked passphrase: {err:?}"
        );
        assert!(
            !err.to_string().contains("encrypted_private_key"),
            "error leaked key material: {err:?}"
        );
    }

    // The audit record holds only identity ids — never secrets.
    let audit = AuditService::new(node.db.executor())
        .get_audit_entries(
            &AuditFilters {
                action: Some("FirstAdminProvisioned".into()),
                ..Default::default()
            },
            0,
            100,
        )
        .expect("audit query");
    let entry = audit
        .entries
        .iter()
        .find(|e| e.status == AuditStatus::Success)
        .expect("success entry");
    let serialized = entry
        .metadata
        .as_ref()
        .expect("metadata present")
        .to_string();
    assert!(!serialized.contains(ADMIN_PASSPHRASE), "audit leaked passphrase");
    assert!(
        !serialized.contains("encrypted_private_key"),
        "audit leaked key material"
    );
    assert!(!serialized.contains("secret"), "audit leaked secret material");
}

// ---------------------------------------------------------------------------
// SEC-002-R — recovery authorization, unified usable-admin predicate, rate limit
//
// SEC002-R-A  recovery branch of `issue_first_admin_key` requires an
//             authenticated ADMIN session (command boundary).
// SEC002-R-B  single usable-admin predicate (`AdminCredentialState`) shared by
//             the password login gate AND the recovery ceremony.
// SEC002-R-C  recovery attempts are rate-limited (`ADMIN_RECOVERY_RATE_LIMIT_KEY`).
// ---------------------------------------------------------------------------

use std::sync::Mutex as StdMutex;

use chrono::{Duration as ChronoDuration, Utc};

use grpc_lib::app::state::AppState;
use grpc_lib::application::services::{
    identity_authentication_policy::AdminCredentialState, UserAccountSyncService,
};
use grpc_lib::commands::identity::{
    issue_first_admin_key_impl, ADMIN_RECOVERY_RATE_LIMIT_KEY,
};
use grpc_lib::domain::session::{CurrentSession, UserSnapshot};

/// Serializes `XDG_DATA_HOME` mutation between env-dependent tests in THIS
/// file. Other test files are separate processes, so they are already isolated.
static XDG_LOCK: StdMutex<()> = StdMutex::new(());

#[test]
fn sec002r_01_policy_mismatched_adminkey_opens_password_path() {
    // SEC-002-R-B: a `.adminkey` whose embedded certificate differs from the
    // ACTIVE certificate is NOT a usable identity — the operator's password
    // path stays open instead of deadlocking (previous behavior treated file
    // presence alone as "active identity").
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    let admin = node
        .db
        .executor()
        .users()
        .get_user_by_username(BOOTSTRAP_USERNAME)
        .expect("query")
        .expect("seeded admin");
    let subject_id = uuid::Uuid::parse_str(&admin.id).expect("uuid");

    let active = raw_admin_cert(subject_id);
    node.db
        .executor()
        .identity_store()
        .upsert(&active, FIXED_NOW)
        .expect("active admin inserted");
    let mut stray = raw_admin_cert(subject_id);
    root_signed_cert(&mut stray);
    write_stray_adminkey(&node, stray);

    assert_eq!(
        IdentityAuthenticationPolicy::admin_credential_state(
            &node.db,
            &node.adminkey_provider
        )
        .expect("state"),
        AdminCredentialState::MismatchedAdminkey
    );
    assert!(!IdentityAuthenticationPolicy::has_active_admin_identity(
        &node.db,
        &node.adminkey_provider
    )
    .expect("fact"));
    assert!(
        IdentityAuthenticationPolicy::password_login_allowed(&node.db, &node.adminkey_provider)
            .expect("policy"),
        "mismatched `.adminkey` must keep the password path open (no deadlock)"
    );
}

#[test]
fn sec002r_02_policy_corrupt_adminkey_is_fail_closed() {
    // SEC-002-R-B: a corrupt/unreadable `.adminkey` is an ERROR (never "allow",
    // never "recoverable silently"). The ceremony and the login gate share this
    // fail-closed reading.
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    let admin = node
        .db
        .executor()
        .users()
        .get_user_by_username(BOOTSTRAP_USERNAME)
        .expect("query")
        .expect("seeded admin");
    let subject_id = uuid::Uuid::parse_str(&admin.id).expect("uuid");
    node.db
        .executor()
        .identity_store()
        .upsert(&raw_admin_cert(subject_id), FIXED_NOW)
        .expect("active admin inserted");
    std::fs::create_dir_all(node.adminkey_provider.file_path().parent().expect("parent"))
        .expect("node dir created");
    std::fs::write(node.adminkey_provider.file_path(), b"not-an-adminkey")
        .expect("corrupt file written");

    assert!(
        IdentityAuthenticationPolicy::admin_credential_state(&node.db, &node.adminkey_provider)
            .is_err(),
        "corrupt `.adminkey` must fail closed"
    );
    assert!(
        IdentityAuthenticationPolicy::password_login_allowed(&node.db, &node.adminkey_provider)
            .is_err(),
        "corrupt `.adminkey` must never be treated as a closed password path"
    );
}

#[test]
fn sec002r_03_policy_matching_adminkey_is_usable_and_blocks_password() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);
    issue_admin(&mut node, BOOTSTRAP_USERNAME).expect("first admin");

    assert_eq!(
        IdentityAuthenticationPolicy::admin_credential_state(
            &node.db,
            &node.adminkey_provider
        )
        .expect("state"),
        AdminCredentialState::Usable
    );
    assert!(IdentityAuthenticationPolicy::has_active_admin_identity(
        &node.db,
        &node.adminkey_provider
    )
    .expect("fact"));
    assert!(
        !IdentityAuthenticationPolicy::password_login_allowed(&node.db, &node.adminkey_provider)
            .expect("policy"),
        "usable ADMIN identity must close the password path"
    );
}

#[test]
fn sec002r_04_password_path_usable_when_adminkey_missing() {
    // SEC-002-R scenario C/D premise: with the `.adminkey` lost, an operator who
    // set a fleet `admin` password (B8) CAN authenticate — the empty-hash
    // identity-only ceremony is the only case with no recoverable password.
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    bootstrap_wilaya(&mut node);
    issue_admin(&mut node, BOOTSTRAP_USERNAME).expect("first admin");
    std::fs::remove_file(node.adminkey_provider.file_path()).expect("remove adminkey");

    assert_eq!(
        IdentityAuthenticationPolicy::admin_credential_state(
            &node.db,
            &node.adminkey_provider
        )
        .expect("state"),
        AdminCredentialState::MissingAdminkey
    );

    UserAccountSyncService::new(node.db.executor(), &Argon2PasswordHashProvider)
        .set_fleet_admin_password("AdminPass123")
        .expect("fleet admin password set");

    let admin = node
        .db
        .executor()
        .users()
        .get_user_by_username(BOOTSTRAP_USERNAME)
        .expect("query")
        .expect("admin row");
    assert!(
        Argon2PasswordHashProvider
            .verify_admin("AdminPass123", &admin.password_hash)
            .expect("verify"),
        "fleet-set admin password must verify when the `.adminkey` is missing"
    );
    assert!(
        IdentityAuthenticationPolicy::password_login_allowed(&node.db, &node.adminkey_provider)
            .expect("policy"),
        "missing `.adminkey` keeps the password path open"
    );
}

/// SEC-002-R command harness. The command enforcement resolves the GLOBAL
/// providers (`node_key_store()` / `adminkey_provider()` → `dirs::data_dir()/GRPC`),
/// so `XDG_DATA_HOME` is pinned to a temp dir and ALL provisioning is driven
/// through those same global paths.
struct CommandHarness {
    _xdg_dir: tempfile::TempDir,
    db: Option<Database>,
    node_key_store: NodeKeyStore,
    adminkey_provider: AdminKeyProvider,
    state: Option<AppState>,
}

fn with_command_harness(f: impl FnOnce(&mut CommandHarness)) {
    // `unwrap_or_else(into_inner)`: a test panic must not poison the shared lock
    // and cascade into every subsequent env-dependent test.
    let _lock = XDG_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let xdg_dir = tempfile::TempDir::new().expect("xdg temp");
    std::env::set_var("XDG_DATA_HOME", xdg_dir.path());
    let grpc_dir = xdg_dir.path().join("GRPC");
    let db = ConnectionFactory::new_for_test().expect("db");
    let mut harness = CommandHarness {
        _xdg_dir: xdg_dir,
        db: Some(db),
        node_key_store: NodeKeyStore::new(grpc_dir.clone()),
        adminkey_provider: AdminKeyProvider::new(grpc_dir),
        state: None,
    };
    f(&mut harness);
    std::env::remove_var("XDG_DATA_HOME");
}

impl CommandHarness {
    fn provision_wilaya(&mut self) {
        bootstrap_wilaya_using(self.db.as_mut().expect("db"), &self.node_key_store);
    }

    fn remove_seeded_admin(&mut self) {
        remove_seeded_admin_using(self.db.as_mut().expect("db"));
    }

    fn provision_first_admin(&mut self) {
        issue_admin_using(
            self.db.as_mut().expect("db"),
            &self.node_key_store,
            &self.adminkey_provider,
            BOOTSTRAP_USERNAME,
        )
        .expect("first admin issued");
    }

    fn make_recovery_mode_missing_adminkey(&mut self) {
        std::fs::remove_file(self.adminkey_provider.file_path()).expect("remove adminkey");
    }

    fn make_recovery_mode_mismatched_adminkey(&mut self) {
        let state = self.state();
        let db_guard = state.get_db().expect("db lock");
        let db = db_guard.as_ref().expect("db present");
        let active = db
            .executor()
            .identity_store()
            .get_active_by_subject_type(SubjectType::Admin)
            .expect("query")
            .expect("active admin cert");
        let subject_id = active.subject_id;
        let mut stray = raw_admin_cert(subject_id);
        root_signed_cert(&mut stray);
        drop(db_guard);
        let encrypted = self
            .adminkey_provider
            .encrypt_private_key(&[9u8; 32], ADMIN_PASSPHRASE)
            .expect("stray key encrypted");
        self.adminkey_provider
            .write(&AdminKeyFile {
                format_version: ADMINKEY_FORMAT_VERSION,
                algorithm_version: IDENTITY_ALGORITHM_PROFILE_ED25519,
                certificate: stray,
                encrypted_private_key: encrypted,
            })
            .expect("stray adminkey written");
    }

    /// Move the DB into `AppState` (once) and return `&AppState`.
    fn state(&mut self) -> &AppState {
        if self.state.is_none() {
            let db = self.db.take().expect("db taken");
            self.state = Some(AppState::new_for_test(db));
        }
        self.state.as_ref().expect("state")
    }

    fn inject_session(&mut self, role: UserRole, expired: bool) {
        let state = self.state();
        let db_guard = state.get_db().expect("db lock");
        let db = db_guard.as_ref().expect("db present");
        let admin = db
            .executor()
            .users()
            .get_user_by_username(BOOTSTRAP_USERNAME)
            .expect("query")
            .expect("admin user row");
        let snapshot = UserSnapshot {
            id: admin.id.clone(),
            username: admin.username.clone(),
            role: role.clone(),
            created_at: Utc::now(),
        };
        let mut session = CurrentSession::new(
            admin.id.clone(),
            admin.username.clone(),
            role.clone(),
            snapshot,
        );
        if expired {
            session.last_activity = Utc::now() - ChronoDuration::minutes(31);
        }
        drop(db_guard);
        *state.current_session.lock().expect("session lock") = Some(session);
    }

    fn active_admin_count(&self) -> usize {
        match (&self.db, &self.state) {
            (Some(db), _) => Self::count_active_admins(db),
            (None, Some(state)) => {
                let guard = state.get_db().expect("db lock");
                Self::count_active_admins(guard.as_ref().expect("db present"))
            }
            (None, None) => panic!("no db"),
        }
    }

    fn count_active_admins(db: &Database) -> usize {
        db.executor()
            .identity_store()
            .list_all()
            .expect("list")
            .iter()
            .filter(|c| {
                c.subject_type == SubjectType::Admin && c.status == CredentialStatus::Active
            })
            .count()
    }

    /// Read the ACTIVE ADMIN certificate from whichever DB container is live.
    fn active_admin_cert(&self) -> IdentityCertificate {
        match (&self.db, &self.state) {
            (Some(db), _) => Self::active_cert_from(db),
            (None, Some(state)) => {
                let guard = state.get_db().expect("db lock");
                Self::active_cert_from(guard.as_ref().expect("db present"))
            }
            (None, None) => panic!("no db"),
        }
    }

    fn active_cert_from(db: &Database) -> IdentityCertificate {
        db.executor()
            .identity_store()
            .get_active_by_subject_type(SubjectType::Admin)
            .expect("query")
            .expect("ACTIVE ADMIN present")
    }

    /// Assert the unified usable-admin predicate against the live DB.
    fn assert_admin_state(&self, expected: AdminCredentialState) {
        let state = self.state.as_ref().expect("state");
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("db present");
        assert_eq!(
            IdentityAuthenticationPolicy::admin_credential_state(db, &self.adminkey_provider)
                .expect("state"),
            expected
        );
    }
}

/// SEC-002-R-A (scenario A/B): RECOVERY with NO session must be rejected at the
/// command boundary, even though the pre-auth first-admin ceremony is legal.
#[test]
fn sec002r_10_command_recovery_requires_session() {
    with_command_harness(|h| {
        h.provision_wilaya();
        h.provision_first_admin();
        h.make_recovery_mode_missing_adminkey();
        h.state();

        let err = issue_first_admin_key_impl(h.state(), "admin".into(), ADMIN_PASSPHRASE.into())
            .expect_err("recovery without a session must be rejected");
        assert!(
            err.contains("لا توجد جلسة نشطة"),
            "unauthenticated recovery must fail closed, got: {err}"
        );
        assert_eq!(h.active_admin_count(), 1, "no credential change");
    });
}

/// SEC-002-R-A (scenario F): recovery is blocked by the identity gate before
/// any credential math runs — a UNIT-role session cannot re-issue the ADMIN.
#[test]
fn sec002r_11_command_recovery_user_session_rejected() {
    with_command_harness(|h| {
        h.provision_wilaya();
        h.provision_first_admin();
        h.make_recovery_mode_missing_adminkey();
        h.inject_session(UserRole::User, false);

        let err = issue_first_admin_key_impl(h.state(), "admin".into(), ADMIN_PASSPHRASE.into())
            .expect_err("UNIT session must be rejected");
        assert!(
            err.contains("Admin"),
            "non-admin session must be denied authorization, got: {err}"
        );
        assert_eq!(h.active_admin_count(), 1, "no credential change");
    });
}

/// SEC-002-R-A (scenario I): an EXPIRED Admin session must not authorize the
/// recovery ceremony.
#[test]
fn sec002r_12_command_recovery_expired_session_rejected() {
    with_command_harness(|h| {
        h.provision_wilaya();
        h.provision_first_admin();
        h.make_recovery_mode_missing_adminkey();
        h.inject_session(UserRole::Admin, true);

        let err = issue_first_admin_key_impl(h.state(), "admin".into(), ADMIN_PASSPHRASE.into())
            .expect_err("expired session must be rejected");
        assert!(
            err.contains("Session expired"),
            "expired session must fail closed, got: {err}"
        );
        assert_eq!(h.active_admin_count(), 1, "no credential change");
    });
}

/// SEC-002-R-A (scenario C/D): a VALID Admin session authorizes the recovery of
/// a missing `.adminkey`; the ceremony supersedes and the node returns to a
/// usable ADMIN identity.
#[test]
fn sec002r_13_command_recovery_valid_admin_session_succeeds() {
    with_command_harness(|h| {
        h.provision_wilaya();
        h.provision_first_admin();
        let superseded_id = h.active_admin_cert().identity_id;
        h.make_recovery_mode_missing_adminkey();
        h.inject_session(UserRole::Admin, false);

        let cert = issue_first_admin_key_impl(h.state(), "admin".into(), ADMIN_PASSPHRASE.into())
            .expect("authorized recovery succeeds");
        assert_eq!(cert.subject_type, SubjectType::Admin);
        assert_ne!(cert.identity_id, superseded_id, "unusable cert is superseded");
        assert_eq!(h.active_admin_count(), 1, "at most one ACTIVE ADMIN");
        h.assert_admin_state(AdminCredentialState::Usable);
    });
}

/// SEC-002-R-A (scenario J): after a successful recovery, the ADMIN identity is
/// usable again and a SECOND recovery is rejected — the at-most-one-success
/// invariant holds end to end at the command boundary.
#[test]
fn sec002r_14_command_second_recovery_rejected() {
    with_command_harness(|h| {
        h.provision_wilaya();
        h.provision_first_admin();
        h.make_recovery_mode_missing_adminkey();
        h.inject_session(UserRole::Admin, false);
        let first = issue_first_admin_key_impl(h.state(), "admin".into(), ADMIN_PASSPHRASE.into())
            .expect("first recovery");

        let err = issue_first_admin_key_impl(h.state(), "admin".into(), ADMIN_PASSPHRASE.into())
            .expect_err("second recovery must be rejected");
        assert!(
            err.contains("already exists"),
            "usable admin must block re-issuance, got: {err}"
        );
        assert_eq!(h.active_admin_count(), 1);
        assert_eq!(h.active_admin_cert().identity_id, first.identity_id);
    });
}

/// SEC-002-R-A (scenario D): recovery via a MISMATCHED `.adminkey` is equally
/// authorized (same predicate), so the operator is never locked out by a stray
/// key file.
#[test]
fn sec002r_15_command_recovery_mismatched_adminkey_succeeds() {
    with_command_harness(|h| {
        h.provision_wilaya();
        h.provision_first_admin();
        h.make_recovery_mode_mismatched_adminkey();
        h.inject_session(UserRole::Admin, false);

        let cert = issue_first_admin_key_impl(h.state(), "admin".into(), ADMIN_PASSPHRASE.into())
            .expect("authorized recovery of a mismatched `.adminkey` succeeds");
        assert_eq!(cert.status, CredentialStatus::Active);
        assert_eq!(h.active_admin_count(), 1);
        let on_disk = h
            .adminkey_provider
            .read_if_exists()
            .expect("read")
            .expect("file present");
        assert_eq!(
            on_disk.certificate.identity_id, cert.identity_id,
            "recovered `.adminkey` must embed the ACTIVE certificate"
        );
    });
}

/// SEC-002-R-A (scenario K): FIRST-ADMIN bootstrap stays PRE-AUTH — no session
/// is required while no ACTIVE ADMIN exists.
#[test]
fn sec002r_16_command_first_admin_stays_pre_auth() {
    with_command_harness(|h| {
        h.provision_wilaya();
        h.remove_seeded_admin();
        h.state();

        let cert = issue_first_admin_key_impl(h.state(), "admin".into(), ADMIN_PASSPHRASE.into())
            .expect("first-admin bootstrap is pre-auth");
        assert_eq!(cert.subject_type, SubjectType::Admin);
        assert_eq!(cert.status, CredentialStatus::Active);
        assert_eq!(h.active_admin_count(), 1);
    });
}

/// SEC-002-R-C: recovery attempts are rate-limited (5/300s) INDEPENDENTLY of
/// the login limiter, and the check is fail-closed (no credential change).
#[test]
fn sec002r_17_command_recovery_rate_limited() {
    with_command_harness(|h| {
        h.provision_wilaya();
        h.provision_first_admin();
        h.make_recovery_mode_missing_adminkey();
        h.inject_session(UserRole::Admin, false);
        let state = h.state();

        {
            let limiter = state.rate_limiter.lock().expect("limiter lock");
            for _ in 0..5 {
                limiter.record_failure(ADMIN_RECOVERY_RATE_LIMIT_KEY);
            }
        }

        let err = issue_first_admin_key_impl(state, "admin".into(), ADMIN_PASSPHRASE.into())
            .expect_err("rate-limited recovery must fail closed");
        assert!(
            err.contains("exceeded the limit"),
            "rate-limit error expected, got: {err}"
        );
        assert_eq!(h.active_admin_count(), 1, "no credential change while locked");
    });
}

