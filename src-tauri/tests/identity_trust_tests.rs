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
use grpc_lib::domain::audit::{AuditAction, AuditFilters};
use grpc_lib::domain::identity::{
    ChallengeMessage, ChallengeState, CredentialStatus, IdentityChallengeState, IdentitySigner,
    IdentityStorePort, SubjectType, MAX_OUTSTANDING_CHALLENGES,
};
use grpc_lib::domain::rate_limiter::RateLimiter;
use grpc_lib::errors::{AppError, AppResult, AuthenticationError, ValidationError};
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
    let rate_limiter = Arc::new(Mutex::new(RateLimiter::new()));
    challenge_service_with_limiter(device, rate_limiter)
}

/// Variant that exposes the injected rate limiter so SEC-001 tests can inspect
/// the lockout state directly.
fn challenge_service_with_limiter(
    device: &ProvisionedDevice,
    rate_limiter: Arc<Mutex<RateLimiter>>,
) -> (Arc<Mutex<IdentityChallengeState>>, IdentityChallengeService) {
    let state = Arc::new(Mutex::new(IdentityChallengeState::default()));
    let service = IdentityChallengeService::new(
        state.clone(),
        device.adminkey_provider.clone(),
        Arc::new(Ed25519SignatureVerifier),
        Arc::new(Argon2PasswordHashProvider),
        rate_limiter,
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
    try_begin_challenge(service, device).expect("challenge issued")
}

fn try_begin_challenge(
    service: &IdentityChallengeService,
    device: &ProvisionedDevice,
) -> AppResult<ChallengeMessage> {
    service.begin(&IdentityStoreRepository::new(device.db.executor()))
}

/// Real wall clock as Unix epoch seconds (used to backdate challenges in
/// deterministic TTL tests).
fn now_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64
}

