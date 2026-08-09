//! ArtifactVerifier (infrastructure): technical (Integrity) verification of a
//! signed licensing artifact — no repository access, no business semantics,
//! no subject binding (ADR-0042 §9).
//!
//! Implements artifact-spec §4 steps 1–4: structure → version → `artifact_id`
//! recomputation → canonicality → Ed25519 signature against the supplied trust
//! anchor. Business validity (status, entitlements, license type, latest known
//! version) and subject binding (ADR-0042 §4) are handled by the application
//! `application::licensing` services.

use ed25519_dalek::VerifyingKey;

use crate::models::SignedLicenseArtifact;

use super::artifact_io::{self, ArtifactIoError};
use super::encoding::decode_base64url;
use super::signing::Verifier;

/// Supported contract major version set (VERSION_MATRIX.md). Currently v1 only.
pub const SUPPORTED_VERSION_MAJOR: u32 = 1;

/// The trust anchor the artifact is verified against: key id + raw 32-byte
/// Ed25519 public key. Bytes only — the stored `licensing_anchor` row supplies
/// these (application layer maps it here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorRef<'a> {
    pub key_id: &'a str,
    pub public_key: &'a [u8],
}

/// Errors produced by artifact verification. Every variant is a fail-closed rejection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    Parse(ArtifactIoError),
    UnsupportedVersion(u32),
    ArtifactIdMismatch { expected: String, found: String },
    NonCanonical,
    AnchorPublicKey(String),
    KeyIdMismatch { anchor: String, artifact: String },
    InvalidSignature,
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "verify: structure: {e}"),
            Self::UnsupportedVersion(v) => write!(f, "verify: unsupported contract version: {v}"),
            Self::ArtifactIdMismatch { expected, found } => write!(
                f,
                "verify: artifact_id mismatch (expected {expected}, found {found})"
            ),
            Self::NonCanonical => write!(f, "verify: non-canonical envelope bytes"),
            Self::AnchorPublicKey(msg) => write!(f, "verify: anchor public key: {msg}"),
            Self::KeyIdMismatch { anchor, artifact } => write!(
                f,
                "verify: key_id mismatch (anchor {anchor}, artifact {artifact})"
            ),
            Self::InvalidSignature => write!(f, "verify: invalid signature"),
        }
    }
}

impl std::error::Error for VerifyError {}

impl From<ArtifactIoError> for VerifyError {
    fn from(e: ArtifactIoError) -> Self {
        Self::Parse(e)
    }
}

/// Verifies the technical integrity of a signed artifact against the supplied
/// trust anchor (artifact-spec §4). Returns the parsed artifact on success.
pub fn verify(bytes: &[u8], anchor: &AnchorRef<'_>) -> Result<SignedLicenseArtifact, VerifyError> {
    // 1. Structure: parse + field/type validation (fail-closed on malformed envelopes).
    let artifact = artifact_io::decode(bytes)?;

    // 2. Version: supported set (VERSION_MATRIX.md).
    if artifact.version != SUPPORTED_VERSION_MAJOR {
        return Err(VerifyError::UnsupportedVersion(artifact.version));
    }

    // 3. Identity: recompute artifact_id from UCR and assert it matches.
    let expected_id = artifact_io::derive_artifact_id(
        artifact.version,
        &artifact.metadata.key_id,
        &artifact.metadata.issued_for,
        &artifact.payload,
    )?;
    if artifact.metadata.artifact_id != expected_id {
        return Err(VerifyError::ArtifactIdMismatch {
            expected: expected_id,
            found: artifact.metadata.artifact_id.clone(),
        });
    }

    // 4. Canonicality: re-encoding the parsed artifact must reproduce the exact
    //    input bytes (rejects whitespace, key reordering, unknown/extra fields).
    let canonical_bytes = artifact_io::encode(&artifact)?;
    if canonical_bytes != bytes {
        return Err(VerifyError::NonCanonical);
    }

    // 5. Key binding: artifact key_id must equal the anchor's key_id.
    if artifact.metadata.key_id != anchor.key_id {
        return Err(VerifyError::KeyIdMismatch {
            anchor: anchor.key_id.to_string(),
            artifact: artifact.metadata.key_id.clone(),
        });
    }

    // 6. Signature: canonical digest over the final unsigned bytes → SHA-256 →
    //    Ed25519, against the anchor public key.
    let final_unsigned =
        artifact_io::encode_unsigned(artifact.version, &artifact.metadata, &artifact.payload)?;
    let public_key = decode_anchor_public_key(anchor.public_key)?;
    let signature_bytes = decode_base64url(&artifact.signature.signature)
        .map_err(|e| VerifyError::Parse(ArtifactIoError::SignatureEncoding(e.to_string())))?;
    let verifier = Verifier::new(public_key);
    if !verifier.verify(&final_unsigned, &signature_bytes) {
        return Err(VerifyError::InvalidSignature);
    }

    Ok(artifact)
}

