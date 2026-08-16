//! Verification of `signature_version = 2` (Ed25519) sync package signatures.
//!
//! RFC 2026-08-04-node-identity-trust §3.10 / B4 / ADR-0046.
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
//!
//! Issuer policy (ADR-0046): `verify_v2_signature` preserves the legacy
//! SEC-003-01 WILAYA-only gate (`.unit` dedicated path). The import pipeline
//! uses `verify_v2_package_for_import`, which applies the kind-scoped policy:
//! UNIT issuers are accepted ONLY for `stock_movements` / `daily_report` /
//! `monthly_summary` on WILAYA importers, and ONLY after Ed25519 signature
//! authentication, with mandatory membership (`cert.subject_id → local units
//! row of the importer's wilaya`) and issuer↔import-target/payload binding.

use serde::Serialize;

use crate::application::sync::unit_issuer_membership::verify_unit_issuer_membership;
use crate::application::sync::SyncPackage;
use crate::application::usecases::sync::import_daily_report_package::DAILY_REPORT_PACKAGE_KIND;
use crate::application::usecases::sync::import_monthly_summary_package::MONTHLY_SUMMARY_PACKAGE_KIND;
use crate::application::usecases::sync::import_stock_movements_package::STOCK_MOVEMENTS_PACKAGE_KIND;
use crate::domain::identity::{
    CredentialStatus, IdentityCertificate, IdentityStorePort, SubjectType,
    SIGNATURE_VERSION_ED25519,
};
use crate::errors::{AppError, AppResult, ValidationError};
use crate::infrastructure::sync::packages::canonical_json::canonical_bytes_for_signature;
use crate::infrastructure::sync::packages::signing::{Ed25519PackageVerifier, PackageVerifier};
use crate::repositories::executor::DbExecutor;
use crate::repositories::RepositoryProvider;

/// Package kinds whose V2 packages may legitimately carry a UNIT issuer
/// (ADR-0046 §3 I2). Exhaustive: any other kind remains WILAYA-only. There is
/// no generic «ACTIVE UNIT signs V2 packages» acceptance path.
const UNIT_ISSUER_ACCEPTED_KINDS: &[&str] = &[
    STOCK_MOVEMENTS_PACKAGE_KIND,
    DAILY_REPORT_PACKAGE_KIND,
    MONTHLY_SUMMARY_PACKAGE_KIND,
];

fn reject(message: &str) -> AppError {
    AppError::Validation(ValidationError::InvalidFormat {
        field: "issuer_identity_id".into(),
        message: message.into(),
    })
}

/// Issuer validity gate shared by both V2 entry points (SEC-003-01, ADR-0046
/// §5 steps 5-6).
///
/// A V2 issuer certificate must have status `Active` and must not be expired
/// (`not_after` is None or in the future). `not_after` is advisory only
/// (Invariant 8 — `EXPIRED` is never derived from a wall clock); this check is
/// a verification-time guard, not lifecycle evaluation.
/// `Revoked`/`Superseded`/`Expired` statuses always reject.
///
/// Rotation compatibility: the legitimate rotation sequence signs the trust
/// package with the OLD key while the OLD certificate is still ACTIVE, so an
/// older generation with `status == Active` and a valid `not_after` remains
/// acceptable. Fail-closed for anything else.
fn validate_issuer_status(certificate: &IdentityCertificate) -> AppResult<()> {
    if certificate.status != CredentialStatus::Active {
        return Err(reject(&format!(
            "المُصدِر غير نشط (الحالة: {})",
            certificate.status
        )));
    }
    if let Some(not_after) = certificate.not_after {
        if not_after <= chrono::Utc::now() {
            return Err(reject("شهادة المُصدِر منتهية الصلاحية"));
        }
    }
    Ok(())
}

/// Resolve the authenticated issuer certificate for a V2 package (ADR-0046
/// §5 steps 1-2). No-op semantics for non-V2 packages are owned by the callers.
fn resolve_v2_issuer_certificate<T: Serialize>(
    executor: DbExecutor<'_>,
    package: &SyncPackage<T>,
) -> AppResult<IdentityCertificate> {
    let signature = package.metadata.signature.as_deref().ok_or_else(|| {
        AppError::Validation(ValidationError::InvalidFormat {
            field: "signature".into(),
            message: "حزمة Ed25519 بلا توقيع".into(),
        })
    })?;

    let issuer_id = package.metadata.issuer_identity_id.ok_or_else(|| {
        reject("حزمة Ed25519 بلا هوية مُصدِر")
    })?;

    let certificate = executor
        .identity_store()
        .get_by_identity_id(&issuer_id)?
        .ok_or_else(|| reject("المُصدِر غير موجود في مخزن الهويات"))?;

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
    Ok(certificate)
}