/// Backdate an already-issued challenge far beyond `CHALLENGE_TTL`.
fn backdate_challenge(state: &Arc<Mutex<IdentityChallengeState>>, challenge: &ChallengeMessage) {
    let mut guard = state.lock().expect("state lock");
    assert!(
        guard.try_begin(challenge.clone(), now_epoch_secs() - 100_000),
        "backdated challenge must be accepted"
    );
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

// ---------------------------------------------------------------------------
// SEC-001 Challenge–Response hardening (AUTH-01..AUTH-12)
// ---------------------------------------------------------------------------

/// One wrong-passphrase attempt against a fresh challenge (each failed attempt
/// consumes its challenge, so a fresh one is required per attempt).
fn fail_challenge_attempt(service: &IdentityChallengeService, device: &mut ProvisionedDevice) {
    let challenge = begin_challenge(service, device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);
    let result = service.complete(
        &mut device.db,
        &challenge.session_id,
        "wrong passphrase",
        &signature,
    );
    assert!(
        result.is_err(),
        "wrong passphrase must fail, got {result:?}"
    );
}

/// Asserts the error is the user-safe lockout rejection (same message family as
/// the password path) and NOT an internal or credential error.
fn assert_lockout_error(result: AppResult<grpc_lib::application::services::EstablishedSession>) {
    assert!(
        matches!(
            result,
            Err(AppError::Validation(ValidationError::InvalidFormat { .. }))
        ),
        "expected lockout rejection, got {result:?}"
    );
}

#[test]
fn auth01_repeated_failures_trigger_rate_limiting() {
    let mut device = provision_device();
    let rate_limiter = Arc::new(Mutex::new(RateLimiter::new()));
    let (state, service) = challenge_service_with_limiter(&device, rate_limiter.clone());

    for _ in 0..5 {
        fail_challenge_attempt(&service, &mut device);
    }
    assert!(
        !rate_limiter.lock().expect("rl lock").is_allowed("admin"),
        "after 5 failures the admin principal must be locked out"
    );

    // The 6th attempt is blocked before it consumes anything.
    let challenge = begin_challenge(&service, &device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);
    let blocked = service.complete(
        &mut device.db,
        &challenge.session_id,
        "wrong passphrase",
        &signature,
    );
    assert_lockout_error(blocked);
    assert_eq!(
        state.lock().expect("state lock").state(&challenge.session_id),
        Some(ChallengeState::Pending),
        "a blocked attempt must not consume the challenge"
    );
}

#[test]
fn auth02_new_challenge_does_not_bypass_rate_limiting() {
    let mut device = provision_device();
    let rate_limiter = Arc::new(Mutex::new(RateLimiter::new()));
    let (_state, service) = challenge_service_with_limiter(&device, rate_limiter.clone());

    for _ in 0..5 {
        fail_challenge_attempt(&service, &mut device);
    }
    assert!(!rate_limiter.lock().expect("rl lock").is_allowed("admin"));

    // A BRAND-NEW challenge with the CORRECT passphrase is still blocked:
    // the limiter keys on the admin principal, never on the challenge UUID.
    let challenge = begin_challenge(&service, &device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);
    let blocked = service.complete(
        &mut device.db,
        &challenge.session_id,
        ADMIN_PASSPHRASE,
        &signature,
    );
    assert_lockout_error(blocked);
}

#[test]
fn auth03_valid_passphrase_succeeds_before_lockout() {
    let mut device = provision_device();
    let (_state, service) = challenge_service(&device);

    // Two failed attempts (below the 5-attempt limit), then a correct attempt.
    fail_challenge_attempt(&service, &mut device);
    fail_challenge_attempt(&service, &mut device);

    let challenge = begin_challenge(&service, &device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);
    let established = service
        .complete(&mut device.db, &challenge.session_id, ADMIN_PASSPHRASE, &signature)
        .expect("valid passphrase succeeds before lockout");
    assert_eq!(established.session.username, "admin");
}

#[test]
fn auth04_correct_passphrase_does_not_bypass_active_lockout() {
    let mut device = provision_device();
    let (_state, service) = challenge_service(&device);

    for _ in 0..5 {
        fail_challenge_attempt(&service, &mut device);
    }

    let challenge = begin_challenge(&service, &device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);
    let blocked = service.complete(
        &mut device.db,
        &challenge.session_id,
        ADMIN_PASSPHRASE,
        &signature,
    );
    assert_lockout_error(blocked);
}

#[test]
fn auth05_expired_challenge_is_rejected() {
    let mut device = provision_device();
    let (state, service) = challenge_service(&device);

    let challenge = begin_challenge(&service, &device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);
    // Backdate the stored challenge beyond CHALLENGE_TTL (deterministic, no sleep).
    backdate_challenge(&state, &challenge);

    let result = service.complete(
        &mut device.db,
        &challenge.session_id,
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
        "expired challenge must be rejected fail-closed, got {result:?}"
    );
}

#[test]
fn auth06_expired_challenge_is_removed() {
    let mut device = provision_device();
    let (state, service) = challenge_service(&device);

    let challenge = begin_challenge(&service, &device);
    backdate_challenge(&state, &challenge);

    let signature = operator_sign(&challenge, &device.adminkey_provider);
    let _ = service.complete(&mut device.db, &challenge.session_id, ADMIN_PASSPHRASE, &signature);

    let guard = state.lock().expect("state lock");
    assert_eq!(
        guard.state(&challenge.session_id),
        None,
        "an expired challenge must be removed, not retained"
    );
    assert_eq!(guard.len(), 0);
}

#[test]
fn auth07_challenge_cannot_be_used_twice() {
    let mut device = provision_device();
    let (_state, service) = challenge_service(&device);

    let challenge = begin_challenge(&service, &device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);
    service
        .complete(&mut device.db, &challenge.session_id, ADMIN_PASSPHRASE, &signature)
        .expect("first completion succeeds");

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
        "replayed challenge must be rejected, got {replay:?}"
    );
}

#[test]
fn auth08_failed_authentication_consumes_the_challenge() {
    let mut device = provision_device();
    let (state, service) = challenge_service(&device);

    let challenge = begin_challenge(&service, &device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);
    let failed = service.complete(
        &mut device.db,
        &challenge.session_id,
        "wrong passphrase",
        &signature,
    );
    assert!(failed.is_err(), "wrong passphrase must fail");
    assert_eq!(
        state.lock().expect("state lock").state(&challenge.session_id),
        Some(ChallengeState::Consumed),
        "a failed attempt must consume the challenge (one-shot fail-closed)"
    );
}

