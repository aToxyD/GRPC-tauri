//! Local trust anchor management — WILAYA certificate install/update.
//!
//! RFC 2026-08-04-node-identity-trust §3.12 / ADR-0038.
//!
//! The WILAYA certificate is the UNIT node's LOCAL TRUST ANCHOR. It arrives at
//! the UNIT through a dedicated bootstrap step (strict two-step flow, §3.12):
//! the certificate is NEVER bundled in the UNIT provisioning file. This service
//! is node-type agnostic — its ONLY responsibility is installing/updating the
//! ACTIVE WILAYA trust anchor. It reads no `NodeKeyStore`, checks no UNIT state,
//! and does not mutate `IdentityBootstrapState`. The same contract is reused
//! for WILAYA certificate rotation (B7) via the Trust Package channel.

use crate::db::Database;
use crate::domain::identity::{
    CredentialStatus, IdentityCertificate, IdentitySignatureVerifier, IdentityStorePort,
    SubjectType,
};
use crate::errors::{AppError, AppResult, BusinessLogicError};
use crate::infrastructure::identity::resolve_root_public_key;
use crate::infrastructure::security::Ed25519SignatureVerifier;
use crate::repositories::RepositoryProvider;

/// Outcome of `install_wilaya_certificate` (B5 idempotency style).
///
/// `AlreadyInstalled` is returned ONLY for the literal same certificate
/// (`is_identical_to`), with ZERO writes. Any real difference — including the
/// same `identity_id` with a different `credential_id` — fails closed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum InstallWilayaCertificateResult {
    Installed(IdentityCertificate),
    AlreadyInstalled(IdentityCertificate),
}

/// Installs and updates the ACTIVE WILAYA certificate (local trust anchor).
pub struct IdentityTrustAnchorService<'a> {
    db: &'a mut Database,
}

impl<'a> IdentityTrustAnchorService<'a> {
    pub fn new(db: &'a mut Database) -> Self {
        Self { db }
    }

    /// Install (or idempotently re-confirm) the ACTIVE WILAYA trust anchor.
    ///
    /// Fail-closed verification (ALL MUST hold):
    /// - the certificate is signed (ADR-0039 §6);
    /// - subject is WILAYA;
    /// - status is ACTIVE;
    /// - the issuer is the offline Authority Root (`None`) — a WILAYA
    ///   certificate is never node-issued;
    /// - the signature verifies against `resolve_root_public_key()`.
    ///
    /// Idempotency: an identical ACTIVE WILAYA certificate returns
    /// `AlreadyInstalled` with ZERO writes. Any DIFFERENT certificate fails
    /// closed — idempotency means "the literal same certificate", not "the same
    /// identity".
    pub fn install_wilaya_certificate(
        &mut self,
        signed_cert: &IdentityCertificate,
        now: &str,
    ) -> AppResult<InstallWilayaCertificateResult> {
        signed_cert.require_signed()?;
        if signed_cert.subject_type != SubjectType::Wilaya {
            return Err(Self::permitted(
                "install_wilaya_certificate requires a WILAYA certificate".into(),
            ));
        }
        if signed_cert.status != CredentialStatus::Active {
            return Err(Self::permitted(
                "install_wilaya_certificate requires an ACTIVE certificate".into(),
            ));
        }
        if signed_cert.issuer_identity_id.is_some() {
            return Err(Self::permitted(
                "WILAYA certificates MUST be issued by the offline Authority Root (issuer None)"
                    .into(),
            ));
        }

        let root_public_key = resolve_root_public_key()?;
        let signature = signed_cert.signature.as_ref().ok_or_else(|| {
            AppError::Internal("Root-signed certificate missing signature".into())
        })?;
        let verifier = Ed25519SignatureVerifier;
        let root_signature_valid = verifier
            .verify_certificate(signed_cert, &root_public_key, signature)
            .map_err(|e| AppError::Internal(format!("Root signature verification failed: {e}")))?;
        if !root_signature_valid {
            return Err(Self::permitted(
                "Certificate signature is not valid for the Authority Root".into(),
            ));
        }

        let store = self.db.executor().identity_store();
        if let Some(existing) = store.get_active_by_subject_type(SubjectType::Wilaya)? {
            if existing.is_identical_to(signed_cert) {
                return Ok(InstallWilayaCertificateResult::AlreadyInstalled(existing));
            }
            return Err(Self::permitted(
                "An ACTIVE WILAYA trust anchor is already installed and differs from the presented certificate"
                    .into(),
            ));
        }

        store.upsert(signed_cert, now)?;
        Ok(InstallWilayaCertificateResult::Installed(signed_cert.clone()))
    }

    fn permitted(message: String) -> AppError {
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { message })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ConnectionFactory;
    use crate::domain::identity::{Ed25519CertificateSignature, IdentitySigner};
    use crate::infrastructure::security::Ed25519SigningProvider;
    use uuid::Uuid;

    const FIXED_NOW: &str = "2026-08-04T00:00:00Z";

    /// RFC 8032 §7.1 TEST 1 secret — matches the debug Root fallback.
    const TEST_ROOT_SECRET: [u8; 32] = [
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
        0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
        0x7f, 0x60,
    ];

