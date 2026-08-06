//! Node key store — x25519-protected node signing keys.
//!
//! RFC 2026-08-04-node-identity-trust §3.5 / ADR-0038 §5 / ADR-0039 §3.
//!
//! WILAYA/UNIT private signing keys are node-managed secrets: they never leave
//! the node and are encrypted at rest with `age::x25519` bound to `GRPC_APP_KEY`
//! (ADR-0039 §3). `.adminkey` is the ONLY portable secret (`age::scrypt`), see
//! `adminkey_provider.rs`. This store reuses `AgeFileEncryptionProvider` and
//! must never introduce `age::scrypt`.

use std::path::PathBuf;

use crate::errors::{AppError, AppResult};
use crate::infrastructure::security::AgeFileEncryptionProvider;

/// File holding the encrypted node signing key (Ed25519 secret key, 32 bytes).
pub const NODE_KEY_FILE_NAME: &str = "node_identity.key";

/// Persists and loads the node Ed25519 signing key, encrypted at rest with
/// `age::x25519` (GRPC_APP_KEY). The plaintext key exists only in memory.
#[derive(Debug, Clone)]
pub struct NodeKeyStore {
    data_dir: PathBuf,
}

impl NodeKeyStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    pub fn file_path(&self) -> PathBuf {
        self.data_dir.join(NODE_KEY_FILE_NAME)
    }

    pub fn exists(&self) -> bool {
        self.file_path().is_file()
    }

    /// Persist the node signing key atomically (write + rename).
    pub fn write(&self, secret_key: &[u8; 32]) -> AppResult<()> {
        let encrypted = AgeFileEncryptionProvider::new().encrypt_data(secret_key)?;
        std::fs::create_dir_all(&self.data_dir).map_err(AppError::Io)?;
        let path = self.file_path();
        let tmp = path.with_extension("key.tmp");
        std::fs::write(&tmp, encrypted).map_err(AppError::Io)?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            AppError::Io(e)
        })?;
        Ok(())
    }

    /// Load the node signing key. Fails if the file is missing or the key
    /// material is malformed.
    pub fn read(&self) -> AppResult<[u8; 32]> {
        let path = self.file_path();
        let encrypted = std::fs::read(&path).map_err(|e| {
            AppError::Internal(format!(
                "Failed to read node signing key at {}: {e}",
                path.display()
            ))
        })?;
        let plaintext = AgeFileEncryptionProvider::new().decrypt_data(&encrypted)?;
        plaintext.try_into().map_err(|v: Vec<u8>| {
            AppError::Internal(format!(
                "Decrypted node signing key has invalid length {} (expected 32)",
                v.len()
            ))
        })
    }

    /// Load the key if present (`None` when the node has not been provisioned).
    pub fn read_if_exists(&self) -> AppResult<Option<[u8; 32]>> {
        if self.exists() {
            Ok(Some(self.read()?))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        let key = [42u8; 32];
        store.write(&key).unwrap();
        assert!(store.exists());
        assert_eq!(store.read().unwrap(), key);
    }

    #[test]
    fn read_if_exists_returns_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        assert_eq!(store.read_if_exists().unwrap(), None);
    }

    #[test]
    fn read_rejects_malformed_key_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(NODE_KEY_FILE_NAME), "garbage").unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        assert!(store.read().is_err());
    }
}
