//! OS keyring secret-storage provider (ADR-0041 §11.4 — SEC-013 Phase 2).
//!
//! Concrete `SecretStoragePort` implementation backed by the `keyring` crate:
//! - Linux: Secret Service over DBus (`sync-secret-service` — gnome-keyring /
//!   KWallet) with pure-Rust transport encryption (`crypto-rust`).
//! - Windows / macOS: native Credential Manager / Keychain.
//!
//! Entry identifier (stable, application-specific):
//! - service: `dz-grpc` — the application namespace (never a username).
//! - account: `app-key` — the App-Key entry (never a username / password /
//!   session id / database content / node credential / auth state).
//!
//! Stored value: the ALREADY VALIDATED, decrypted App Key (`AGE-SECRET-KEY-1...`
//! x25519 identity). No user-authentication material is ever stored; the App-Key
//! passphrase is never stored (the validated key is stored instead, avoiding
//! per-launch scrypt work — ADR-0041 §11.4).
//!
//! Unsupported / headless behavior: the `keyring` crate silently uses its
//! non-persistent in-memory mock store on targets without a configured native
//! backend, and a headless Linux session without a Secret Service returns
//! `NoEntry` / `PlatformFailure`. Both surface as "absent" or "unavailable"
//! through this provider — the resolver then falls through per its hierarchy
//! and the interactive unlock path remains available.

use crate::domain::ports::SecretStoragePort;
use crate::errors::{AppError, AppResult};
use keyring::Entry;

/// Stable application namespace for keyring entries.
pub const KEYRING_SERVICE: &str = "dz-grpc";

/// Stable account identifier of the App-Key entry.
pub const APP_KEY_RING_ENTRY: &str = "app-key";

/// OS keyring provider. Stateless — every call opens its own `Entry`.
pub struct KeyringSecretStorage;

impl SecretStoragePort for KeyringSecretStorage {
    fn get_secret(&self, key: &str) -> AppResult<Option<String>> {
        let entry = entry_for(key)?;
        classify_read(entry.get_password())
    }

    fn set_secret(&self, key: &str, value: &str) -> AppResult<()> {
        let entry = entry_for(key)?;
        entry.set_password(value).map_err(classify_write)
    }

    fn delete_secret(&self, key: &str) -> AppResult<bool> {
        let entry = entry_for(key)?;
        classify_delete(entry.delete_credential())
    }
}

fn entry_for(key: &str) -> AppResult<Entry> {
    Entry::new(KEYRING_SERVICE, key).map_err(|e| {
        AppError::Internal(format!(
            "OS secret storage is unavailable for this entry: {e}"
        ))
    })
}

/// `NoEntry` → absent (`Ok(None)`); any other error → unavailable (`Err`).
fn classify_read(result: Result<String, keyring::Error>) -> AppResult<Option<String>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Internal(format!(
            "OS secret storage read failed (entry unreadable or store unavailable): {e}"
        ))),
    }
}

fn classify_write(e: keyring::Error) -> AppError {
    AppError::Internal(format!("OS secret storage write failed: {e}"))
}

/// `NoEntry` → nothing to delete (`Ok(false)`); success → removed (`Ok(true)`);
/// any other error → unavailable (`Err`).
fn classify_delete(result: Result<(), keyring::Error>) -> AppResult<bool> {
    match result {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(AppError::Internal(format!(
            "OS secret storage delete failed: {e}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_no_entry_is_absent() {
        assert_eq!(classify_read(Err(keyring::Error::NoEntry)).expect("absent"), None);
    }

    #[test]
    fn read_success_is_present() {
        assert_eq!(
            classify_read(Ok("AGE-SECRET-KEY-1abc".into())).expect("value"),
            Some("AGE-SECRET-KEY-1abc".into())
        );
    }

    #[test]
    fn read_bad_encoding_is_unavailable() {
        assert!(classify_read(Err(keyring::Error::BadEncoding(vec![0xff]))).is_err());
    }

    #[test]
    fn read_platform_failure_is_unavailable() {
        assert!(classify_read(Err(keyring::Error::PlatformFailure(Box::new(
            TestError
        ))))
        .is_err());
    }

    #[test]
    fn delete_no_entry_means_nothing_to_forget() {
        assert!(!classify_delete(Err(keyring::Error::NoEntry)).expect("absent"));
        assert!(classify_delete(Ok(())).expect("removed"));
    }

    #[test]
    fn entry_identifier_never_contains_auth_material() {
        // The identifiers are fixed application constants — no username,
        // password, session, database, node credential, or auth state.
        assert_eq!(KEYRING_SERVICE, "dz-grpc");
        assert_eq!(APP_KEY_RING_ENTRY, "app-key");
    }

    #[derive(Debug)]
    struct TestError;
    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "platform error")
        }
    }
    impl std::error::Error for TestError {}
}