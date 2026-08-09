//! Verification of `signature_version = 2` (Ed25519) sync package signatures.
//!
//! RFC 2026-08-04-node-identity-trust §3.10 / B4.
//!
//! V2 signatures are verified against the ISSUER's certificate public key from
//! the Identity Store (the single source of truth for identity state, ADR-0038).
//! The signed bytes are the Canonical JSON V2 envelope with `integrity_hash`
//! present and `signature` excluded (`canonical_bytes_for_signature`), matching
//! what `PackageBuilder`/`Ed25519PackageSigner` produced.
//!
//! Fail-closed: an unknown issuer, missing/absent verification material, or an
//! invalid signature rejects the package. V1/legacy packages (`signature_version`
//! absent or 1) are untouched — they remain verified by the deserializer's HMAC
//! path during the deprecation window.

use serde::Serialize;

use crate::application::sync::SyncPackage;
use crate::domain::identity::IdentityStorePort;
use crate::errors::{AppError, AppResult, ValidationError};
use crate::infrastructure::sync::packages::canonical_json::canonical_bytes_for_signature;
use crate::infrastructure::sync::packages::signing::{Ed25519PackageVerifier, PackageVerifier};
use crate::repositories::executor::DbExecutor;
use crate::repositories::RepositoryProvider;

pub struct SyncPackageIdentityVerificationService;

