//! Node key store — x25519-protected node signing keys.
//!
//! RFC 2026-08-04-node-identity-trust §3.5 / ADR-0038 §5 / ADR-0039 §3.
//!
//! WILAYA/UNIT private signing keys are node-managed secrets: they never leave
//! the node and are encrypted at rest with `age::x25519` bound to `GRPC_APP_KEY`
//! (ADR-0039 §3). `.adminkey` is the ONLY portable secret (`age::scrypt`), see
//! `adminkey_provider.rs`. This store reuses `AgeFileEncryptionProvider` and
//! must never introduce `age::scrypt`.
//!
//! Packaged-identity exception (ADR-0044 amendment): in the packaged `.unit`
//! bootstrap flow the WILAYA generates a UNIT Ed25519 keypair IN MEMORY at
//! export time and embeds it inside the encrypted `.unit` package. That UNIT
//! secret is never written to this store on the WILAYA node; it only reaches
//! the UNIT node's own `NodeKeyStore`, through the guarded
//! `install_node_key_matching` install (absent → write, identical → no-op,
//! different/corrupt → fail closed — never overwrite).

use std::path::PathBuf;

use crate::errors::{AppError, AppResult};
use crate::infrastructure::security::AgeFileEncryptionProvider;

/// File holding the encrypted node signing key (Ed25519 secret key, 32 bytes).
pub const NODE_KEY_FILE_NAME: &str = "node_identity.key";

/// File holding a staged (rotated, not yet promoted) node signing key.
pub const NODE_KEY_PENDING_FILE_NAME: &str = "node_identity.key.pending";

