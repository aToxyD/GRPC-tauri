//! Default export persistence: age encryption + local filesystem.

use crate::application::ports::file_output::{ExportFileSink, ExportPayloadEncryptor};
use crate::errors::{AppError, AppResult};
use crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use std::io::{Read, Write};

/// Encrypts export payloads using the app’s simple age encryption file format.
#[derive(Clone, Default)]
pub struct AgeExportEncryptor {
    crypto_port: AgeFileEncryptionProvider,
}

impl AgeExportEncryptor {
    pub fn new() -> Self {
        Self {
            crypto_port: AgeFileEncryptionProvider::new(),
        }
    }
}

impl ExportPayloadEncryptor for AgeExportEncryptor {
    fn encrypt_for_storage(&self, plaintext: &[u8]) -> AppResult<Vec<u8>> {
        self.crypto_port.encrypt_data(plaintext)
    }

    fn encrypt_stream(&self, input: &mut dyn Read, output: &mut dyn Write) -> AppResult<()> {
        self.crypto_port.encrypt_stream(input, output)
    }
}

/// Writes bytes to a local path.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalFilesystemExportSink;

impl ExportFileSink for LocalFilesystemExportSink {
    fn write_all(&self, path: &str, data: &[u8]) -> AppResult<()> {
        std::fs::write(path, data).map_err(|e| AppError::Internal(format!("فشل حفظ الملف: {}", e)))
    }
}
