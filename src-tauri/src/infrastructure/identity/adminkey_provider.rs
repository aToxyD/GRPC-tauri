//! `.adminkey` provider — the ONLY legal user of `age::scrypt`.
//!
//! ADR-0039 / Rule 38 (amended): `age::scrypt` is permitted EXCLUSIVELY for
//! portable operator key material (`.adminkey`). This file is the sole scrypt
//! site in the codebase; node-managed secrets (WILAYA/UNIT keys) use
//! `age::x25519` (`node_key_store.rs`).
//!
//! `.adminkey` is self-contained and portable: it depends on no node-local
//! secret, no network, no Root, and no WILAYA key to decrypt — only the
//! operator passphrase.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::domain::identity::AdminKeyFile;
use crate::errors::{AppError, AppResult};

/// Data subdirectory for operator key material under `dirs::data_dir()`.
pub const GRPC_DATA_DIR: &str = "GRPC";

/// `.adminkey` file name (portable operator key).
pub const ADMINKEY_FILE_NAME: &str = ".adminkey";

/// Reads/writes the portable `.adminkey` and encrypts/decrypts its private key
/// with the operator passphrase via `age::scrypt`.
#[derive(Debug, Clone)]
pub struct AdminKeyProvider {
    data_dir: PathBuf,
}

impl AdminKeyProvider {
    /// Resolve the on-disk GRPC data directory (`dirs::data_dir()/GRPC`).
    pub fn default_data_dir() -> AppResult<PathBuf> {
        dirs::data_dir()
            .map(|dir| dir.join(GRPC_DATA_DIR))
            .ok_or_else(|| AppError::Internal("Cannot resolve system data directory".into()))
    }

    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    pub fn file_path(&self) -> PathBuf {
        self.data_dir.join(ADMINKEY_FILE_NAME)
    }

    pub fn exists(&self) -> bool {
        self.file_path().is_file()
    }

    /// Persist an `.adminkey` file atomically (write + rename).
    pub fn write(&self, file: &AdminKeyFile) -> AppResult<()> {
        file.validate()?;
        let json = serde_json::to_string_pretty(file)
            .map_err(|e| AppError::Internal(format!("Failed to serialize .adminkey: {e}")))?;
        std::fs::create_dir_all(&self.data_dir).map_err(AppError::Io)?;
        let path = self.file_path();
        let tmp = path.with_extension("adminkey.tmp");
        std::fs::write(&tmp, json).map_err(AppError::Io)?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            AppError::Io(e)
        })?;
        Ok(())
    }

    /// Load and structurally validate the `.adminkey` file.
    pub fn read(&self) -> AppResult<AdminKeyFile> {
        let path = self.file_path();
        let json = std::fs::read_to_string(&path).map_err(|e| {
            AppError::Internal(format!(
                "Failed to read .adminkey at {}: {e}",
                path.display()
            ))
        })?;
        let file: AdminKeyFile = serde_json::from_str(&json)
            .map_err(|e| AppError::FileFormat(format!("Malformed .adminkey file: {e}")))?;
        file.validate()?;
        Ok(file)
    }

    /// Encrypt a 32-byte Ed25519 secret key with the operator passphrase.
    pub fn encrypt_private_key(
        &self,
        secret_key: &[u8; 32],
        passphrase: &str,
    ) -> AppResult<Vec<u8>> {
        encrypt_scrypt(passphrase, secret_key)
    }

    /// Decrypt the `.adminkey` secret key with the operator passphrase.
    ///
    /// Wrong passphrase yields an error (fail-closed); the challenge is
    /// consumed regardless by the caller.
    pub fn decrypt_private_key(&self, encrypted: &[u8], passphrase: &str) -> AppResult<[u8; 32]> {
        let plaintext = decrypt_scrypt(passphrase, encrypted)?;
        plaintext.try_into().map_err(|v: Vec<u8>| {
            AppError::Internal(format!(
                "Decrypted .adminkey secret key has invalid length {} (expected 32)",
                v.len()
            ))
        })
    }
}

/// `age::scrypt` encryption (ADR-0039 — allowed only in this module).
fn encrypt_scrypt(passphrase: &str, data: &[u8]) -> AppResult<Vec<u8>> {
    use age::secrecy::SecretString;
    let recipient = age::scrypt::Recipient::new(SecretString::from(passphrase.to_owned()));

    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .ok()
            .ok_or_else(|| AppError::Internal("Failed to create scrypt encryptor".into()))?;

    let mut encrypted = vec![];
    let mut writer = encryptor
        .wrap_output(&mut encrypted)
        .map_err(|e| AppError::Internal(format!("Failed to start scrypt encryption: {e}")))?;
    writer
        .write_all(data)
        .map_err(|e| AppError::Internal(format!("Failed to write scrypt payload: {e}")))?;
    writer
        .finish()
        .map_err(|e| AppError::Internal(format!("Failed to finish scrypt encryption: {e}")))?;
    Ok(encrypted)
}

