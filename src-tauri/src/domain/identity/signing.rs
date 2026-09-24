//! Signing and verification interfaces.
//!
//! RFC 2026-08-04-node-identity-trust §3.5 / ADR-0038 §5 / ADR-0039 §5.
//!
//! The Ed25519 provider lives in `infrastructure/security/identity` and is
//! wired into Challenge–Response and package signing in B3/B4. The domain
//! depends only on these interfaces.
//!
//! Single-path contract (ADR-0039 §5): all certificate/challenge signing and
//! verification flows through the entity-level default methods
//! (`sign_certificate`, `verify_certificate`, `sign_challenge`,
//! `verify_challenge`), which operate exclusively on `canonical_bytes()`. The
//! raw `sign(&[u8])` / `verify(&[u8], ...)` methods are IMPLEMENTATION-INTERNAL:
//! they are implemented by infrastructure providers only and MUST NOT be called
//! from domain or application layers — no code may build the signed byte layout
//! by hand.

use crate::domain::identity::{ChallengeMessage, Ed25519CertificateSignature, IdentityCertificate};

/// `signature_version` / `algorithm_version` value for Ed25519 (RFC 8032).
///
/// SEC-007 (ADR-0047) / SEC-008 (ADR-0048): HMAC V1 was permanently removed —
/// Ed25519 is the ONLY supported signature scheme (sync V2 and fiscal closure
/// packages). This constant is the extension point guarded by check_arch
/// Rule 129 via `package_metadata::signature_version: Option<u16>`.
pub const SIGNATURE_VERSION_ED25519: u16 = 2;

/// Signing port — implementations hold the private signing key.
pub trait IdentitySigner: Send + Sync {
    /// Algorithm id reported with generated signatures (`SIGNATURE_VERSION_ED25519`).
    fn algorithm_version(&self) -> u16;

    /// IMPLEMENTATION-INTERNAL: sign raw `message` bytes. Infrastructure-only;
    /// domain/application MUST use the entity-level default methods below.
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, String>;

    /// Public verification material for this signer (opaque bytes).
    fn public_key(&self) -> Vec<u8>;

    /// Sign a certificate over its canonical bytes — the single certificate
    /// signing path (ADR-0039 §5).
    fn sign_certificate(&self, certificate: &IdentityCertificate) -> Result<Vec<u8>, String> {
        self.sign(&certificate.canonical_bytes())
    }

    /// Sign a challenge over its canonical bytes — the single challenge signing
    /// path (ADR-0039 §5).
    fn sign_challenge(&self, challenge: &ChallengeMessage) -> Result<Vec<u8>, String> {
        self.sign(&challenge.canonical_bytes())
    }
}

/// Verification port — stateless, takes key material per call.
pub trait IdentitySignatureVerifier: Send + Sync {
    /// IMPLEMENTATION-INTERNAL: verify raw `signature` over raw `message`.
    /// Infrastructure-only; domain/application MUST use the entity-level
    /// default methods below.
    ///
    /// `Ok(true)` = valid, `Ok(false)` = invalid signature, `Err` = malformed
    /// key or signature material. Implementations MUST use constant-time
    /// comparison (e.g. `subtle::ConstantTimeEq`).
    fn verify(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<bool, String>;

    /// Verify an ISSUER signature over a certificate's canonical bytes.
    ///
    /// Issuance (ADR-0039 §5): the issuing identity signs the certificate with
    /// its own private key; `issuer_public_key` is the issuer's public key.
    /// Verification mirrors issuance — it uses the issuer's key, NOT the
    /// certificate's own public key (self-signing is forbidden for ADMIN certs).
    fn verify_certificate(
        &self,
        certificate: &IdentityCertificate,
        issuer_public_key: &[u8],
        signature: &Ed25519CertificateSignature,
    ) -> Result<bool, String> {
        self.verify(
            issuer_public_key,
            &certificate.canonical_bytes(),
            signature.as_bytes(),
        )
    }

    /// Verify a challenge signature over its canonical bytes against a given
    /// public key (ADR-0039 §5).
    fn verify_challenge(
        &self,
        challenge: &ChallengeMessage,
        public_key: &[u8],
        signature: &[u8],
    ) -> Result<bool, String> {
        self.verify(public_key, &challenge.canonical_bytes(), signature)
    }
}
