//! Helpers for reading encrypted sync packages.
//!
//! ## Memory-aware model
//!
//! The preferred entry points are the `*_from_file` variants which open the
//! file with a `BufReader` and pipe directly through `decrypt_stream`
//! into a temporary file. This avoids loading the full encrypted or
//! decrypted payload into RAM.
//!
//! Peak RAM: Bounded by buffer sizes and JSON deserialization, not file size.

use crate::application::sync::SyncPackage;
use crate::application::usecases::exports::types::{
    DailyReportExportDataset, MonthlySummaryExportDataset, ProductsExportDataset,
    StockMovementsExportDataset,
};
use crate::errors::{AppError, AppResult};
use crate::models::UnitNodePackage;

use super::package_deserializer::SerdeJsonSyncPackageDeserializer;
use crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use crate::infrastructure::MAX_IMPORT_SIZE;
use std::io::BufReader;
use std::path::Path;

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Decrypt an encrypted file into a temporary plaintext file.
///
/// Memory-aware decryption of an encrypted file into a temporary plaintext file.
/// Decryption uses buffered IO to keep memory usage predictable.
fn decrypt_encrypted_file_to_temp(
    path: &Path,
    crypto_port: &AgeFileEncryptionProvider,
) -> AppResult<tempfile::NamedTempFile> {
    let metadata = std::fs::metadata(path).map_err(|e| {
        AppError::Internal(format!(
            "Failed to read metadata for '{}': {}",
            path.display(),
            e
        ))
    })?;

    if metadata.len() > MAX_IMPORT_SIZE {
        return Err(AppError::Validation(
            crate::errors::ValidationError::InvalidFormat {
                field: "file_size".into(),
                message: format!(
                    "حجم الملف كبير جدًا ({} MB). الحد الأقصى المسموح به هو {} MB.",
                    metadata.len() / 1024 / 1024,
                    MAX_IMPORT_SIZE / 1024 / 1024
                ),
            },
        ));
    }

    let file = std::fs::File::open(path).map_err(|e| {
        AppError::Internal(format!(
            "Failed to open encrypted file '{}': {}",
            path.display(),
            e
        ))
    })?;
    let mut reader = BufReader::new(file);

    let temp_plaintext = tempfile::Builder::new()
        .prefix("grpc_sync_decrypt_")
        .suffix(".json_tmp")
        .tempfile()
        .map_err(|e| AppError::Internal(format!("Failed to create decrypt temp file: {}", e)))?;

    {
        let mut writer = std::io::BufWriter::new(temp_plaintext.as_file());
        crypto_port
            .decrypt_stream(&mut reader, &mut writer)
            .map_err(|e| AppError::Internal(format!("Decryption failed: {}", e)))?;
        use std::io::Write;
        writer
            .flush()
            .map_err(|e| AppError::Internal(format!("Flush decrypt temp: {}", e)))?;
    }

    Ok(temp_plaintext)
}

// ─────────────────────────────────────────────────────────────────────────────
// File-path entry points (preferred — Memory-aware buffered reading)
// ─────────────────────────────────────────────────────────────────────────────

pub fn read_monthly_summary_package_from_file(
    path: &Path,
    crypto_port: &AgeFileEncryptionProvider,
) -> AppResult<SyncPackage<MonthlySummaryExportDataset>> {
    let temp_plaintext = decrypt_encrypted_file_to_temp(path, crypto_port)?;
    let file = std::fs::File::open(temp_plaintext.path())
        .map_err(|e| AppError::Internal(format!("Failed to re-open decrypt temp: {}", e)))?;
    SerdeJsonSyncPackageDeserializer::monthly_summary_from_reader(std::io::BufReader::new(file))
}

