//! Canonical encoding primitives for identity objects.
//!
//! RFC 2026-08-04-node-identity-trust §3.7 / ADR-0038 / ADR-0039 §5.
//!
//! `IdentityCertificate::canonical_bytes()` and `ChallengeMessage::canonical_bytes()`
//! are the SINGLE source of truth for the signed representation. Issuance,
//! verification, fingerprinting (future), and tests all call the same functions;
//! no code may build the signed byte layout by hand. `CANONICAL_ENCODING_VERSION`
//! is emitted as the first two bytes of both encodings so that a future field
//! reordering cannot silently break compatibility.

/// Version of the canonical byte encoding emitted by identity objects.
///
/// Bump ONLY when the byte layout of either canonical encoding changes in a way
/// that is not backward compatible. This is an architecture contract
/// (ADR-0039 §5); golden tests pin the encoded output.
pub const CANONICAL_ENCODING_VERSION: u16 = 1;

use crate::domain::identity::{CredentialStatus, SubjectType};

pub fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

pub fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

pub fn write_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

pub fn write_uuid(out: &mut Vec<u8>, value: &uuid::Uuid) {
    out.extend_from_slice(value.as_bytes());
}

/// Deterministic byte tag for `SubjectType` (canonical representation).
pub fn subject_type_byte(value: SubjectType) -> u8 {
    match value {
        SubjectType::Wilaya => 0,
        SubjectType::Unit => 1,
        SubjectType::Admin => 2,
    }
}

/// Deterministic byte tag for `CredentialStatus` (canonical representation).
pub fn credential_status_byte(value: CredentialStatus) -> u8 {
    match value {
        CredentialStatus::Active => 0,
        CredentialStatus::Revoked => 1,
        CredentialStatus::Superseded => 2,
        CredentialStatus::Expired => 3,
    }
}
