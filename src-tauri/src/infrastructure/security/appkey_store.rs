//! `appkey.age` store — passphrase-protected app-key-at-rest (ADR-0041 §2–§3).
//!
//! One of exactly TWO legal `age::scrypt` sites in the codebase (the other is
//! `infrastructure/identity/adminkey_provider.rs`, ADR-0039). The store holds the
//! node's application encryption key (`AGE-SECRET-KEY-1...`) wrapped with the
//! operator passphrase, so a clean release node can be provisioned in-product and
//! the key is no longer forced into the process environment.
//!
//! Node-managed secrets remain `age::x25519`-only (ADR-0039); this file only ever
//! protects portable, operator-passphrase custody material.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::errors::{AppError, AppResult};

/// `appkey.age` file name (passphrase-protected app-key-at-rest).
pub const APPKEY_FILE_NAME: &str = "appkey.age";

/// File-schema evolution space (independent of algorithm versions; ADR-0039 §4 convention).
pub const APPKEY_FORMAT_VERSION: u32 = 1;

/// Profile of the wrapped key material (x25519 identity) in this file.
pub const APPKEY_ALGORITHM_VERSION: u32 = 1;

/// On-disk structure of `appkey.age` (ADR-0041 §2).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AppKeyFile {
    pub format_version: u32,
    pub algorithm_version: u32,
    /// Raw `AGE-SECRET-KEY-1...` identity string bytes, encrypted with
    /// `age::scrypt(passphrase)`. The passphrase never leaves the backend.
    pub encrypted_identity: Vec<u8>,
}

impl AppKeyFile {
    /// Structural validation before read/write (fail-closed on unknown versions).
    pub fn validate(&self) -> AppResult<()> {
        if self.format_version != APPKEY_FORMAT_VERSION {
            return Err(AppError::FileFormat(format!(
                "Unsupported appkey.age format_version {} (expected {})",
                self.format_version, APPKEY_FORMAT_VERSION
            )));
        }
        if self.algorithm_version != APPKEY_ALGORITHM_VERSION {
            return Err(AppError::FileFormat(format!(
                "Unsupported appkey.age algorithm_version {} (expected {})",
                self.algorithm_version, APPKEY_ALGORITHM_VERSION
            )));
        }
        if self.encrypted_identity.is_empty() {
            return Err(AppError::FileFormat(
                "appkey.age has an empty encrypted_identity".into(),
            ));
        }
        Ok(())
    }
}

/// Reads/writes `appkey.age` and encrypts/decrypts the wrapped identity with the
/// operator passphrase via `age::scrypt`.
#[derive(Debug, Clone)]
pub struct AppKeyStore {
    data_dir: PathBuf,
}

impl AppKeyStore {
    /// Resolve the on-disk identity data directory (ADR-0062): the single
    /// resolver in `infrastructure/identity/data_dir.rs` owns this decision.
    pub fn default_data_dir() -> AppResult<PathBuf> {
        crate::infrastructure::identity::data_dir::identity_data_dir()
    }

    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    pub fn file_path(&self) -> PathBuf {
        self.data_dir.join(APPKEY_FILE_NAME)
    }

    pub fn exists(&self) -> bool {
        self.file_path().is_file()
    }

    /// Persist an `appkey.age` file atomically (write temp + rename), file mode
    /// `0600`, creating the data directory as needed (ADR-0041 §2).
    pub fn write(&self, file: &AppKeyFile) -> AppResult<()> {
        file.validate()?;
        let json = serde_json::to_string_pretty(file)
            .map_err(|e| AppError::Internal(format!("Failed to serialize appkey.age: {e}")))?;
        std::fs::create_dir_all(&self.data_dir).map_err(AppError::Io)?;
        let path = self.file_path();
        let tmp = path.with_extension("age.tmp");
        std::fs::write(&tmp, json).map_err(AppError::Io)?;
        apply_restrictive_permissions(&tmp)?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            AppError::Io(e)
        })?;
        Ok(())
    }

    /// Load and structurally validate the `appkey.age` file. A corrupt or
    /// unparseable file is treated as operator action required (ADR-0041 §8).
    pub fn read(&self) -> AppResult<AppKeyFile> {
        let path = self.file_path();
        let json = std::fs::read_to_string(&path).map_err(|e| {
            AppError::Internal(format!(
                "Failed to read appkey.age at {}: {e}",
                path.display()
            ))
        })?;
        let file: AppKeyFile = serde_json::from_str(&json)
            .map_err(|e| AppError::FileFormat(format!("Malformed appkey.age file: {e}")))?;
        file.validate()?;
        Ok(file)
    }

    /// Wrap a raw `AGE-SECRET-KEY-1...` identity with the operator passphrase.
    pub fn encrypt_identity(&self, identity: &str, passphrase: &str) -> AppResult<Vec<u8>> {
        if !identity.trim().starts_with("AGE-SECRET-KEY-1") {
            return Err(AppError::Validation(
                crate::errors::ValidationError::InvalidFormat {
                    field: "app identity".into(),
                    message: "must be a valid age x25519 identity (starts with AGE-SECRET-KEY-1)"
                        .into(),
                },
            ));
        }
        encrypt_scrypt(passphrase, identity.trim().as_bytes())
    }

    /// Unwrap the identity with the operator passphrase.
    ///
    /// A wrong passphrase yields an error (fail-closed); the store stays locked
    /// and there is no fallback to env/dev (ADR-0041 §4 `UnlockFailed`).
    pub fn decrypt_identity(&self, encrypted: &[u8], passphrase: &str) -> AppResult<String> {
        let plaintext = decrypt_scrypt(passphrase, encrypted)?;
        let identity = String::from_utf8(plaintext).map_err(|e| {
            AppError::FileFormat(format!(
                "Decrypted appkey.age identity is not valid UTF-8: {e}"
            ))
        })?;
        if !identity.starts_with("AGE-SECRET-KEY-1") {
            return Err(AppError::FileFormat(
                "Decrypted appkey.age content is not a valid age x25519 identity".into(),
            ));
        }
        Ok(identity)
    }
}

