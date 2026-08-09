//! Identity certificate — pure data model.
//!
//! RFC 2026-08-04-node-identity-trust §3.2 / ADR-0038 §1.
//!
//! `signature` is `Option` ONLY as a migration mechanism during the legacy
//! window (ADR-0039 §6): records issued before Identity Trust activation may
//! carry `None`. After activation every new certificate MUST be signed; a
//! certificate without a signature is a programming error (`require_signed`).

use serde::{Deserialize, Serialize};

use crate::domain::identity::{
    canonical, CredentialStatus, Ed25519CertificateSignature, SubjectType,
};

/// Identity algorithm profile identifier — an INDEPENDENT identifier space.
///
/// ADR-0039 §4: `algorithm_version` (profile) and `signature_version` are
/// independent spaces. Their values coincide today (`2`) by regulatory
/// convention, NOT by semantic relationship. `assert_eq!(algorithm_version,
/// signature_version)` is a defect. This profile covers Ed25519 (RFC 8032) with
/// `CANONICAL_ENCODING_VERSION = 1`.
pub const IDENTITY_ALGORITHM_PROFILE_ED25519: u16 = 2;

/// A node identity certificate.
///
/// Signatures and verification are handled by the `IdentitySigner` /
/// `IdentitySignatureVerifier` ports (see `signing.rs`); this struct carries
/// the immutable identity facts plus the public verification material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityCertificate {
    /// Immutable identity identifier — never changes across Rotate/Re-Issue (Invariant 1).
    pub identity_id: uuid::Uuid,
    pub subject_type: SubjectType,
    /// Referenced entity UUID (`units.id`, wilaya node, or admin subject).
    pub subject_id: uuid::Uuid,
    /// Issuing identity; `None` = offline Authority Root (escrow).
    pub issuer_identity_id: Option<uuid::Uuid>,
    /// Immutable across Rotate; changes on Re-Issue (Invariant 2).
    pub credential_id: uuid::Uuid,
    /// Strictly monotonic generation, starts at 1 (Invariant 3).
    pub generation: u64,
    pub status: CredentialStatus,
    /// Public verification material (opaque bytes, e.g. 32-byte EdDSA key).
    pub public_key: Vec<u8>,
    /// Identity algorithm profile (ADR-0039 §4), not the file `format_version`.
    pub algorithm_version: u16,
    /// Advisory expiry only — never used for lifecycle evaluation (Invariant 8).
    pub not_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Audit trace within IdentityState. The actual transport ordering ledger
    /// lives separately in `run_import_pipeline` (Transport Guard).
    pub package_sequence: Option<u64>,
    /// Issuer signature over `canonical_bytes()`. `None` allowed ONLY for legacy
    /// migration records; new certificates MUST be signed (ADR-0039 §6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<Ed25519CertificateSignature>,
}

