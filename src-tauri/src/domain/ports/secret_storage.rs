//! Secret-storage port (ADR-0041 §11.4 — OS secret storage / keyring source).
//!
//! Domain-boundary abstraction: domain, application, and commands depend on
//! this port — never on a concrete keyring crate. The concrete provider (OS
//! keyring) lives in `infrastructure/security/`.
//!
//! Semantic contract of the port:
//! - `Ok(Some(value))` — a secret exists and was readable.
//! - `Ok(None)` — no entry exists for the key (absent source).
//! - `Err` — the store is unavailable or the entry is unreadable. Callers must
//!   treat the source as unavailable and MUST NEVER fall back to plaintext.

use crate::errors::AppResult;

/// Minimal secret-storage abstraction required by the App-Key resolution
/// hierarchy (ADR-0041 §11.4).
pub trait SecretStoragePort: Send + Sync {
    /// Read a secret. `Ok(None)` = absent. `Err` = store unavailable / entry
    /// unreadable — never a plaintext fallback signal.
    fn get_secret(&self, key: &str) -> AppResult<Option<String>>;

    /// Persist (or overwrite) a secret. `Err` = persistence failed.
    fn set_secret(&self, key: &str, value: &str) -> AppResult<()>;

    /// Remove a secret. Returns `true` if an entry was removed, `false` if no
    /// entry existed. `Err` = the store could not be contacted.
    fn delete_secret(&self, key: &str) -> AppResult<bool>;
}