/// `age::scrypt` encryption (ADR-0041 §3 — allowed only in this module and in
/// `adminkey_provider.rs`).
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

/// `age::scrypt` decryption (ADR-0041 §3 — allowed only in this module and in
/// `adminkey_provider.rs`).
fn decrypt_scrypt(passphrase: &str, encrypted: &[u8]) -> AppResult<Vec<u8>> {
    use age::secrecy::SecretString;
    let identity = age::scrypt::Identity::new(SecretString::from(passphrase.to_owned()));

    let decryptor = age::Decryptor::new(encrypted)
        .map_err(|e| AppError::Internal(format!("Invalid scrypt ciphertext: {e}")))?;
    let mut reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|e| {
            AppError::Internal(format!(
                "Failed to decrypt appkey.age (wrong passphrase or corrupt store?): {e}"
            ))
        })?;
    let mut plaintext = vec![];
    std::io::copy(&mut reader, &mut plaintext)
        .map_err(|e| AppError::Internal(format!("Failed to read decrypted appkey.age: {e}")))?;
    Ok(plaintext)
}

/// Restrict `appkey.age` to owner-only access (mode `0600`, ADR-0041 §2).
fn apply_restrictive_permissions(path: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(AppError::Io)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_APP_KEY: &str =
        "AGE-SECRET-KEY-1KTYK6RVLN5TAPE7VF6FQQSKZ9HWWCDSKUGXXNUQDWZ7XXT5YK5LSF3UTKQ";

    #[test]
    fn scrypt_roundtrip_with_correct_passphrase() {
        let store = AppKeyStore::new(PathBuf::from("/tmp/unused"));
        let encrypted = store
            .encrypt_identity(VALID_APP_KEY, "correct horse battery staple")
            .unwrap();
        let decrypted = store
            .decrypt_identity(&encrypted, "correct horse battery staple")
            .unwrap();
        assert_eq!(decrypted, VALID_APP_KEY);
        assert_ne!(encrypted, VALID_APP_KEY.as_bytes());
    }

    #[test]
    fn scrypt_wrong_passphrase_fails_closed() {
        let store = AppKeyStore::new(PathBuf::from("/tmp/unused"));
        let encrypted = store
            .encrypt_identity(VALID_APP_KEY, "right passphrase")
            .unwrap();
        assert!(store
            .decrypt_identity(&encrypted, "wrong passphrase")
            .is_err());
    }

    #[test]
    fn encrypt_rejects_malformed_identity() {
        let store = AppKeyStore::new(PathBuf::from("/tmp/unused"));
        assert!(store
            .encrypt_identity("NOT-AN-AGE-KEY", "passphrase")
            .is_err());
    }

    #[test]
    fn write_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = AppKeyStore::new(dir.path().to_path_buf());
        let file = AppKeyFile {
            format_version: APPKEY_FORMAT_VERSION,
            algorithm_version: APPKEY_ALGORITHM_VERSION,
            encrypted_identity: vec![0u8, 1, 2, 3],
        };
        store.write(&file).unwrap();
        assert!(store.exists());
        let loaded = store.read().unwrap();
        assert_eq!(loaded, file);
    }

    #[cfg(unix)]
    #[test]
    fn write_restricts_file_mode_to_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let store = AppKeyStore::new(dir.path().to_path_buf());
        store
            .write(&AppKeyFile {
                format_version: APPKEY_FORMAT_VERSION,
                algorithm_version: APPKEY_ALGORITHM_VERSION,
                encrypted_identity: vec![9u8],
            })
            .unwrap();
        let mode = std::fs::metadata(store.file_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn read_rejects_malformed_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(APPKEY_FILE_NAME), "not-json").unwrap();
        let store = AppKeyStore::new(dir.path().to_path_buf());
        assert!(store.read().is_err());
    }

    #[test]
    fn read_rejects_unknown_format_version() {
        let dir = tempfile::tempdir().unwrap();
        let store = AppKeyStore::new(dir.path().to_path_buf());
        store
            .write(&AppKeyFile {
                format_version: APPKEY_FORMAT_VERSION,
                algorithm_version: APPKEY_ALGORITHM_VERSION,
                encrypted_identity: vec![1u8],
            })
            .unwrap();
        let upgraded = AppKeyFile {
            format_version: 99,
            algorithm_version: APPKEY_ALGORITHM_VERSION,
            encrypted_identity: vec![1u8],
        };
        std::fs::write(
            store.file_path(),
            serde_json::to_string_pretty(&upgraded).unwrap(),
        )
        .unwrap();
        assert!(store.read().is_err());
    }
}
