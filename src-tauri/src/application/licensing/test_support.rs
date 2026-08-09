//! Shared test fixtures (library tests only).
//!
//! Builds structurally valid provisioning packages and signed license
//! artifacts for the application/licensing unit tests. The signing helpers
//! (`signing::Signer`) are `#[cfg(test)]` — `grpc` has no issuing ability in
//! production builds (ADR-0002 Invariant 1).

#![cfg(test)]

use crate::infrastructure::licensing::artifact_io;
use crate::infrastructure::licensing::encoding::encode_base64url;
use crate::infrastructure::licensing::provisioning::encode_provisioning_package;
use crate::infrastructure::licensing::signing::{generate_signing_key, Signer};
use crate::models::{
    ArtifactMetadata, ArtifactPayload, ArtifactSignatureRef, ContractVersionPayload,
    LicensePayload, SignedLicenseArtifact, SubjectPayload,
};

/// A licensor key pair + key_id: signs artifacts, its public key is the anchor.
pub struct Licensor {
    pub key_id: String,
    pub signing_key: ed25519_dalek::SigningKey,
}

impl Licensor {
    pub fn new(key_id: &str) -> Self {
        Self {
            key_id: key_id.to_string(),
            signing_key: generate_signing_key(),
        }
    }

    pub fn public_key(&self) -> Vec<u8> {
        self.signing_key.verifying_key().to_bytes().to_vec()
    }

    pub fn provisioning_package(&self) -> String {
        String::from_utf8(
            encode_provisioning_package(&self.key_id, &self.public_key())
                .expect("provisioning package"),
        )
        .expect("utf8")
    }

    /// Derive the Ed25519 public key for a raw 32-byte node secret.
    pub fn node_public_key(secret: &[u8; 32]) -> Vec<u8> {
        ed25519_dalek::SigningKey::from_bytes(secret)
            .verifying_key()
            .to_bytes()
            .to_vec()
    }

    /// Derives the artifact_id, signs the final unsigned bytes, and returns the
    /// canonical envelope bytes as a String (byte-identical to what `verify`
    /// requires for the canonicality check).
    pub fn sign_artifact(&self, artifact: &mut SignedLicenseArtifact) -> String {
        let artifact_id = artifact_io::derive_artifact_id(
            artifact.version,
            &artifact.metadata.key_id,
            &artifact.metadata.issued_for,
            &artifact.payload,
        )
        .expect("artifact id");
        artifact.metadata.artifact_id = artifact_id;
        let unsigned =
            artifact_io::encode_unsigned(artifact.version, &artifact.metadata, &artifact.payload)
                .expect("unsigned");
        let signature = Signer::new(self.signing_key.clone()).sign(&unsigned);
        artifact.signature.signature = encode_base64url(&signature);
        String::from_utf8(artifact_io::encode(artifact).expect("canonical")).expect("utf8")
    }
}

/// Canonical signed artifact for a given subject and entitlement set.
pub fn build_artifact(
    licensor: &Licensor,
    subject_id: &str,
    entitlements: &[&str],
    status: &str,
) -> String {
    let payload = ArtifactPayload {
        license: LicensePayload {
            id: "lic-test-0001".to_string(),
            type_key: "production".to_string(),
            status: status.to_string(),
        },
        subject: SubjectPayload {
            id: subject_id.to_string(),
        },
        entitlements: entitlements.iter().map(|e| e.to_string()).collect(),
        contract_version: ContractVersionPayload { major: 1, minor: 0 },
    };
    let mut artifact = SignedLicenseArtifact {
        version: 1,
        metadata: ArtifactMetadata {
            key_id: licensor.key_id.clone(),
            algorithm: "Ed25519".to_string(),
            artifact_id: String::new(),
            issued_for: subject_id.to_string(),
        },
        payload,
        signature: ArtifactSignatureRef {
            algorithm: "Ed25519".to_string(),
            key_id: licensor.key_id.clone(),
            signature: String::new(),
        },
    };
    licensor.sign_artifact(&mut artifact)
}
