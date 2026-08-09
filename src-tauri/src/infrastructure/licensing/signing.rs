//! Ed25519 signing/verification primitives (artifact-spec §4, ADR-0006 §2).
//!
//! Bytes-only API: no knowledge of licensing or artifacts. The pipeline is
//! `message -> SHA-256 -> Ed25519` exactly as defined by the artifact contract.
//! Signatures are deterministic (RFC 8032).
//!
//! The CONSUMER verifies only. The `Signer` / `generate_signing_key` helpers
//! exist solely to build signed fixtures in `#[cfg(test)]` — `grpc` gains no
//! issuing ability (ADR-0002 Invariant 1).

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

/// The signing algorithm identifier persisted with licensing artifacts and the
/// single active trust anchor. Owned here (the signing layer) and passed down
/// through services; repositories never hardcode algorithm names.
pub const LICENSE_SIGNING_ALGORITHM: &str = "Ed25519";

/// Cryptographic verifier wrapping a public (verifying) key.
pub struct Verifier {
    verifying_key: VerifyingKey,
}

impl Verifier {
    /// Wraps a verifying key.
    pub fn new(verifying_key: VerifyingKey) -> Self {
        Self { verifying_key }
    }

    /// Verifies `signature` against `sha256(message)`. Any failure returns false (fail-closed).
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        let Ok(sig) = Signature::from_slice(signature) else {
            return false;
        };
        let digest = Sha256::digest(message);
        self.verifying_key.verify_strict(&digest, &sig).is_ok()
    }

    /// The wrapped verifying key.
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }
}

impl From<VerifyingKey> for Verifier {
    fn from(key: VerifyingKey) -> Self {
        Self::new(key)
    }
}

/// Test-only signer wrapping a signing key. NOT compiled into non-test builds —
/// the consumer has no issuing ability.
#[cfg(test)]
pub struct Signer {
    signing_key: ed25519_dalek::SigningKey,
}

#[cfg(test)]
impl Signer {
    /// Wraps a signing key.
    pub fn new(signing_key: ed25519_dalek::SigningKey) -> Self {
        Self { signing_key }
    }

    /// Signs `sha256(message)` with Ed25519. Returns the raw 64-byte signature.
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        use ed25519_dalek::Signer as _;
        let digest = Sha256::digest(message);
        self.signing_key.sign(&digest).to_bytes().to_vec()
    }

    /// The public (verifying) key corresponding to this signer.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }
}

/// Test-only Ed25519 key generation.
#[cfg(test)]
pub fn generate_signing_key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message() -> &'static [u8] {
        b"canonical-bytes-of-a-license-artifact"
    }

    #[test]
    fn sign_verify_round_trip() {
        let key = generate_signing_key();
        let signer = Signer::new(key);
        let verifier = Verifier::new(signer.verifying_key());
        let sig = signer.sign(message());
        assert!(verifier.verify(message(), &sig));
    }

    #[test]
    fn tampered_message_fails() {
        let key = generate_signing_key();
        let signer = Signer::new(key);
        let verifier = Verifier::new(signer.verifying_key());
        let sig = signer.sign(message());
        let mut tampered = message().to_vec();
        tampered[0] ^= 0xFF;
        assert!(!verifier.verify(&tampered, &sig));
    }

    #[test]
    fn tampered_signature_fails() {
        let key = generate_signing_key();
        let signer = Signer::new(key);
        let verifier = Verifier::new(signer.verifying_key());
        let mut sig = signer.sign(message());
        sig[0] ^= 0xFF;
        assert!(!verifier.verify(message(), &sig));
    }

    #[test]
    fn wrong_key_fails() {
        let signer = Signer::new(generate_signing_key());
        let other = Verifier::new(VerifyingKey::from(&generate_signing_key()));
        let sig = signer.sign(message());
        assert!(!other.verify(message(), &sig));
    }

    #[test]
    fn invalid_signature_bytes_fail() {
        let key = generate_signing_key();
        let signer = Signer::new(key);
        let verifier = Verifier::new(signer.verifying_key());
        assert!(!verifier.verify(message(), b""));
        assert!(!verifier.verify(message(), &[0u8; 32]));
    }
}
