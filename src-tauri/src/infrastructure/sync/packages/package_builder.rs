//! Builder for creating encrypted sync packages.

use crate::application::ports::file_output::ExportPayloadEncryptor;
use crate::application::sync::SyncPackage;
use crate::errors::AppResult;

use super::integrity::PackageHasher;
use super::package_serializer::SyncPackageSerializer;
use super::signing::PackageSigner;
use crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default)]
pub struct PackageBuilder;

impl PackageBuilder {
    pub fn new() -> Self {
        Self
    }

    /// High-level streaming build that writes directly to an encrypted file.
    ///
    /// Memory-aware export that streams the encrypted payload directly to a file.
    /// Includes integrity hash and signature calculation using canonical JSON.
    pub fn build_encrypted_stream_path<T, S, G>(
        &self,
        package: &SyncPackage<T>,
        _serializer: &S,
        signer: &G,
        crypto_port: &AgeFileEncryptionProvider,
        target_path: &Path,
    ) -> AppResult<()>
    where
        T: serde::Serialize,
        S: SyncPackageSerializer,
        G: PackageSigner,
    {
        // 1. Calculate integrity hash and signature (First Pass)
        // We use the strict canonical JSON format (ADR-0009) to ensure the hash
        // matches what the importer will calculate.

        let mut package_val = serde_json::to_value(package).map_err(|e| {
            crate::errors::AppError::Internal(format!("Failed to convert package to Value: {}", e))
        })?;

        // Pass A: Calculate Integrity Hash
        // Remove hash and signature before hashing
        let canonical_for_hash = super::canonical_json::canonical_bytes_for_integrity(&package_val)?;
        let hash = super::integrity::Sha256PackageHasher.hash(&canonical_for_hash)?;

        // Update value with hash
        if let Some(meta) = package_val
            .get_mut("metadata")
            .and_then(|m| m.as_object_mut())
        {
            meta.insert(
                "integrity_hash".to_string(),
                serde_json::Value::String(hash.clone()),
            );
        }

        // Pass B: Calculate Signature
        // Sign includes integrity_hash but not signature itself
        let canonical_for_sig = super::canonical_json::canonical_bytes_for_signature(&package_val)?;
        let signature = signer.sign(&canonical_for_sig)?;

        // Update value with signature
        if let Some(meta) = package_val
            .get_mut("metadata")
            .and_then(|m| m.as_object_mut())
        {
            meta.insert(
                "signature".to_string(),
                serde_json::Value::String(signature),
            );
        }

        // 2. Create a temp file for the final plaintext JSON
        let mut temp_plaintext = tempfile::Builder::new()
            .prefix("grpc_sync_export_")
            .suffix(".json_tmp")
            .tempfile()
            .map_err(|e| {
                crate::errors::AppError::Internal(format!(
                    "Failed to create export temp file: {}",
                    e
                ))
            })?;

        // 3. Serialize FINAL JSON into the temp file (buffered)
        {
            let mut writer = std::io::BufWriter::new(temp_plaintext.as_file_mut());
            // We write the canonical bytes to disk for trial-deployment consistency
            let final_bytes = super::canonical_json::canonical_json_bytes(&package_val)?;
            writer.write_all(&final_bytes).map_err(|e| {
                crate::errors::AppError::Internal(format!("Write final canonical JSON: {}", e))
            })?;
            writer.flush().map_err(|e| {
                crate::errors::AppError::Internal(format!("Flush export temp: {}", e))
            })?;
        }

        // 4. Reset temp file pointer
        use std::io::Seek;
        temp_plaintext
            .as_file_mut()
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|e| crate::errors::AppError::Internal(format!("Seek export temp: {}", e)))?;

        // 5. Open target file
        let target_file = std::fs::File::create(target_path).map_err(|e| {
            crate::errors::AppError::Internal(format!(
                "Failed to create target file '{}': {}",
                target_path.display(),
                e
            ))
        })?;
        let mut target_writer = std::io::BufWriter::new(target_file);

        // 6. Stream from temp plaintext through age encryptor to target file
        crypto_port.encrypt_stream(&mut temp_plaintext, &mut target_writer)?;

        Ok(())
    }
}

impl ExportPayloadEncryptor for PackageBuilder {
    fn encrypt_for_storage(&self, plaintext: &[u8]) -> AppResult<Vec<u8>> {
        let crypto_port = AgeFileEncryptionProvider::new();
        crypto_port.encrypt_data(plaintext)
    }

    fn encrypt_stream(&self, input: &mut dyn Read, output: &mut dyn Write) -> AppResult<()> {
        let crypto_port = AgeFileEncryptionProvider::new();
        crypto_port.encrypt_stream(input, output)
    }
}