#[test]
fn auth09_max_outstanding_challenges_is_enforced() {
    let device = provision_device();
    let (state, service) = challenge_service(&device);

    for _ in 0..MAX_OUTSTANDING_CHALLENGES {
        begin_challenge(&service, &device);
    }
    assert_eq!(
        state.lock().expect("state lock").len(),
        MAX_OUTSTANDING_CHALLENGES
    );

    let overflow = try_begin_challenge(&service, &device);
    assert!(
        matches!(overflow, Err(AppError::Internal(_))),
        "a challenge beyond the cap must be rejected, got {overflow:?}"
    );
    assert_eq!(
        state.lock().expect("state lock").len(),
        MAX_OUTSTANDING_CHALLENGES,
        "the cap must not be exceeded"
    );
}

#[test]
fn auth10_expired_challenges_do_not_consume_capacity() {
    let device = provision_device();
    let (state, service) = challenge_service(&device);

    // Fill the store to the cap with already-expired challenges.
    for _ in 0..MAX_OUTSTANDING_CHALLENGES {
        let expired =
            ChallengeMessage::new(uuid::Uuid::new_v4(), device.wilaya_identity_id, [1u8; 32]);
        assert!(
            state
                .lock()
                .expect("state lock")
                .try_begin(expired, now_epoch_secs() - 100_000)
        );
    }
    assert_eq!(state.lock().expect("state lock").len(), MAX_OUTSTANDING_CHALLENGES);

    // A fresh begin prunes the expired entries and succeeds.
    let fresh = try_begin_challenge(&service, &device).expect("fresh challenge accepted");
    let guard = state.lock().expect("state lock");
    assert_eq!(
        guard.state(&fresh.session_id),
        Some(ChallengeState::Pending),
        "the fresh challenge must be tracked"
    );
    assert!(
        guard.len() <= MAX_OUTSTANDING_CHALLENGES,
        "expired challenges must not permanently consume capacity"
    );
}

#[test]
fn auth11_malformed_response_cannot_bypass_rate_limiter() {
    let mut device = provision_device();
    let (_state, service) = challenge_service(&device);

    // Malformed responses: unknown valid-UUID session ids. Each is a failed
    // completion and counts toward the limit.
    for _ in 0..5 {
        let bogus = uuid::Uuid::new_v4();
        let result = service.complete(&mut device.db, &bogus, ADMIN_PASSPHRASE, &[0u8; 64]);
        assert!(
            matches!(
                result,
                Err(AppError::Authentication(
                    AuthenticationError::InvalidCredentials { .. }
                ))
            ),
            "malformed response must be rejected fail-closed, got {result:?}"
        );
    }

    // Even a fully valid challenge with the correct signature is now blocked.
    let challenge = begin_challenge(&service, &device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);
    let blocked = service.complete(
        &mut device.db,
        &challenge.session_id,
        ADMIN_PASSPHRASE,
        &signature,
    );
    assert_lockout_error(blocked);
}

#[test]
fn auth12_concurrent_attempts_respect_cap_and_limiter() {
    // The outstanding-challenge cap is enforced atomically under the state
    // mutex: no interleaving can push the store past the bound.
    let state = Arc::new(Mutex::new(IdentityChallengeState::default()));
    let mut handles = Vec::new();
    for _ in 0..64 {
        let state = state.clone();
        handles.push(std::thread::spawn(move || {
            let challenge =
                ChallengeMessage::new(uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), [3u8; 32]);
            state.lock().expect("state lock").try_begin(challenge, 0)
        }));
    }
    let accepted: usize = handles
        .into_iter()
        .map(|handle| handle.join().expect("thread") as usize)
        .sum();
    assert!(accepted <= MAX_OUTSTANDING_CHALLENGES);
    assert!(state.lock().expect("state lock").len() <= MAX_OUTSTANDING_CHALLENGES);

    // Concurrent failure recording on the shared limiter never panics and ends
    // in a consistent (locked) state.
    let limiter = Arc::new(Mutex::new(RateLimiter::with_settings(60, 5)));
    let mut handles = Vec::new();
    for _ in 0..20 {
        let limiter = limiter.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..3 {
                limiter.lock().expect("rl lock").record_failure("admin");
            }
        }));
    }
    for handle in handles {
        handle.join().expect("thread");
    }
    assert!(
        !limiter.lock().expect("rl lock").is_allowed("admin"),
        "concurrent failures must still lock the principal out"
    );
}

// ───────────────────── SEC-001-10: Challenge–Response failure audit ─────────────────────

