//! Identity Challenge–Response application service (B3).
//!
//! RFC 2026-08-04-node-identity-trust §3.7 / ADR-0038 §5 / ADR-0039 §5.
//!
//! Two-step login for ADMIN operators:
//! - `begin()` issues a fresh one-shot challenge bound to the local WILAYA node
//!   identity (no wall clock; replay protection = one-shot state machine).
//! - `complete()` consumes the challenge atomically (fail-closed), validates the
//!   portable `.adminkey`, matches the presented ADMIN certificate against the
//!   Identity Store, verifies the issuer (WILAYA) signature and the challenge
//!   signature, decrypts the secret key with the operator passphrase, and
//!   establishes a session via `SessionEstablishmentService` (shared path).
//!
//! Security invariants:
//! - The challenge is flipped to `Consumed` on the FIRST completion attempt,
//!   regardless of outcome — replayed messages are always rejected.
//! - Verification mirrors issuance: issuer signatures use the issuer's public
//!   key; self-signing is forbidden (ADR-0039 §5).
//! - `age::scrypt` is used ONLY through `AdminKeyProvider` (Rule 38 carve-out).

use std::sync::{Arc, Mutex};

use crate::db::Database;
use crate::domain::audit::{AuditAction, EntityType};
use crate::domain::identity::{
    AdminKeyFile, ChallengeMessage, CredentialStatus, IdentityCertificate, IdentityChallengeState,
    IdentitySignatureVerifier, IdentitySigner, IdentityStorePort, MAX_OUTSTANDING_CHALLENGES,
    SubjectType,
};
use crate::domain::rate_limiter::RateLimiter;
use crate::domain::security::PasswordHashPort;
use crate::errors::{AppError, AppResult, AuthenticationError, ValidationError};
use crate::infrastructure::identity::AdminKeyProvider;
use crate::repositories::{identity_store::IdentityStoreRepository, RepositoryProvider};

use super::identity_provisioning_service::BOOTSTRAP_ADMIN_USERNAME;
use super::session_establishment_service::{EstablishedSession, SessionEstablishmentService};
use super::{AuditService, UserService};

/// Rate-limit key for Admin Challenge–Response (SEC-001-A).
///
/// The challenge path authenticates exactly one principal: the node-local
/// ADMIN operator (`BOOTSTRAP_ADMIN_USERNAME`). Keying the limiter on that
/// principal — NEVER on the challenge UUID — guarantees that requesting a
/// fresh challenge cannot reset the failure counter. This also unifies the
/// per-principal attempt budget with the password path for the same identity.
pub(crate) const CHALLENGE_RATE_LIMIT_KEY: &str = BOOTSTRAP_ADMIN_USERNAME;

/// One-shot challenge login service.
pub struct IdentityChallengeService {
    challenge_state: Arc<Mutex<IdentityChallengeState>>,
    adminkey_provider: AdminKeyProvider,
    verifier: Arc<dyn IdentitySignatureVerifier>,
    password_port: Arc<dyn PasswordHashPort>,
    rate_limiter: Arc<Mutex<RateLimiter>>,
}

impl IdentityChallengeService {
    pub fn new(
        challenge_state: Arc<Mutex<IdentityChallengeState>>,
        adminkey_provider: AdminKeyProvider,
        verifier: Arc<dyn IdentitySignatureVerifier>,
        password_port: Arc<dyn PasswordHashPort>,
        rate_limiter: Arc<Mutex<RateLimiter>>,
    ) -> Self {
        Self {
            challenge_state,
            adminkey_provider,
            verifier,
            password_port,
            rate_limiter,
        }
    }

    /// Production constructor: wires the default Ed25519 verifier.
    ///
    /// Keeps asymmetric crypto confined to the application/identity layers
    /// (Rule 126) — commands never reference `Ed25519*` types directly.
    pub fn with_default_verifier(
        challenge_state: Arc<Mutex<IdentityChallengeState>>,
        adminkey_provider: AdminKeyProvider,
        password_port: Arc<dyn PasswordHashPort>,
        rate_limiter: Arc<Mutex<RateLimiter>>,
    ) -> Self {
        Self::new(
            challenge_state,
            adminkey_provider,
            Arc::new(crate::infrastructure::security::Ed25519SignatureVerifier),
            password_port,
            rate_limiter,
        )
    }

