pub mod appkey_store;
pub mod file_encryption;
pub mod identity;
pub mod keyring_secret_storage;
pub mod node_identity_provider;
pub mod password_hash_provider;

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use crate::domain::ports::SecretStoragePort;
use crate::errors::{AppError, AppResult, ValidationError};

pub use appkey_store::{AppKeyFile, AppKeyStore};
pub use file_encryption::AgeFileEncryptionProvider;
pub use identity::{Ed25519SignatureVerifier, Ed25519SigningProvider};
pub use keyring_secret_storage::{KeyringSecretStorage, APP_KEY_RING_ENTRY};
pub use node_identity_provider::{NodeIdentityProvider, SettingsNodeIdentityProvider};
pub use password_hash_provider::Argon2PasswordHashProvider;

fn is_production_mode() -> bool {
    matches!(
        std::env::var("GRPC_ENV"),
        Ok(v) if v.eq_ignore_ascii_case("production") || v.eq_ignore_ascii_case("prod")
    )
}

/// In-memory cache of the unlocked app encryption key (ADR-0041 §4 `Unlocked`).
/// Populated on successful `unlock_app_key` / `initialize_app_key`; consulted by
/// `resolve_app_encryption_key()` after env and keyring (rank 3 in the
/// resolution hierarchy).
static APP_KEY_CACHE: Mutex<Option<String>> = Mutex::new(None);

/// TEST-ONLY provider override for the rank-2 OS keyring source (ADR-0041
/// §11.4). `None` (the production default, set at process start and never
/// touched by application code) means the real `KeyringSecretStorage` is used.
/// Deterministic integration tests install an in-memory fake so they never
/// depend on — and never write to — the machine's ambient desktop keyring.
/// Same pattern as `ConnectionFactory::new_for_test` / `AppState::new_for_test`.
static KEYRING_PORT_OVERRIDE: Mutex<Option<Arc<dyn SecretStoragePort + Send + Sync>>> =
    Mutex::new(None);

/// The active rank-2 provider: the test override when installed, otherwise the
/// real OS secret storage. Production resolution semantics are identical in
/// both cases — the override only substitutes the storage backend.
fn keyring_port() -> Arc<dyn SecretStoragePort + Send + Sync> {
    KEYRING_PORT_OVERRIDE
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .unwrap_or_else(|| Arc::new(KeyringSecretStorage))
}

/// TEST-ONLY: install an in-memory `SecretStoragePort` fake so keyring rank
/// resolution is deterministic (no ambient desktop keyring dependency, no real
/// keyring writes). Never called by application code.
pub fn install_test_keyring_port(port: Arc<dyn SecretStoragePort + Send + Sync>) {
    if let Ok(mut guard) = KEYRING_PORT_OVERRIDE.lock() {
        *guard = Some(port);
    }
}

/// TEST-ONLY: restore the real OS keyring provider (must be paired with
/// `install_test_keyring_port`).
pub fn uninstall_test_keyring_port() {
    if let Ok(mut guard) = KEYRING_PORT_OVERRIDE.lock() {
        *guard = None;
    }
}

/// Development-only embedded AGE identity. Reachable **only** in debug builds.
/// Excluded from release binaries via `#[cfg(debug_assertions)]`; release
/// builds must supply `GRPC_APP_KEY`, an unlocked `appkey.age` store, or reach
/// the Security Setup UI (ADR-0041).
#[cfg(debug_assertions)]
const DEV_AGE_KEY: &str =
    "AGE-SECRET-KEY-1KTYK6RVLN5TAPE7VF6FQQSKZ9HWWCDSKUGXXNUQDWZ7XXT5YK5LSF3UTKQ";

#[cfg(debug_assertions)]
fn dev_age_key_fallback() -> AppResult<String> {
    log::warn!(target: "grpc::security", "[DEV_SECURITY_WARNING] GRPC_APP_KEY missing — using embedded development AGE key. NOT FOR PRODUCTION.");
    Ok(DEV_AGE_KEY.to_string())
}

#[cfg(not(debug_assertions))]
fn dev_age_key_fallback() -> AppResult<String> {
    Err(AppError::Configuration(
        "DEV_AGE_KEY fallback is unavailable in release builds".to_string(),
    ))
}