/// Every `LoginFailed` audit entry recorded on this device's audit log.
fn login_failed_audits(db: &Database) -> Vec<grpc_lib::domain::audit::AuditEntry> {
    grpc_lib::application::services::AuditService::new(db.executor())
        .get_audit_entries(
            &AuditFilters {
                action: Some(AuditAction::LoginFailed.as_str().to_string()),
                ..Default::default()
            },
            0,
            1000,
        )
        .expect("audit query")
        .entries
}

/// SEC-001-10: a Challenge–Response attempt that reaches the completion
/// decision and fails MUST leave a `LoginFailed` audit record carrying only
/// the session id and a coarse reason — never the passphrase or signature.
#[test]
fn sec00110_failed_challenge_is_audited() {
    let mut device = provision_device();
    seed_admin_user(&mut device.db);
    let (_state, service) = challenge_service(&device);

    let challenge = begin_challenge(&service, &device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);
    let failed = service.complete(
        &mut device.db,
        &challenge.session_id,
        "wrong passphrase",
        &signature,
    );
    assert!(failed.is_err(), "wrong passphrase must fail");

    let entries = login_failed_audits(&device.db);
    assert_eq!(entries.len(), 1, "exactly one LoginFailed audit expected");
    let entry = &entries[0];
    assert_eq!(entry.username, "admin", "bootstrap admin principal");
    assert_eq!(
        entry.entity_id.as_deref(),
        Some(challenge.session_id.to_string().as_str()),
        "entity_id must be the challenge session id"
    );
    assert_eq!(
        entry.session_id.as_deref(),
        Some(challenge.session_id.to_string().as_str()),
        "session_id must be the challenge session id"
    );
    assert_eq!(
        entry.error_message.as_deref(),
        Some("بيانات الدخول غير صحيحة"),
        "coarse reason category only"
    );
    let rendered = serde_json::to_string(entry).expect("serialize entry");
    assert!(
        !rendered.contains("wrong passphrase") && !rendered.contains(ADMIN_PASSPHRASE),
        "no passphrase material may leak into the audit record"
    );
}

/// SEC-001-10: a successful completion must NOT emit a failure audit.
#[test]
fn sec00110_successful_challenge_is_not_audited_as_failure() {
    let mut device = provision_device();
    seed_admin_user(&mut device.db);
    let (_state, service) = challenge_service(&device);

    let challenge = begin_challenge(&service, &device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);
    service
        .complete(&mut device.db, &challenge.session_id, ADMIN_PASSPHRASE, &signature)
        .expect("valid credentials succeed");

    assert_eq!(
        login_failed_audits(&device.db).len(),
        0,
        "successful authentication must not be audited as a failure"
    );
}

/// SEC-001-10: an attempt blocked by the rate limiter never reaches the
/// completion decision — it must neither consume the challenge nor emit an
/// audit record (no-consume semantics preserved).
#[test]
fn sec00110_rate_limited_attempt_is_neither_consumed_nor_audited() {
    let mut device = provision_device();
    seed_admin_user(&mut device.db);
    let (_state, service) = challenge_service(&device);

    // 5 failed attempts (each consumes its own challenge) → lockout.
    for _ in 0..5 {
        fail_challenge_attempt(&service, &mut device);
    }

    // The 6th attempt is blocked before any decision.
    let challenge = begin_challenge(&service, &device);
    let signature = operator_sign(&challenge, &device.adminkey_provider);
    let blocked = service.complete(
        &mut device.db,
        &challenge.session_id,
        "wrong passphrase",
        &signature,
    );
    assert_lockout_error(blocked);
    assert_eq!(
        _state.lock().expect("state lock").state(&challenge.session_id),
        Some(ChallengeState::Pending),
        "a blocked attempt must not consume the challenge"
    );
    assert_eq!(
        login_failed_audits(&device.db).len(),
        5,
        "only the 5 attempts that reached a decision may be audited"
    );
}

/// Repoint the bootstrap admin `users` row id to `'admin'` so the audit_log
/// FK (user_id → users.id) accepts the audit writes emitted by the challenge
/// service, which address the canonical admin by `BOOTSTRAP_ADMIN_USERNAME`.
fn seed_admin_user(db: &mut Database) {
    db.get_connection()
        .execute(
            "UPDATE users SET id = 'admin' WHERE username = 'admin' AND deleted = 0",
            [],
        )
        .expect("repoint admin user id");
}