/// `age::scrypt` decryption (ADR-0039 — allowed only in this module).
fn decrypt_scrypt(passphrase: &str, encrypted: &[u8]) -> AppResult<Vec<u8>> {
    use age::secrecy::SecretString;
    let identity = age::scrypt::Identity::new(SecretString::from(passphrase.to_owned()));

    let decryptor = age::Decryptor::new(encrypted)
        .map_err(|e| AppError::Internal(format!("Invalid scrypt ciphertext: {e}")))?;
    let mut reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|e| {
            AppError::Internal(format!(
                "Failed to decrypt .adminkey (wrong passphrase?): {e}"
            ))
        })?;
    let mut plaintext = vec![];
    std::io::copy(&mut reader, &mut plaintext)
        .map_err(|e| AppError::Internal(format!("Failed to read decrypted .adminkey: {e}")))?;
    Ok(plaintext)
}

impl AdminKeyProvider {
    /// Convenience: read the file if present.
    pub fn read_if_exists(&self) -> AppResult<Option<AdminKeyFile>> {
        if self.exists() {
            Ok(Some(self.read()?))
        } else {
            Ok(None)
        }
    }

    /// Ensure the data directory exists.
    pub fn ensure_dir(&self) -> AppResult<()> {
        std::fs::create_dir_all(&self.data_dir).map_err(AppError::Io)
    }
}

/// Assert the data directory path is well-formed for diagnostics.
pub fn adminkey_path_description(data_dir: &Path) -> String {
    data_dir.join(ADMINKEY_FILE_NAME).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::SIGNATURE_VERSION_ED25519;
    use crate::domain::identity::{
        AdminKeyFile, IdentityCertificate, IDENTITY_ALGORITHM_PROFILE_ED25519,
    };
    use crate::domain::identity::{CredentialStatus, Ed25519CertificateSignature, SubjectType};
    use uuid::Uuid;

    fn sample_file() -> AdminKeyFile {
        let certificate = IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_type: SubjectType::Admin,
            subject_id: Uuid::new_v4(),
            issuer_identity_id: Some(Uuid::new_v4()),
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: vec![1u8; 32],
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: Some(1),
            signature: Some(Ed25519CertificateSignature::from_bytes([4u8; 64])),
        };
        AdminKeyFile {
            format_version: crate::domain::identity::ADMINKEY_FORMAT_VERSION,
            algorithm_version: IDENTITY_ALGORITHM_PROFILE_ED25519,
            certificate,
            encrypted_private_key: vec![0u8, 1, 2, 3],
        }
    }

    #[test]
    fn scrypt_roundtrip_with_correct_passphrase() {
        let provider = AdminKeyProvider::new(PathBuf::from("/tmp/unused"));
        let secret = [7u8; 32];
        let encrypted = provider
            .encrypt_private_key(&secret, "correct horse battery staple")
            .unwrap();
        let decrypted = provider
            .decrypt_private_key(&encrypted, "correct horse battery staple")
            .unwrap();
        assert_eq!(decrypted, secret);
        assert_ne!(&encrypted[..], &secret[..]);
    }

    #[test]
    fn scrypt_wrong_passphrase_fails_closed() {
        let provider = AdminKeyProvider::new(PathBuf::from("/tmp/unused"));
        let secret = [7u8; 32];
        let encrypted = provider
            .encrypt_private_key(&secret, "right passphrase")
            .unwrap();
        assert!(provider
            .decrypt_private_key(&encrypted, "wrong passphrase")
            .is_err());
    }

    #[test]
    fn write_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let provider = AdminKeyProvider::new(dir.path().to_path_buf());
        let file = sample_file();
        provider.write(&file).unwrap();
        assert!(provider.exists());
        let loaded = provider.read().unwrap();
        assert_eq!(loaded, file);
    }

    #[test]
    fn read_rejects_malformed_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(ADMINKEY_FILE_NAME), "not-json").unwrap();
        let provider = AdminKeyProvider::new(dir.path().to_path_buf());
        assert!(provider.read().is_err());
    }
}