    /// Begin a one-shot challenge against the identity store of `db`.
    ///
    /// Convenience for IPC handlers: avoids repository construction in the
    /// commands layer (Rule 16).
    pub fn begin_with_db(&self, db: &Database) -> AppResult<ChallengeMessage> {
        self.begin(&db.executor().identity_store())
    }

    /// Issue a fresh one-shot challenge bound to the local WILAYA node identity.
    ///
    /// Bounded by `MAX_OUTSTANDING_CHALLENGES`: expired challenges are pruned
    /// opportunistically and a store at capacity is rejected instead of growing
    /// without limit (SEC-001-C). `begin` deliberately does NOT consult the
    /// rate limiter — it is not an authentication attempt — so it can never
    /// reset the failure counter (SEC-001-A).
    pub fn begin(&self, store: &dyn IdentityStorePort) -> AppResult<ChallengeMessage> {
        let wilaya = store
            .get_active_by_subject_type(SubjectType::Wilaya)?
            .ok_or_else(|| AppError::Internal("No ACTIVE WILAYA identity is provisioned".into()))?;
        let mut nonce = [0u8; 32];
        use rand::RngCore;
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let challenge = ChallengeMessage::new(uuid::Uuid::new_v4(), wilaya.identity_id, nonce);
        let mut state = self
            .challenge_state
            .lock()
            .map_err(|e| AppError::Internal(format!("Failed to lock challenge state: {e}")))?;
        if !state.begin(challenge.clone()) {
            return Err(AppError::Internal(format!(
                "Too many outstanding challenges (limit {MAX_OUTSTANDING_CHALLENGES}); try again later"
            )));
        }
        Ok(challenge)
    }

    /// Complete a challenge with the operator passphrase (B5 production path).
    ///
    /// The operator enters the `.adminkey` passphrase; the backend decrypts the
    /// secret key, signs the challenge IN RUST, and follows the exact same
    /// verification path as `complete(...)`. The frontend never sees key
    /// material — only the passphrase crosses the IPC boundary.
    ///
    /// Rate limiting (SEC-001-A): a locked-out principal is rejected before any
    /// challenge is consumed and nothing is recorded; every attempt that passes
    /// the guard records success (reset) or a failure on
    /// `CHALLENGE_RATE_LIMIT_KEY`.
    pub fn complete_with_passphrase(
        &self,
        db: &mut Database,
        session_id: &uuid::Uuid,
        passphrase: &str,
    ) -> AppResult<EstablishedSession> {
        self.guard_rate_limit()?;
        let result = self.try_complete_with_passphrase(db, session_id, passphrase);
        if result.is_err() {
            // SEC-001-10: the attempt reached the completion decision and
            // failed — record a failure audit event (fail-soft). This runs
            // AFTER the decision and never touches the challenge state, so
            // one-shot consumption semantics are unchanged.
            self.audit_failure(db, session_id);
        }
        self.record_outcome(&result);
        result
    }