pub fn read_products_package_from_file(
    path: &Path,
    crypto_port: &AgeFileEncryptionProvider,
) -> AppResult<SyncPackage<ProductsExportDataset>> {
    let temp_plaintext = decrypt_encrypted_file_to_temp(path, crypto_port)?;
    let file = std::fs::File::open(temp_plaintext.path())
        .map_err(|e| AppError::Internal(format!("Failed to re-open decrypt temp: {}", e)))?;
    SerdeJsonSyncPackageDeserializer::products_from_reader(std::io::BufReader::new(file))
}

pub fn read_daily_report_package_from_file(
    path: &Path,
    crypto_port: &AgeFileEncryptionProvider,
) -> AppResult<SyncPackage<DailyReportExportDataset>> {
    let temp_plaintext = decrypt_encrypted_file_to_temp(path, crypto_port)?;
    let file = std::fs::File::open(temp_plaintext.path())
        .map_err(|e| AppError::Internal(format!("Failed to re-open decrypt temp: {}", e)))?;
    SerdeJsonSyncPackageDeserializer::daily_report_from_reader(std::io::BufReader::new(file))
}

pub fn read_stock_movements_package_from_file(
    path: &Path,
    crypto_port: &AgeFileEncryptionProvider,
) -> AppResult<SyncPackage<StockMovementsExportDataset>> {
    let temp_plaintext = decrypt_encrypted_file_to_temp(path, crypto_port)?;
    let file = std::fs::File::open(temp_plaintext.path())
        .map_err(|e| AppError::Internal(format!("Failed to re-open decrypt temp: {}", e)))?;
    SerdeJsonSyncPackageDeserializer::stock_movements_from_reader(std::io::BufReader::new(file))
}

pub fn read_unit_node_package_from_file(
    path: &Path,
    crypto_port: &AgeFileEncryptionProvider,
) -> AppResult<SyncPackage<UnitNodePackage>> {
    let temp_plaintext = decrypt_encrypted_file_to_temp(path, crypto_port)?;
    let file = std::fs::File::open(temp_plaintext.path())
        .map_err(|e| AppError::Internal(format!("Failed to re-open decrypt temp: {}", e)))?;
    SerdeJsonSyncPackageDeserializer::unit_node_package_from_reader(std::io::BufReader::new(file))
}

pub fn read_trust_package_from_file(
    path: &Path,
    crypto_port: &AgeFileEncryptionProvider,
) -> AppResult<
    SyncPackage<crate::application::usecases::sync::import_trust_package::TrustPackagePayload>,
> {
    let temp_plaintext = decrypt_encrypted_file_to_temp(path, crypto_port)?;
    let file = std::fs::File::open(temp_plaintext.path())
        .map_err(|e| AppError::Internal(format!("Failed to re-open decrypt temp: {}", e)))?;
    SerdeJsonSyncPackageDeserializer::trust_package_from_reader(std::io::BufReader::new(file))
}

pub fn read_registry_package_from_file(
    path: &Path,
    crypto_port: &AgeFileEncryptionProvider,
) -> AppResult<
    SyncPackage<crate::application::usecases::sync::import_registry_package::RegistryPackagePayload>,
> {
    let temp_plaintext = decrypt_encrypted_file_to_temp(path, crypto_port)?;
    let file = std::fs::File::open(temp_plaintext.path())
        .map_err(|e| AppError::Internal(format!("Failed to re-open decrypt temp: {}", e)))?;
    SerdeJsonSyncPackageDeserializer::registry_package_from_reader(std::io::BufReader::new(file))
}

pub fn read_identity_access_package_from_file(
    path: &Path,
    crypto_port: &AgeFileEncryptionProvider,
) -> AppResult<SyncPackage<crate::models::IdentityAccessPayload>> {
    let temp_plaintext = decrypt_encrypted_file_to_temp(path, crypto_port)?;
    let file = std::fs::File::open(temp_plaintext.path())
        .map_err(|e| AppError::Internal(format!("Failed to re-open decrypt temp: {}", e)))?;
    SerdeJsonSyncPackageDeserializer::identity_access_from_reader(std::io::BufReader::new(file))
}

// Note: Byte-slice entry points were removed to enforce the streaming model.
