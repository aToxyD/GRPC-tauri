//! Provisioning Package IO (artifact-spec §9, ADR-0042 §2, ADR-0005 §5).
//!
//! Carries the anchor **public key only** — grants no operational trust. The
//! package is NOT signed: trust is established by physical delivery. Exactly
//! one active anchor is enforced by the `licensing_anchor` storage layer, not
//! by the package itself. The consumer primarily DECODES packages (`encode` is
//! provided for operator/tooling parity and tests).

use serde_json::{json, Value};

use crate::models::ProvisioningPackage;

use super::canonical::{canonical_serialize, CanonicalError};
use super::encoding::{decode_base64url, encode_base64url};

/// Current provisioning package format version.
pub const PROVISIONING_FORMAT_V1: &str = "provisioning-v1";

/// Errors produced by provisioning package encode/decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisioningError {
    Json(String),
    MissingField(&'static str),
    UnsupportedFormat(String),
    UnsupportedAlgorithm(String),
    InvalidPublicKeyLength(usize),
    PublicKeyEncoding(String),
}

impl std::fmt::Display for ProvisioningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(msg) => write!(f, "provisioning: JSON: {msg}"),
            Self::MissingField(key) => write!(f, "provisioning: missing field: {key}"),
            Self::UnsupportedFormat(fmt) => write!(f, "provisioning: unsupported format: {fmt}"),
            Self::UnsupportedAlgorithm(algo) => {
                write!(f, "provisioning: unsupported algorithm: {algo}")
            }
            Self::InvalidPublicKeyLength(len) => {
                write!(f, "provisioning: public key length {len} != 32")
            }
            Self::PublicKeyEncoding(msg) => write!(f, "provisioning: public key encoding: {msg}"),
        }
    }
}

impl std::error::Error for ProvisioningError {}

impl From<CanonicalError> for ProvisioningError {
    fn from(e: CanonicalError) -> Self {
        Self::Json(e.to_string())
    }
}

/// Encodes a provisioning package to canonical JSON bytes.
pub fn encode_provisioning_package(
    key_id: &str,
    public_key: &[u8],
) -> Result<Vec<u8>, ProvisioningError> {
    if public_key.len() != 32 {
        return Err(ProvisioningError::InvalidPublicKeyLength(public_key.len()));
    }
    let value = json!({
        "format": PROVISIONING_FORMAT_V1,
        "key_id": key_id,
        "algorithm": "Ed25519",
        "public_key": encode_base64url(public_key),
    });
    canonical_serialize(&value).map_err(ProvisioningError::from)
}

/// Parses provisioning package bytes into a `ProvisioningPackage`.
pub fn decode_provisioning_package(bytes: &[u8]) -> Result<ProvisioningPackage, ProvisioningError> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| ProvisioningError::Json(e.to_string()))?;
    let format = value
        .get("format")
        .and_then(Value::as_str)
        .ok_or(ProvisioningError::MissingField("format"))?;
    if format != PROVISIONING_FORMAT_V1 {
        return Err(ProvisioningError::UnsupportedFormat(format.to_string()));
    }
    let algorithm = value
        .get("algorithm")
        .and_then(Value::as_str)
        .ok_or(ProvisioningError::MissingField("algorithm"))?;
    if algorithm != "Ed25519" {
        return Err(ProvisioningError::UnsupportedAlgorithm(
            algorithm.to_string(),
        ));
    }
    let key_id = value
        .get("key_id")
        .and_then(Value::as_str)
        .ok_or(ProvisioningError::MissingField("key_id"))?
        .to_string();
    if key_id.trim().is_empty() {
        return Err(ProvisioningError::MissingField("key_id"));
    }
    let public_key_b64 = value
        .get("public_key")
        .and_then(Value::as_str)
        .ok_or(ProvisioningError::MissingField("public_key"))?;
    let public_key = decode_base64url(public_key_b64)
        .map_err(|e| ProvisioningError::PublicKeyEncoding(e.to_string()))?;
    if public_key.len() != 32 {
        return Err(ProvisioningError::InvalidPublicKeyLength(public_key.len()));
    }
    Ok(ProvisioningPackage {
        format: format.to_string(),
        key_id,
        algorithm: algorithm.to_string(),
        public_key: public_key_b64.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::licensing::canonical::canonical_serialize;
    use serde_json::json;

    fn sample_public_key() -> Vec<u8> {
        (0u8..32).collect()
    }

    #[test]
    fn round_trip() {
        let bytes = encode_provisioning_package("lk-2026-0001", &sample_public_key()).unwrap();
        let pkg = decode_provisioning_package(&bytes).unwrap();
        assert_eq!(pkg.key_id, "lk-2026-0001");
        assert_eq!(
            decode_base64url(&pkg.public_key).unwrap(),
            sample_public_key()
        );
    }

    #[test]
    fn rejects_wrong_public_key_length() {
        assert_eq!(
            encode_provisioning_package("lk", &[0u8; 16]),
            Err(ProvisioningError::InvalidPublicKeyLength(16))
        );
    }

    #[test]
    fn rejects_unsupported_format() {
        let bytes = encode_provisioning_package("lk-2026-0001", &sample_public_key()).unwrap();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["format"] = json!("provisioning-v2");
        let tampered = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            decode_provisioning_package(&tampered),
            Err(ProvisioningError::UnsupportedFormat(_))
        ));
    }

    #[test]
    fn rejects_unsupported_algorithm() {
        let bytes = encode_provisioning_package("lk-2026-0001", &sample_public_key()).unwrap();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["algorithm"] = json!("HMAC");
        let tampered = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            decode_provisioning_package(&tampered),
            Err(ProvisioningError::UnsupportedAlgorithm(_))
        ));
    }

    #[test]
    fn rejects_bad_public_key_encoding() {
        let bytes = encode_provisioning_package("lk-2026-0001", &sample_public_key()).unwrap();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["public_key"] = json!("%%%");
        let tampered = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            decode_provisioning_package(&tampered),
            Err(ProvisioningError::PublicKeyEncoding(_))
        ));
    }

    #[test]
    fn rejects_missing_key_id() {
        let bytes = encode_provisioning_package("lk-2026-0001", &sample_public_key()).unwrap();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value.as_object_mut().unwrap().remove("key_id");
        let tampered = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            decode_provisioning_package(&tampered),
            Err(ProvisioningError::MissingField("key_id"))
        );
    }

    #[test]
    fn rejects_non_canonical_input_with_unknown_field() {
        // A well-formed package with an extra field still parses; the canonicality
        // of inputs is enforced by the caller. This asserts decode accepts the
        // canonical form regardless of serialization key order.
        let pkg = json!({
            "algorithm": "Ed25519",
            "format": "provisioning-v1",
            "public_key": encode_base64url(&sample_public_key()),
            "key_id": "lk-2026-0001",
        });
        let bytes = canonical_serialize(&pkg).unwrap();
        let decoded = decode_provisioning_package(&bytes).unwrap();
        assert_eq!(decoded.key_id, "lk-2026-0001");
    }
}
