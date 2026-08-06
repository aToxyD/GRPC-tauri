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
use crate::domain::identity::{
    AdminKeyFile, ChallengeMessage, CredentialStatus, IdentityCertificate, IdentityChallengeState,
    IdentitySignatureVerifier, IdentitySigner, IdentityStorePort, SubjectType,
};
use crate::domain::security::PasswordHashPort;
use crate::errors::{AppError, AppResult, AuthenticationError};
use crate::infrastructure::identity::AdminKeyProvider;
use crate::repositories::{RepositoryProvider, identity_store::IdentityStoreRepository};

use super::identity_provisioning_service::BOOTSTRAP_ADMIN_USERNAME;
use super::session_establishment_service::{EstablishedSession, SessionEstablishmentService};
use super::UserService;

/// One-shot challenge login service.
pub struct IdentityChallengeService {
    challenge_state: Arc<Mutex<IdentityChallengeState>>,
    adminkey_provider: AdminKeyProvider,
    verifier: Arc<dyn IdentitySignatureVerifier>,
    password_port: Arc<dyn PasswordHashPort>,
}

impl IdentityChallengeService {
    pub fn new(
        challenge_state: Arc<Mutex<IdentityChallengeState>>,
        adminkey_provider: AdminKeyProvider,
        verifier: Arc<dyn IdentitySignatureVerifier>,
        password_port: Arc<dyn PasswordHashPort>,
    ) -> Self {
        Self {
            challenge_state,
            adminkey_provider,
            verifier,
            password_port,
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
    ) -> Self {
        Self::new(
            challenge_state,
            adminkey_provider,
            Arc::new(crate::infrastructure::security::Ed25519SignatureVerifier),
            password_port,
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
    pub fn begin(&self, store: &dyn IdentityStorePort) -> AppResult<ChallengeMessage> {
        let wilaya = store
            .get_active_by_subject_type(SubjectType::Wilaya)?
            .ok_or_else(|| {
                AppError::Internal("No ACTIVE WILAYA identity is provisioned".into())
            })?;
        let mut nonce = [0u8; 32];
        use rand::RngCore;
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let challenge =
            ChallengeMessage::new(uuid::Uuid::new_v4(), wilaya.identity_id, nonce);
        self.challenge_state
            .lock()
            .map_err(|e| AppError::Internal(format!("Failed to lock challenge state: {e}")))?
            .begin(challenge.clone());
        Ok(challenge)
    }

    /// Complete a challenge with the operator passphrase (B5 production path).
    ///
    /// The operator enters the `.adminkey` passphrase; the backend decrypts the
    /// secret key, signs the challenge IN RUST, and follows the exact same
    /// verification path as `complete(...)`. The frontend never sees key
    /// material — only the passphrase crosses the IPC boundary.
    pub fn complete_with_passphrase(
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

        let user = UserService::new(db.executor(), self.password_port.as_ref())
            .get_user_by_username(BOOTSTRAP_ADMIN_USERNAME)?
            .ok_or_else(|| {
                AppError::Internal("Local admin user is not provisioned".into())
            })?;

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
            .map_err(|e| AppError::Internal(format!("Challenge signature verification failed: {e}")))?
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
