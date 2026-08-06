//! Identity rotation application service (B7).
//!
//! RFC 2026-08-04-node-identity-trust §3.3 / §3.4.2 / ADR-0038.
//!
//! Two-phase rotation model:
//! - `plan`  builds the unsigned draft certificate (the CSR) for a Rotate or
//!   Re-Issue from the currently stored ACTIVE identity and a new node key.
//!   The secret NEVER leaves the node (it is staged via `NodeKeyStore`
//!   `write_pending`); the CSR is a transport artifact like bootstrap.
//! - `verify_finalize` validates a signed rotation certificate before it is
//!   installed. Fail-closed: identity stability (Invariant 1), subject
//!   binding, ACTIVE status, Root-pin/issuer signature, R5 (the staged node
//!   key MUST match the certificate public key), and the Credential Guard
//!   (strictly increasing generation per credential). An identical
//!   re-presentation yields `Replay` with ZERO writes; any real difference
//!   fails closed instead of mutating state.
//!
//! No wall clock is consulted (Invariant 8). `issuer_identity_id` is bound by
//! the issuer at signing time (Root for WILAYA, the ACTIVE WILAYA identity for
//! UNIT), mirroring the bootstrap CSR contract.

use crate::application::services::NodeIdentityResolver;
use crate::application::sync_integrity::credential_guard::{CredentialGuard, CredentialVerdict};
use crate::domain::identity::{
    CredentialStatus, IdentityCertificate, IdentitySignatureVerifier,
    IDENTITY_ALGORITHM_PROFILE_ED25519,
};
use crate::errors::{AppError, AppResult, BusinessLogicError};
use crate::infrastructure::security::Ed25519SignatureVerifier;
use uuid::Uuid;

/// Rotation operation kind (RFC §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationOperation {
    /// Same `credential_id`, `generation + 1`, new public key.
    Rotate,
    /// New `credential_id`, `generation = 1`, new public key.
    ReIssue,
}

impl RotationOperation {
    pub const fn as_str(&self) -> &'static str {
        match self {
            RotationOperation::Rotate => "ROTATE",
            RotationOperation::ReIssue => "RE-ISSUE",
        }
    }
}

impl std::fmt::Display for RotationOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A planned rotation: the unsigned draft certificate (the CSR) plus the
/// transition metadata relative to the stored credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotationPlan {
    pub operation: RotationOperation,
    pub previous_credential_id: Uuid,
    pub certificate: IdentityCertificate,
}

/// Outcome of `verify_finalize`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalizeVerdict {
    /// New authoritative credential state — safe to install.
    Accept { certificate: IdentityCertificate },
    /// Identical re-presentation of the stored ACTIVE certificate — ZERO writes.
    Replay { certificate: IdentityCertificate },
}

/// Pure rotation planner / verifier — no persistence, no identity state mutation.
pub struct IdentityRotationService;

