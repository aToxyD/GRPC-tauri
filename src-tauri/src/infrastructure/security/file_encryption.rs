use crate::errors::{AppError, AppResult};
use std::io::{Read, Write};
use std::str::FromStr;

/// Simple file encryption provider using age x25519.
#[derive(Clone, Copy, Debug, Default)]
pub struct AgeFileEncryptionProvider;

impl AgeFileEncryptionProvider {
    pub fn new() -> Self {
        Self
    }

    fn get_identity() -> AppResult<age::x25519::Identity> {
        let key_str = crate::infrastructure::security::resolve_app_encryption_key()?;
        age::x25519::Identity::from_str(&key_str).map_err(|e| {
            AppError::Internal(format!("Invalid x25519 identity in GRPC_APP_KEY: {}", e))
        })
    }

    /// Encrypt a single payload fully into memory
    pub fn encrypt_data(&self, data: &[u8]) -> AppResult<Vec<u8>> {
        let identity = Self::get_identity()?;
        let recipient = identity.to_public();

        let encryptor =
            age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
                .ok()
                .ok_or_else(|| {
                    AppError::Internal("Failed to create encryptor with recipient".into())
                })?;

        let mut encrypted = vec![];
        let mut writer = encryptor
            .wrap_output(&mut encrypted)
            .map_err(|e| AppError::Internal(format!("Failed to start encryption: {}", e)))?;
        writer
            .write_all(data)
            .map_err(|e| AppError::Internal(format!("Failed to encrypt data: {}", e)))?;
        writer
            .finish()
            .map_err(|e| AppError::Internal(format!("Failed to finish encryption: {}", e)))?;

        Ok(encrypted)
    }

    pub fn decrypt_data(&self, encrypted_data: &[u8]) -> AppResult<Vec<u8>> {
        let identity = Self::get_identity()?;

        let decryptor = age::Decryptor::new(encrypted_data)
            .map_err(|e| AppError::Internal(format!("Failed to create decryptor: {}", e)))?;

        let mut decrypted = vec![];

        let mut reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
            .map_err(|e| AppError::Internal(format!("Failed to start decryption: {}", e)))?;

        std::io::copy(&mut reader, &mut decrypted)
            .map_err(|e| AppError::Internal(format!("Failed to decrypt data: {}", e)))?;

        Ok(decrypted)
    }

    pub fn encrypt_stream(&self, input: &mut dyn Read, output: &mut dyn Write) -> AppResult<()> {
        let identity = Self::get_identity()?;
        let recipient = identity.to_public();

        let encryptor =
            age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
                .ok()
                .ok_or_else(|| {
                    AppError::Internal("Failed to create encryptor with recipient".into())
                })?;

        let mut writer = encryptor
            .wrap_output(output)
            .map_err(|e| AppError::Internal(format!("Failed to start encryption: {}", e)))?;

        std::io::copy(input, &mut writer)
            .map_err(|e| AppError::Internal(format!("Failed to encrypt stream: {}", e)))?;
        writer
            .finish()
            .map_err(|e| AppError::Internal(format!("Failed to finish encryption: {}", e)))?;

        Ok(())
    }

    pub fn decrypt_stream(&self, input: &mut dyn Read, output: &mut dyn Write) -> AppResult<()> {
        let identity = Self::get_identity()?;

        let decryptor = age::Decryptor::new(input)
            .map_err(|e| AppError::Internal(format!("Failed to create decryptor: {}", e)))?;

        let mut reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
            .map_err(|e| AppError::Internal(format!("Failed to start decryption: {}", e)))?;

        std::io::copy(&mut reader, output)
            .map_err(|e| AppError::Internal(format!("Failed to decrypt stream: {}", e)))?;

        Ok(())
    }
}