    fn root_signer() -> Ed25519SigningProvider {
        Ed25519SigningProvider::new(TEST_ROOT_SECRET)
    }

    fn wilaya_cert(identity_id: Uuid, public_key: Vec<u8>) -> IdentityCertificate {
        IdentityCertificate {
            identity_id,
            subject_type: SubjectType::Wilaya,
            subject_id: identity_id,
            issuer_identity_id: None,
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key,
            algorithm_version: crate::domain::identity::SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        }
    }

    fn root_signed(cert: &IdentityCertificate) -> IdentityCertificate {
        let signature = root_signer()
            .sign_certificate(cert)
            .expect("root signed");
        let mut signed = cert.clone();
        signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("wrap"));
        signed
    }

    #[test]
    fn installs_active_wilaya_trust_anchor() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let cert = root_signed(&wilaya_cert(Uuid::new_v4(), root_signer().public_key()));
        let outcome = IdentityTrustAnchorService::new(&mut db)
            .install_wilaya_certificate(&cert, FIXED_NOW)
            .unwrap();
        assert!(matches!(outcome, InstallWilayaCertificateResult::Installed(_)));
    }

    #[test]
    fn identical_reinstall_is_idempotent_zero_writes() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let cert = root_signed(&wilaya_cert(Uuid::new_v4(), root_signer().public_key()));
        {
            let mut service = IdentityTrustAnchorService::new(&mut db);
            service.install_wilaya_certificate(&cert, FIXED_NOW).unwrap();
        }
        let before = db.executor().identity_store().list_all().unwrap().len();
        {
            let mut service = IdentityTrustAnchorService::new(&mut db);
            let outcome = service.install_wilaya_certificate(&cert, FIXED_NOW).unwrap();
            assert!(
                matches!(outcome, InstallWilayaCertificateResult::AlreadyInstalled(_)),
                "identical re-install must be a no-op"
            );
        }
        let after = db.executor().identity_store().list_all().unwrap().len();
        assert_eq!(before, after);
    }

    #[test]
    fn same_identity_different_credential_fails_closed() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let identity_id = Uuid::new_v4();
        let first = root_signed(&wilaya_cert(identity_id, root_signer().public_key()));
        let mut service = IdentityTrustAnchorService::new(&mut db);
        service.install_wilaya_certificate(&first, FIXED_NOW).unwrap();

        let mut rotated = wilaya_cert(identity_id, root_signer().public_key());
        rotated.credential_id = Uuid::new_v4();
        let rotated = root_signed(&rotated);
        let err = service.install_wilaya_certificate(&rotated, FIXED_NOW).unwrap_err();
        assert!(
            err.to_string().contains("already installed"),
            "different credential_id for same identity must fail closed, got {err:?}"
        );
    }

    #[test]
    fn rejects_unsigned_non_wilaya_non_active_and_node_issued() {
        let mut db = ConnectionFactory::new_for_test().unwrap();

        let unsigned = wilaya_cert(Uuid::new_v4(), root_signer().public_key());
        let err = IdentityTrustAnchorService::new(&mut db)
            .install_wilaya_certificate(&unsigned, FIXED_NOW)
            .unwrap_err();
        assert!(matches!(err, AppError::Internal(_)), "got {err:?}");

        let mut as_admin = root_signed(&wilaya_cert(Uuid::new_v4(), root_signer().public_key()));
        as_admin.subject_type = SubjectType::Admin;
        let err = IdentityTrustAnchorService::new(&mut db)
            .install_wilaya_certificate(&as_admin, FIXED_NOW)
            .unwrap_err();
        assert!(err.to_string().contains("WILAYA certificate"), "got {err:?}");

        let mut revoked = root_signed(&wilaya_cert(Uuid::new_v4(), root_signer().public_key()));
        revoked.status = CredentialStatus::Revoked;
        let err = IdentityTrustAnchorService::new(&mut db)
            .install_wilaya_certificate(&revoked, FIXED_NOW)
            .unwrap_err();
        assert!(err.to_string().contains("ACTIVE"), "got {err:?}");

        let mut node_issued = root_signed(&wilaya_cert(Uuid::new_v4(), root_signer().public_key()));
        node_issued.issuer_identity_id = Some(Uuid::new_v4());
        let err = IdentityTrustAnchorService::new(&mut db)
            .install_wilaya_certificate(&node_issued, FIXED_NOW)
            .unwrap_err();
        assert!(err.to_string().contains("offline Authority Root"), "got {err:?}");
    }

    #[test]
    fn rejects_imposter_root_signature() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let mut cert = wilaya_cert(Uuid::new_v4(), root_signer().public_key());
        let imposter = Ed25519SigningProvider::new([7u8; 32]);
        let signature = imposter.sign_certificate(&cert).unwrap();
        cert.signature = Some(Ed25519CertificateSignature::try_from(signature).unwrap());
        let err = IdentityTrustAnchorService::new(&mut db)
            .install_wilaya_certificate(&cert, FIXED_NOW)
            .unwrap_err();
        assert!(
            err.to_string().contains("not valid for the Authority Root"),
            "got {err:?}"
        );
    }
}
