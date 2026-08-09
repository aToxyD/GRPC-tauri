//! Artifact reader: maps the signed licensing artifact between the model
//! `SignedLicenseArtifact` type and Canonical JSON envelope bytes
//! (artifact-spec §1–§2).
//!
//! Read/write only — `encode` does NOT sign, `decode` does NOT cryptographically
//! verify (that is `verification::verify`). Structural rules (field presence,
//! key_id consistency, version consistency, algorithm values, signature shape)
//! ARE enforced here, so malformed envelopes fail closed.

use serde_json::json;

use sha2::{Digest, Sha256};

use crate::models::{ArtifactMetadata, ArtifactPayload, SignedLicenseArtifact};

use super::canonical::{canonical_serialize, CanonicalError};
use super::encoding::decode_base64url;

/// Errors produced by artifact decode/encode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactIoError {
    Json(String),
    MissingField(&'static str),
    InvalidField(&'static str),
    UnsupportedAlgorithm(String),
    SignatureEncoding(String),
}

impl std::fmt::Display for ArtifactIoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(msg) => write!(f, "artifact IO: JSON: {msg}"),
            Self::MissingField(key) => write!(f, "artifact IO: missing field: {key}"),
            Self::InvalidField(key) => write!(f, "artifact IO: invalid field: {key}"),
            Self::UnsupportedAlgorithm(algo) => {
                write!(f, "artifact IO: unsupported algorithm: {algo}")
            }
            Self::SignatureEncoding(msg) => write!(f, "artifact IO: signature encoding: {msg}"),
        }
    }
}

impl std::error::Error for ArtifactIoError {}

impl From<CanonicalError> for ArtifactIoError {
    fn from(e: CanonicalError) -> Self {
        Self::Json(e.to_string())
    }
}

/// Serializes a parsed artifact to its canonical envelope bytes (artifact-spec §2).
/// Re-encoding a canonically-produced input is byte-identical (used by the
/// canonicality check in step 4).
pub fn encode(artifact: &SignedLicenseArtifact) -> Result<Vec<u8>, ArtifactIoError> {
    canonical_serialize(artifact).map_err(ArtifactIoError::from)
}

/// Builds the `Unsigned Canonical Representation` (UCR) — the canonical encoding
/// of the envelope with all fields, `artifact_id` treated as the empty string,
/// and `signature` excluded (artifact-spec §3.1).
pub fn ucr(
    version: u32,
    key_id: &str,
    issued_for: &str,
    payload: &ArtifactPayload,
) -> Result<Vec<u8>, ArtifactIoError> {
    let envelope = json!({
        "version": version,
        "metadata": {
            "key_id": key_id,
            "algorithm": "Ed25519",
            "artifact_id": "",
            "issued_for": issued_for,
        },
        "payload": payload_value(payload),
    });
    canonical_serialize(&envelope).map_err(ArtifactIoError::from)
}

/// Derives the deterministic `artifact_id`: `hex(lowercase(SHA-256(UCR)))`
/// (artifact-spec §3.2). Exactly 64 lowercase hex characters; no clock, UUID,
/// or registry state participates.
pub fn derive_artifact_id(
    version: u32,
    key_id: &str,
    issued_for: &str,
    payload: &ArtifactPayload,
) -> Result<String, ArtifactIoError> {
    let bytes = ucr(version, key_id, issued_for, payload)?;
    let digest = Sha256::digest(&bytes);
    Ok(hex::encode(digest))
}

/// Serializes the **final unsigned bytes**: the canonical envelope with
/// `artifact_id` filled in and NO `signature` field (artifact-spec §3.3 / §4
/// step 2 — the message actually signed).
pub fn encode_unsigned(
    version: u32,
    metadata: &ArtifactMetadata,
    payload: &ArtifactPayload,
) -> Result<Vec<u8>, ArtifactIoError> {
    let envelope = json!({
        "version": version,
        "metadata": {
            "key_id": metadata.key_id,
            "algorithm": "Ed25519",
            "artifact_id": metadata.artifact_id,
            "issued_for": metadata.issued_for,
        },
        "payload": payload_value(payload),
    });
    canonical_serialize(&envelope).map_err(ArtifactIoError::from)
}