impl SyncPackageIdentityVerificationService {
    /// Verify the Ed25519 signature of a B4 package when
    /// `metadata.signature_version == 2`. No-op for V1/legacy packages.
    pub fn verify_v2_signature<T: Serialize>(
        executor: DbExecutor<'_>,
        package: &SyncPackage<T>,
    ) -> AppResult<()> {
        if package.metadata.signature_version
            != Some(crate::domain::identity::SIGNATURE_VERSION_ED25519)
        {
            return Ok(());
        }

        let signature = package.metadata.signature.as_deref().ok_or_else(|| {
            AppError::Validation(ValidationError::InvalidFormat {
                field: "signature".into(),
                message: "حزمة Ed25519 بلا توقيع".into(),
            })
        })?;

        let issuer_id = package.metadata.issuer_identity_id.ok_or_else(|| {
            AppError::Validation(ValidationError::InvalidFormat {
                field: "issuer_identity_id".into(),
                message: "حزمة Ed25519 بلا هوية مُصدِر".into(),
            })
        })?;

        let certificate = executor
            .identity_store()
            .get_by_identity_id(&issuer_id)?
            .ok_or_else(|| {
                AppError::Validation(ValidationError::InvalidFormat {
                    field: "issuer_identity_id".into(),
                    message: "المُصدِر غير موجود في مخزن الهويات".into(),
                })
            })?;

        let public_key: [u8; 32] = certificate.public_key.as_slice().try_into().map_err(|_| {
            AppError::Validation(ValidationError::InvalidFormat {
                field: "public_key".into(),
                message: "مفتاح المُصدِر العام ليس مفتاح Ed25519 بطول 32 بايت".into(),
            })
        })?;

        let canonical = serde_json::to_value(package).map_err(|e| {
            AppError::Validation(ValidationError::InvalidFormat {
                field: "sync_package".into(),
                message: format!("تعذّر تحويل الحزمة للتحقق من التوقيع: {e}"),
            })
        })?;
        let canonical_bytes = canonical_bytes_for_signature(&canonical)?;

        let valid = Ed25519PackageVerifier::new(public_key).verify(&canonical_bytes, signature)?;
        if !valid {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "signature".into(),
                message: "فشل التحقق من توقيع Ed25519 للحزمة".into(),
            }));
        }
        Ok(())
    }

    /// Resolve the issuer's last applied transport sequence from the
    /// `sync_issuer_sequence` ledger. Thin wrapper so the import pipeline
    /// (commands layer) never constructs repositories directly.
    ///
    /// Only consumed by `run_import_pipeline` as input to `TransportGuard::check`.
    pub fn last_applied_sequence(
        executor: DbExecutor<'_>,
        issuer_identity_id: &str,
    ) -> AppResult<Option<u64>> {
        executor
            .sync_applied_packages()
            .last_applied_sequence_for_issuer(issuer_identity_id)
    }

    /// Advance the per-issuer transport ledger after a package has been applied.
    /// Must run inside the same transaction as the import (fail-closed).
    pub fn advance_issuer_sequence(
        executor: DbExecutor<'_>,
        issuer_identity_id: &str,
        package_sequence: u64,
    ) -> AppResult<()> {
        executor
            .sync_applied_packages()
            .record_issuer_sequence(issuer_identity_id, package_sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::sync::{PackageId, SchemaVersion};
    use crate::db::ConnectionFactory;
    use crate::db::Database;
    use crate::domain::identity::{
        CredentialStatus, IdentityCertificate, IdentitySigner, SubjectType,
    };
    use crate::infrastructure::sync::packages::canonical_json::canonical_bytes_for_integrity;
    use crate::infrastructure::sync::packages::integrity::{PackageHasher, Sha256PackageHasher};
    use crate::infrastructure::sync::packages::signing::{Ed25519PackageSigner, PackageSigner};
    use crate::repositories::executor::DbExecutor;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    const ISSUER_SECRET: [u8; 32] = [42u8; 32];

    fn make_executor(db: &Database) -> DbExecutor<'_> {
        db.executor()
    }

    fn seed_issuer(db: &Database, identity_id: Uuid) {
        let certificate = IdentityCertificate {
            identity_id,
            subject_type: SubjectType::Wilaya,
            subject_id: identity_id,
            issuer_identity_id: None,
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: crate::infrastructure::security::Ed25519SigningProvider::new(ISSUER_SECRET)
                .public_key(),
            algorithm_version: crate::domain::identity::SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: Some(1),
            signature: None,
        };
        IdentityStorePort::upsert(
            &make_executor(db).identity_store(),
            &certificate,
            &Utc::now().to_rfc3339(),
        )
        .unwrap();
    }

    fn build_v2_package(issuer_id: Uuid) -> SyncPackage<serde_json::Value> {
        let signer = Ed25519PackageSigner::new(ISSUER_SECRET);
        let mut package = SyncPackage {
            metadata: crate::application::sync::SyncPackageMetadata {
                created_at: Utc::now(),
                integrity_hash: None,
                package_sequence: Some(1),
                issuer_identity_id: Some(issuer_id),
                package_id: PackageId(Uuid::new_v4().to_string()),
                schema_version: SchemaVersion::V2,
                signature: None,
                signature_version: Some(crate::domain::identity::SIGNATURE_VERSION_ED25519),
                signing_key_id: Some(signer.public_key_hex()),
                source_node_id: "wilaya-a".to_string(),
            },
            payload: json!({ "units": [] }),
        };

        // Mirror PackageBuilder Pass A: integrity hash over canonical bytes.
        let value = serde_json::to_value(&package).unwrap();
        let hash = Sha256PackageHasher
            .hash(&canonical_bytes_for_integrity(&value).unwrap())
            .unwrap();
        package.metadata.integrity_hash = Some(hash);

        // Mirror PackageBuilder Pass B: signature over canonical bytes (hash present).
        let value = serde_json::to_value(&package).unwrap();
        let signature = signer
            .sign(&canonical_bytes_for_signature(&value).unwrap())
            .unwrap();
        package.metadata.signature = Some(signature);
        package
    }

    #[test]
    fn valid_v2_package_is_accepted() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        seed_issuer(&db, issuer_id);

        let package = build_v2_package(issuer_id);
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_ok(), "expected accept, got: {result:?}");
    }

    #[test]
    fn tampered_payload_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        seed_issuer(&db, issuer_id);

        let mut package = build_v2_package(issuer_id);
        package.payload = json!({ "units": [{ "tampered": true }] });
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_err());
    }

    #[test]
    fn unknown_issuer_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        seed_issuer(&db, Uuid::new_v4());

        let package = build_v2_package(issuer_id);
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_err());
    }

    #[test]
    fn issuer_certificate_with_wrong_key_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        let certificate = IdentityCertificate {
            identity_id: issuer_id,
            subject_type: SubjectType::Wilaya,
            subject_id: issuer_id,
            issuer_identity_id: None,
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: crate::infrastructure::security::Ed25519SigningProvider::new([7u8; 32])
                .public_key(),
            algorithm_version: crate::domain::identity::SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: Some(1),
            signature: None,
        };
        IdentityStorePort::upsert(
            &make_executor(&db).identity_store(),
            &certificate,
            &Utc::now().to_rfc3339(),
        )
        .unwrap();

        let package = build_v2_package(issuer_id);
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_err());
    }

    #[test]
    fn missing_signature_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        seed_issuer(&db, issuer_id);

        let mut package = build_v2_package(issuer_id);
        package.metadata.signature = None;
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_err());
    }

    #[test]
    fn v1_legacy_package_is_a_noop() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let package = SyncPackage {
            metadata: crate::application::sync::SyncPackageMetadata {
                created_at: Utc::now(),
                integrity_hash: None,
                package_sequence: None,
                issuer_identity_id: None,
                package_id: PackageId(Uuid::new_v4().to_string()),
                schema_version: SchemaVersion::V1,
                signature: None,
                signature_version: None,
                signing_key_id: None,
                source_node_id: "unit-a".to_string(),
            },
            payload: json!({ "items": [] }),
        };
        // No issuer seeded — V1 must be untouched (no-op early return).
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn non_v2_signature_version_is_a_noop() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let package = SyncPackage {
            metadata: crate::application::sync::SyncPackageMetadata {
                created_at: Utc::now(),
                integrity_hash: None,
                package_sequence: None,
                issuer_identity_id: None,
                package_id: PackageId(Uuid::new_v4().to_string()),
                schema_version: SchemaVersion::V2,
                signature: Some("deadbeef".to_string()),
                signature_version: Some(1),
                signing_key_id: None,
                source_node_id: "unit-a".to_string(),
            },
            payload: json!({ "items": [] }),
        };
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_ok());
    }
}