/// Cache the resolved app encryption key in memory (ADR-0041 §4 `Unlocked`).
pub fn cache_app_key(identity: &str) -> AppResult<()> {
    let trimmed = identity.trim().to_string();
    if !trimmed.starts_with("AGE-SECRET-KEY-1") {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "app identity".into(),
            message: "must be a valid age x25519 identity (starts with AGE-SECRET-KEY-1)".into(),
        }));
    }
    let mut guard = APP_KEY_CACHE
        .lock()
        .map_err(|e| AppError::Internal(format!("Failed to lock app key cache: {e}")))?;
    *guard = Some(trimmed);
    Ok(())
}

/// Current in-memory app key cache value (lock/unlock state).
pub fn cached_app_key() -> Option<String> {
    APP_KEY_CACHE.lock().ok().and_then(|g| g.clone())
}

/// Drop the cached app key (e.g. on shutdown or lock-out).
pub fn clear_app_key_cache() {
    if let Ok(mut guard) = APP_KEY_CACHE.lock() {
        *guard = None;
    }
}

/// Whether an application encryption key resolves **now** (ADR-0041 §1, as
/// amended by §11.4): env, then OS keyring, then cache, then dev fallback
/// (debug only). `false` in release means the app is Locked or Unprovisioned
/// and DB bootstrap must be deferred.
pub fn app_key_unlocked() -> bool {
    resolve_app_encryption_key().is_ok()
}

/// Live provisioning status projection for `get_security_status` (ADR-0041 §9,
/// as amended by §11.4). `source` distinguishes `env` | `keyring` | `cache` |
/// `dev` | `none`.
pub fn app_key_status() -> AppResult<crate::models::AppKeyStatus> {
    use crate::models::AppKeyStatus;

    let store = AppKeyStore::new(AppKeyStore::default_data_dir()?);
    let has_env = std::env::var("GRPC_APP_KEY")
        .ok()
        .map(|v| v.trim().starts_with("AGE-SECRET-KEY-1"))
        .unwrap_or(false);
    let keyring_key = keyring_app_key().is_some();
    let cached = cached_app_key().is_some();
    let unlocked = has_env || keyring_key || cached || cfg!(debug_assertions);
    let source = if has_env {
        "env"
    } else if keyring_key {
        "keyring"
    } else if cached {
        "cache"
    } else if cfg!(debug_assertions) {
        "dev"
    } else {
        "none"
    };

    Ok(AppKeyStatus {
        provisioned: store.exists(),
        unlocked,
        store_path: store.file_path().display().to_string(),
        source: source.to_string(),
        requires_action: !unlocked,
    })
}

pub fn validate_production_security_environment() -> Result<(), String> {
    if !is_production_mode() {
        return Ok(());
    }

    // ADR-0041: GRPC_APP_KEY is no longer REQUIRED when GRPC_ENV=production. It
    // may be provisioned via the passphrase-protected store (`appkey.age`) or the
    // first-run Security Setup UI. When present it must still be valid.
    if let Ok(app) = std::env::var("GRPC_APP_KEY") {
        if !app.trim().starts_with("AGE-SECRET-KEY-1") {
            return Err(
                "Validation error: GRPC_APP_KEY must be a valid age x25519 identity (starts with AGE-SECRET-KEY-1)."
                    .to_string(),
            );
        }
    }

    // SEC-007 (ADR-0047) / SEC-008 (ADR-0048): HMAC package signing is removed
    // from the entire package surface — sync packages are V2/Ed25519 only and
    // the fiscal closure package (`.fiscal-close.sync`) is now signed with the
    // WILAYA node identity's Ed25519 key. `GRPC_PACKAGE_SIGNING_KEY` and
    // `GRPC_ACTIVE_SIGNING_KEY_ID` are no longer read anywhere.

    Ok(())
}

/// Read + validate the remembered App Key from the OS keyring (ADR-0041 §11.4
/// rank 2). Port-parameterized core so deterministic tests can inject a fake.
///
/// Failure semantics: absent (`Ok(None)` from the port) → nothing remembered;
/// malformed/unusable stored value → rejected (NEVER accepted, no plaintext
/// fallback); store unavailable → the source is skipped. Any of these yields
/// `None`, letting the resolver continue to the next rank.
pub fn keyring_app_key_from(port: &dyn SecretStoragePort) -> Option<String> {
    match port.get_secret(APP_KEY_RING_ENTRY) {
        Ok(Some(raw)) => match parse_and_validate_app_key(&raw) {
            Some(key) => Some(key),
            None => {
                log::warn!(
                    target: "grpc::security",
                    "keyring app-key entry is malformed or unusable — rejected (no plaintext fallback)"
                );
                None
            }
        },
        Ok(None) => None,
        Err(e) => {
            log::debug!(
                target: "grpc::security",
                "OS secret storage unavailable — skipping keyring rank: {e}"
            );
            None
        }
    }
}