/// Maps the seven canonical components onto the envelope `payload` object
/// (ARTIFACT_MODEL.md §2).
fn payload_value(payload: &ArtifactPayload) -> serde_json::Value {
    json!({
        "license": {
            "id": payload.license.id,
            "type_key": payload.license.type_key,
            "status": payload.license.status,
        },
        "subject": {
            "id": payload.subject.id,
        },
        "entitlements": payload.entitlements,
        "contract_version": {
            "major": payload.contract_version.major,
            "minor": payload.contract_version.minor,
        },
    })
}

/// Parses canonical envelope bytes into a `SignedLicenseArtifact`.
///
/// Structural validation only (artifact-spec §4 step 1): field presence/typing
/// via serde, Ed25519 algorithm on both metadata and signature, `signature.key_id
/// == metadata.key_id`, envelope `version == contract_version.major`, 64-lowercase-
/// hex `artifact_id`, and a base64url 64-byte signature.
pub fn decode(bytes: &[u8]) -> Result<SignedLicenseArtifact, ArtifactIoError> {
    let artifact: SignedLicenseArtifact =
        serde_json::from_slice(bytes).map_err(|e| ArtifactIoError::Json(e.to_string()))?;

    require_ed25519(&artifact.metadata.algorithm)?;
    require_ed25519(&artifact.signature.algorithm)?;

    if artifact.signature.key_id != artifact.metadata.key_id {
        return Err(ArtifactIoError::InvalidField("key_id"));
    }
    if artifact.version != artifact.payload.contract_version.major {
        return Err(ArtifactIoError::InvalidField("version"));
    }
    if !is_lower_hex_64(&artifact.metadata.artifact_id) {
        return Err(ArtifactIoError::InvalidField("artifact_id"));
    }
    let signature_bytes = decode_base64url(&artifact.signature.signature)
        .map_err(|e| ArtifactIoError::SignatureEncoding(e.to_string()))?;
    if signature_bytes.len() != 64 {
        return Err(ArtifactIoError::InvalidField("signature.length"));
    }
    Ok(artifact)
}

