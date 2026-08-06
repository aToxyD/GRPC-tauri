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
    FinalizeWilayaProvisionResult, IdentityAuthenticationPolicy, IdentityBootstrapStatusService,
    IdentityChallengeService, IdentityProvisioningService,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    ChallengeMessage, CredentialStatus, Ed25519CertificateSignature, IdentityBootstrapState,
    IdentityCertificate, IdentityChallengeState, IdentitySigner, IdentityStorePort, SubjectType,
};
use grpc_lib::domain::security::PasswordHashPort;
use grpc_lib::errors::AuthenticationError;
use grpc_lib::infrastructure::identity::{AdminKeyProvider, NodeKeyStore};
use grpc_lib::infrastructure::security::{
    Argon2PasswordHashProvider, Ed25519SignatureVerifier, Ed25519SigningProvider,
};
use grpc_lib::repositories::identity_store::IdentityStoreRepository;
use grpc_lib::repositories::RepositoryProvider;

/// RFC 8032 §7.1 TEST 1 secret — the matching public key IS the debug-mode
/// development Root fallback (root_public_key.rs). Never a production key.
const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
    0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
    0x7f, 0x60,
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
    IdentityBootstrapStatusService::compute(
        &node.db,
        &node.node_key_store,
        &node.adminkey_provider,
    )
    .expect("status computed")
}

fn root_signer() -> Ed25519SigningProvider {
    Ed25519SigningProvider::new(TEST_ROOT_SECRET)
}

/// Full offline flow: request (node keygen + CSR), offline Root signs, finalize.
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

/// Remove the seeded admin so the node behaves like a fresh B5 fleet node.
fn remove_seeded_admin(node: &mut Node) {
    let admin = node
        .db
        .executor()
        .users()
        .get_user_by_username(BOOTSTRAP_USERNAME)
        .expect("seed query")
        .expect("seeded admin present");
    node.db
        .executor()
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
    );
    (state, service)
}

fn begin_challenge(
    service: &IdentityChallengeService,
    node: &Node,
) -> ChallengeMessage {
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
        .complete_with_passphrase(
            &mut node.db,
            &challenge.session_id,
            ADMIN_PASSPHRASE,
        )
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

    // Legacy password path still authenticates (additive window).
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
        .verify_password("admin", &admin.node_id, &admin.password_hash)
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
    assert_eq!(before, after, "identical re-import must perform ZERO writes");
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
    let signature = root_signer()
        .sign_certificate(&other)
        .expect("root signed");
    other.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig"));

    let err = provisioning
        .finalize_wilaya_provision(&other, &node.node_key_store, FIXED_NOW)
        .expect_err("different WILAYA cert must fail closed");
    assert!(
        err.to_string().contains("already exists"),
        "got {err:?}"
    );
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
        err.to_string().contains("does not match the node signing key"),
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
    assert!(err.to_string().contains("WILAYA certificate"), "got {err:?}");
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
            grpc_lib::errors::AppError::Authentication(AuthenticationError::InvalidCredentials {
                ..
            })
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
        .verify_password("anything", &admin.node_id, &admin.password_hash)
        .is_err());
}

// ---------------------------------------------------------------------------
// B6-A: password login gate (IdentityAuthenticationPolicy)
// ---------------------------------------------------------------------------

#[test]
fn password_login_gate_is_sole_security_fact() {
    // No identity material → the password path stays open.
    let node = fresh_node();
    assert!(
        IdentityAuthenticationPolicy::password_login_allowed_with_override(
            &node.db,
            &node.adminkey_provider,
            false,
        )
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
        IdentityAuthenticationPolicy::has_active_admin_identity(
            &node.db,
            &node.adminkey_provider,
        )
        .expect("fact"),
        "ACTIVE ADMIN cert + `.adminkey` must be detected"
    );
    assert!(
        !IdentityAuthenticationPolicy::password_login_allowed_with_override(
            &node.db,
            &node.adminkey_provider,
            false,
        )
        .expect("policy"),
        "ACTIVE ADMIN identity must close the password path"
    );
    // Temporary override (`GRPC_LEGACY_AUTH=1`) re-opens the legacy path.
    assert!(
        IdentityAuthenticationPolicy::password_login_allowed_with_override(
            &node.db,
            &node.adminkey_provider,
            true,
        )
        .expect("policy"),
        "GRPC_LEGACY_AUTH override must re-open the legacy path (B6-A window)"
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
        IdentityAuthenticationPolicy::password_login_allowed_with_override(
            &node.db,
            &node.adminkey_provider,
            false,
        )
        .expect("policy"),
        "missing `.adminkey` must keep the password path open (no lockout)"
    );
}
