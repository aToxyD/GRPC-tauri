//! Ed25519 (RFC 8032) package signing — `signature_version = 2`.
//!
//! RFC 2026-08-04-node-identity-trust §3.10: B4 sync packages are signed by
//! the issuing node identity with Ed25519 over the Canonical JSON V2 bytes
//! (ADR-0009) — the same envelope the integrity hash covers. HMAC V1 remains
//! readable during the deprecation window (`HmacPackageSigner`).
//!
//! Placement: `infrastructure/sync/packages/signing` — check_arch Rule 126
//! permits the ed25519 token in infrastructure layers; it is forbidden in
//! `commands/**`, `models/**`, and `repositories/**`.

use core::fmt::Write as _;

use crate::domain::identity::{IdentitySignatureVerifier, IdentitySigner};
use crate::errors::{AppError, AppResult};
use crate::infrastructure::security::{Ed25519SignatureVerifier, Ed25519SigningProvider};

use super::{PackageSigner, PackageVerifier};

const ED25519_SIGNATURE_LEN: usize = 64;
const ED25519_PUBLIC_KEY_LEN: usize = 32;

/// Package signer bound to a node identity's raw 32-byte Ed25519 signing key.
#[derive(Debug, Clone)]
pub struct Ed25519PackageSigner {
    secret_key: [u8; 32],
}

impl Ed25519PackageSigner {
    pub fn new(secret_key: [u8; 32]) -> Self {
        Self { secret_key }
    }

    /// Hex-encoded public verification key — the package `signing_key_id` for V2.
    pub fn public_key_hex(&self) -> String {
        hex::encode(Ed25519SigningProvider::new(self.secret_key).public_key())
    }

    /// `SIGNATURE_VERSION_ED25519` (2) — written into `metadata.signature_version`.
    pub fn signature_version(&self) -> u16 {
        crate::domain::identity::SIGNATURE_VERSION_ED25519
    }
}

impl PackageSigner for Ed25519PackageSigner {
    fn sign(&self, plaintext: &[u8]) -> AppResult<String> {
        let signature = Ed25519SigningProvider::new(self.secret_key)
            .sign(plaintext)
            .map_err(|e| AppError::Internal(format!("Ed25519 package signing failed: {e}")))?;
        if signature.len() != ED25519_SIGNATURE_LEN {
            return Err(AppError::Internal(format!(
                "Ed25519 provider returned unexpected signature length {}",
                signature.len()
            )));
        }
        let mut out = String::with_capacity(signature.len() * 2);
        for b in signature {
            let _ = write!(&mut out, "{:02x}", b);
        }
        Ok(out)
    }
}

/// Stateless Ed25519 package verifier bound to the issuer's public key.
#[derive(Debug, Clone, Copy)]
pub struct Ed25519PackageVerifier {
    public_key: [u8; ED25519_PUBLIC_KEY_LEN],
}

impl Ed25519PackageVerifier {
    pub fn new(public_key: [u8; ED25519_PUBLIC_KEY_LEN]) -> Self {
        Self { public_key }
    }

    /// Build a verifier from a hex-encoded 32-byte public key string.
    pub fn from_hex(public_key_hex: &str) -> AppResult<Self> {
        let s = public_key_hex.trim();
        if s.len() != ED25519_PUBLIC_KEY_LEN * 2 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(AppError::Validation(
                crate::errors::ValidationError::InvalidFormat {
                    field: "public_key".into(),
                    message: "مفتاح عام غير صالح (يُتوقّع 32 بايت سداسي)".into(),
                },
            ));
        }
        let mut out = [0u8; ED25519_PUBLIC_KEY_LEN];
        for i in 0..ED25519_PUBLIC_KEY_LEN {
            out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| {
                AppError::Validation(crate::errors::ValidationError::InvalidFormat {
                    field: "public_key".into(),
                    message: "تعذّر فك ترميز المفتاح العام السداسي".into(),
                })
            })?;
        }
        Ok(Self { public_key: out })
    }
}