pub struct SyncPackageIdentityVerificationService;

impl SyncPackageIdentityVerificationService {
    /// Legacy WILAYA-only V2 entry point (SEC-003-01, `.unit` dedicated path).
    ///
    /// Kept as the strict gate for the `.unit` bootstrap (ADR-0044): the issuer
    /// of a `.unit` package MUST be a WILAYA certificate. No-op for V1/legacy
    /// packages.
    pub fn verify_v2_signature<T: Serialize>(
        executor: DbExecutor<'_>,
        package: &SyncPackage<T>,
    ) -> AppResult<()> {
        if package.metadata.signature_version != Some(SIGNATURE_VERSION_ED25519) {
            return Ok(());
        }
        let certificate = resolve_v2_issuer_certificate(executor, package)?;
        validate_issuer_status(&certificate)?;
        if certificate.subject_type != SubjectType::Wilaya {
            return Err(reject("المُصدِر ليس عقدة ولاية (WILAYA)"));
        }
        Ok(())
    }
}

/// Trusted local import context for the kind-scoped issuer policy (ADR-0046
/// §5). Every value originates from trusted local state or from the command
/// layer's per-command constants — never from package metadata or filenames.
/// Extracts every present, non-empty signed payload unit id from a package
/// payload (used for `stock_movements` payload binding).
pub type PayloadUnitIdExtractor<'a, T> = Option<&'a dyn Fn(&T) -> Vec<String>>;

/// Import-context policy for the kind-scoped V2 verifier (ADR-0046 DESIGN B).
/// The authoritative package kind is supplied by the command layer's per-command
/// constants — never from package metadata or filenames.
pub struct V2ImportPolicy<'a, T> {
    /// Authoritative package kind (command-layer constant, e.g.
    /// `STOCK_MOVEMENTS_PACKAGE_KIND`).
    pub package_kind: &'a str,
    /// Importer node type: UNIT-issued packages are accepted ONLY on WILAYA
    /// importers (ADR-0046 §3 I3.3 / I5 — no UNIT↔UNIT data authority).
    pub importer_is_wilaya: bool,
    /// Trusted local `settings.wilaya_code` (persisted state).
    pub importer_wilaya_code: &'a str,
    /// Renderer-selected import target UNIT id. For UNIT issuers this MUST
    /// equal the authenticated `cert.subject_id` (post-signature binding).
    pub import_unit_id: Option<&'a str>,
    /// Extracts every present, non-empty signed payload unit id (used for
    /// `stock_movements` payload binding). Invoked ONLY after signature
    /// authentication and ONLY for UNIT issuers.
    pub payload_unit_ids: PayloadUnitIdExtractor<'a, T>,
}

impl<'a, T> V2ImportPolicy<'a, T> {
    pub fn new(
        package_kind: &'a str,
        importer_is_wilaya: bool,
        importer_wilaya_code: &'a str,
        import_unit_id: Option<&'a str>,
        payload_unit_ids: PayloadUnitIdExtractor<'a, T>,
    ) -> Self {
        Self {
            package_kind,
            importer_is_wilaya,
            importer_wilaya_code,
            import_unit_id,
            payload_unit_ids,
        }
    }
}

