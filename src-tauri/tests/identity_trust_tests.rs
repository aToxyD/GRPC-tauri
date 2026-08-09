//! Identity Trust B3 integration tests.
//!
//! RFC 2026-08-04-node-identity-trust §3.7 / ADR-0038 / ADR-0039.
//!
//! Exercises the full Challenge–Response login path end-to-end against a real
//! SQLite database and real crypto:
//! - WILAYA provisioning (on-node keygen, authority-root-signed cert);
//! - ADMIN issuance (WILAYA-signed cert, portable `.adminkey`);
//! - challenge begin/complete with session binding;
//! - fail-closed replay protection (atomic `Pending → Consumed`);
//! - tampered-signature and wrong-passphrase rejection;
//! - `.adminkey` portability (passphrase-only, no node secrets).

#[allow(dead_code)]
mod common;

use std::sync::{Arc, Mutex};
use tempfile::TempDir;

use grpc_lib::application::services::{IdentityChallengeService, IdentityProvisioningService};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    ChallengeMessage, CredentialStatus, IdentityChallengeState, IdentitySigner, IdentityStorePort,
    SubjectType,
};
use grpc_lib::errors::{AppError, AuthenticationError};
use grpc_lib::infrastructure::identity::{AdminKeyProvider, NodeKeyStore};
use grpc_lib::infrastructure::security::{
    Argon2PasswordHashProvider, Ed25519SignatureVerifier, Ed25519SigningProvider,
};
use grpc_lib::models::UserRole;
use grpc_lib::repositories::identity_store::IdentityStoreRepository;

/// Test authority Root signing key (never a real production key).
const TEST_ROOT_SECRET: [u8; 32] = [42u8; 32];
/// Operator passphrase protecting the portable `.adminkey`.
const ADMIN_PASSPHRASE: &str = "correct horse battery staple";
/// Fixed timestamp for deterministic persistence.
const FIXED_NOW: &str = "2026-08-04T00:00:00Z";

/// A fully provisioned WILAYA device: real DB + node key store + `.adminkey`.
struct ProvisionedDevice {
    #[allow(dead_code)]
    _dir: TempDir,
    db: Database,
    adminkey_provider: AdminKeyProvider,
    wilaya_identity_id: uuid::Uuid,
}

fn provision_device() -> ProvisionedDevice {
    let dir = TempDir::new().expect("temp dir");
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
    let adminkey_provider = AdminKeyProvider::new(dir.path().to_path_buf());

    let root_signer = Ed25519SigningProvider::new(TEST_ROOT_SECRET);
    let mut provisioning = IdentityProvisioningService::new(&mut db);
    let wilaya = provisioning
        .provision_wilaya(
            uuid::Uuid::new_v4(),
            &root_signer,
            &node_key_store,
            FIXED_NOW,
        )
        .expect("wilaya provisioned");
    provisioning
        .issue_admin(
            uuid::Uuid::new_v4(),
            ADMIN_PASSPHRASE,
            wilaya.identity_id,
            &node_key_store,
            &adminkey_provider,
            FIXED_NOW,
        )
        .expect("admin issued");

    ProvisionedDevice {
        _dir: dir,
        db,
        adminkey_provider,
        wilaya_identity_id: wilaya.identity_id,
    }
}

fn challenge_service(
    device: &ProvisionedDevice,
) -> (Arc<Mutex<IdentityChallengeState>>, IdentityChallengeService) {
    let state = Arc::new(Mutex::new(IdentityChallengeState::default()));
    let service = IdentityChallengeService::new(
        state.clone(),
        device.adminkey_provider.clone(),
        Arc::new(Ed25519SignatureVerifier),
        Arc::new(Argon2PasswordHashProvider),
    );
    (state, service)
}

/// Simulates the operator: decrypts the `.adminkey` secret with the passphrase
/// and signs the challenge canonical bytes with the ADMIN private key.
fn operator_sign(challenge: &ChallengeMessage, adminkey_provider: &AdminKeyProvider) -> Vec<u8> {
    let adminkey = adminkey_provider.read().expect("adminkey readable");
    let secret = adminkey_provider
        .decrypt_private_key(&adminkey.encrypted_private_key, ADMIN_PASSPHRASE)
        .expect("passphrase decrypt");
    let signer = Ed25519SigningProvider::new(secret);
    signer.sign_challenge(challenge).expect("challenge signed")
}

fn begin_challenge(
    service: &IdentityChallengeService,
    device: &ProvisionedDevice,
) -> ChallengeMessage {
    service
        .begin(&IdentityStoreRepository::new(device.db.executor()))
        .expect("challenge issued")
}

#[test]
fn provisioning_writes_store_certificates_and_portable_adminkey() {
    let device = provision_device();

    let store = IdentityStoreRepository::new(device.db.executor());
    let wilaya = store
        .get_active_by_subject_type(SubjectType::Wilaya)
        .expect("store query")
        .expect("wilaya active");
    assert_eq!(wilaya.identity_id, device.wilaya_identity_id);
    assert_eq!(wilaya.status, CredentialStatus::Active);
    assert!(
        wilaya.signature.is_some(),
        "wilaya cert must be root-signed"
    );

    let admin = store
        .list_all()
        .expect("store list")
        .into_iter()
        .find(|c| c.subject_type == SubjectType::Admin)
        .expect("admin cert stored");
    assert_eq!(
        admin.issuer_identity_id,
        Some(device.wilaya_identity_id),
        "ADMIN cert must be issued by the WILAYA node"
    );
    assert!(admin.signature.is_some(), "admin cert must be signed");

    // Portable `.adminkey` round-trips and validates.
    let adminkey = device.adminkey_provider.read().expect("adminkey");
    assert_eq!(adminkey.certificate, admin);
    assert!(adminkey.validate().is_ok());
}