/// Converts raw 32-byte anchor public key into an Ed25519 `VerifyingKey`.
fn decode_anchor_public_key(public_key: &[u8]) -> Result<VerifyingKey, VerifyError> {
    if public_key.len() != 32 {
        return Err(VerifyError::AnchorPublicKey(format!(
            "length {} != 32",
            public_key.len()
        )));
    }
    let mut raw = [0u8; 32];
    raw.copy_from_slice(public_key);
    VerifyingKey::from_bytes(&raw).map_err(|e| VerifyError::AnchorPublicKey(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::licensing::encoding::encode_base64url;
    use crate::infrastructure::licensing::signing::{generate_signing_key, Signer};
    use crate::models::{
        ArtifactMetadata, ArtifactPayload, ArtifactSignatureRef, ContractVersionPayload,
        LicensePayload, SubjectPayload,
    };

    use serde_json::{json, Value};

    fn payload(status: &str) -> ArtifactPayload {
        ArtifactPayload {
            license: LicensePayload {
                id: "license-1".to_string(),
                type_key: "production".to_string(),
                status: status.to_string(),
            },
            subject: SubjectPayload {
                id: "subject-1".to_string(),
            },
            entitlements: vec!["core.sync".to_string()],
            contract_version: ContractVersionPayload { major: 1, minor: 0 },
        }
    }

    fn anchor_for(key_id: &str, signing_key: &ed25519_dalek::SigningKey) -> AnchorRef<'static> {
        let vk = ed25519_dalek::VerifyingKey::from(signing_key);
        // Leak boxed copies so the references can be 'static for the test helper.
        let bytes: &'static [u8] = Box::leak(vk.as_bytes().to_vec().into_boxed_slice());
        let key_id: &'static str = Box::leak(key_id.to_string().into_boxed_str());
        AnchorRef {
            key_id,
            public_key: bytes,
        }
    }

    fn signed_bytes(key_id: &str, signing_key: &ed25519_dalek::SigningKey) -> Vec<u8> {
        let payload = payload("active");
        let artifact_id =
            artifact_io::derive_artifact_id(1, key_id, "subject-1", &payload).unwrap();
        let metadata = ArtifactMetadata {
            key_id: key_id.to_string(),
            algorithm: "Ed25519".to_string(),
            artifact_id,
            issued_for: "subject-1".to_string(),
        };
        let unsigned = artifact_io::encode_unsigned(1, &metadata, &payload).unwrap();
        let sig = Signer::new(signing_key.clone()).sign(&unsigned);
        let artifact = SignedLicenseArtifact {
            version: 1,
            metadata,
            payload,
            signature: ArtifactSignatureRef {
                algorithm: "Ed25519".to_string(),
                key_id: key_id.to_string(),
                signature: encode_base64url(&sig),
            },
        };
        artifact_io::encode(&artifact).unwrap()
    }

    #[test]
    fn verifies_valid_artifact() {
        let key = generate_signing_key();
        let key_id = "lk-2026-0001";
        let anchor = anchor_for(key_id, &key);
        let bytes = signed_bytes(key_id, &key);
        let artifact = verify(&bytes, &anchor).unwrap();
        assert_eq!(artifact.metadata.artifact_id.len(), 64);
        assert_eq!(artifact.metadata.key_id, key_id);
    }

    #[test]
    fn rejects_signature_tampering() {
        let key = generate_signing_key();
        let key_id = "lk-2026-0001";
        let anchor = anchor_for(key_id, &key);
        let bytes = signed_bytes(key_id, &key);
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["signature"]["signature"] = json!(encode_base64url(&[0u8; 64]));
        let tampered = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            verify(&tampered, &anchor),
            Err(VerifyError::InvalidSignature)
        );
    }

    #[test]
    fn rejects_signature_from_wrong_key() {
        let key = generate_signing_key();
        let other_key = generate_signing_key();
        let key_id = "lk-2026-0001";
        let anchor = anchor_for(key_id, &other_key);
        let bytes = signed_bytes(key_id, &key);
        assert_eq!(verify(&bytes, &anchor), Err(VerifyError::InvalidSignature));
    }

    #[test]
    fn rejects_key_id_mismatch_with_anchor() {
        let key = generate_signing_key();
        let anchor = anchor_for("lk-ANCHOR", &key);
        let bytes = signed_bytes("lk-ARTIFACT", &key);
        assert!(matches!(
            verify(&bytes, &anchor),
            Err(VerifyError::KeyIdMismatch { .. })
        ));
    }

    #[test]
    fn rejects_unsupported_version() {
        let key = generate_signing_key();
        let key_id = "lk-2026-0001";
        let anchor = anchor_for(key_id, &key);
        let bytes = signed_bytes(key_id, &key);
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["version"] = json!(2);
        value["payload"]["contract_version"]["major"] = json!(2);
        let tampered = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            verify(&tampered, &anchor),
            Err(VerifyError::UnsupportedVersion(2))
        );
    }

    #[test]
    fn rejects_artifact_id_tampering() {
        let key = generate_signing_key();
        let key_id = "lk-2026-0001";
        let anchor = anchor_for(key_id, &key);
        let bytes = signed_bytes(key_id, &key);
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["metadata"]["artifact_id"] = json!("0".repeat(64));
        let tampered = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            verify(&tampered, &anchor),
            Err(VerifyError::ArtifactIdMismatch { .. })
        ));
    }

    #[test]
    fn rejects_non_canonical_bytes() {
        let key = generate_signing_key();
        let key_id = "lk-2026-0001";
        let anchor = anchor_for(key_id, &key);
        let bytes = signed_bytes(key_id, &key);
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["extra"] = json!(1);
        let tampered = serde_json::to_vec(&value).unwrap();
        assert_eq!(verify(&tampered, &anchor), Err(VerifyError::NonCanonical));
    }

    #[test]
    fn rejects_invalid_anchor_public_key() {
        let key = generate_signing_key();
        let key_id = "lk-2026-0001";
        let bytes = signed_bytes(key_id, &key);
        let bad = AnchorRef {
            key_id,
            public_key: &[0u8; 16],
        };
        assert!(matches!(
            verify(&bytes, &bad),
            Err(VerifyError::AnchorPublicKey(_))
        ));
    }
}
