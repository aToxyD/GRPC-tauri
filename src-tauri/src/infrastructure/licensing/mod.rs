//! Licensing consumer infrastructure — bytes only (ADR-0042 §9).
//!
//! Canonical JSON (artifact-spec §3), base64url encoding, Ed25519+SHA-256
//! verification (spec §4 steps 1–4), envelope IO, and provisioning package
//! parsing. No repository access, no business semantics, no licensing state.
//! The consumer has NO issuing ability: signing helpers exist only under
//! `#[cfg(test)]` for fixtures.

pub mod artifact_io;
pub mod canonical;
pub mod encoding;
pub mod provisioning;
pub mod signing;
pub mod verification;

pub use artifact_io::{derive_artifact_id, encode_unsigned, ucr, ArtifactIoError};
pub use canonical::{canonical_serialize, CanonicalError};
pub use encoding::{decode_base64url, encode_base64url, EncodingError};
pub use provisioning::ProvisioningError;
pub use signing::Verifier;
pub use verification::{verify, VerifyError, SUPPORTED_VERSION_MAJOR};
