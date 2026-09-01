//! Decode plaintext interchange bytes produced by [`super::SerdeJsonSyncPackageSerializer`](super::SerdeJsonSyncPackageSerializer).
//!
//! SEC-007 (ADR-0047): V1/HMAC sync packages are permanently removed. Every
//! decoded package MUST carry complete V2 metadata — present `integrity_hash`,
//! present non-empty `signature`, and `signature_version = 2` (Ed25519). The
//! legacy "missing field means V1/Golden" allowances and the HMAC verification
//! path are deleted; failures are fail-closed.

use crate::application::sync::{SchemaVersion, SyncPackage};
use crate::application::usecases::exports::types::DailyReportExportDataset;
use crate::application::usecases::exports::types::{
    MonthlySummaryExportDataset, ProductsExportDataset, StockMovementsExportDataset,
};
use crate::application::usecases::sync::import_registry_package::RegistryPackagePayload;
use crate::application::usecases::sync::import_trust_package::TrustPackagePayload;
use crate::domain::identity::SIGNATURE_VERSION_ED25519;
use crate::errors::{AppError, AppResult};
use crate::infrastructure::MAX_IMPORT_SIZE;
use crate::models::{IdentityAccessPayload, UnitNodePackage};
use serde::{de::DeserializeOwned, Serialize};

use super::canonical_json::canonical_bytes_for_integrity;
use super::integrity::{PackageHasher, Sha256PackageHasher};

#[derive(Clone, Copy, Debug, Default)]
pub struct SerdeJsonSyncPackageDeserializer;

impl SerdeJsonSyncPackageDeserializer {
    /// Buffered JSON deserialization.
    /// Peak RAM is bounded by buffer sizes and the resulting object graph.
    fn parse_json_from_reader<T: DeserializeOwned, R: std::io::Read>(
        reader: R,
    ) -> AppResult<SyncPackage<T>> {
        // We wrap in a Take to prevent unbounded reading (security limit)
        let limited_reader = reader.take(MAX_IMPORT_SIZE);
        serde_json::from_reader(limited_reader).map_err(|e| {
            AppError::Validation(crate::errors::ValidationError::InvalidFormat {
                field: "sync_package".into(),
                message: format!("تعذّر تحليل JSON من التدفق: {}", e),
            })
        })
    }

    fn serialize_for_integrity_hash<T: Serialize>(
        package: &SyncPackage<T>,
        schema: SchemaVersion,
    ) -> AppResult<Vec<u8>> {
        let canonical = serde_json::to_value(package).map_err(|e| {
            AppError::Validation(crate::errors::ValidationError::InvalidFormat {
                field: "sync_package".into(),
                message: format!("تعذّر تحويل الحزمة للتحقق من السلامة: {}", e),
            })
        })?;
        if schema >= SchemaVersion::V2 {
            canonical_bytes_for_integrity(&canonical).map_err(|e| {
                AppError::Validation(crate::errors::ValidationError::InvalidFormat {
                    field: "sync_package".into(),
                    message: format!("تعذّر التسلسل القانوني للحزمة: {}", e),
                })
            })
        } else {
            // SEC-007 (ADR-0047): V1 packages are rejected before this point by
            // `verify_integrity` (missing V2 metadata). The legacy non-canonical
            // V1 serialization branch is dead code — fail closed instead of
            // resurrecting legacy semantics.
            Err(AppError::Validation(
                crate::errors::ValidationError::InvalidFormat {
                    field: "schema_version".into(),
                    message: "حزمة V1 غير مدعومة — الإصدار V2 (Ed25519) إلزامي".into(),
                },
            ))
        }
    }

    fn verify_integrity<T: Serialize>(package: &SyncPackage<T>) -> AppResult<()> {
        let expected_hash = package.metadata.integrity_hash.as_deref().ok_or_else(|| {
            AppError::Validation(crate::errors::ValidationError::InvalidFormat {
                field: "integrity_hash".into(),
                message: "الحزمة بدون تجزئة سلامة — V2 إلزامي (متطلبات الأمان)".into(),
            })
        })?;

        let schema = package.metadata.schema_version;
        let canonical_bytes = Self::serialize_for_integrity_hash(package, schema)?;
        let actual_hash = Sha256PackageHasher.hash(&canonical_bytes)?;
        if actual_hash != expected_hash {
            return Err(AppError::Validation(
                crate::errors::ValidationError::InvalidFormat {
                    field: "integrity_hash".into(),
                    message: "فشل التحقق من سلامة الحزمة (hash mismatch)".into(),
                },
            ));
        }
        Ok(())
    }