    /// Rate-limit guard, mirroring the password path (`commands/auth.rs`):
    /// blocked principals receive the same user-safe Arabic message and the
    /// pending challenge is NOT consumed by a blocked attempt.
    fn guard_rate_limit(&self) -> AppResult<()> {
        let rate_limiter = self.rate_limiter.lock().map_err(|e| {
            AppError::Internal(format!("Failed to lock rate limiter: {e}"))
        })?;
        if rate_limiter.is_allowed(CHALLENGE_RATE_LIMIT_KEY) {
            return Ok(());
        }
        let remaining_secs = Option::unwrap_or(
            rate_limiter.get_remaining_lockout_secs(CHALLENGE_RATE_LIMIT_KEY),
            300,
        );
        log::warn!(
            target: "grpc::auth",
            "challenge login rate limited: principal={} remaining_secs={}",
            CHALLENGE_RATE_LIMIT_KEY,
            remaining_secs
        );
        Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "login".to_string(),
            message: format!(
                "عدد المحاولات تجاوز الحد. انتظر {} دقيقة",
                remaining_secs / 60
            ),
        }))
    }

    /// Apply the existing security policy to the attempt outcome: a successful
    /// authentication resets the failure counter (`record_success`); every
    /// failed attempt counts toward the limit (`record_failure`).
    fn record_outcome(&self, result: &AppResult<EstablishedSession>) {
        let rate_limiter = match self.rate_limiter.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::warn!(target: "grpc::auth", "rate limiter lock failed on outcome record: {e}");
                return;
            }
        };
        match result {
            Ok(_) => rate_limiter.record_success(CHALLENGE_RATE_LIMIT_KEY),
            Err(_) => rate_limiter.record_failure(CHALLENGE_RATE_LIMIT_KEY),
        }
    }

    /// Emit a fail-soft `LoginFailed` audit event for a Challenge–Response
    /// attempt that reached the authentication completion decision and failed
    /// (SEC-001-10).
    ///
    /// The record carries only the challenge session id and the same coarse
    /// reason string used by the password path (`commands/auth.rs`): the
    /// challenge service's error taxonomy is deliberately fail-closed and
    /// coarse (user-caused and internal failures are intentionally
    /// indistinguishable), so a single category is the honest representation.
    /// It is emitted AFTER the failure decision, does not modify the challenge
    /// state (one-shot consumption semantics unchanged), and never alters the
    /// authentication outcome. Never contains the passphrase, the challenge
    /// nonce, the challenge signature, or any key material.
    fn audit_failure(&self, db: &Database, session_id: &uuid::Uuid) {
        if let Err(e) = AuditService::new(db.executor()).log_failure(
            BOOTSTRAP_ADMIN_USERNAME,
            BOOTSTRAP_ADMIN_USERNAME,
            AuditAction::LoginFailed,
            EntityType::User,
            Some(&session_id.to_string()),
            "بيانات الدخول غير صحيحة",
            Some(&session_id.to_string()),
        ) {
            log::error!(
                target: "grpc::audit",
                "AUDIT WRITE FAILED [challenge_login_failure] session={} err={:?}",
                session_id,
                e
            );
        }
    }

    /// Authentication attempt proper (after the rate-limit guard).
    fn try_complete_with_passphrase(
        &self,
        db: &mut Database,
        session_id: &uuid::Uuid,
        passphrase: &str,
    ) -> AppResult<EstablishedSession> {
        let challenge = self.consume(session_id)?;
        let adminkey = self.adminkey_provider.read()?;
        let signer = self.resolve_signer(&adminkey, passphrase)?;
        let challenge_signature = signer
            .sign_challenge(&challenge)
            .map_err(|e| AppError::Internal(format!("Challenge signing failed: {e}")))?;
        self.verify_and_establish(db, &challenge, &adminkey, &challenge_signature)
    }

    /// Complete a challenge with a precomputed challenge signature (test path).
    ///
    /// # Errors
    /// Every authentication failure returns `InvalidCredentials` (fail-closed);
    /// infrastructure failures (I/O, malformed key material) surface as their
    /// concrete `AppError` variants.
    pub fn complete(
        &self,
        db: &mut Database,
        session_id: &uuid::Uuid,
        passphrase: &str,
        challenge_signature: &[u8],
    ) -> AppResult<EstablishedSession> {
        self.guard_rate_limit()?;
        let result = self.try_complete(db, session_id, passphrase, challenge_signature);
        if result.is_err() {
            self.audit_failure(db, session_id);
        }
        self.record_outcome(&result);
        result
    }

    /// Authentication attempt proper (after the rate-limit guard).
    fn try_complete(
        &self,
        db: &mut Database,
        session_id: &uuid::Uuid,
        passphrase: &str,
        challenge_signature: &[u8],
    ) -> AppResult<EstablishedSession> {
        let challenge = self.consume(session_id)?;
        let adminkey = self.adminkey_provider.read()?;
        let signer = self.resolve_signer(&adminkey, passphrase)?;
        let _ = signer;
        self.verify_and_establish(db, &challenge, &adminkey, challenge_signature)
    }

    /// Decrypt the `.adminkey` secret key and bind it to the certificate public
    /// key (fail-closed: mismatch is an authentication rejection).
    fn resolve_signer(
        &self,
        adminkey: &AdminKeyFile,
        passphrase: &str,
    ) -> AppResult<crate::infrastructure::security::Ed25519SigningProvider> {
        let secret_key = self
            .adminkey_provider
            .decrypt_private_key(&adminkey.encrypted_private_key, passphrase)?;
        let signer = crate::infrastructure::security::Ed25519SigningProvider::new(secret_key);
        if signer.public_key() != adminkey.certificate.public_key {
            return Err(reject_challenge());
        }
        Ok(signer)
    }

    /// Shared verification + session establishment for every challenge path.
    fn verify_and_establish(
        &self,
        db: &mut Database,
        challenge: &ChallengeMessage,
        adminkey: &AdminKeyFile,
        challenge_signature: &[u8],
    ) -> AppResult<EstablishedSession> {
        let presented = adminkey.certificate.clone();
        {
            let store = db.executor().identity_store();
            let stored = store
                .get_active_by_credential_id(&presented.credential_id)?
                .ok_or_else(reject_challenge)?;
            verify_identity_match(&presented, &stored)?;
            self.verify_issuer_and_challenge(challenge, &presented, &store, challenge_signature)?;
        }

        use crate::infrastructure::security::node_identity_provider::NodeIdentityProvider as _;
        let node_scope =
            crate::infrastructure::security::SettingsNodeIdentityProvider::new(db.executor())
                .current_node_id()
                .unwrap_or_else(|_| "WILAYA".to_string());
        let user = UserService::new(db.executor(), self.password_port.as_ref())
            .get_user_by_username(BOOTSTRAP_ADMIN_USERNAME, &node_scope)?
            .ok_or_else(|| AppError::Internal("Local admin user is not provisioned".into()))?;

        SessionEstablishmentService::establish(
            db,
            &user,
            Some(challenge.session_id.to_string()),
            "challenge",
        )
    }

    /// Atomic `Pending → Consumed` transition (fail-closed replay protection).
    fn consume(&self, session_id: &uuid::Uuid) -> AppResult<ChallengeMessage> {
        self.challenge_state
            .lock()
            .map_err(|e| AppError::Internal(format!("Failed to lock challenge state: {e}")))?
            .consume(session_id)
            .ok_or_else(reject_challenge)
    }

    /// Issuer verification (WILAYA signed the ADMIN cert) + challenge-signature
    /// verification against the ADMIN certificate's public key.
    fn verify_issuer_and_challenge(
        &self,
        challenge: &ChallengeMessage,
        presented: &IdentityCertificate,
        store: &IdentityStoreRepository<'_>,
        challenge_signature: &[u8],
    ) -> AppResult<()> {
        let issuer_id = presented.issuer_identity_id.ok_or_else(reject_challenge)?;
        if challenge.node_identity_id != issuer_id {
            return Err(reject_challenge());
        }
        let issuer_cert = store
            .get_by_identity_id(&issuer_id)?
            .ok_or_else(reject_challenge)?;
        let cert_signature = presented.signature.ok_or_else(reject_challenge)?;

        if !self
            .verifier
            .verify_certificate(presented, &issuer_cert.public_key, &cert_signature)
            .map_err(|e| AppError::Internal(format!("ADMIN cert verification failed: {e}")))?
        {
            return Err(reject_challenge());
        }
        if !self
            .verifier
            .verify_challenge(challenge, &presented.public_key, challenge_signature)
            .map_err(|e| {
                AppError::Internal(format!("Challenge signature verification failed: {e}"))
            })?
        {
            return Err(reject_challenge());
        }
        Ok(())
    }
}

/// Identity-level certificate match (Invariant 6/9, ADR-0038): the presented
/// cert must equal the ACTIVE stored cert for its credential on every identity
/// fact (delegated to `IdentityCertificate::is_identical_to`, the SOLE
/// equivalence definition). Revoked/superseded/tampered material is rejected
/// even if the `.adminkey` file itself is well-formed.
fn verify_identity_match(
    presented: &IdentityCertificate,
    stored: &IdentityCertificate,
) -> AppResult<()> {
    if stored.status != CredentialStatus::Active {
        return Err(reject_challenge());
    }
    if !presented.is_identical_to(stored) {
        return Err(reject_challenge());
    }
    Ok(())
}

/// Uniform fail-closed rejection for every authentication failure.
fn reject_challenge() -> AppError {
    AppError::Authentication(AuthenticationError::InvalidCredentials {
        username: "<challenge>".to_string(),
    })
}