impl SyncPackageIdentityVerificationService {
    /// Kind-scoped V2 verification for the import pipeline (ADR-0046 DESIGN B,
    /// signature-first per I4). Order:
    ///   1. issuer_identity_id present
    ///   2. issuer certificate exists — Ed25519 package signature verified
    ///      against it before any subject-derived decision (I4)
    ///   3. package-kind issuer policy — UNIT issuers allowed only for the
    ///      accepted data kinds on WILAYA importers (fail-closed)
    ///   4. certificate ACTIVE + not_after
    ///   5. ONLY after signature authentication, for UNIT issuers:
    ///      a. membership: cert.subject_id → units → wilaya_code == settings
    ///      b. import unit_id == cert.subject_id
    ///      c. stock_movements payload unit_ids (present, non-empty) == subject
    ///
    /// The critical invariant (ADR-0046 §3 I4): `cert.subject_id` is never used as
    /// an authorization/binding authority before the signature has been verified.
    pub fn verify_v2_package_for_import<T: Serialize>(
    executor: DbExecutor<'_>,
    package: &SyncPackage<T>,
    policy: &V2ImportPolicy<'_, T>,
) -> AppResult<()> {
    if package.metadata.signature_version != Some(SIGNATURE_VERSION_ED25519) {
        return Ok(());
    }

    // Steps 1-2 (Ed25519 authentication, I4): resolve the certificate and
    // authenticate the signature. The certificate is resolved here because the
    // verifier needs it for authentication; its subject becomes
    // security-authoritative ONLY after successful verification.
    let certificate = resolve_v2_issuer_certificate(executor, package)?;

    // Step 3: package-kind issuer policy (fail-closed; no authority granted
    // by this stage — the coarse subject category only gates early rejection).
    let unit_issuer = match certificate.subject_type {
        SubjectType::Wilaya => false,
        SubjectType::Unit => {
            if !UNIT_ISSUER_ACCEPTED_KINDS.contains(&policy.package_kind) {
                return Err(reject(&format!(
                    "المُصدِر وحدة (UNIT) غير مسموح له بصنف الحزمة «{}»",
                    policy.package_kind
                )));
            }
            if !policy.importer_is_wilaya {
                return Err(reject(
                    "حزمة مُصدِرة من UNIT تُستورد فقط على عقدة WILAYA",
                ));
            }
            true
        }
        other => {
            return Err(reject(&format!(
                "المُصدِر من نوع غير مسموح ({other})"
            )));
        }
    };

    // Step 4: certificate ACTIVE and not expired.
    validate_issuer_status(&certificate)?;

    // Step 5: UNIT post-signature membership and binding (I4 — after Ed25519
    // authentication above).
    if unit_issuer {
        let subject_id = certificate.subject_id.to_string();

        // 8a. Membership: subject must be a local unit of the importer's WILAYA.
        verify_unit_issuer_membership(executor, &subject_id, policy.importer_wilaya_code)?;

        // 8b. Import-target binding: the authenticated issuer decides the
        //     target UNIT — the renderer value must match it exactly.
        let import_unit_id = policy.import_unit_id.ok_or_else(|| {
            reject("حزمة مُصدِرة من UNIT بدون وحدة استيراد مستهدفة")
        })?;
        if import_unit_id.trim() != subject_id {
            return Err(reject(&format!(
                "الوحدة المستهدفة «{}» لا تطابق هوية المُصدِر الموثّقة «{}»",
                import_unit_id.trim(),
                subject_id
            )));
        }

        // 8c. Stock-movements payload binding: every signed movement unit_id,
        //     when present and non-empty, must equal the authenticated subject.
        //     Genuinely absent/empty values keep the mutation restamp, whose
        //     authoritative source is now the authenticated subject (the
        //     renderer target above is bound to it).
        if let Some(extract) = policy.payload_unit_ids {
            for payload_unit_id in extract(&package.payload) {
                if payload_unit_id != subject_id {
                    return Err(reject(&format!(
                        "وحدة بيانات الحزمة «{payload_unit_id}» لا تطابق هوية المُصدِر الموثّقة «{subject_id}»"
                    )));
                }
            }
        }
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
        seed_certificate(db, identity_id, SubjectType::Wilaya, CredentialStatus::Active, None);
    }

    fn seed_certificate(
        db: &Database,
        identity_id: Uuid,
        subject_type: SubjectType,
        status: CredentialStatus,
        not_after: Option<chrono::DateTime<chrono::Utc>>,
    ) {
        let certificate = IdentityCertificate {
            identity_id,
            subject_type,
            subject_id: identity_id,
            issuer_identity_id: None,
            credential_id: Uuid::new_v4(),
            generation: 1,
            status,
            public_key: crate::infrastructure::security::Ed25519SigningProvider::new(ISSUER_SECRET)
                .public_key(),
            algorithm_version: crate::domain::identity::SIGNATURE_VERSION_ED25519,
            not_after,
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

    // ── SEC-003-01: issuer validity (WILAYA + ACTIVE + not expired) ──────

    #[test]
    fn revoked_issuer_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        seed_certificate(&db, issuer_id, SubjectType::Wilaya, CredentialStatus::Revoked, None);
        let package = build_v2_package(issuer_id);
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_err(), "revoked issuer must be rejected: {result:?}");
    }

    #[test]
    fn superseded_issuer_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        seed_certificate(&db, issuer_id, SubjectType::Wilaya, CredentialStatus::Superseded, None);
        let package = build_v2_package(issuer_id);
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_err(), "superseded issuer must be rejected: {result:?}");
    }

    #[test]
    fn expired_status_issuer_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        seed_certificate(&db, issuer_id, SubjectType::Wilaya, CredentialStatus::Expired, None);
        let package = build_v2_package(issuer_id);
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_err(), "expired-status issuer must be rejected: {result:?}");
    }

    #[test]
    fn issuer_with_past_not_after_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        let past = chrono::DateTime::from_timestamp(1_500_000_000, 0).unwrap();
        seed_certificate(&db, issuer_id, SubjectType::Wilaya, CredentialStatus::Active, Some(past));
        let package = build_v2_package(issuer_id);
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_err(), "past not_after must be rejected: {result:?}");
    }

    #[test]
    fn unit_issuer_is_rejected() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        seed_certificate(&db, issuer_id, SubjectType::Unit, CredentialStatus::Active, None);
        let package = build_v2_package(issuer_id);
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_err(), "UNIT issuer must be rejected: {result:?}");
    }

    #[test]
    fn active_wilaya_issuer_with_future_not_after_is_accepted() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        let future = chrono::Utc::now() + chrono::Duration::days(365);
        seed_certificate(&db, issuer_id, SubjectType::Wilaya, CredentialStatus::Active, Some(future));
        let package = build_v2_package(issuer_id);
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_ok(), "ACTIVE WILAYA with future not_after accepted: {result:?}");
    }

    #[test]
    fn issuer_with_malformed_public_key_is_rejected() {
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
            public_key: vec![0u8; 16],
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

    // ── SEC-003-01: rotation regression through the real V2 path ──────────

    #[test]
    fn rotation_old_key_accepted_while_old_certificate_still_active() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let old_issuer = Uuid::new_v4();
        // OLD certificate remains ACTIVE while the rotation trust package is
        // being distributed (signed with the OLD key) — must be ACCEPTED.
        seed_certificate(&db, old_issuer, SubjectType::Wilaya, CredentialStatus::Active, None);
        let package = build_v2_package(old_issuer);
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_ok(), "OLD ACTIVE issuer accepted during rotation: {result:?}");
    }

    #[test]
    fn rotation_old_key_rejected_after_old_certificate_superseded() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let old_issuer = Uuid::new_v4();
        seed_certificate(&db, old_issuer, SubjectType::Wilaya, CredentialStatus::Superseded, None);
        let package = build_v2_package(old_issuer);
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_err(), "OLD SUPERSEDED issuer must be rejected: {result:?}");
    }

    #[test]
    fn rotation_old_key_rejected_after_old_certificate_revoked() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let old_issuer = Uuid::new_v4();
        seed_certificate(&db, old_issuer, SubjectType::Wilaya, CredentialStatus::Revoked, None);
        let package = build_v2_package(old_issuer);
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_err(), "OLD REVOKED issuer must be rejected: {result:?}");
    }

    #[test]
    fn rotation_new_key_accepted_with_new_active_certificate() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let new_issuer = Uuid::new_v4();
        seed_certificate(&db, new_issuer, SubjectType::Wilaya, CredentialStatus::Active, None);
        let package = build_v2_package(new_issuer);
        let result = SyncPackageIdentityVerificationService::verify_v2_signature(
            make_executor(&db),
            &package,
        );
        assert!(result.is_ok(), "NEW ACTIVE issuer accepted after rotation: {result:?}");
    }

    const WILAYA_CODE: &str = "16";

    fn seed_unit_issuer(
        db: &Database,
        identity_id: Uuid,
        unit_id: Uuid,
        wilaya_code: &str,
        status: CredentialStatus,
        not_after: Option<chrono::DateTime<chrono::Utc>>,
    ) {
        make_executor(db)
            .units()
            .upsert_raw_unit(
                &unit_id.to_string(),
                &format!("U-{unit_id}"),
                &format!("Unit {unit_id}"),
                wilaya_code,
                &Utc::now().to_rfc3339(),
            )
            .unwrap();
        let certificate = IdentityCertificate {
            identity_id,
            subject_type: SubjectType::Unit,
            subject_id: unit_id,
            issuer_identity_id: None,
            credential_id: Uuid::new_v4(),
            generation: 1,
            status,
            public_key: crate::infrastructure::security::Ed25519SigningProvider::new(ISSUER_SECRET)
                .public_key(),
            algorithm_version: crate::domain::identity::SIGNATURE_VERSION_ED25519,
            not_after,
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

    fn movement_extractor(payload: &serde_json::Value) -> Vec<String> {
        payload["movements"]
            .as_array()
            .map(|movements| {
                movements
                    .iter()
                    .filter_map(|m| m["unit_id"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn build_stock_movements_package(issuer_id: Uuid, unit_id: Uuid) -> SyncPackage<serde_json::Value> {
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
                source_node_id: "unit-a".to_string(),
            },
            payload: json!({
                "movements": [
                    { "id": "m1", "unit_id": unit_id.to_string() },
                    { "id": "m2", "unit_id": unit_id.to_string() }
                ]
            }),
        };
        let value = serde_json::to_value(&package).unwrap();
        let hash = Sha256PackageHasher
            .hash(&canonical_bytes_for_integrity(&value).unwrap())
            .unwrap();
        package.metadata.integrity_hash = Some(hash);
        let value = serde_json::to_value(&package).unwrap();
        let signature = signer
            .sign(&canonical_bytes_for_signature(&value).unwrap())
            .unwrap();
        package.metadata.signature = Some(signature);
        package
    }

    #[test]
    fn unit_issuer_accepted_for_stock_movements_on_wilaya_importer() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        let unit_id = Uuid::new_v4();
        seed_unit_issuer(&db, issuer_id, unit_id, WILAYA_CODE, CredentialStatus::Active, None);

        let package = build_stock_movements_package(issuer_id, unit_id);
        let unit_id_str = unit_id.to_string();
        let policy = V2ImportPolicy::new(
            STOCK_MOVEMENTS_PACKAGE_KIND,
            true,
            WILAYA_CODE,
            Some(&unit_id_str),
            Some(&movement_extractor),
        );
        let result = SyncPackageIdentityVerificationService::verify_v2_package_for_import(
            make_executor(&db),
            &package,
            &policy,
        );
        assert!(result.is_ok(), "ACTIVE UNIT issuer on WILAYA importer accepted: {result:?}");
    }

    #[test]
    fn unit_issuer_rejected_when_importer_is_not_wilaya() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        let unit_id = Uuid::new_v4();
        seed_unit_issuer(&db, issuer_id, unit_id, WILAYA_CODE, CredentialStatus::Active, None);

        let package = build_stock_movements_package(issuer_id, unit_id);
        let unit_id_str = unit_id.to_string();
        let policy = V2ImportPolicy::new(
            STOCK_MOVEMENTS_PACKAGE_KIND,
            false,
            WILAYA_CODE,
            Some(&unit_id_str),
            Some(&movement_extractor),
        );
        let result = SyncPackageIdentityVerificationService::verify_v2_package_for_import(
            make_executor(&db),
            &package,
            &policy,
        );
        assert!(result.is_err(), "UNIT↔UNIT import must be rejected: {result:?}");
    }

    #[test]
    fn unit_issuer_rejected_for_foreign_wilaya_membership() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        let unit_id = Uuid::new_v4();
        seed_unit_issuer(&db, issuer_id, unit_id, "10", CredentialStatus::Active, None);

        let package = build_stock_movements_package(issuer_id, unit_id);
        let unit_id_str = unit_id.to_string();
        let policy = V2ImportPolicy::new(
            STOCK_MOVEMENTS_PACKAGE_KIND,
            true,
            WILAYA_CODE,
            Some(&unit_id_str),
            Some(&movement_extractor),
        );
        let result = SyncPackageIdentityVerificationService::verify_v2_package_for_import(
            make_executor(&db),
            &package,
            &policy,
        );
        assert!(result.is_err(), "UNIT of another WILAYA must be rejected: {result:?}");
    }

    #[test]
    fn unit_issuer_rejected_when_membership_unit_row_missing() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        let unit_id = Uuid::new_v4();
        seed_certificate(&db, issuer_id, SubjectType::Unit, CredentialStatus::Active, None);

        let package = build_stock_movements_package(issuer_id, unit_id);
        let unit_id_str = unit_id.to_string();
        let policy = V2ImportPolicy::new(
            STOCK_MOVEMENTS_PACKAGE_KIND,
            true,
            WILAYA_CODE,
            Some(&unit_id_str),
            Some(&movement_extractor),
        );
        let result = SyncPackageIdentityVerificationService::verify_v2_package_for_import(
            make_executor(&db),
            &package,
            &policy,
        );
        assert!(result.is_err(), "UNIT without a local units row must be rejected: {result:?}");
    }

    #[test]
    fn unit_issuer_rejected_when_import_unit_id_missing() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        let unit_id = Uuid::new_v4();
        seed_unit_issuer(&db, issuer_id, unit_id, WILAYA_CODE, CredentialStatus::Active, None);

        let package = build_stock_movements_package(issuer_id, unit_id);
        let policy = V2ImportPolicy::new(
            STOCK_MOVEMENTS_PACKAGE_KIND,
            true,
            WILAYA_CODE,
            None,
            Some(&movement_extractor),
        );
        let result = SyncPackageIdentityVerificationService::verify_v2_package_for_import(
            make_executor(&db),
            &package,
            &policy,
        );
        assert!(result.is_err(), "UNIT package without import target must be rejected: {result:?}");
    }

    #[test]
    fn unit_issuer_rejected_when_import_unit_id_mismatches_subject() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        let unit_id = Uuid::new_v4();
        seed_unit_issuer(&db, issuer_id, unit_id, WILAYA_CODE, CredentialStatus::Active, None);

        let package = build_stock_movements_package(issuer_id, unit_id);
        let foreign_unit = Uuid::new_v4().to_string();
        let policy = V2ImportPolicy::new(
            STOCK_MOVEMENTS_PACKAGE_KIND,
            true,
            WILAYA_CODE,
            Some(&foreign_unit),
            Some(&movement_extractor),
        );
        let result = SyncPackageIdentityVerificationService::verify_v2_package_for_import(
            make_executor(&db),
            &package,
            &policy,
        );
        assert!(result.is_err(), "Renderered unit_id different from authenticated subject must be rejected: {result:?}");
    }

    #[test]
    fn unit_issuer_rejected_when_payload_unit_id_mismatches_subject() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        let unit_id = Uuid::new_v4();
        seed_unit_issuer(&db, issuer_id, unit_id, WILAYA_CODE, CredentialStatus::Active, None);

        let package = build_stock_movements_package(issuer_id, unit_id);
        let unit_id_str = unit_id.to_string();
        let foreign = Uuid::new_v4();
        let tamper_extractor = |p: &serde_json::Value| -> Vec<String> {
            let mut ids = movement_extractor(p);
            ids[0] = foreign.to_string();
            ids
        };
        let policy = V2ImportPolicy::new(
            STOCK_MOVEMENTS_PACKAGE_KIND,
            true,
            WILAYA_CODE,
            Some(&unit_id_str),
            Some(&tamper_extractor as &dyn Fn(&serde_json::Value) -> Vec<String>),
        );
        let result = SyncPackageIdentityVerificationService::verify_v2_package_for_import(
            make_executor(&db),
            &package,
            &policy,
        );
        assert!(result.is_err(), "Signed movement of another unit must be rejected: {result:?}");
    }

    #[test]
    fn unit_issuer_rejected_when_certificate_revoked() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        let unit_id = Uuid::new_v4();
        seed_unit_issuer(&db, issuer_id, unit_id, WILAYA_CODE, CredentialStatus::Revoked, None);

        let package = build_stock_movements_package(issuer_id, unit_id);
        let unit_id_str = unit_id.to_string();
        let policy = V2ImportPolicy::new(
            STOCK_MOVEMENTS_PACKAGE_KIND,
            true,
            WILAYA_CODE,
            Some(&unit_id_str),
            Some(&movement_extractor),
        );
        let result = SyncPackageIdentityVerificationService::verify_v2_package_for_import(
            make_executor(&db),
            &package,
            &policy,
        );
        assert!(result.is_err(), "REVOKED UNIT issuer must be rejected: {result:?}");
    }

    #[test]
    fn unit_issuer_rejected_when_certificate_superseded() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        let unit_id = Uuid::new_v4();
        seed_unit_issuer(&db, issuer_id, unit_id, WILAYA_CODE, CredentialStatus::Superseded, None);

        let package = build_stock_movements_package(issuer_id, unit_id);
        let unit_id_str = unit_id.to_string();
        let policy = V2ImportPolicy::new(
            STOCK_MOVEMENTS_PACKAGE_KIND,
            true,
            WILAYA_CODE,
            Some(&unit_id_str),
            Some(&movement_extractor),
        );
        let result = SyncPackageIdentityVerificationService::verify_v2_package_for_import(
            make_executor(&db),
            &package,
            &policy,
        );
        assert!(result.is_err(), "SUPERSEDED UNIT issuer must be rejected: {result:?}");
    }

    #[test]
    fn unit_issuer_rejected_when_certificate_expired() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        let unit_id = Uuid::new_v4();
        seed_unit_issuer(
            &db,
            issuer_id,
            unit_id,
            WILAYA_CODE,
            CredentialStatus::Active,
            Some(Utc::now() - chrono::Duration::days(1)),
        );

        let package = build_stock_movements_package(issuer_id, unit_id);
        let unit_id_str = unit_id.to_string();
        let policy = V2ImportPolicy::new(
            STOCK_MOVEMENTS_PACKAGE_KIND,
            true,
            WILAYA_CODE,
            Some(&unit_id_str),
            Some(&movement_extractor),
        );
        let result = SyncPackageIdentityVerificationService::verify_v2_package_for_import(
            make_executor(&db),
            &package,
            &policy,
        );
        assert!(result.is_err(), "EXPIRED UNIT issuer must be rejected: {result:?}");
    }