impl IdentityCertificate {
    /// Canonical signed representation — the SINGLE source of truth for signing
    /// and verification (ADR-0039 §5). Deterministic, no wall clock, no
    /// randomness. `signature` and `package_sequence` are excluded: `signature`
    /// would be self-referential, and `package_sequence` is an audit trace, not
    /// an identity fact.
    ///
    /// Layout (v1): [version u16 BE][identity_id 16][subject_type 1][subject_id
    /// 16][issuer flag 1][issuer 16][credential_id 16][generation u64 BE]
    /// [status 1][public_key_len u32 BE][public_key][algorithm_version u16 BE]
    /// [not_after flag 1][not_after millis i64 BE].
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(140 + self.public_key.len());
        canonical::write_u16(&mut out, canonical::CANONICAL_ENCODING_VERSION);
        canonical::write_uuid(&mut out, &self.identity_id);
        out.push(canonical::subject_type_byte(self.subject_type));
        canonical::write_uuid(&mut out, &self.subject_id);
        match self.issuer_identity_id {
            Some(issuer) => {
                out.push(1);
                canonical::write_uuid(&mut out, &issuer);
            }
            None => out.push(0),
        }
        canonical::write_uuid(&mut out, &self.credential_id);
        canonical::write_u64(&mut out, self.generation);
        out.push(canonical::credential_status_byte(self.status));
        canonical::write_u32(&mut out, self.public_key.len() as u32);
        out.extend_from_slice(&self.public_key);
        canonical::write_u16(&mut out, self.algorithm_version);
        match self.not_after {
            Some(not_after) => {
                out.push(1);
                canonical::write_u64(&mut out, not_after.timestamp_millis() as u64);
            }
            None => out.push(0),
        }
        out
    }

    /// Certificate equivalence — the SOLE definition of "same certificate"
    /// (B5, RFC §3.6). Used by idempotent import paths to detect an identical
    /// re-presentation with ZERO writes.
    ///
    /// Compares the identity facts + signature:
    /// `identity_id`, `subject_type`, `subject_id`, `issuer_identity_id`,
    /// `credential_id`, `generation`, `public_key`, `algorithm_version`,
    /// `signature`.
    ///
    /// Intentionally EXCLUDED: `package_sequence` (transport audit trace) and
    /// `not_after` (advisory expiry). `status` is a lifecycle field managed by
    /// the Identity Store, not part of the signed identity facts.
    ///
    /// "Same identity, different generation" (Rotate/Re-Issue) is a DIFFERENT
    /// concern and MUST NOT use this helper.
    pub fn is_identical_to(&self, other: &IdentityCertificate) -> bool {
        self.identity_id == other.identity_id
            && self.subject_type == other.subject_type
            && self.subject_id == other.subject_id
            && self.issuer_identity_id == other.issuer_identity_id
            && self.credential_id == other.credential_id
            && self.generation == other.generation
            && self.public_key == other.public_key
            && self.algorithm_version == other.algorithm_version
            && self.signature == other.signature
    }

    /// Fail-closed guard: after Identity Trust activation every certificate MUST
    /// be signed (ADR-0039 §6). Unsigned records are migration leftovers only.
    pub fn require_signed(&self) -> crate::errors::AppResult<()> {
        if self.signature.is_none() {
            return Err(crate::errors::AppError::Internal(format!(
                "IdentityCertificate {} is unsigned — new certificates MUST be signed (ADR-0039 §6)",
                self.identity_id
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::{
        signing::SIGNATURE_VERSION_ED25519, CredentialStatus, Ed25519CertificateSignature,
    };
    use uuid::Uuid;

    fn fixed_certificate() -> IdentityCertificate {
        IdentityCertificate {
            identity_id: Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0001),
            subject_type: SubjectType::Wilaya,
            subject_id: Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0001),
            issuer_identity_id: None,
            credential_id: Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0002),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: vec![7u8; 32],
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        }
    }

    #[test]
    fn golden_canonical_bytes_v1() {
        // Golden test (ADR-0039 §5): pins the exact v1 canonical encoding.
        // [CANONICAL_ENCODING_VERSION u16 BE][identity_id 16][subject_type 1]
        // [subject_id 16][issuer flag 1][credential_id 16][generation u64 BE]
        // [status 1][public_key_len u32 BE][public_key 32][algorithm_version u16 BE]
        // [not_after flag 1]
        let expected = concat!(
            "0001",
            "00000000000000000000000000000001",
            "00",
            "00000000000000000000000000000001",
            "00",
            "00000000000000000000000000000002",
            "0000000000000001",
            "00",
            "00000020",
            "07070707070707070707070707070707",
            "07070707070707070707070707070707",
            "0002",
            "00"
        );
        assert_eq!(hex::encode(fixed_certificate().canonical_bytes()), expected);
        assert_eq!(fixed_certificate().canonical_bytes().len(), 100);
    }

    #[test]
    fn canonical_bytes_exclude_signature_and_sequence() {
        // The signed representation must not depend on the signature itself
        // (self-referential) nor on the transport audit trace.
        let mut with_sig = fixed_certificate();
        with_sig.signature = Some(Ed25519CertificateSignature::from_bytes([1u8; 64]));
        with_sig.package_sequence = Some(42);
        assert_eq!(
            with_sig.canonical_bytes(),
            fixed_certificate().canonical_bytes()
        );
    }

    #[test]
    fn canonical_bytes_are_deterministic() {
        let cert = fixed_certificate();
        assert_eq!(cert.canonical_bytes(), cert.canonical_bytes());
    }

    #[test]
    fn issuer_and_not_after_are_encoded_deterministically() {
        let mut cert = fixed_certificate();
        cert.issuer_identity_id = Some(Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0003));
        let bytes_with_issuer = cert.canonical_bytes();
        assert_eq!(bytes_with_issuer.len(), 100 + 16);
        assert_eq!(bytes_with_issuer[35], 1);
        assert_eq!(
            &bytes_with_issuer[36..52],
            Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0003).as_bytes()
        );

        let mut with_expiry = fixed_certificate();
        with_expiry.not_after = Some(chrono::DateTime::from_timestamp(1_752_700_000, 0).unwrap());
        let bytes_with_expiry = with_expiry.canonical_bytes();
        assert_eq!(bytes_with_expiry.len(), 100 + 8);
        assert_eq!(bytes_with_expiry[99], 1);
    }

    #[test]
    fn require_signed_rejects_unsigned_new_certificates() {
        assert!(fixed_certificate().require_signed().is_err());
        let mut cert = fixed_certificate();
        cert.signature = Some(Ed25519CertificateSignature::from_bytes([1u8; 64]));
        assert!(cert.require_signed().is_ok());
    }

    #[test]
    fn is_identical_to_true_for_exact_duplicate() {
        let a = fixed_certificate();
        let b = fixed_certificate();
        assert!(a.is_identical_to(&b));
        assert!(b.is_identical_to(&a));
    }

    #[test]
    fn is_identical_to_ignores_package_sequence_only() {
        let mut a = fixed_certificate();
        a.package_sequence = Some(7);
        let mut b = fixed_certificate();
        b.package_sequence = Some(99);
        // Transport audit trace is not an identity fact.
        assert!(a.is_identical_to(&b));
    }

    #[test]
    fn is_identical_to_ignores_advisory_status_and_expiry() {
        let mut a = fixed_certificate();
        a.status = CredentialStatus::Superseded;
        a.not_after = Some(chrono::DateTime::from_timestamp(1_752_700_000, 0).unwrap());
        assert!(a.is_identical_to(&fixed_certificate()));
    }

    #[test]
    fn is_identical_to_detects_identity_fact_drift() {
        let base = fixed_certificate();
        let mut identity_id = base.clone();
        identity_id.identity_id = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_00FF);
        assert!(!base.is_identical_to(&identity_id));

        let mut credential_id = base.clone();
        credential_id.credential_id = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_00FE);
        assert!(!base.is_identical_to(&credential_id));

        let mut generation = base.clone();
        generation.generation = 2;
        assert!(!base.is_identical_to(&generation));

        let mut public_key = base.clone();
        public_key.public_key = vec![8u8; 32];
        assert!(!base.is_identical_to(&public_key));

        let mut signature = base.clone();
        signature.signature = Some(Ed25519CertificateSignature::from_bytes([2u8; 64]));
        assert!(!base.is_identical_to(&signature));

        let mut issuer = base.clone();
        issuer.issuer_identity_id =
            Some(Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_00FD));
        assert!(!base.is_identical_to(&issuer));

        let mut subject = base.clone();
        subject.subject_id = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_00FC);
        assert!(!base.is_identical_to(&subject));

        let mut subject_type = base.clone();
        subject_type.subject_type = SubjectType::Admin;
        assert!(!base.is_identical_to(&subject_type));

        let mut algorithm = base.clone();
        algorithm.algorithm_version = 3;
        assert!(!base.is_identical_to(&algorithm));
    }
}