/// Persists and loads the node Ed25519 signing key, encrypted at rest with
/// `age::x25519` (GRPC_APP_KEY). The plaintext key exists only in memory.
///
/// Rotation staging: a new key can be written to a PENDING slot without
/// touching the currently active key (`write_pending`). The active key stays
/// authoritative until `promote_pending` atomically replaces it, so a rotation
/// package can be signed with the OLD key and the secret only switches over
/// once the rotation has been finalized.
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

    fn pending_file_path(&self) -> PathBuf {
        self.data_dir.join(NODE_KEY_PENDING_FILE_NAME)
    }

    pub fn exists(&self) -> bool {
        self.file_path().is_file()
    }

    pub fn pending_exists(&self) -> bool {
        self.pending_file_path().is_file()
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

    /// Stage a new node signing key WITHOUT touching the currently active key.
    ///
    /// The staged key is written atomically (write + rename) and only becomes
    /// the active key through `promote_pending`. Re-staging overwrites any
    /// previously staged key.
    pub fn write_pending(&self, secret_key: &[u8; 32]) -> AppResult<()> {
        let encrypted = AgeFileEncryptionProvider::new().encrypt_data(secret_key)?;
        std::fs::create_dir_all(&self.data_dir).map_err(AppError::Io)?;
        let path = self.pending_file_path();
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, encrypted).map_err(AppError::Io)?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            AppError::Io(e)
        })?;
        Ok(())
    }

    /// Promote the staged key: atomically replace the active key with it.
    ///
    /// Idempotent — a no-op (Ok) when no key is staged. The active key is only
    /// replaced here, never in `write_pending`.
    pub fn promote_pending(&self) -> AppResult<()> {
        let pending = self.pending_file_path();
        if !pending.is_file() {
            return Ok(());
        }
        std::fs::rename(&pending, self.file_path()).map_err(AppError::Io)?;
        Ok(())
    }

    /// Discard a staged key (abort path). Best-effort — a missing staged key
    /// is treated as success.
    pub fn discard_pending(&self) -> AppResult<()> {
        match std::fs::remove_file(self.pending_file_path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::Io(e)),
        }
    }

    /// Read the staged key if present (`None` when nothing is staged).
    pub fn read_pending(&self) -> AppResult<Option<[u8; 32]>> {
        if !self.pending_exists() {
            return Ok(None);
        }
        let path = self.pending_file_path();
        let encrypted = std::fs::read(&path).map_err(|e| {
            AppError::Internal(format!(
                "Failed to read staged node signing key at {}: {e}",
                path.display()
            ))
        })?;
        let plaintext = AgeFileEncryptionProvider::new().decrypt_data(&encrypted)?;
        let key = plaintext.try_into().map_err(|v: Vec<u8>| {
            AppError::Internal(format!(
                "Decrypted staged node signing key has invalid length {} (expected 32)",
                v.len()
            ))
        })?;
        Ok(Some(key))
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

    /// Derive the node's public key from the stored signing key without
    /// exposing the secret. `None` when the node is not provisioned.
    pub fn node_public_key(&self) -> AppResult<Option<Vec<u8>>> {
        use crate::domain::identity::IdentitySigner;
        use crate::infrastructure::security::Ed25519SigningProvider;
        Ok(self
            .read_if_exists()?
            .map(|secret| Ed25519SigningProvider::new(secret).public_key()))
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
    fn node_public_key_matches_derived_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        assert_eq!(store.node_public_key().unwrap(), None);
        let secret = [42u8; 32];
        store.write(&secret).unwrap();
        let derived = {
            use crate::domain::identity::IdentitySigner;
            crate::infrastructure::security::Ed25519SigningProvider::new(secret).public_key()
        };
        assert_eq!(store.node_public_key().unwrap(), Some(derived));
    }

    #[test]
    fn read_rejects_malformed_key_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(NODE_KEY_FILE_NAME), "garbage").unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        assert!(store.read().is_err());
    }

    #[test]
    fn write_pending_does_not_touch_active_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        let active = [1u8; 32];
        let staged = [2u8; 32];
        store.write(&active).unwrap();
        store.write_pending(&staged).unwrap();
        assert_eq!(
            store.read().unwrap(),
            active,
            "active key must stay untouched"
        );
        assert_eq!(store.read_pending().unwrap(), Some(staged));
        assert!(store.pending_exists());
    }

    #[test]
    fn write_pending_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        let staged = [9u8; 32];
        store.write_pending(&staged).unwrap();
        assert_eq!(store.read_pending().unwrap(), Some(staged));
    }

    #[test]
    fn read_pending_returns_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        assert_eq!(store.read_pending().unwrap(), None);
        assert!(!store.pending_exists());
    }

    #[test]
    fn promote_pending_activates_staged_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        let active = [1u8; 32];
        let staged = [2u8; 32];
        store.write(&active).unwrap();
        store.write_pending(&staged).unwrap();
        store.promote_pending().unwrap();
        assert_eq!(store.read().unwrap(), staged);
        assert!(!store.pending_exists());
        assert_eq!(store.read_pending().unwrap(), None);
    }

    #[test]
    fn promote_pending_is_idempotent_noop_without_staged_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        let active = [3u8; 32];
        store.write(&active).unwrap();
        store.promote_pending().unwrap();
        store.promote_pending().unwrap();
        assert_eq!(store.read().unwrap(), active);
        assert!(!store.pending_exists());
    }

    #[test]
    fn promote_pending_twice_keeps_promoted_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        let staged = [4u8; 32];
        store.write_pending(&staged).unwrap();
        store.promote_pending().unwrap();
        store.promote_pending().unwrap();
        assert_eq!(store.read().unwrap(), staged);
    }

    #[test]
    fn discard_pending_removes_staged_key_keeping_active() {
        let dir = tempfile::tempdir().unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        let active = [5u8; 32];
        store.write(&active).unwrap();
        store.write_pending(&[6u8; 32]).unwrap();
        store.discard_pending().unwrap();
        assert!(!store.pending_exists());
        assert_eq!(store.read().unwrap(), active);
    }

    #[test]
    fn discard_pending_is_noop_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        store.discard_pending().unwrap();
        assert!(!store.pending_exists());
    }

    #[test]
    fn restage_overwrites_previous_pending_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = NodeKeyStore::new(dir.path().to_path_buf());
        store.write_pending(&[7u8; 32]).unwrap();
        store.write_pending(&[8u8; 32]).unwrap();
        assert_eq!(store.read_pending().unwrap(), Some([8u8; 32]));
    }
}