fn decode_hex_ed25519_signature(signature: &str) -> AppResult<[u8; ED25519_SIGNATURE_LEN]> {
    let s = signature.trim();
    if s.len() != ED25519_SIGNATURE_LEN * 2 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::Validation(
            crate::errors::ValidationError::InvalidFormat {
                field: "signature".into(),
                message: "توقيع الحزمة ليس توقيعًا سداسيًا بطول 64 بايت".into(),
            },
        ));
    }
    let mut out = [0u8; ED25519_SIGNATURE_LEN];
    for i in 0..ED25519_SIGNATURE_LEN {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| {
            AppError::Validation(crate::errors::ValidationError::InvalidFormat {
                field: "signature".into(),
                message: "تعذّر فك ترميز التوقيع السداسي".into(),
            })
        })?;
    }
    Ok(out)
}

impl PackageVerifier for Ed25519PackageVerifier {
    fn verify(&self, plaintext: &[u8], signature: &str) -> AppResult<bool> {
        let signature = match decode_hex_ed25519_signature(signature) {
            Ok(s) => s,
            // Malformed signature material is a rejection, not an error.
            Err(_) => return Ok(false),
        };
        Ed25519SignatureVerifier
            .verify(&self.public_key, plaintext, &signature)
            .map_err(|e| AppError::Internal(format!("Ed25519 package verification failed: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET_KEY: [u8; 32] = [42u8; 32];

    #[test]
    fn sign_verify_roundtrip_over_canonical_bytes() {
        let signer = Ed25519PackageSigner::new(SECRET_KEY);
        assert_eq!(signer.signature_version(), 2);
        assert_eq!(signer.public_key_hex().len(), 64);

        let verifier = Ed25519PackageVerifier::new(
            Ed25519SigningProvider::new(SECRET_KEY)
                .public_key()
                .try_into()
                .unwrap(),
        );

        let payload = b"canonical envelope bytes";
        let signature = signer.sign(payload).unwrap();
        assert!(verifier.verify(payload, &signature).unwrap());
    }

    #[test]
    fn tampered_payload_is_rejected() {
        let signer = Ed25519PackageSigner::new(SECRET_KEY);
        let public_key: [u8; 32] = Ed25519SigningProvider::new(SECRET_KEY)
            .public_key()
            .try_into()
            .unwrap();
        let verifier = Ed25519PackageVerifier::new(public_key);

        let signature = signer.sign(b"original").unwrap();
        assert!(!verifier.verify(b"tampered", &signature).unwrap());
    }

    #[test]
    fn wrong_public_key_is_rejected() {
        let signer = Ed25519PackageSigner::new(SECRET_KEY);
        let other: [u8; 32] = Ed25519SigningProvider::new([7u8; 32])
            .public_key()
            .try_into()
            .unwrap();
        let verifier = Ed25519PackageVerifier::new(other);

        let signature = signer.sign(b"message").unwrap();
        assert!(!verifier.verify(b"message", &signature).unwrap());
    }

    #[test]
    fn malformed_signature_is_rejected_not_errored() {
        let public_key: [u8; 32] = Ed25519SigningProvider::new(SECRET_KEY)
            .public_key()
            .try_into()
            .unwrap();
        let verifier = Ed25519PackageVerifier::new(public_key);
        assert!(!verifier.verify(b"m", "not-hex").unwrap());
        assert!(!verifier.verify(b"m", &"ab".repeat(31)).unwrap());
        assert!(!verifier.verify(b"m", &"00".repeat(64)).unwrap());
    }

    #[test]
    fn verifier_from_hex_accepts_valid_32_byte_key() {
        let signer = Ed25519PackageSigner::new(SECRET_KEY);
        let from_hex = Ed25519PackageVerifier::from_hex(&signer.public_key_hex()).unwrap();

        let signature = signer.sign(b"payload").unwrap();
        assert!(from_hex.verify(b"payload", &signature).unwrap());

        assert!(Ed25519PackageVerifier::from_hex("zz").is_err());
        assert!(Ed25519PackageVerifier::from_hex(&"00".repeat(31)).is_err());
    }
}