/// OS keyring read for the App Key (rank 2 in the resolution hierarchy).
pub fn keyring_app_key() -> Option<String> {
    keyring_app_key_from(keyring_port().as_ref())
}

/// Full validation of a candidate App Key: trim, prefix check, then a real
/// x25519 parse (the same parse the encryption provider performs). The keyring
/// value is a decrypted App Key, so it must survive this validation verbatim.
fn parse_and_validate_app_key(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_string();
    if !trimmed.starts_with("AGE-SECRET-KEY-1") {
        return None;
    }
    age::x25519::Identity::from_str(&trimmed)
        .ok()
        .map(|_| trimmed)
}

/// Best-effort remember (ADR-0041 §11.4): persists the VALIDATED decrypted App
/// Key to the OS keyring only when the operator explicitly opted in
/// (`remember == true`). Port-parameterized core for deterministic tests.
///
/// Failure NEVER rolls back an in-progress unlock and never propagates: the
/// application stays unlocked for the current process and the caller continues
/// normally (a clear warning is logged; the next startup may require manual
/// unlock).
pub fn remember_app_key_best_effort_with(port: &dyn SecretStoragePort, remember: bool, key: &str) {
    if !remember {
        return;
    }
    match port.set_secret(APP_KEY_RING_ENTRY, key) {
        Ok(()) => {
            log::info!(
                target: "grpc::security",
                "app key remembered on this device (OS keyring)"
            );
        }
        Err(e) => {
            log::warn!(
                target: "grpc::security",
                "remember-on-device: keyring persistence failed — the app stays unlocked for this process: {e}"
            );
        }
    }
}

/// Remember the validated App Key on this device (operator opt-in required).
pub fn remember_app_key_best_effort(remember: bool, key: &str) {
    remember_app_key_best_effort_with(keyring_port().as_ref(), remember, key);
}

/// Remove the remembered App Key from the OS keyring. Device-local preference
/// only: `appkey.age`, the database, identities, users, sessions, and
/// provisioning state are untouched. Returns `true` if an entry was removed,
/// `false` if none existed.
pub fn forget_remembered_app_key() -> AppResult<bool> {
    forget_remembered_app_key_with(keyring_port().as_ref())
}

/// Port-parameterized core of `forget_remembered_app_key` (test seam).
pub fn forget_remembered_app_key_with(port: &dyn SecretStoragePort) -> AppResult<bool> {
    port.delete_secret(APP_KEY_RING_ENTRY)
}

/// Shared core for `resolve_app_encryption_key` (ADR-0041 §1 resolution order,
/// as amended by §11.4).
///
/// Hierarchy is fixed: env → OS keyring → unlocked store cache → dev fallback
/// (debug only) → fail-closed. `allow_dev_fallback` is threaded explicitly so
/// both debug and release semantics are testable in a single (debug) test
/// binary. An explicitly supplied but INVALID `GRPC_APP_KEY` remains terminal
/// (rank 1 preserves its existing fail-closed semantics — the keyring is never
/// consulted when the environment variable is present).
///
/// Public so integration tests can verify fail-closed semantics deterministically
/// (the pure core decouples the test from the debug-only dev fallback).
pub fn resolve_app_encryption_key_impl(
    env_raw: Option<String>,
    keyring_secret: Option<String>,
    cached: Option<String>,
    allow_dev_fallback: bool,
) -> AppResult<String> {
    match env_raw {
        Some(raw) => {
            let trimmed = raw.trim().to_string();
            if !trimmed.starts_with("AGE-SECRET-KEY-1") {
                return Err(AppError::Validation(ValidationError::InvalidFormat {
                    field: "GRPC_APP_KEY".into(),
                    message: "must be a valid age x25519 identity (starts with AGE-SECRET-KEY-1)"
                        .into(),
                }));
            }
            Ok(trimmed)
        }
        None => match keyring_secret {
            Some(key) => Ok(key),
            None => match cached {
                Some(key) => Ok(key),
                None if allow_dev_fallback => dev_age_key_fallback(),
                None => Err(AppError::Configuration(
                    "GRPC_APP_KEY is locked; unlock the app key store or set GRPC_APP_KEY"
                        .to_string(),
                )),
            },
        },
    }
}

