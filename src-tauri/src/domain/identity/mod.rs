//! Node Identity & Trust domain model.
//!
//! RFC 2026-08-04-node-identity-trust / ADR-0038.
//!
//! Pure domain: MUST NOT import infrastructure, application, or commands.
//! The Identity Store is the single source of truth for identity state.

pub mod adminkey;
pub mod bootstrap;
pub mod canonical;
pub mod certificate;
pub mod challenge;
pub mod ports;
pub mod signature;
pub mod signing;
pub mod state;

pub use adminkey::{AdminKeyFile, ADMINKEY_FORMAT_VERSION};
pub use bootstrap::IdentityBootstrapState;
pub use canonical::CANONICAL_ENCODING_VERSION;
pub use certificate::{IdentityCertificate, IDENTITY_ALGORITHM_PROFILE_ED25519};
pub use challenge::{
    ChallengeMessage, ChallengeState, IdentityChallengeState, CHALLENGE_PROTOCOL_VERSION,
    CHALLENGE_VERSION,
};
pub use ports::IdentityStorePort;
pub use signature::Ed25519CertificateSignature;
pub use signing::{IdentitySignatureVerifier, IdentitySigner, SIGNATURE_VERSION_ED25519};
pub use state::{CredentialStatus, SubjectType};

/// Test-support: mock signature material for repository row-mapping tests.
///
/// Kept in the domain layer (the owner of the signature type) so that
/// repositories can exercise the `signature` column round-trip without
/// referencing asymmetric-crypto types outside their allowed layer
/// (Rule 126, RFC 2026-08-04 / ADR-0038).
#[cfg(test)]
pub fn test_signature(bytes: [u8; 64]) -> Ed25519CertificateSignature {
    Ed25519CertificateSignature::from_bytes(bytes)
}