#[test]
fn challenge_login_succeeds_and_session_reuses_challenge_id() {
    let mut device = provision_device();
    let (_state, service) = challenge_service(&device);

    let challenge = begin_challenge(&service, &device);
    assert_eq!(challenge.node_identity_id, device.wilaya_identity_id);

    let signature = operator_sign(&challenge, &device.adminkey_provider);
    let established = service
        .complete(
            &mut device.db,
            &challenge.session_id,
            ADMIN_PASSPHRASE,
            &signature,
        )
        .expect("login succeeded");

    assert_eq!(
        established.session.session_id,
        challenge.session_id.to_string(),
        "challenge session_id becomes the session id (B3)"
    );
    assert_eq!(established.session.username, "admin");
    assert_eq!(established.session.user_role, UserRole::Admin);
}

#[test]
fn challenge_replay_after_success_is_rejected() {
    let mut device = provision_device();
    let (_state, service) = challenge_service(&device);
    let challenge = begin_challenge(&service, &device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);

    assert!(service
        .complete(
            &mut device.db,
            &challenge.session_id,
            ADMIN_PASSPHRASE,
            &signature,
        )
        .is_ok());
    let replay = service.complete(
        &mut device.db,
        &challenge.session_id,
        ADMIN_PASSPHRASE,
        &signature,
    );
    assert!(
        matches!(
            replay,
            Err(AppError::Authentication(
                AuthenticationError::InvalidCredentials { .. }
            ))
        ),
        "replayed challenge must be rejected fail-closed, got {replay:?}"
    );
}

#[test]
fn tampered_challenge_signature_is_rejected_and_consumes_challenge() {
    let mut device = provision_device();
    let (_state, service) = challenge_service(&device);
    let challenge = begin_challenge(&service, &device);
    let mut signature = operator_sign(&challenge, &device.adminkey_provider);
    signature[0] ^= 0xFF;

    let tampered = service.complete(
        &mut device.db,
        &challenge.session_id,
        ADMIN_PASSPHRASE,
        &signature,
    );
    assert!(
        matches!(
            tampered,
            Err(AppError::Authentication(
                AuthenticationError::InvalidCredentials { .. }
            ))
        ),
        "tampered signature must be rejected, got {tampered:?}"
    );

    // The challenge was consumed on the failed attempt: a correct replay fails too.
    let correct = operator_sign(&challenge, &device.adminkey_provider);
    let replay = service.complete(
        &mut device.db,
        &challenge.session_id,
        ADMIN_PASSPHRASE,
        &correct,
    );
    assert!(
        replay.is_err(),
        "consumed challenge must reject a correct replay"
    );
}

#[test]
fn wrong_passphrase_is_rejected_and_consumes_challenge() {
    let mut device = provision_device();
    let (_state, service) = challenge_service(&device);
    let challenge = begin_challenge(&service, &device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);

    let wrong = service.complete(
        &mut device.db,
        &challenge.session_id,
        "wrong passphrase",
        &signature,
    );
    assert!(wrong.is_err(), "wrong passphrase must fail closed");

    let correct = service.complete(
        &mut device.db,
        &challenge.session_id,
        ADMIN_PASSPHRASE,
        &signature,
    );
    assert!(
        correct.is_err(),
        "challenge consumed even after failed attempt"
    );
}

#[test]
fn challenge_bound_to_foreign_node_is_rejected() {
    let mut device = provision_device();
    let (state, service) = challenge_service(&device);

    // A challenge crafted for a DIFFERENT node identity must be rejected even
    // though the signature and passphrase are valid.
    let foreign = ChallengeMessage::new(uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), [7u8; 32]);
    state.lock().expect("state lock").begin(foreign.clone());
    let signature = operator_sign(&foreign, &device.adminkey_provider);

    let result = service.complete(
        &mut device.db,
        &foreign.session_id,
        ADMIN_PASSPHRASE,
        &signature,
    );
    assert!(
        matches!(
            result,
            Err(AppError::Authentication(
                AuthenticationError::InvalidCredentials { .. }
            ))
        ),
        "foreign-node challenge must be rejected, got {result:?}"
    );
}

#[test]
fn adminkey_is_portable_and_passphrase_only() {
    let device = provision_device();

    // Simulate a second device: a fresh data dir with only the `.adminkey` file
    // (no node secrets, no GRPC_APP_KEY dependency).
    let portable_dir = TempDir::new().expect("temp dir");
    std::fs::copy(
        device.adminkey_provider.file_path(),
        portable_dir
            .path()
            .join(grpc_lib::infrastructure::identity::ADMINKEY_FILE_NAME),
    )
    .expect("copy adminkey");

    let portable = AdminKeyProvider::new(portable_dir.path().to_path_buf());
    let adminkey = portable.read().expect("portable adminkey readable");
    assert!(adminkey.validate().is_ok());

    // The secret is recoverable on the second device with the passphrase alone.
    let secret = portable
        .decrypt_private_key(&adminkey.encrypted_private_key, ADMIN_PASSPHRASE)
        .expect("passphrase-only decrypt");
    assert_eq!(secret.len(), 32);
    assert!(
        portable
            .decrypt_private_key(&adminkey.encrypted_private_key, "wrong passphrase")
            .is_err(),
        "wrong passphrase must fail on the portable device"
    );
}
