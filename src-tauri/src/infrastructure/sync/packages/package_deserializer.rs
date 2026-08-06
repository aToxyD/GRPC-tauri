//! Decode plaintext interchange bytes produced by [`super::SerdeJsonSyncPackageSerializer`](super::SerdeJsonSyncPackageSerializer).

use crate::application::sync::{SchemaVersion, SyncPackage};
use crate::application::usecases::exports::types::DailyReportExportDataset;
use crate::application::usecases::exports::types::{
    MonthlySummaryExportDataset, ProductsExportDataset, StockMovementsExportDataset,
};
use crate::application::usecases::sync::import_registry_package::RegistryPackagePayload;
use crate::application::usecases::sync::import_trust_package::TrustPackagePayload;
use crate::errors::{AppError, AppResult};
use crate::infrastructure::MAX_IMPORT_SIZE;
use crate::models::UnitNodePackage;
use serde::{de::DeserializeOwned, Serialize};

use super::canonical_json::{canonical_bytes_for_integrity, canonical_bytes_for_signature};
use super::integrity::{PackageHasher, Sha256PackageHasher};
use super::signing::{HmacPackageSigner, PackageVerifier};

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
            // Legacy V1: strip fields then plain serde bytes (no canonicalization).
            let mut legacy = canonical;
            if let Some(meta_obj) = legacy
                .get_mut("metadata")
                .and_then(|m| m.as_object_mut())
            {
                meta_obj.remove("integrity_hash");
                meta_obj.remove("signature");
            }
            serde_json::to_vec(&legacy).map_err(|e| {
                AppError::Validation(crate::errors::ValidationError::InvalidFormat {
                    field: "sync_package".into(),
                    message: format!("تعذّر تسلسل الحزمة للتحقق من السلامة: {}", e),
                })
            })
        }
    }

    fn serialize_for_signature<T: Serialize>(
        package: &SyncPackage<T>,
        schema: SchemaVersion,
    ) -> AppResult<Vec<u8>> {
        let canonical = serde_json::to_value(package).map_err(|e| {
            AppError::Validation(crate::errors::ValidationError::InvalidFormat {
                field: "sync_package".into(),
                message: format!("تعذّر تحويل الحزمة للتحقق من التوقيع: {}", e),
            })
        })?;
        if schema >= SchemaVersion::V2 {
            canonical_bytes_for_signature(&canonical).map_err(|e| {
                AppError::Validation(crate::errors::ValidationError::InvalidFormat {
                    field: "sync_package".into(),
                    message: format!("تعذّر التسلسل القانوني للتوقيع: {}", e),
                })
            })
        } else {
            // Legacy V1: strip the signature then plain serde bytes.
            let mut legacy = canonical;
            if let Some(meta_obj) = legacy
                .get_mut("metadata")
                .and_then(|m| m.as_object_mut())
            {
                meta_obj.remove("signature");
            }
            serde_json::to_vec(&legacy).map_err(|e| {
                AppError::Validation(crate::errors::ValidationError::InvalidFormat {
                    field: "sync_package".into(),
                    message: format!("تعذّر تسلسل الحزمة للتحقق من التوقيع: {}", e),
                })
            })
        }
    }

    fn verify_integrity<T: Serialize>(package: &SyncPackage<T>) -> AppResult<()> {
        let expected_hash = match package.metadata.integrity_hash.as_deref() {
            Some(h) => h,
            None => return Ok(()), // Allow missing hash for legacy compatibility (V1/Golden fixtures)
        };

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
        // B4 (RFC 2026-08-04 §3.10): signature_version = 2 (Ed25519) signatures are
        // verified by `SyncPackageIdentityVerificationService` in the import pipeline,
        // which owns Identity Store access. This deserializer only handles V1/legacy
        // HMAC signatures during the deprecation window.
        if package.metadata.signature_version
            == Some(crate::domain::identity::SIGNATURE_VERSION_ED25519)
        {
            return Ok(());
        }

        let signature = match package.metadata.signature.as_deref() {
            Some(s) => s,
            None => return Ok(()), // Allow missing signature for legacy compatibility
        };
        let signer_id = package.metadata.source_node_id.trim();
        if !crate::infrastructure::security::is_trusted_signer(signer_id) {
            log::error!(
                "sync signature verification failed: package_id={}, signer_id={}, key_id={}, signature_version={}, reason=SIGNER_NOT_TRUSTED",
                package.metadata.package_id.0,
                signer_id,
                package
                    .metadata
                    .signing_key_id
                    .as_deref()
                    .unwrap_or("default"),
                package
                    .metadata
                    .signature_version
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            );
            return Err(AppError::Validation(
                crate::errors::ValidationError::InvalidFormat {
                    field: "source_node_id".into(),
                    message: "هوية الموقّع غير موثوقة لهذه العقدة".into(),
                },
            ));
        }
        let key_id = package
            .metadata
            .signing_key_id
            .as_deref()
            .unwrap_or("default");
        let accepted = crate::infrastructure::security::resolve_accepted_verification_key_ids();
        if !accepted.iter().any(|k| k == key_id) {
            log::error!(
                "sync signature verification failed: package_id={}, key_id={}, signature_version={}, reason=SIGNING_KEY_NOT_ACCEPTED",
                package.metadata.package_id.0,
                key_id,
                package
                    .metadata
                    .signature_version
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            );
            return Err(AppError::Validation(
                crate::errors::ValidationError::InvalidFormat {
                    field: "signing_key_id".into(),
                    message: "مفتاح التوقيع غير مقبول لهذه العقدة".into(),
                },
            ));
        }
        if crate::infrastructure::security::is_signing_key_deprecated(key_id) {
            log::error!(
                "sync signature verification failed: package_id={}, key_id={}, signature_version={}, reason=SIGNING_KEY_DEPRECATED",
                package.metadata.package_id.0,
                key_id,
                package
                    .metadata
                    .signature_version
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            );
            return Err(AppError::Validation(
                crate::errors::ValidationError::InvalidFormat {
                    field: "signing_key_id".into(),
                    message: "مفتاح التوقيع منتهي الصلاحية (خارج نافذة الإيقاف)".into(),
                },
            ));
        }

        let schema = package.metadata.schema_version;
        let canonical_bytes = Self::serialize_for_signature(package, schema)?;
        let ok = HmacPackageSigner.verify(&canonical_bytes, signature)?;
        if !ok {
            log::error!(
                "sync signature verification failed: package_id={}, key_id={}, signature_version={}, reason=SIGNATURE_INVALID",
                package.metadata.package_id.0,
                key_id,
                package
                    .metadata
                    .signature_version
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            );
            return Err(AppError::Validation(
                crate::errors::ValidationError::InvalidFormat {
                    field: "signature".into(),
                    message: "فشل التحقق من موثوقية الحزمة (signature mismatch)".into(),
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
}