    fn verify_signature<T: Serialize>(package: &SyncPackage<T>) -> AppResult<()> {
        // SEC-007 (ADR-0047): signature_version = 2 (Ed25519) is MANDATORY.
        // The HMAC/trusted-signer path and the "missing signature for legacy
        // compatibility" allowance are removed. The Ed25519 signature itself is
        // verified by `SyncPackageIdentityVerificationService` in the import
        // pipeline (B4, RFC 2026-08-04 §3.10), which owns Identity Store access;
        // this deserializer enforces the metadata contract fail-closed.
        if package.metadata.signature_version != Some(SIGNATURE_VERSION_ED25519) {
            return Err(AppError::Validation(
                crate::errors::ValidationError::InvalidFormat {
                    field: "signature_version".into(),
                    message: "إصدار التوقيع غير مدعوم — يجب أن يكون V2 (Ed25519)".into(),
                },
            ));
        }
        if package
            .metadata
            .signature
            .as_deref()
            .is_none_or(str::is_empty)
        {
            return Err(AppError::Validation(
                crate::errors::ValidationError::InvalidFormat {
                    field: "signature".into(),
                    message: "الحزمة بدون توقيع — V2 إلزامي (متطلبات الأمان)".into(),
                },
            ));
        }
        Ok(())
    }

    pub fn monthly_summary_from_reader<R: std::io::Read>(
        reader: R,
    ) -> AppResult<SyncPackage<MonthlySummaryExportDataset>> {
        let package = Self::parse_json_from_reader(reader)?;
        Self::verify_integrity(&package)?;
        Self::verify_signature(&package)?;
        Ok(package)
    }

    pub fn products_from_reader<R: std::io::Read>(
        reader: R,
    ) -> AppResult<SyncPackage<ProductsExportDataset>> {
        let package = Self::parse_json_from_reader(reader)?;
        Self::verify_integrity(&package)?;
        Self::verify_signature(&package)?;
        Ok(package)
    }

    pub fn daily_report_from_reader<R: std::io::Read>(
        reader: R,
    ) -> AppResult<SyncPackage<DailyReportExportDataset>> {
        let package = Self::parse_json_from_reader(reader)?;
        Self::verify_integrity(&package)?;
        Self::verify_signature(&package)?;
        Ok(package)
    }

    pub fn stock_movements_from_reader<R: std::io::Read>(
        reader: R,
    ) -> AppResult<SyncPackage<StockMovementsExportDataset>> {
        let package = Self::parse_json_from_reader(reader)?;
        Self::verify_integrity(&package)?;
        Self::verify_signature(&package)?;
        Ok(package)
    }

    pub fn unit_node_package_from_reader<R: std::io::Read>(
        reader: R,
    ) -> AppResult<SyncPackage<UnitNodePackage>> {
        let package = Self::parse_json_from_reader(reader)?;
        Self::verify_integrity(&package)?;
        Self::verify_signature(&package)?;
        Ok(package)
    }

    pub fn trust_package_from_reader<R: std::io::Read>(
        reader: R,
    ) -> AppResult<SyncPackage<TrustPackagePayload>> {
        let package = Self::parse_json_from_reader(reader)?;
        Self::verify_integrity(&package)?;
        Self::verify_signature(&package)?;
        Ok(package)
    }

    pub fn registry_package_from_reader<R: std::io::Read>(
        reader: R,
    ) -> AppResult<SyncPackage<RegistryPackagePayload>> {
        let package = Self::parse_json_from_reader(reader)?;
        Self::verify_integrity(&package)?;
        Self::verify_signature(&package)?;
        Ok(package)
    }

    pub fn identity_access_from_reader<R: std::io::Read>(
        reader: R,
    ) -> AppResult<SyncPackage<IdentityAccessPayload>> {
        let package = Self::parse_json_from_reader(reader)?;
        Self::verify_integrity(&package)?;
        Self::verify_signature(&package)?;
        Ok(package)
    }

    pub fn admin_access_from_reader<R: std::io::Read>(
        reader: R,
    ) -> AppResult<SyncPackage<crate::models::AdminAccessPayload>> {
        let package = Self::parse_json_from_reader(reader)?;
        Self::verify_integrity(&package)?;
        Self::verify_signature(&package)?;
        Ok(package)
    }
}