#[test]
    fn unit_issuer_rejected_for_non_data_kind() {
        // Every kind OUTSIDE the accepted data kinds must reject a UNIT issuer —
        // including trust/registry/identity_access/products (ADR-0046 I2).
        for kind in [
            "trust",
            "registry",
            "identity_access",
            "products",
            ".unit",
            "unknown_kind",
        ] {
            let db = ConnectionFactory::new_for_test().unwrap();
            let issuer_id = Uuid::new_v4();
            let unit_id = Uuid::new_v4();
            seed_unit_issuer(&db, issuer_id, unit_id, WILAYA_CODE, CredentialStatus::Active, None);

            let package = build_stock_movements_package(issuer_id, unit_id);
            let unit_id_str = unit_id.to_string();
            let policy = V2ImportPolicy::new(
                kind,
                true,
                WILAYA_CODE,
                Some(&unit_id_str),
                Some(&movement_extractor),
            );
            let result = SyncPackageIdentityVerificationService::verify_v2_package_for_import(
                make_executor(&db),
                &package,
                &policy,
            );
            assert!(
                result.is_err(),
                "UNIT issuer on non-data kind «{kind}» must be rejected: {result:?}"
            );
        }
    }

    #[test]
    fn wilaya_issuer_accepted_without_unit_context() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let issuer_id = Uuid::new_v4();
        seed_issuer(&db, issuer_id);

        let package = build_v2_package(issuer_id);
        let policy = V2ImportPolicy::new(
            "products",
            false,
            WILAYA_CODE,
            None,
            None,
        );
        let result = SyncPackageIdentityVerificationService::verify_v2_package_for_import(
            make_executor(&db),
            &package,
            &policy,
        );
        assert!(result.is_ok(), "WILAYA issuer path ignores UNIT context: {result:?}");
    }

    #[test]
    fn non_v2_package_bypasses_issuer_policy() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let mut package = build_v2_package(Uuid::new_v4());
        package.metadata.signature_version = None;
        package.metadata.signature = None;

        let policy = V2ImportPolicy::new(
            "products",
            false,
            WILAYA_CODE,
            None,
            None,
        );
        let result = SyncPackageIdentityVerificationService::verify_v2_package_for_import(
            make_executor(&db),
            &package,
            &policy,
        );
        assert!(result.is_ok(), "V1/legacy packages remain untouched by issuer policy: {result:?}");
    }
}