impl IdentityRotationService {
    /// Build the unsigned draft certificate (CSR) for a Rotate or Re-Issue.
    ///
    /// The draft preserves the immutable identity facts (`identity_id`,
    /// `subject_type`, `subject_id` — Invariant 1), derives the new public key
    /// from `node_key` (single derivation path, R5), and sets the credential
    /// lifecycle per the operation:
    /// - `Rotate`   → same `credential_id`, `generation + 1`;
    /// - `Re-Issue` → new `credential_id`, `generation = 1`.
    ///
    /// `algorithm_version` is pinned to the current Ed25519 profile (ADR-0039
    /// §4). The draft is UNSIGNED (`signature == None`) and has NO
    /// `issuer_identity_id`: both are bound by the issuer at signing time.
    pub fn plan(
        operation: RotationOperation,
        stored: &IdentityCertificate,
        node_key: &[u8; 32],
    ) -> AppResult<RotationPlan> {
        stored.require_signed()?;
        if stored.status != CredentialStatus::Active {
            return Err(Self::permitted(format!(
                "Rotation requires an ACTIVE stored identity (status {})",
                stored.status
            )));
        }
        if stored.generation < 1 {
            return Err(Self::permitted(format!(
                "Stored identity generation {} is below 1",
                stored.generation
            )));
        }

        let (credential_id, generation) = match operation {
            RotationOperation::Rotate => {
                let generation = stored.generation.checked_add(1).ok_or_else(|| {
                    AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                        message: format!(
                            "Rotation generation overflow for identity {}",
                            stored.identity_id
                        ),
                    })
                })?;
                (stored.credential_id, generation)
            }
            RotationOperation::ReIssue => (Uuid::new_v4(), 1),
        };

        let certificate = IdentityCertificate {
            identity_id: stored.identity_id,
            subject_type: stored.subject_type,
            subject_id: stored.subject_id,
            issuer_identity_id: None,
            credential_id,
            generation,
            status: CredentialStatus::Active,
            public_key: NodeIdentityResolver::derive_public_key(node_key),
            algorithm_version: IDENTITY_ALGORITHM_PROFILE_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        };

        Ok(RotationPlan {
            operation,
            previous_credential_id: stored.credential_id,
            certificate,
        })
    }

    /// Verify a signed rotation certificate before installation.
    ///
    /// All checks MUST hold, fail-closed:
    /// - signed (`require_signed`);
    /// - identity stability (Invariant 1): `identity_id` never changes;
    /// - subject binding: `subject_type` + `subject_id` unchanged;
    /// - ACTIVE status;
    /// - Root-pin/issuer: signature verifies against `issuer_public_key`
    ///   (the caller resolves the key: the Authority Root for WILAYA, the
    ///   ACTIVE WILAYA issuer for UNIT);
    /// - R5: the staged node key derives the certificate public key;
    /// - Re-Issue definition: a changed `credential_id` MUST restart at
    ///   generation 1;
    /// - Credential Guard (RFC §3.4.2): strictly increasing generation per
    ///   credential. `Accept` proceeds; `Replay` of the stored certificate is
    ///   a ZERO-write idempotent no-op; any conflicting same-generation state,
    ///   `Rollback`, or `RejectZero` fails closed.
    pub fn verify_finalize(
        signed_cert: &IdentityCertificate,
        stored: &IdentityCertificate,
        node_key: &[u8; 32],
        issuer_public_key: &[u8],
    ) -> AppResult<FinalizeVerdict> {
        signed_cert.require_signed()?;

        if signed_cert.identity_id != stored.identity_id {
            return Err(Self::permitted(format!(
                "Rotation certificate identity_id {} does not match the stored identity {} (Invariant 1)",
                signed_cert.identity_id, stored.identity_id
            )));
        }
        if signed_cert.subject_type != stored.subject_type
            || signed_cert.subject_id != stored.subject_id
        {
            return Err(Self::permitted(format!(
                "Rotation certificate subject does not match the stored identity {}",
                stored.identity_id
            )));
        }
        if signed_cert.status != CredentialStatus::Active {
            return Err(Self::permitted(format!(
                "Rotation requires an ACTIVE certificate (status {})",
                signed_cert.status
            )));
        }

        let signature = signed_cert.signature.as_ref().ok_or_else(|| {
            AppError::Internal("Rotation certificate missing signature".into())
        })?;
        let signature_valid = Ed25519SignatureVerifier
            .verify_certificate(signed_cert, issuer_public_key, signature)
            .map_err(|e| {
                AppError::Internal(format!("Issuer signature verification failed: {e}"))
            })?;
        if !signature_valid {
            return Err(Self::permitted(
                "Rotation certificate signature is not valid for the pinned issuer".into(),
            ));
        }

        if NodeIdentityResolver::derive_public_key(node_key) != signed_cert.public_key {
            return Err(Self::permitted(
                "R5: the staged node key does not match the rotation certificate public key".into(),
            ));
        }

        if signed_cert.credential_id != stored.credential_id && signed_cert.generation != 1 {
            return Err(Self::permitted(
                "A changed credential_id (Re-Issue) MUST restart at generation 1".into(),
            ));
        }

        let stored_generation = if signed_cert.credential_id == stored.credential_id {
            Some(stored.generation)
        } else {
            None
        };
        match CredentialGuard::check(
            &signed_cert.credential_id.to_string(),
            signed_cert.generation,
            stored_generation,
        ) {
            CredentialVerdict::Accept { .. } => {}
            CredentialVerdict::Replay { .. } => {
                if signed_cert.is_identical_to(stored) {
                    return Ok(FinalizeVerdict::Replay {
                        certificate: stored.clone(),
                    });
                }
                return Err(Self::permitted(
                    "Credential Guard: duplicate generation with a conflicting certificate".into(),
                ));
            }
            CredentialVerdict::Rollback { .. } => {
                return Err(Self::permitted(
                    "Credential Guard: certificate would roll back the stored generation".into(),
                ));
            }
            CredentialVerdict::RejectZero { .. } => {
                return Err(Self::permitted(
                    "Credential Guard: generation 0 is impossible; rejecting fail-closed".into(),
                ));
            }
        }

        Ok(FinalizeVerdict::Accept {
            certificate: signed_cert.clone(),
        })
    }

    fn permitted(message: String) -> AppError {
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { message })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::{Ed25519CertificateSignature, IdentitySigner, SubjectType};
    use crate::infrastructure::security::Ed25519SigningProvider;

    fn keypair(seed: u8) -> ([u8; 32], Ed25519SigningProvider) {
        let secret = [seed; 32];
        (secret, Ed25519SigningProvider::new(secret))
    }

    fn signed_stored(seed: u8, subject_type: SubjectType, generation: u64) -> IdentityCertificate {
        let (_, signer) = keypair(seed);
        let mut cert = IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_type,
            subject_id: Uuid::new_v4(),
            issuer_identity_id: match subject_type {
                SubjectType::Wilaya => None,
                SubjectType::Unit => Some(Uuid::new_v4()),
                SubjectType::Admin => Some(Uuid::new_v4()),
            },
            credential_id: Uuid::new_v4(),
            generation,
            status: CredentialStatus::Active,
            public_key: signer.public_key(),
            algorithm_version: IDENTITY_ALGORITHM_PROFILE_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        };
        let signature = signer.sign_certificate(&cert).unwrap();
        cert.signature = Some(Ed25519CertificateSignature::try_from(signature).unwrap());
        cert
    }

    /// Sign `cert` with the given signer and return the signed certificate
    /// together with the signer's public key (the issuer verification key).
    fn issued_by(
        cert: &mut IdentityCertificate,
        signer: &dyn IdentitySigner,
    ) -> IdentityCertificate {
        let signature = signer.sign_certificate(cert).unwrap();
        cert.signature = Some(Ed25519CertificateSignature::try_from(signature).unwrap());
        cert.clone()
    }

    #[test]
    fn plan_rotate_keeps_identity_and_advances_generation() {
        let stored = signed_stored(1, SubjectType::Wilaya, 1);
        let (node_key, _) = keypair(2);
        let plan = IdentityRotationService::plan(RotationOperation::Rotate, &stored, &node_key)
            .unwrap();
        assert_eq!(plan.operation, RotationOperation::Rotate);
        assert_eq!(plan.previous_credential_id, stored.credential_id);
        assert_eq!(plan.certificate.identity_id, stored.identity_id);
        assert_eq!(plan.certificate.subject_type, stored.subject_type);
        assert_eq!(plan.certificate.subject_id, stored.subject_id);
        assert_eq!(plan.certificate.credential_id, stored.credential_id);
        assert_eq!(plan.certificate.generation, 2);
        assert_eq!(plan.certificate.public_key, keypair(2).1.public_key());
        assert_eq!(plan.certificate.algorithm_version, IDENTITY_ALGORITHM_PROFILE_ED25519);
        assert_eq!(plan.certificate.status, CredentialStatus::Active);
        assert_eq!(plan.certificate.issuer_identity_id, None);
        assert_eq!(plan.certificate.signature, None);
    }

    #[test]
    fn plan_reissue_generates_fresh_credential_at_generation_one() {
        let stored = signed_stored(1, SubjectType::Unit, 4);
        let (node_key, _) = keypair(2);
        let plan = IdentityRotationService::plan(RotationOperation::ReIssue, &stored, &node_key)
            .unwrap();
        assert_eq!(plan.operation, RotationOperation::ReIssue);
        assert_eq!(plan.previous_credential_id, stored.credential_id);
        assert_eq!(plan.certificate.identity_id, stored.identity_id);
        assert_ne!(plan.certificate.credential_id, stored.credential_id);
        assert_eq!(plan.certificate.generation, 1);
    }

    #[test]
    fn plan_rejects_unsigned_stored_identity() {
        let mut stored = signed_stored(1, SubjectType::Wilaya, 1);
        stored.signature = None;
        let (node_key, _) = keypair(2);
        let err =
            IdentityRotationService::plan(RotationOperation::Rotate, &stored, &node_key).unwrap_err();
        assert!(err.to_string().contains("unsigned"), "got {err:?}");
    }

    #[test]
    fn plan_rejects_inactive_stored_identity() {
        let mut stored = signed_stored(1, SubjectType::Wilaya, 1);
        stored.status = CredentialStatus::Revoked;
        let (node_key, _) = keypair(2);
        let err =
            IdentityRotationService::plan(RotationOperation::Rotate, &stored, &node_key).unwrap_err();
        assert!(err.to_string().contains("ACTIVE"), "got {err:?}");
    }

    #[test]
    fn plan_rejects_generation_overflow() {
        let mut stored = signed_stored(1, SubjectType::Wilaya, u64::MAX);
        stored.generation = u64::MAX;
        let (node_key, _) = keypair(2);
        let err =
            IdentityRotationService::plan(RotationOperation::Rotate, &stored, &node_key).unwrap_err();
        assert!(err.to_string().contains("overflow"), "got {err:?}");
    }

    #[test]
    fn verify_finalize_accepts_rotate() {
        let stored = signed_stored(1, SubjectType::Wilaya, 1);
        let issuer_signer = keypair(9).1;
        let (node_key, node_signer) = keypair(2);
        let mut draft = IdentityRotationService::plan(RotationOperation::Rotate, &stored, &node_key)
            .unwrap()
            .certificate;
        let signed = issued_by(&mut draft, &issuer_signer);
        let verdict = IdentityRotationService::verify_finalize(
            &signed,
            &stored,
            &node_key,
            &issuer_signer.public_key(),
        )
        .unwrap();
        match verdict {
            FinalizeVerdict::Accept { certificate } => {
                assert_eq!(certificate.identity_id, stored.identity_id);
                assert_eq!(certificate.credential_id, stored.credential_id);
                assert_eq!(certificate.generation, 2);
                assert_eq!(certificate.public_key, node_signer.public_key());
            }
            FinalizeVerdict::Replay { .. } => panic!("expected Accept"),
        }
    }

    #[test]
    fn verify_finalize_accepts_reissue() {
        let stored = signed_stored(1, SubjectType::Unit, 3);
        let issuer_signer = keypair(9).1;
        let (node_key, node_signer) = keypair(2);
        let mut draft = IdentityRotationService::plan(RotationOperation::ReIssue, &stored, &node_key)
            .unwrap()
            .certificate;
        let signed = issued_by(&mut draft, &issuer_signer);
        let verdict = IdentityRotationService::verify_finalize(
            &signed,
            &stored,
            &node_key,
            &issuer_signer.public_key(),
        )
        .unwrap();
        match verdict {
            FinalizeVerdict::Accept { certificate } => {
                assert_eq!(certificate.identity_id, stored.identity_id);
                assert_ne!(certificate.credential_id, stored.credential_id);
                assert_eq!(certificate.generation, 1);
                assert_eq!(certificate.public_key, node_signer.public_key());
            }
            FinalizeVerdict::Replay { .. } => panic!("expected Accept"),
        }
    }

    #[test]
    fn verify_finalize_replay_is_zero_writes_idempotent() {
        let stored = signed_stored(1, SubjectType::Wilaya, 1);
        let stored_issuer = keypair(1).1;
        let (node_key, _) = keypair(1);
        let verdict = IdentityRotationService::verify_finalize(
            &stored,
            &stored,
            &node_key,
            &stored_issuer.public_key(),
        )
        .unwrap();
        match verdict {
            FinalizeVerdict::Replay { certificate } => {
                assert!(certificate.is_identical_to(&stored));
            }
            FinalizeVerdict::Accept { .. } => panic!("expected Replay"),
        }
    }

    #[test]
    fn verify_finalize_rejects_rollback() {
        let stored = signed_stored(1, SubjectType::Wilaya, 5);
        let issuer_signer = keypair(9).1;
        let (node_key, node_signer) = keypair(2);
        let mut draft = signed_stored(1, SubjectType::Wilaya, 4);
        draft.identity_id = stored.identity_id;
        draft.subject_id = stored.subject_id;
        draft.subject_type = stored.subject_type;
        draft.credential_id = stored.credential_id;
        draft.public_key = node_signer.public_key();
        draft.signature = None;
        let signed = issued_by(&mut draft, &issuer_signer);
        let err = IdentityRotationService::verify_finalize(
            &signed,
            &stored,
            &node_key,
            &issuer_signer.public_key(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("roll back"), "got {err:?}");
    }

    #[test]
    fn verify_finalize_rejects_identity_id_stability_violation() {
        let stored = signed_stored(1, SubjectType::Wilaya, 1);
        let issuer_signer = keypair(9).1;
        let (node_key, _) = keypair(2);
        let mut draft = IdentityRotationService::plan(RotationOperation::Rotate, &stored, &node_key)
            .unwrap()
            .certificate;
        draft.identity_id = Uuid::new_v4();
        let signed = issued_by(&mut draft, &issuer_signer);
        let err = IdentityRotationService::verify_finalize(
            &signed,
            &stored,
            &node_key,
            &issuer_signer.public_key(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("Invariant 1"), "got {err:?}");
    }

    #[test]
    fn verify_finalize_rejects_subject_drift() {
        let stored = signed_stored(1, SubjectType::Unit, 1);
        let issuer_signer = keypair(9).1;
        let (node_key, _) = keypair(2);
        let mut draft = IdentityRotationService::plan(RotationOperation::Rotate, &stored, &node_key)
            .unwrap()
            .certificate;
        draft.subject_id = Uuid::new_v4();
        let signed = issued_by(&mut draft, &issuer_signer);
        let err = IdentityRotationService::verify_finalize(
            &signed,
            &stored,
            &node_key,
            &issuer_signer.public_key(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("subject"), "got {err:?}");
    }

    #[test]
    fn verify_finalize_rejects_invalid_issuer_signature() {
        let stored = signed_stored(1, SubjectType::Wilaya, 1);
        let issuer_signer = keypair(9).1;
        let wrong_issuer = keypair(10).1;
        let (node_key, _) = keypair(2);
        let mut draft = IdentityRotationService::plan(RotationOperation::Rotate, &stored, &node_key)
            .unwrap()
            .certificate;
        let signed = issued_by(&mut draft, &issuer_signer);
        let err = IdentityRotationService::verify_finalize(
            &signed,
            &stored,
            &node_key,
            &wrong_issuer.public_key(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("signature"), "got {err:?}");
    }

    #[test]
    fn verify_finalize_rejects_r5_key_mismatch() {
        let stored = signed_stored(1, SubjectType::Wilaya, 1);
        let issuer_signer = keypair(9).1;
        let (node_key, _) = keypair(2);
        let mut draft = IdentityRotationService::plan(RotationOperation::Rotate, &stored, &node_key)
            .unwrap()
            .certificate;
        let (_, other_signer) = keypair(3);
        draft.public_key = other_signer.public_key();
        draft.signature = None;
        let signed = issued_by(&mut draft, &issuer_signer);
        let err = IdentityRotationService::verify_finalize(
            &signed,
            &stored,
            &node_key,
            &issuer_signer.public_key(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("R5"), "got {err:?}");
    }

    #[test]
    fn verify_finalize_rejects_changed_credential_above_generation_one() {
        let stored = signed_stored(1, SubjectType::Unit, 1);
        let issuer_signer = keypair(9).1;
        let (node_key, node_signer) = keypair(2);
        let mut draft = IdentityRotationService::plan(RotationOperation::ReIssue, &stored, &node_key)
            .unwrap()
            .certificate;
        draft.generation = 5;
        draft.public_key = node_signer.public_key();
        draft.signature = None;
        let signed = issued_by(&mut draft, &issuer_signer);
        let err = IdentityRotationService::verify_finalize(
            &signed,
            &stored,
            &node_key,
            &issuer_signer.public_key(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("generation 1"), "got {err:?}");
    }

    #[test]
    fn verify_finalize_rejects_inactive_certificate() {
        let stored = signed_stored(1, SubjectType::Wilaya, 1);
        let issuer_signer = keypair(9).1;
        let (node_key, _) = keypair(2);
        let mut draft = IdentityRotationService::plan(RotationOperation::Rotate, &stored, &node_key)
            .unwrap()
            .certificate;
        draft.status = CredentialStatus::Superseded;
        let signed = issued_by(&mut draft, &issuer_signer);
        let err = IdentityRotationService::verify_finalize(
            &signed,
            &stored,
            &node_key,
            &issuer_signer.public_key(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("ACTIVE"), "got {err:?}");
    }

    #[test]
    fn verify_finalize_rejects_zero_generation_fail_closed() {
        let stored = signed_stored(1, SubjectType::Wilaya, 1);
        let issuer_signer = keypair(9).1;
        let (node_key, _) = keypair(2);
        let mut draft = IdentityRotationService::plan(RotationOperation::Rotate, &stored, &node_key)
            .unwrap()
            .certificate;
        draft.generation = 0;
        let signed = issued_by(&mut draft, &issuer_signer);
        let err = IdentityRotationService::verify_finalize(
            &signed,
            &stored,
            &node_key,
            &issuer_signer.public_key(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("fail-closed"), "got {err:?}");
    }

    #[test]
    fn operation_roundtrip() {
        for op in [RotationOperation::Rotate, RotationOperation::ReIssue] {
            assert_eq!(op.to_string(), op.as_str());
            assert_eq!(format!("{op}"), op.as_str());
        }
    }
}