pub fn resolve_app_encryption_key() -> AppResult<String> {
    resolve_app_encryption_key_impl(
        std::env::var("GRPC_APP_KEY").ok(),
        keyring_app_key(),
        cached_app_key(),
        cfg!(debug_assertions),
    )
}

pub fn get_sync_security_diagnostics() -> AppResult<crate::models::SyncSecurityDiagnostics> {
    use crate::models::SyncSecurityDiagnostics;

    let has_app_key_env = match std::env::var("GRPC_APP_KEY") {
        Ok(s) => s.trim().starts_with("AGE-SECRET-KEY-1"),
        Err(_) => false,
    };

    let is_prod = is_production_mode();

    Ok(SyncSecurityDiagnostics {
        production_mode: is_prod,
        has_app_key_env,
        bootstrap_would_fail: is_prod && !has_app_key_env,
    })
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Mutex, MutexGuard};

    /// Serializes access to process-global environment variables that the
    /// security stack reads (`GRPC_APP_KEY`, `GRPC_ENV`, `GRPC_PACKAGE_SIGNING_KEY`,
    /// `GRPC_ACTIVE_SIGNING_KEY_ID`). `std::env` is process-global and not
    /// thread-safe: any test that mutates these variables — or reads them and
    /// requires a stable value across operations (e.g. age-based key store
    /// round-trips) — MUST hold this lock for its whole critical section.
    ///
    /// Test isolation only: never referenced from production code.
    pub(crate) static SECURITY_TEST_ENV_LOCK: Mutex<()> = Mutex::new(());

    pub(crate) fn lock_security_test_env() -> MutexGuard<'static, ()> {
        SECURITY_TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod key_resolution_tests {
    use super::*;
    use crate::errors::AppError;
    // Shared with every test that reads/mutates the process-global security
    // environment (e.g. node key store tests): see SECURITY_TEST_ENV_LOCK.
    use super::test_support::SECURITY_TEST_ENV_LOCK as ENV_LOCK;

    const VALID_APP_KEY: &str =
        "AGE-SECRET-KEY-1KTYK6RVLN5TAPE7VF6FQQSKZ9HWWCDSKUGXXNUQDWZ7XXT5YK5LSF3UTKQ";

    fn clear_key_env() {
        std::env::remove_var("GRPC_ENV");
        std::env::remove_var("GRPC_APP_KEY");
        std::env::remove_var("GRPC_PACKAGE_SIGNING_KEY");
        std::env::remove_var("GRPC_ACTIVE_SIGNING_KEY_ID");
        std::env::remove_var("GRPC_ENFORCE_TRUSTED_SIGNERS");
        std::env::remove_var("GRPC_TRUSTED_SIGNER_IDS");
    }

    // ---- resolve_app_encryption_key_impl ----

    #[test]
    fn app_key_valid_env_returns_trimmed_in_both_modes() {
        for allow in [true, false] {
            let got = resolve_app_encryption_key_impl(
                Some(format!("  {}  ", VALID_APP_KEY)),
                None,
                None,
                allow,
            )
            .expect("valid key accepted");
            assert_eq!(got, VALID_APP_KEY);
        }
    }

    #[test]
    fn app_key_malformed_prefix_is_validation_error_in_both_modes() {
        for allow in [true, false] {
            let err =
                resolve_app_encryption_key_impl(Some("NOT-AN-AGE-KEY".into()), None, None, allow)
                    .unwrap_err();
            assert!(
                matches!(err, AppError::Validation(_)),
                "malformed key must be a validation error, got {err:?}"
            );
        }
    }

    #[test]
    fn app_key_missing_with_dev_fallback_returns_embedded_key() {
        let got =
            resolve_app_encryption_key_impl(None, None, None, true).expect("dev fallback allowed");
        assert_eq!(got, DEV_AGE_KEY);
    }

    #[test]
    fn app_key_missing_without_dev_fallback_is_configuration_error() {
        let err = resolve_app_encryption_key_impl(None, None, None, false).unwrap_err();
        assert!(
            matches!(err, AppError::Configuration(_)),
            "missing key must be a configuration error, got {err:?}"
        );
    }

    // ---- ADR-0041 resolution hierarchy (env → keyring → store cache → dev → fail-closed) ----

    #[test]
    fn app_key_env_beats_keyring_and_cached_store_key_in_both_modes() {
        for allow in [true, false] {
            let got = resolve_app_encryption_key_impl(
                Some(VALID_APP_KEY.to_string()),
                Some("AGE-SECRET-KEY-1KEYRINGVALUE0000000000000000000000000000".to_string()),
                Some("AGE-SECRET-KEY-1DIFFERENTVALUEHERE".to_string()),
                allow,
            )
            .expect("env key wins");
            assert_eq!(got, VALID_APP_KEY);
        }
    }

    #[test]
    fn app_key_invalid_env_is_terminal_even_with_valid_keyring() {
        // Rank 1 semantics preserved exactly (ADR-0041 §1): an explicitly
        // supplied but invalid GRPC_APP_KEY is a hard error — the keyring
        // source is NEVER consulted as a silent fallback.
        let err = resolve_app_encryption_key_impl(
            Some("NOT-AN-AGE-KEY".into()),
            Some("AGE-SECRET-KEY-1KEYRINGVALUE0000000000000000000000000000".to_string()),
            Some("AGE-SECRET-KEY-1DIFFERENTVALUEHERE".to_string()),
            false,
        )
        .unwrap_err();
        assert!(
            matches!(err, AppError::Validation(_)),
            "invalid explicit env key must stay terminal, got {err:?}"
        );
    }

    #[test]
    fn app_key_keyring_beats_cache_and_dev_fallback_in_debug_mode() {
        let keyring = "AGE-SECRET-KEY-1KEYRINGVALUE0000000000000000000000000000".to_string();
        let cached =
            "AGE-SECRET-KEY-1CACHEVALUE000000000000000000000000000000000000000".to_string();
        let got = resolve_app_encryption_key_impl(None, Some(keyring.clone()), Some(cached), true)
            .expect("keyring key accepted");
        assert_eq!(got, keyring);
    }

    #[test]
    fn app_key_keyring_resolves_without_dev_fallback_in_release_mode() {
        let keyring = "AGE-SECRET-KEY-1KEYRINGVALUE0000000000000000000000000000".to_string();
        let got = resolve_app_encryption_key_impl(None, Some(keyring.clone()), None, false)
            .expect("keyring key accepted in release");
        assert_eq!(got, keyring);
    }

    #[test]
    fn app_key_keyring_absent_falls_through_to_cache() {
        let cached =
            "AGE-SECRET-KEY-1CACHEVALUE000000000000000000000000000000000000000".to_string();
        let got = resolve_app_encryption_key_impl(None, None, Some(cached.clone()), false)
            .expect("cached store key accepted when keyring absent");
        assert_eq!(got, cached);
    }

    #[test]
    fn app_key_cache_used_when_env_and_keyring_absent_in_release_mode() {
        let cached =
            "AGE-SECRET-KEY-1CACHEVALUE000000000000000000000000000000000000000".to_string();
        let got = resolve_app_encryption_key_impl(None, None, Some(cached.clone()), false)
            .expect("cached store key accepted in release");
        assert_eq!(got, cached);
    }

    #[test]
    fn app_key_cache_beats_dev_fallback_in_debug_mode() {
        let cached =
            "AGE-SECRET-KEY-1CACHEVALUE000000000000000000000000000000000000000".to_string();
        let got = resolve_app_encryption_key_impl(None, None, Some(cached.clone()), true)
            .expect("cached store key accepted");
        assert_eq!(got, cached);
    }

    // ---- cache_app_key / cached_app_key ----

    fn clear_app_key_cache() {
        super::clear_app_key_cache();
    }

    #[test]
    fn cache_app_key_roundtrip() {
        clear_app_key_cache();
        assert!(cached_app_key().is_none());
        cache_app_key(VALID_APP_KEY).expect("valid key cached");
        assert_eq!(cached_app_key().as_deref(), Some(VALID_APP_KEY));
        clear_app_key_cache();
        assert!(cached_app_key().is_none());
    }

    #[test]
    fn cache_app_key_trims_whitespace() {
        clear_app_key_cache();
        cache_app_key(&format!("  {VALID_APP_KEY}  ")).expect("trimmed before caching");
        assert_eq!(cached_app_key().as_deref(), Some(VALID_APP_KEY));
        clear_app_key_cache();
    }

    #[test]
    fn cache_app_key_rejects_malformed_identity() {
        clear_app_key_cache();
        assert!(cache_app_key("NOT-AN-AGE-KEY").is_err());
        assert!(cached_app_key().is_none());
    }

    // ---- file_encryption boundary (ADR-0041 §10 / Model C interchange) ----

    #[test]
    fn file_encryption_same_key_roundtrips_different_key_fails_closed() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_key_env();

        let provider = AgeFileEncryptionProvider::new();
        let to_str = |id: age::x25519::Identity| {
            use age::secrecy::ExposeSecret;
            id.to_string().expose_secret().to_string()
        };
        let key_a = to_str(age::x25519::Identity::generate());
        let key_b = to_str(age::x25519::Identity::generate());
        assert_ne!(key_a, key_b);

        // Model C premise: the exporter App Key value is shared verbatim between
        // WILAYA and the UNIT fleet (`GRPC_APP_KEY`, ADR-0041 §10.3).
        std::env::set_var("GRPC_APP_KEY", &key_a);
        let plaintext = b"GRPC APP KEY INTERCHANGE BOUNDARY";
        let encrypted = provider
            .encrypt_data(plaintext)
            .expect("encrypt with key A");

        // Same App Key → decrypt succeeds (round-trip).
        let roundtrip = provider
            .decrypt_data(&encrypted)
            .expect("same App Key must decrypt");
        assert_eq!(roundtrip, plaintext, "round-trip plaintext mismatch");

        // Different App Key → decrypt fails closed (fail-closed, ADR-0041 §10.5).
        std::env::set_var("GRPC_APP_KEY", &key_b);
        let err = provider
            .decrypt_data(&encrypted)
            .expect_err("different App Key must fail closed");
        assert!(
            matches!(err, AppError::Io(_) | AppError::Internal(_)),
            "wrong App Key must produce a decryption failure, got {err:?}"
        );

        clear_key_env();
    }

    // ---- resolve_package_signing_key_32_impl ----

    // SEC-008 (ADR-0048): the HMAC package-signing resolver was removed
    // entirely — fiscal closure packages are signed with the WILAYA node
    // identity's Ed25519 key (see fiscal_closure_package_service.rs and
    // fiscal_closure_ed25519_security_tests.rs). No shared-secret fallback
    // exists in any code path.

    // ---- validate_production_security_environment ----

    #[test]
    fn validate_skips_when_not_production() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_ENV", "development");
        assert!(validate_production_security_environment().is_ok());
    }

    #[test]
    fn validate_allows_missing_app_key_in_production() {
        // ADR-0041: GRPC_APP_KEY is optional in production; it may come from the
        // passphrase-protected store or first-run Security Setup.
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_ENV", "production");
        let result = validate_production_security_environment();
        assert!(
            result.is_ok(),
            "missing app key is allowed (store/setup path), got: {result:?}"
        );
    }

    #[test]
    fn validate_accepts_valid_app_key_in_production() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_ENV", "production");
        std::env::set_var("GRPC_APP_KEY", VALID_APP_KEY);
        assert!(validate_production_security_environment().is_ok());
    }

    #[test]
    fn validate_rejects_malformed_app_key_in_production() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_ENV", "production");
        std::env::set_var("GRPC_APP_KEY", "NOT-AN-AGE-KEY");
        let err = validate_production_security_environment().unwrap_err();
        assert!(
            err.contains("GRPC_APP_KEY") && err.starts_with("Validation error:"),
            "malformed app key must be a validation error, got: {err}"
        );
    }

    #[test]
    fn validate_ignores_leftover_signing_key_env_in_production() {
        // SEC-008 (ADR-0048): GRPC_PACKAGE_SIGNING_KEY is no longer read
        // anywhere; leftover values in external environments must neither
        // cause failure nor be consulted.
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_ENV", "production");
        std::env::set_var("GRPC_APP_KEY", VALID_APP_KEY);
        std::env::set_var("GRPC_PACKAGE_SIGNING_KEY", "not-base64!!");
        std::env::set_var("GRPC_ACTIVE_SIGNING_KEY_ID", "prod-key-1");
        assert!(validate_production_security_environment().is_ok());
    }

    // ---- SEC-007 (ADR-0047): trusted-signer enforcement is removed ----
    // GRPC_ENFORCE_TRUSTED_SIGNERS / GRPC_TRUSTED_SIGNER_IDS are no longer
    // read anywhere; leftover values in external environments must not cause
    // the application to fail (they are simply ignored).

    // ---- public wrappers (debug build: env honored, fallback available) ----

    #[test]
    fn public_resolve_app_encryption_key_honors_env_first() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_APP_KEY", VALID_APP_KEY);
        assert_eq!(
            resolve_app_encryption_key().expect("env key used"),
            VALID_APP_KEY
        );
    }
}