fn require_ed25519(algorithm: &str) -> Result<(), ArtifactIoError> {
    if algorithm != "Ed25519" {
        return Err(ArtifactIoError::UnsupportedAlgorithm(algorithm.to_string()));
    }
    Ok(())
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ArtifactSignatureRef;
    use serde_json::{json, Value};

    fn payload() -> ArtifactPayload {
        ArtifactPayload {
            license: crate::models::LicensePayload {
                id: "license-1".to_string(),
                type_key: "production".to_string(),
                status: "active".to_string(),
            },
            subject: crate::models::SubjectPayload {
                id: "subject-1".to_string(),
            },
            entitlements: vec!["feature-a".to_string(), "feature-b".to_string()],
            contract_version: crate::models::ContractVersionPayload { major: 1, minor: 0 },
        }
    }

    fn artifact() -> SignedLicenseArtifact {
        SignedLicenseArtifact {
            version: 1,
            metadata: crate::models::ArtifactMetadata {
                key_id: "lk-2026-0001".to_string(),
                algorithm: "Ed25519".to_string(),
                artifact_id: "a".repeat(64),
                issued_for: "subject-1".to_string(),
            },
            payload: payload(),
            signature: ArtifactSignatureRef {
                algorithm: "Ed25519".to_string(),
                key_id: "lk-2026-0001".to_string(),
                signature: super::super::encoding::encode_base64url(&[7u8; 64]),
            },
        }
    }

    fn valid_bytes() -> Vec<u8> {
        // Derive a real artifact_id so structural decode passes.
        let mut a = artifact();
        let id = derive_artifact_id(1, "lk-2026-0001", "subject-1", &a.payload).unwrap();
        a.metadata.artifact_id = id;
        // Sign with a dummy key for encode (no signature verification here).
        let signing_key = super::super::signing::generate_signing_key();
        let signer = super::super::signing::Signer::new(signing_key);
        let unsigned = encode_unsigned(1, &a.metadata, &a.payload).unwrap();
        let sig = signer.sign(&unsigned);
        a.signature.signature = super::super::encoding::encode_base64url(&sig);
        encode(&a).unwrap()
    }

    #[test]
    fn decode_accepts_well_formed_envelope() {
        assert!(decode(&valid_bytes()).is_ok());
    }

    #[test]
    fn decode_rejects_key_id_mismatch() {
        let bytes = valid_bytes();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["signature"]["key_id"] = json!("lk-DIFFERENT");
        let tampered = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            decode(&tampered),
            Err(ArtifactIoError::InvalidField("key_id"))
        );
    }

    #[test]
    fn decode_rejects_version_mismatch() {
        let bytes = valid_bytes();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["version"] = json!(2);
        let tampered = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            decode(&tampered),
            Err(ArtifactIoError::InvalidField("version"))
        );
    }

    #[test]
    fn decode_rejects_bad_artifact_id_shape() {
        let bytes = valid_bytes();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["metadata"]["artifact_id"] = json!("0".repeat(63));
        assert_eq!(
            decode(&serde_json::to_vec(&value).unwrap()),
            Err(ArtifactIoError::InvalidField("artifact_id"))
        );
        let mut value2: Value = serde_json::from_slice(&bytes).unwrap();
        value2["metadata"]["artifact_id"] = json!("A".repeat(64));
        assert_eq!(
            decode(&serde_json::to_vec(&value2).unwrap()),
            Err(ArtifactIoError::InvalidField("artifact_id"))
        );
    }

    #[test]
    fn decode_rejects_unsupported_algorithm() {
        let bytes = valid_bytes();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["metadata"]["algorithm"] = json!("HMAC");
        let tampered = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            decode(&tampered),
            Err(ArtifactIoError::UnsupportedAlgorithm(_))
        ));
    }

    #[test]
    fn decode_rejects_malformed_signature() {
        let bytes = valid_bytes();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["signature"]["signature"] = json!("###not-base64url###");
        let tampered = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            decode(&tampered),
            Err(ArtifactIoError::SignatureEncoding(_))
        ));
    }

    #[test]
    fn decode_rejects_short_signature() {
        let bytes = valid_bytes();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["signature"]["signature"] =
            json!(super::super::encoding::encode_base64url(&[0u8; 32]));
        let tampered = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            decode(&tampered),
            Err(ArtifactIoError::InvalidField("signature.length"))
        );
    }

    #[test]
    fn decode_rejects_missing_field() {
        let bytes = valid_bytes();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["payload"].as_object_mut().unwrap().remove("license");
        let tampered = serde_json::to_vec(&value).unwrap();
        assert!(matches!(decode(&tampered), Err(ArtifactIoError::Json(_))));
    }

    #[test]
    fn ucr_is_deterministic_and_excludes_signature() {
        let a = ucr(1, "lk-2026-0001", "subject-1", &payload()).unwrap();
        let b = ucr(1, "lk-2026-0001", "subject-1", &payload()).unwrap();
        assert_eq!(a, b);
        let value: Value = serde_json::from_slice(&a).unwrap();
        assert_eq!(value["metadata"]["artifact_id"], json!(""));
        assert!(value.get("signature").is_none());
    }

    #[test]
    fn derive_artifact_id_is_lowercase_hex_64() {
        let id = derive_artifact_id(1, "lk-2026-0001", "subject-1", &payload()).unwrap();
        assert_eq!(id.len(), 64);
        assert!(is_lower_hex_64(&id));
        assert!(id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    }

    #[test]
    fn derive_artifact_id_changes_when_payload_or_key_changes() {
        let base = derive_artifact_id(1, "lk-2026-0001", "subject-1", &payload()).unwrap();
        let mut other = payload();
        other.license.status = "revoked".to_string();
        let revoked = derive_artifact_id(1, "lk-2026-0001", "subject-1", &other).unwrap();
        assert_ne!(base, revoked);
        let other_subject = derive_artifact_id(1, "lk-2026-0001", "subject-2", &payload()).unwrap();
        assert_ne!(base, other_subject);
    }

    #[test]
    fn encode_unsigned_fills_artifact_id_and_omits_signature() {
        let metadata = ArtifactMetadata {
            key_id: "lk-2026-0001".to_string(),
            algorithm: "Ed25519".to_string(),
            artifact_id: "a".repeat(64),
            issued_for: "subject-1".to_string(),
        };
        let unsigned = encode_unsigned(1, &metadata, &payload()).unwrap();
        let value: Value = serde_json::from_slice(&unsigned).unwrap();
        assert_eq!(value["metadata"]["artifact_id"], json!("a".repeat(64)));
        assert!(value.get("signature").is_none());
    }
}
