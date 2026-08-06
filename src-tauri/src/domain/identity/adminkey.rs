//! Portable operator key file (`.adminkey`) — self-contained.
//!
//! RFC 2026-08-04-node-identity-trust §3.8 / ADR-0038 §5 / ADR-0039 §3.
//!
//! `.adminkey` represents the ADMIN identity and MUST be portable between
//! devices: it depends on no node-local secret (`GRPC_APP_KEY`, node identity,
//! or any local secret). The private key is encrypted with `age::scrypt`
//! (operator passphrase) — the ONLY legal use of scrypt (ADR-0039, Rule 38).
//!
//! `format_version` (file schema), `algorithm_version` (identity algorithm
//! profile), and `signature_version` (certificate signing scheme) are three
//! INDEPENDENT identifier spaces (ADR-0039 §4).

use serde::{Deserialize, Serialize};

use crate::domain::identity::{
    IdentityCertificate, IDENTITY_ALGORITHM_PROFILE_ED25519, SIGNATURE_VERSION_ED25519,
};
use crate::errors::{AppError, AppResult};

/// `.adminkey` file schema version — bump only on file-layout changes.
pub const ADMINKEY_FORMAT_VERSION: u16 = 1;

/// Base64 encoding of the age-encrypted secret key inside the JSON file.
mod base64_bytes {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(serde::de::Error::custom)
    }
}

/// The self-contained portable operator key file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminKeyFile {
    /// `.adminkey` file schema version (`ADMINKEY_FORMAT_VERSION`).
    pub format_version: u16,
    /// Identity algorithm profile (`IDENTITY_ALGORITHM_PROFILE_ED25519`) —
    /// independent of `format_version` and `signature_version` (ADR-0039 §4).
    pub algorithm_version: u16,
    /// Current ADMIN certificate (includes the issuer signature).
    pub certificate: IdentityCertificate,
    /// Ed25519 secret key, age-encrypted with `age::scrypt(passphrase)`.
    #[serde(with = "base64_bytes")]
    pub encrypted_private_key: Vec<u8>,
}

impl AdminKeyFile {
    /// Fail-closed structural validation (ADR-0039 §3, §4).
    ///
    /// Checks each identifier space independently. It does NOT assert
    /// `algorithm_version == certificate.algorithm_version` — that equality is
    /// coincidental today and is not a semantic relationship.
    pub fn validate(&self) -> AppResult<()> {
        if self.format_version != ADMINKEY_FORMAT_VERSION {
            return Err(AppError::FileFormat(format!(
                "Unsupported .adminkey format_version: {} (expected {ADMINKEY_FORMAT_VERSION})",
                self.format_version
            )));
        }
        if self.algorithm_version != IDENTITY_ALGORITHM_PROFILE_ED25519 {
            return Err(AppError::FileFormat(format!(
                "Unsupported .adminkey algorithm_version: {} (expected {IDENTITY_ALGORITHM_PROFILE_ED25519})",
                self.algorithm_version
            )));
        }
        // The embedded certificate must be signed (ADR-0039 §6).
        self.certificate.require_signed()?;
        // The certificate's signing scheme must be Ed25519 for this profile.
        if self.certificate.algorithm_version != SIGNATURE_VERSION_ED25519 {
            return Err(AppError::FileFormat(format!(
                "Unsupported certificate algorithm_version: {} (expected {SIGNATURE_VERSION_ED25519})",
                self.certificate.algorithm_version
            )));
        }
        if self.encrypted_private_key.is_empty() {
            return Err(AppError::FileFormat(
                ".adminkey encrypted_private_key is empty".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::{CredentialStatus, SubjectType};
    use uuid::Uuid;

    fn sample_file() -> AdminKeyFile {
        let certificate = IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_type: SubjectType::Admin,
            subject_id: Uuid::new_v4(),
            issuer_identity_id: Some(Uuid::new_v4()),
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: vec![1u8; 32],
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: Some(1),
            signature: Some(crate::domain::identity::Ed25519CertificateSignature::from_bytes(
                [9u8; 64],
            )),
        };
        AdminKeyFile {
            format_version: ADMINKEY_FORMAT_VERSION,
            algorithm_version: IDENTITY_ALGORITHM_PROFILE_ED25519,
            certificate,
            encrypted_private_key: vec![0u8, 1, 2, 3, 4],
        }
    }

    #[test]
    fn json_roundtrip_with_base64_key() {
        let file = sample_file();
        let json = serde_json::to_string(&file).unwrap();
        let back: AdminKeyFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, file);
        assert!(!json.contains("[0,1,2,3,4]"));
        assert!(back.validate().is_ok());
    }

    #[test]
    fn validate_rejects_wrong_format_version() {
        let mut file = sample_file();
        file.format_version = 99;
        assert!(file.validate().is_err());
    }

    #[test]
    fn validate_rejects_wrong_algorithm_profile() {
        let mut file = sample_file();
        file.algorithm_version = 3;
        assert!(file.validate().is_err());
    }

    #[test]
    fn validate_rejects_unsigned_certificate() {
        let mut file = sample_file();
        file.certificate.signature = None;
        assert!(file.validate().is_err());
    }

    #[test]
    fn validate_rejects_legacy_cert_algorithm_version() {
        let mut file = sample_file();
        file.certificate.algorithm_version = 1;
        assert!(file.validate().is_err());
    }

    #[test]
    fn validate_rejects_empty_encrypted_key() {
        let mut file = sample_file();
        file.encrypted_private_key = vec![];
        assert!(file.validate().is_err());
    }
}
