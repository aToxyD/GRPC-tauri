//! Apply decrypted Trust Package (certificates + revocations) to the Identity Store.
//!
//! RFC 2026-08-04 §3.9 (B4): kind = `trust`. The Trust Package is the ONLY
//! channel for trust distribution (no side channel, §3.4.4). Certificates are
//! gated by the Credential Guard `(credential_id, generation)` (§3.4.2); the
//! Transport Guard lives in `run_import_pipeline` (§3.4.1).
//!
//! SEC-010 (ADR): the envelope MUST be signed by the ACTIVE LOCAL WILAYA
//! identity; the payload MAY carry ONLY Root-issued WILAYA certificates
//! (Root-verified, anchor-immutable) and revocations scoped to UNIT identities
//! issued by the package signer. UNIT/ADMIN certificates and ADMIN/WILAYA
//! revocations are rejected. Validation completes before ANY mutation —
//! all-or-nothing (the pipeline transaction rolls back the whole package).

use serde::{Deserialize, Serialize};

use crate::application::sync::ImportedPackageRegistry;
use crate::application::sync::SyncPackage;
use crate::application::sync_integrity::credential_guard::{CredentialGuard, CredentialVerdict};
use crate::domain::identity::{
    CredentialStatus, IdentityCertificate, IdentitySignatureVerifier, IdentityStorePort,
    SubjectType,
};
use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
use crate::infrastructure::identity::resolve_root_public_key;
use crate::infrastructure::security::Ed25519SignatureVerifier;
use crate::repositories::{DbExecutor, IdentityStoreRepository};

pub const TRUST_PACKAGE_KIND: &str = "trust";

/// A certificate revocation directive targeting a persisted identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateRevocation {
    pub identity_id: uuid::Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Payload of a Trust Package: certificate distributions + revocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustPackagePayload {
    pub certificates: Vec<IdentityCertificate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revocations: Vec<CertificateRevocation>,
}

#[derive(Debug, Clone)]
pub struct ImportTrustPackageInput {
    pub package: SyncPackage<TrustPackagePayload>,
    pub imported_by: String,
}

#[derive(Debug, Clone)]
pub struct ImportTrustPackageOutcome {
    pub certificate_count: usize,
    pub revocation_count: usize,
    pub package_id: String,
}

/// Uniform fail-closed rejection for SEC-010 trust-package validation failures.
fn permitted(message: String) -> AppError {
    AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { message })
}

