//! Ed25519 (RFC 8032) signing and verification providers.
//!
//! RFC 2026-08-04-node-identity-trust §3.5 / ADR-0038 §5.
//!
//! Signing: raw secret key (32 bytes) bound to a node identity. Verification:
//! stateless, takes the public key per call. Signatures are verified with
//! `verify_strict` and comparison is constant-time (ed25519-dalek contract).

use crate::domain::identity::{IdentitySignatureVerifier, IdentitySigner};

const ED25519_PUBLIC_KEY_LEN: usize = 32;
const ED25519_SIGNATURE_LEN: usize = 64;

/// Signer holding an Ed25519 signing key.
#[derive(Clone)]
pub struct Ed25519SigningProvider {
    keypair: ed25519_dalek::SigningKey,
}

impl Ed25519SigningProvider {
    /// Build a signer from the raw 32-byte secret key.
    pub fn new(secret_key: [u8; 32]) -> Self {
        Self {
            keypair: ed25519_dalek::SigningKey::from_bytes(&secret_key),
        }
    }
}

impl IdentitySigner for Ed25519SigningProvider {
    fn algorithm_version(&self) -> u16 {
        crate::domain::identity::SIGNATURE_VERSION_ED25519
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, String> {
        use ed25519_dalek::Signer;
        Ok(self.keypair.sign(message).to_bytes().to_vec())
    }

    fn public_key(&self) -> Vec<u8> {
        self.keypair.verifying_key().to_bytes().to_vec()
    }
}

/// Stateless Ed25519 verifier.
pub struct Ed25519SignatureVerifier;

impl IdentitySignatureVerifier for Ed25519SignatureVerifier {
    fn verify(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<bool, String> {
        let pk_bytes: [u8; ED25519_PUBLIC_KEY_LEN] = public_key.try_into().map_err(|_| {
            format!(
                "invalid public key length: expected {ED25519_PUBLIC_KEY_LEN} bytes, got {}",
                public_key.len()
            )
        })?;
        let sig_bytes: [u8; ED25519_SIGNATURE_LEN] = signature.try_into().map_err(|_| {
            format!(
                "invalid signature length: expected {ED25519_SIGNATURE_LEN} bytes, got {}",
                signature.len()
            )
        })?;

        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pk_bytes)
            .map_err(|e| format!("invalid Ed25519 public key: {e}"))?;
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);

        Ok(verifying_key.verify_strict(message, &sig).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET_KEY: [u8; 32] = [42u8; 32];

    #[test]
    fn sign_verify_roundtrip() {
        let signer = Ed25519SigningProvider::new(SECRET_KEY);
        assert_eq!(signer.algorithm_version(), 2);

        let public_key = signer.public_key();
        assert_eq!(public_key.len(), ED25519_PUBLIC_KEY_LEN);

        let message = b"canonical challenge bytes";
        let signature = signer.sign(message).unwrap();
        assert_eq!(signature.len(), ED25519_SIGNATURE_LEN);

        let verifier = Ed25519SignatureVerifier;
        assert!(verifier.verify(&public_key, message, &signature).unwrap());
    }

    #[test]
    fn tampered_message_is_rejected() {
        let signer = Ed25519SigningProvider::new(SECRET_KEY);
        let verifier = Ed25519SignatureVerifier;

        let signature = signer.sign(b"original").unwrap();
        assert!(!verifier
            .verify(&signer.public_key(), b"tampered", &signature)
            .unwrap());
    }

    #[test]
    fn wrong_key_is_rejected() {
        let signer = Ed25519SigningProvider::new(SECRET_KEY);
        let other = Ed25519SigningProvider::new([7u8; 32]);
        let verifier = Ed25519SignatureVerifier;

        let signature = signer.sign(b"message").unwrap();
        assert!(!verifier
            .verify(&other.public_key(), b"message", &signature)
            .unwrap());
    }

    #[test]
    fn malformed_material_is_an_error_not_a_rejection() {
        let verifier = Ed25519SignatureVerifier;
        assert!(verifier.verify(&[1u8; 16], b"m", &[0u8; 64]).is_err());
        assert!(verifier.verify(&[1u8; 32], b"m", &[0u8; 63]).is_err());
    }
}