pub fn execute(
    executor: DbExecutor<'_>,
    registry: &impl ImportedPackageRegistry,
    input: ImportTrustPackageInput,
) -> AppResult<ImportTrustPackageOutcome> {
    if input.package.metadata.source_node_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "source_node_id".into(),
            message: "حزمة ثقة بلا مصدر".into(),
        }));
    }

    let package_id = input.package.metadata.package_id.clone();
    if registry.has_imported(&package_id)? {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::DuplicateSyncPackage {
                package_id: package_id.0.clone(),
            },
        ));
    }

    let identity_store = IdentityStoreRepository::new(executor);
    let now = chrono::Utc::now().to_rfc3339();

    // SEC-010 (envelope issuer pin): the package MUST be signed by the ACTIVE
    // LOCAL WILAYA identity — never "some ACTIVE WILAYA". A package signed by
    // another WILAYA, a stale identity, or a revoked/superseded identity is
    // rejected here (fail-closed; `get_active_by_subject_type` returns only
    // ACTIVE rows). Envelope signature verification itself remains the
    // pipeline's responsibility (`verify_v2_package_for_import`).
    let local_wilaya = identity_store
        .get_active_by_subject_type(SubjectType::Wilaya)?
        .ok_or_else(|| {
            permitted(
                "Trust package import requires an ACTIVE local WILAYA identity (SEC-010 issuer pin)"
                    .into(),
            )
        })?;
    let package_issuer = input.package.metadata.issuer_identity_id.ok_or_else(|| {
        AppError::Validation(ValidationError::InvalidFormat {
            field: "issuer_identity_id".into(),
            message: "حزمة ثقة بلا هوية مُصدِر".into(),
        })
    })?;
    if package_issuer != local_wilaya.identity_id {
        return Err(permitted(format!(
            "Trust package issuer {package_issuer} is not the ACTIVE local WILAYA identity {} — \
             a trust package MUST be signed by the ACTIVE local WILAYA identity (SEC-010)",
            local_wilaya.identity_id
        )));
    }

    // SEC-010: validate the ENTIRE payload before ANY mutation (all-or-nothing).
    // Certificate-level authenticity/authority is established here; the
    // CredentialGuard below remains a monotonicity/replay control only.
    let root_public_key = resolve_root_public_key()?;

    for certificate in &input.package.payload.certificates {
        // New certificates MUST be signed (ADR-0039 §6); legacy migration
        // records carry NULL signatures and are not distributable.
        certificate.require_signed()?;

        // SEC-010: trust packages carry ONLY Root-issued WILAYA certificates.
        if certificate.subject_type != SubjectType::Wilaya {
            return Err(permitted(format!(
                "Trust package certificate {} is not a WILAYA certificate — UNIT/ADMIN \
                 certificates MUST NOT be distributed through trust packages (SEC-010)",
                certificate.identity_id
            )));
        }
        if certificate.issuer_identity_id.is_some() {
            return Err(permitted(format!(
                "WILAYA certificate {} in a trust package MUST be issued by the offline \
                 Authority Root (issuer None) (SEC-010)",
                certificate.identity_id
            )));
        }
        if certificate.status != CredentialStatus::Active {
            return Err(permitted(format!(
                "Trust package certificate {} MUST be ACTIVE (status {}) (SEC-010)",
                certificate.identity_id, certificate.status
            )));
        }

        // SEC-010: `issuer_identity_id == None` is NOT sufficient — the Root
        // signature over the canonical certificate bytes is the authority
        // proof. A WILAYA certificate signed only by the WILAYA key fails here.
        let signature = certificate.signature.as_ref().ok_or_else(|| {
            AppError::Internal("Root-signed certificate missing signature".into())
        })?;
        let root_signature_valid = Ed25519SignatureVerifier
            .verify_certificate(certificate, &root_public_key, signature)
            .map_err(|e| AppError::Internal(format!("Root signature verification failed: {e}")))?;
        if !root_signature_valid {
            return Err(permitted(format!(
                "Trust package certificate {} signature is not valid for the Authority Root \
                 (SEC-010)",
                certificate.identity_id
            )));
        }

        // SEC-010 (anchor immutability): trust package import MUST NOT replace
        // the ACTIVE WILAYA anchor. Rotation/provisioning is the ceremony that
        // changes the local anchor; an identical re-presentation is an
        // idempotent no-op, anything else fails closed.
        if !certificate.is_identical_to(&local_wilaya) {
            return Err(permitted(format!(
                "Trust package certificate {} would replace the ACTIVE WILAYA anchor — trust \
                 package import MUST NOT mutate the WILAYA anchor (SEC-010)",
                certificate.identity_id
            )));
        }
    }

    // SEC-010 (revocation authority): a WILAYA may revoke ONLY UNIT identities
    // that it issued. ADMIN/WILAYA/other-issuer/nonexistent targets are rejected.
    for revocation in &input.package.payload.revocations {
        let existing = identity_store
            .get_by_identity_id(&revocation.identity_id)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
                    resource: "identity".into(),
                    id: revocation.identity_id.to_string(),
                })
            })?;
        if existing.subject_type != SubjectType::Unit {
            return Err(permitted(format!(
                "Trust package revocation targets {} ({}) — only UNIT identities may be \
                 revoked through trust packages (SEC-010)",
                existing.identity_id, existing.subject_type
            )));
        }
        if existing.issuer_identity_id != Some(package_issuer) {
            return Err(permitted(format!(
                "Trust package revocation targets UNIT {} issued by {} — a WILAYA may revoke \
                 only UNIT identities it issued (SEC-010)",
                existing.identity_id,
                existing
                    .issuer_identity_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "unknown".into())
            )));
        }
    }

    // SEC-010: apply phase — authenticity/authority already proven for every
    // element; the CredentialGuard remains a monotonicity/replay control,
    // never a substitute for cryptographic authenticity.
    for certificate in &input.package.payload.certificates {
        let stored = identity_store.max_generation_for_credential(&certificate.credential_id)?;
        match CredentialGuard::check(
            &certificate.credential_id.to_string(),
            certificate.generation,
            stored,
        ) {
            CredentialVerdict::Accept { .. } => {
                identity_store.upsert(certificate, &now)?;
            }
            CredentialVerdict::Replay { .. } => {
                // Already-known state: idempotent no-op.
            }
            CredentialVerdict::Rollback {
                credential_id,
                stored,
                incoming,
            } => {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::OperationNotPermitted {
                        message: format!(
                            "Credential rollback rejected: {credential_id} stored generation {stored} >= incoming {incoming}"
                        ),
                    },
                ));
            }
            CredentialVerdict::RejectZero { credential_id } => {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::OperationNotPermitted {
                        message: format!("Impossible credential generation 0: {credential_id}"),
                    },
                ));
            }
        }
    }

    let mut revocation_count = 0usize;
    for revocation in &input.package.payload.revocations {
        let mut revoked = identity_store
            .get_by_identity_id(&revocation.identity_id)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
                    resource: "identity".into(),
                    id: revocation.identity_id.to_string(),
                })
            })?;
        revoked.status = CredentialStatus::Revoked;
        identity_store.upsert(&revoked, &now)?;
        revocation_count += 1;
    }

    let certificate_count = input.package.payload.certificates.len();
    registry.mark_imported(&package_id)?;

    Ok(ImportTrustPackageOutcome {
        certificate_count,
        revocation_count,
        package_id: package_id.0.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::services::IdentityTrustAnchorService;
    use crate::application::sync::{
        PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
    };
    use crate::db::{ConnectionFactory, Database};
    use crate::domain::identity::{
        test_signature, Ed25519CertificateSignature, IdentitySigner, SubjectType,
        SIGNATURE_VERSION_ED25519,
    };
    use crate::errors::{AppError, BusinessLogicError};
    use crate::infrastructure::db::sync_import::SqliteImportedPackageRegistry;
    use crate::infrastructure::security::Ed25519SigningProvider;
    use crate::repositories::{DbExecutor, RepositoryProvider};
    use uuid::Uuid;

    const FIXED_NOW: &str = "2026-08-04T00:00:00Z";

    /// RFC 8032 §7.1 TEST 1 secret — matches the debug Root fallback
    /// (`DEV_ROOT_PUBLIC_KEY` in `infrastructure/identity/root_public_key.rs`).
    const TEST_ROOT_SECRET: [u8; 32] = [
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
        0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
        0x7f, 0x60,
    ];

    fn root_signer() -> Ed25519SigningProvider {
        Ed25519SigningProvider::new(TEST_ROOT_SECRET)
    }

    fn root_signed(cert: &IdentityCertificate) -> IdentityCertificate {
        let signature = root_signer().sign_certificate(cert).expect("root signed");
        let mut signed = cert.clone();
        signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("wrap"));
        signed
    }

    fn wilaya_cert(
        identity_id: Uuid,
        credential_id: Uuid,
        generation: u64,
        public_key: Vec<u8>,
    ) -> IdentityCertificate {
        IdentityCertificate {
            identity_id,
            subject_type: SubjectType::Wilaya,
            subject_id: identity_id,
            issuer_identity_id: None,
            credential_id,
            generation,
            status: CredentialStatus::Active,
            public_key,
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        }
    }

    /// A WILAYA-issued certificate (UNIT or ADMIN) as produced by the local
    /// identity issuance ceremony (R5-bound in production; raw in tests).
    fn issued_cert(
        subject_type: SubjectType,
        identity_id: Uuid,
        subject_id: Uuid,
        issuer: Uuid,
    ) -> IdentityCertificate {
        IdentityCertificate {
            identity_id,
            subject_type,
            subject_id,
            issuer_identity_id: Some(issuer),
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: vec![7u8; 32],
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: Some(1),
            signature: Some(test_signature([3u8; 64])),
        }
    }

    /// Seed the local ACTIVE WILAYA anchor (Root-signed, install ceremony) and
    /// return its `identity_id` (the identity that must sign trust packages).
    fn seed_local_wilaya(db: &Database) -> Uuid {
        let identity_id = Uuid::new_v4();
        let cert = root_signed(&wilaya_cert(
            identity_id,
            Uuid::new_v4(),
            1,
            root_signer().public_key(),
        ));
        IdentityTrustAnchorService::new(db.executor())
            .install_wilaya_certificate(&cert, FIXED_NOW)
            .unwrap();
        identity_id
    }

    fn anchor_cert(db: &Database) -> IdentityCertificate {
        db.executor()
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)
            .unwrap()
            .expect("local WILAYA anchor present")
    }

    fn make_executor(db: &Database) -> DbExecutor<'_> {
        db.executor()
    }

    fn trust_package(
        issuer: Uuid,
        pkg_id: &str,
        certificates: Vec<IdentityCertificate>,
        revocations: Vec<CertificateRevocation>,
    ) -> SyncPackage<TrustPackagePayload> {
        SyncPackage {
            metadata: SyncPackageMetadata {
                schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
                created_at: chrono::Utc::now(),
                source_node_id: "w-node".to_string(),
                package_sequence: Some(1),
                issuer_identity_id: Some(issuer),
                package_id: PackageId(pkg_id.to_string()),
                signature_version: Some(2),
                signing_key_id: None,
                integrity_hash: None,
                signature: None,
            },
            payload: TrustPackagePayload {
                certificates,
                revocations,
            },
        }
    }

    fn registry<'a>(db: &'a Database) -> SqliteImportedPackageRegistry<'a> {
        SqliteImportedPackageRegistry::new(
            make_executor(db),
            TRUST_PACKAGE_KIND,
            Some("w-node"),
            "admin",
            Some(1),
            Some("issuer-a"),
        )
    }

    #[test]
    fn accepts_identical_root_signed_anchor_idempotently() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let before = db.executor().identity_store().list_all().unwrap().len();

        let package = trust_package(wilaya_id, "pkg-trust-identical", vec![anchor_cert(&db)], vec![]);
        let outcome = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap();

        assert_eq!(outcome.certificate_count, 1);
        assert_eq!(outcome.revocation_count, 0);
        let after = db.executor().identity_store().list_all().unwrap().len();
        assert_eq!(before, after, "identical anchor re-presentation is a ZERO-write no-op");
    }

    #[test]
    fn unit_certificate_in_trust_package_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let before = db.executor().identity_store().list_all().unwrap().len();

        let unit = issued_cert(
            SubjectType::Unit,
            Uuid::new_v4(),
            Uuid::new_v4(),
            wilaya_id,
        );
        let package = trust_package(wilaya_id, "pkg-trust-unit-cert", vec![unit], vec![]);
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected UNIT-certificate rejection, got {err:?}"
        );
        assert_eq!(
            db.executor().identity_store().list_all().unwrap().len(),
            before,
            "UNIT certificate must not be persisted"
        );
    }

    #[test]
    fn admin_certificate_in_trust_package_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let before = db.executor().identity_store().list_all().unwrap().len();

        let admin = issued_cert(
            SubjectType::Admin,
            Uuid::new_v4(),
            Uuid::new_v4(),
            wilaya_id,
        );
        let package = trust_package(wilaya_id, "pkg-trust-admin-cert", vec![admin], vec![]);
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected ADMIN-certificate rejection, got {err:?}"
        );
        assert_eq!(
            db.executor().identity_store().list_all().unwrap().len(),
            before,
            "ADMIN certificate must not be persisted"
        );
    }

    #[test]
    fn forged_wilaya_certificate_signed_by_stolen_wilaya_key_is_rejected() {
        // SEC-009-01 regression: a compromised WILAYA signing key forging a
        // WILAYA certificate (issuer None, attacker-controlled public key,
        // higher generation) inside a WILAYA-signed trust package MUST be
        // rejected and the original anchor MUST remain unchanged.
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let anchor = anchor_cert(&db);

        let attacker_key = Ed25519SigningProvider::new([9u8; 32]);
        let mut forged = wilaya_cert(
            anchor.identity_id,
            anchor.credential_id,
            anchor.generation + 1,
            attacker_key.public_key(),
        );
        let forged = {
            let sig = attacker_key.sign_certificate(&forged).unwrap();
            forged.signature = Some(Ed25519CertificateSignature::try_from(sig).unwrap());
            forged
        };

        let package = trust_package(wilaya_id, "pkg-trust-forged-wilaya", vec![forged], vec![]);
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected forged-WILAYA rejection, got {err:?}"
        );

        let stored = db
            .executor()
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)
            .unwrap()
            .expect("anchor still present");
        assert!(
            stored.is_identical_to(&anchor),
            "the WILAYA anchor must remain unchanged after a forged import attempt"
        );
    }

    #[test]
    fn forged_wilaya_certificate_with_imposter_signature_is_rejected() {
        // A non-Root signer (imposter key) must fail Root verification.
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let anchor = anchor_cert(&db);

        let mut forged = wilaya_cert(
            anchor.identity_id,
            Uuid::new_v4(),
            anchor.generation + 1,
            vec![8u8; 32],
        );
        let imposter = Ed25519SigningProvider::new([7u8; 32]);
        let sig = imposter.sign_certificate(&forged).unwrap();
        forged.signature = Some(Ed25519CertificateSignature::try_from(sig).unwrap());

        let package = trust_package(wilaya_id, "pkg-trust-imposter", vec![forged], vec![]);
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected imposter-signature rejection, got {err:?}"
        );
        let stored = db
            .executor()
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)
            .unwrap()
            .expect("anchor still present");
        assert!(stored.is_identical_to(&anchor));
    }

    #[test]
    fn missing_certificate_signature_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let mut cert = anchor_cert(&db);
        cert.signature = None;

        let package = trust_package(wilaya_id, "pkg-trust-unsigned", vec![cert], vec![]);
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(err, AppError::Internal(_)),
            "expected unsigned-certificate rejection, got {err:?}"
        );
    }

    #[test]
    fn garbage_certificate_signature_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let anchor = anchor_cert(&db);

        let mut forged = anchor.clone();
        forged.generation += 1;
        forged.signature = Some(test_signature([1u8; 64]));

        let package = trust_package(wilaya_id, "pkg-trust-garbage-sig", vec![forged], vec![]);
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected garbage-signature rejection, got {err:?}"
        );
    }

    #[test]
    fn root_signed_certificate_with_tampered_fields_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);

        // A genuinely Root-signed certificate whose subject_id was tampered
        // AFTER signing must fail Root verification (canonical bytes cover it).
        let mut tampered = root_signed(&wilaya_cert(
            Uuid::new_v4(),
            Uuid::new_v4(),
            1,
            root_signer().public_key(),
        ));
        tampered.subject_id = Uuid::new_v4();

        let package = trust_package(wilaya_id, "pkg-trust-tampered", vec![tampered], vec![]);
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected tampered-fields rejection, got {err:?}"
        );
    }

    #[test]
    fn root_signed_successor_certificate_cannot_replace_anchor() {
        // Even a legitimately Root-signed successor WILAYA certificate must not
        // rotate the anchor through trust-package import (anchor immutability:
        // rotation is the `finalize_wilaya_rotation` ceremony).
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let anchor = anchor_cert(&db);

        let successor = root_signed(&wilaya_cert(
            anchor.identity_id,
            Uuid::new_v4(),
            anchor.generation + 1,
            root_signer().public_key(),
        ));
        let package = trust_package(wilaya_id, "pkg-trust-successor", vec![successor], vec![]);
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected anchor-replacement rejection, got {err:?}"
        );
        let stored = db
            .executor()
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)
            .unwrap()
            .expect("anchor still present");
        assert!(stored.is_identical_to(&anchor), "anchor must not be replaced");
    }

    #[test]
    fn root_signed_different_wilaya_identity_is_rejected() {
        // A second WILAYA identity (different identity_id) must not be
        // introduced through trust-package import.
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let before = db.executor().identity_store().list_all().unwrap().len();

        let other = root_signed(&wilaya_cert(
            Uuid::new_v4(),
            Uuid::new_v4(),
            1,
            root_signer().public_key(),
        ));
        let package = trust_package(wilaya_id, "pkg-trust-other-wilaya", vec![other], vec![]);
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected other-WILAYA rejection, got {err:?}"
        );
        assert_eq!(
            db.executor().identity_store().list_all().unwrap().len(),
            before,
            "no second WILAYA identity may be persisted"
        );
    }

    #[test]
    fn package_signed_by_another_wilaya_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let _ = seed_local_wilaya(&db);
        let other_wilaya_id = Uuid::new_v4();

        let package = trust_package(other_wilaya_id, "pkg-trust-wrong-issuer", vec![anchor_cert(&db)], vec![]);
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected other-WILAYA issuer rejection, got {err:?}"
        );
    }

    #[test]
    fn package_without_issuer_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        seed_local_wilaya(&db);

        let mut package = trust_package(Uuid::new_v4(), "pkg-trust-no-issuer", vec![], vec![]);
        package.metadata.issuer_identity_id = None;
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::Validation(crate::errors::ValidationError::InvalidFormat { .. })
            ),
            "expected missing-issuer rejection, got {err:?}"
        );
    }

    #[test]
    fn package_requires_local_active_wilaya() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let package = trust_package(Uuid::new_v4(), "pkg-trust-no-anchor", vec![], vec![]);
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected no-anchor rejection, got {err:?}"
        );
    }

    #[test]
    fn unit_revocation_issued_by_local_wilaya_is_accepted() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);

        let unit_id = Uuid::new_v4();
        db.executor()
            .identity_store()
            .upsert(
                &issued_cert(SubjectType::Unit, unit_id, Uuid::new_v4(), wilaya_id),
                FIXED_NOW,
            )
            .unwrap();

        let package = trust_package(
            wilaya_id,
            "pkg-trust-revoke-unit",
            vec![],
            vec![CertificateRevocation {
                identity_id: unit_id,
                reason: Some("loss of custody".into()),
            }],
        );
        let outcome = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap();
        assert_eq!(outcome.revocation_count, 1);

        let stored = db
            .executor()
            .identity_store()
            .get_by_identity_id(&unit_id)
            .unwrap()
            .expect("identity present");
        assert_eq!(stored.status, CredentialStatus::Revoked);
    }

    #[test]
    fn admin_revocation_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);

        let admin_id = Uuid::new_v4();
        db.executor()
            .identity_store()
            .upsert(
                &issued_cert(SubjectType::Admin, admin_id, Uuid::new_v4(), wilaya_id),
                FIXED_NOW,
            )
            .unwrap();

        let package = trust_package(
            wilaya_id,
            "pkg-trust-revoke-admin",
            vec![],
            vec![CertificateRevocation {
                identity_id: admin_id,
                reason: None,
            }],
        );
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected ADMIN-revocation rejection, got {err:?}"
        );
        let stored = db
            .executor()
            .identity_store()
            .get_by_identity_id(&admin_id)
            .unwrap()
            .expect("identity present");
        assert_eq!(stored.status, CredentialStatus::Active, "ADMIN must not be revoked");
    }

    #[test]
    fn wilaya_anchor_revocation_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let anchor = anchor_cert(&db);

        let package = trust_package(
            wilaya_id,
            "pkg-trust-revoke-anchor",
            vec![],
            vec![CertificateRevocation {
                identity_id: wilaya_id,
                reason: None,
            }],
        );
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected anchor-revocation rejection, got {err:?}"
        );
        let stored = db
            .executor()
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)
            .unwrap()
            .expect("anchor still present");
        assert!(stored.is_identical_to(&anchor), "anchor must remain ACTIVE");
    }

    #[test]
    fn cross_wilaya_unit_revocation_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let other_wilaya_id = Uuid::new_v4();

        let unit_id = Uuid::new_v4();
        db.executor()
            .identity_store()
            .upsert(
                &issued_cert(SubjectType::Unit, unit_id, Uuid::new_v4(), other_wilaya_id),
                FIXED_NOW,
            )
            .unwrap();

        let package = trust_package(
            wilaya_id,
            "pkg-trust-revoke-cross",
            vec![],
            vec![CertificateRevocation {
                identity_id: unit_id,
                reason: None,
            }],
        );
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected cross-WILAYA revocation rejection, got {err:?}"
        );
        let stored = db
            .executor()
            .identity_store()
            .get_by_identity_id(&unit_id)
            .unwrap()
            .expect("identity present");
        assert_eq!(stored.status, CredentialStatus::Active, "UNIT must remain ACTIVE");
    }

    #[test]
    fn revocation_of_unknown_identity_rejects_package() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let package = trust_package(
            wilaya_id,
            "pkg-trust-rev-unknown",
            vec![],
            vec![CertificateRevocation {
                identity_id: Uuid::new_v4(),
                reason: None,
            }],
        );
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::ResourceNotFound { .. })
            ),
            "expected resource not found, got {err:?}"
        );
    }

    #[test]
    fn mixed_valid_and_forged_certificate_rejects_entire_package_zero_writes() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let anchor = anchor_cert(&db);

        // First element is valid (identical anchor), second is a forged WILAYA
        // cert signed by a stolen WILAYA key.
        let attacker_key = Ed25519SigningProvider::new([9u8; 32]);
        let mut forged = wilaya_cert(
            anchor.identity_id,
            anchor.credential_id,
            anchor.generation + 1,
            attacker_key.public_key(),
        );
        let sig = attacker_key.sign_certificate(&forged).unwrap();
        forged.signature = Some(Ed25519CertificateSignature::try_from(sig).unwrap());

        let package = trust_package(
            wilaya_id,
            "pkg-trust-mixed-cert",
            vec![anchor.clone(), forged],
            vec![],
        );
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected mixed-payload rejection, got {err:?}"
        );
        let stored = db
            .executor()
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)
            .unwrap()
            .expect("anchor still present");
        assert!(stored.is_identical_to(&anchor), "zero certificate mutations allowed");
        assert_eq!(db.executor().identity_store().list_all().unwrap().len(), 1);
    }

    #[test]
    fn mixed_valid_and_unauthorized_revocation_rejects_entire_package_zero_writes() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);

        // A legitimately-revocable UNIT + an ADMIN revocation in the same package.
        let unit_id = Uuid::new_v4();
        db.executor()
            .identity_store()
            .upsert(
                &issued_cert(SubjectType::Unit, unit_id, Uuid::new_v4(), wilaya_id),
                FIXED_NOW,
            )
            .unwrap();
        let admin_id = Uuid::new_v4();
        db.executor()
            .identity_store()
            .upsert(
                &issued_cert(SubjectType::Admin, admin_id, Uuid::new_v4(), wilaya_id),
                FIXED_NOW,
            )
            .unwrap();

        let package = trust_package(
            wilaya_id,
            "pkg-trust-mixed-rev",
            vec![],
            vec![
                CertificateRevocation {
                    identity_id: unit_id,
                    reason: None,
                },
                CertificateRevocation {
                    identity_id: admin_id,
                    reason: None,
                },
            ],
        );
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected mixed-revocation rejection, got {err:?}"
        );
        let unit = db
            .executor()
            .identity_store()
            .get_by_identity_id(&unit_id)
            .unwrap()
            .expect("identity present");
        assert_eq!(
            unit.status,
            CredentialStatus::Active,
            "zero revocation mutations allowed on failure"
        );
        let admin = db
            .executor()
            .identity_store()
            .get_by_identity_id(&admin_id)
            .unwrap()
            .expect("identity present");
        assert_eq!(admin.status, CredentialStatus::Active);
    }

    #[test]
    fn duplicate_certificates_are_deterministic_noop() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let anchor = anchor_cert(&db);
        let before = db.executor().identity_store().list_all().unwrap().len();

        let package = trust_package(
            wilaya_id,
            "pkg-trust-dup-certs",
            vec![anchor.clone(), anchor],
            vec![],
        );
        let outcome = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap();
        assert_eq!(outcome.certificate_count, 2);
        assert_eq!(db.executor().identity_store().list_all().unwrap().len(), before);
    }

    #[test]
    fn duplicate_package_id_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let package = trust_package(wilaya_id, "pkg-trust-dup", vec![anchor_cert(&db)], vec![]);
        let input = ImportTrustPackageInput {
            package,
            imported_by: "admin".into(),
        };
        execute(make_executor(&db), &registry(&db), input.clone()).unwrap();

        let err = execute(make_executor(&db), &registry(&db), input).unwrap_err();
        assert!(matches!(
            err,
            AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage { .. })
        ));
    }

    #[test]
    fn unit_certificate_forgery_is_rejected_before_upsert() {
        // SEC-009 regression: a forged UNIT certificate (attacker public key)
        // in a WILAYA-signed trust package must be rejected before any upsert.
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let before = db.executor().identity_store().list_all().unwrap().len();

        let attacker_key = Ed25519SigningProvider::new([5u8; 32]);
        let mut forged_unit = issued_cert(
            SubjectType::Unit,
            Uuid::new_v4(),
            Uuid::new_v4(),
            wilaya_id,
        );
        forged_unit.public_key = attacker_key.public_key();
        let sig = attacker_key.sign_certificate(&forged_unit).unwrap();
        forged_unit.signature = Some(Ed25519CertificateSignature::try_from(sig).unwrap());

        let package = trust_package(wilaya_id, "pkg-trust-forged-unit", vec![forged_unit], vec![]);
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected forged-UNIT rejection, got {err:?}"
        );
        assert_eq!(
            db.executor().identity_store().list_all().unwrap().len(),
            before,
            "forged UNIT certificate must not be persisted"
        );
    }

    #[test]
    fn unauthorized_revocation_is_rejected_before_mutation() {
        // SEC-009 regression: an unauthorized revocation must be rejected before
        // any mutation (the WILAYA anchor stays ACTIVE).
        let db = ConnectionFactory::new_for_test().unwrap();
        let wilaya_id = seed_local_wilaya(&db);
        let anchor = anchor_cert(&db);

        let package = trust_package(
            wilaya_id,
            "pkg-trust-unauth-rev",
            vec![],
            vec![CertificateRevocation {
                identity_id: wilaya_id,
                reason: None,
            }],
        );
        let err = execute(
            make_executor(&db),
            &registry(&db),
            ImportTrustPackageInput {
                package,
                imported_by: "admin".into(),
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
            ),
            "expected unauthorized-revocation rejection, got {err:?}"
        );
        let stored = db
            .executor()
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)
            .unwrap()
            .expect("anchor still present");
        assert!(stored.is_identical_to(&anchor), "anchor must remain unchanged");
    }
}