pub mod file_encryption;
pub mod identity;
pub mod node_identity_provider;
pub mod password_hash_provider;

use crate::errors::{AppError, AppResult, ValidationError};

pub use file_encryption::AgeFileEncryptionProvider;
pub use identity::{Ed25519SignatureVerifier, Ed25519SigningProvider};
pub use node_identity_provider::{NodeIdentityProvider, SettingsNodeIdentityProvider};
pub use password_hash_provider::Argon2PasswordHashProvider;

fn is_production_mode() -> bool {
    matches!(
        std::env::var("GRPC_ENV"),
        Ok(v) if v.eq_ignore_ascii_case("production") || v.eq_ignore_ascii_case("prod")
    )
}

/// Development-only embedded AGE identity. Reachable **only** in debug builds via
/// `#[cfg(debug_assertions)]`; release builds must supply `GRPC_APP_KEY` or fail startup.
const DEV_AGE_KEY: &str =
    "AGE-SECRET-KEY-1KTYK6RVLN5TAPE7VF6FQQSKZ9HWWCDSKUGXXNUQDWZ7XXT5YK5LSF3UTKQ";

pub fn validate_production_security_environment() -> Result<(), String> {
    if !is_production_mode() {
        return Ok(());
    }

    let app = std::env::var("GRPC_APP_KEY").map_err(|_| {
        "Configuration error: GRPC_APP_KEY is required when GRPC_ENV=production".to_string()
    })?;
    if !app.trim().starts_with("AGE-SECRET-KEY-1") {
        return Err(
            "Validation error: GRPC_APP_KEY must be a valid age x25519 identity (starts with AGE-SECRET-KEY-1)."
                .to_string(),
        );
    }

    let sign = std::env::var("GRPC_PACKAGE_SIGNING_KEY").map_err(|_| {
        "Configuration error: GRPC_PACKAGE_SIGNING_KEY is required when GRPC_ENV=production"
            .to_string()
    })?;

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let decoded = STANDARD.decode(sign.trim()).map_err(|_| {
        "Validation error: GRPC_PACKAGE_SIGNING_KEY must be a valid Base64 string.".to_string()
    })?;

    if decoded.len() != 32 {
        return Err(format!(
            "Validation error: GRPC_PACKAGE_SIGNING_KEY must decode to exactly 32 bytes (got {}).",
            decoded.len()
        ));
    }

    Ok(())
}

/// Shared core for `resolve_app_encryption_key`.
///
/// `allow_dev_fallback` is threaded explicitly so both debug and release
/// semantics are testable in a single (debug) test binary.
fn resolve_app_encryption_key_impl(
    env_raw: Option<String>,
    allow_dev_fallback: bool,
) -> AppResult<String> {
    match env_raw {
        Some(raw) => {
            let trimmed = raw.trim().to_string();
            if !trimmed.starts_with("AGE-SECRET-KEY-1") {
                return Err(AppError::Validation(ValidationError::InvalidFormat {
                    field: "GRPC_APP_KEY".into(),
                    message:
                        "must be a valid age x25519 identity (starts with AGE-SECRET-KEY-1)"
                            .into(),
                }));
            }
            Ok(trimmed)
        }
        None if allow_dev_fallback => {
            log::warn!(target: "grpc::security", "[DEV_SECURITY_WARNING] GRPC_APP_KEY missing — using embedded development AGE key. NOT FOR PRODUCTION.");
            Ok(DEV_AGE_KEY.to_string())
        }
        None => Err(AppError::Configuration(
            "GRPC_APP_KEY is required; the embedded development key is forbidden outside debug builds"
                .to_string(),
        )),
    }
}

pub fn resolve_app_encryption_key() -> AppResult<String> {
    resolve_app_encryption_key_impl(std::env::var("GRPC_APP_KEY").ok(), cfg!(debug_assertions))
}

/// Shared core for `resolve_package_signing_key_32`.
///
/// `allow_dev_fallback` is threaded explicitly so both debug and release
/// semantics are testable in a single (debug) test binary.
fn resolve_package_signing_key_32_impl(
    env_raw: Option<String>,
    allow_dev_fallback: bool,
) -> AppResult<[u8; 32]> {
    match env_raw {
        Some(raw) => {
            use base64::engine::general_purpose::STANDARD;
            use base64::Engine;

            let bytes = STANDARD.decode(raw.trim()).map_err(|e| {
                log::error!(target: "grpc::security", "Failed to decode Base64 GRPC_PACKAGE_SIGNING_KEY: {}", e);
                AppError::Validation(ValidationError::InvalidFormat {
                    field: "GRPC_PACKAGE_SIGNING_KEY".into(),
                    message: "must be a valid Base64 string".into(),
                })
            })?;

            if bytes.len() != 32 {
                log::error!(target: "grpc::security", "GRPC_PACKAGE_SIGNING_KEY must decode to exactly 32 bytes (got {})", bytes.len());
                return Err(AppError::Validation(ValidationError::InvalidFormat {
                    field: "GRPC_PACKAGE_SIGNING_KEY".into(),
                    message: format!("must decode to exactly 32 bytes (got {})", bytes.len()),
                }));
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes[..32]);
            Ok(key)
        }
        None if allow_dev_fallback => {
            log::warn!(target: "grpc::security", "[DEV_SECURITY_WARNING] GRPC_PACKAGE_SIGNING_KEY missing — using all-zero dev fallback key. NOT FOR PRODUCTION.");
            Ok([0u8; 32])
        }
        None => Err(AppError::Configuration(
            "GRPC_PACKAGE_SIGNING_KEY is required; the development fallback key is forbidden outside debug builds"
                .to_string(),
        )),
    }
}

pub fn resolve_package_signing_key_32() -> AppResult<[u8; 32]> {
    resolve_package_signing_key_32_impl(
        std::env::var("GRPC_PACKAGE_SIGNING_KEY").ok(),
        cfg!(debug_assertions),
    )
}

/// Active signing key id for **new** package exports. `None` preserves legacy packages (no id in metadata).
pub fn resolve_active_signing_key_id() -> Option<String> {
    std::env::var("GRPC_ACTIVE_SIGNING_KEY_ID")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub fn resolve_accepted_verification_key_ids() -> Vec<String> {
    let raw = std::env::var("GRPC_ACCEPTED_SIGNING_KEY_IDS")
        .ok()
        .unwrap_or_else(|| {
            resolve_active_signing_key_id().unwrap_or_else(|| "default".to_string())
        });
    let mut out: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if out.is_empty() {
        out.push("default".to_string());
    }
    out
}

pub fn is_signing_key_deprecated(key_id: &str) -> bool {
    let deprecated: Vec<String> = std::env::var("GRPC_DEPRECATED_SIGNING_KEY_IDS")
        .ok()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if deprecated.is_empty() || !deprecated.iter().any(|k| k == key_id) {
        return false;
    }

    let Some(deadline_raw) = std::env::var("GRPC_SIGNING_KEY_DEPRECATION_DEADLINE_UTC")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    else {
        return false;
    };

    match chrono::DateTime::parse_from_rfc3339(&deadline_raw) {
        Ok(deadline) => chrono::Utc::now() >= deadline.with_timezone(&chrono::Utc), // [arch:allow-utc-now] see ADR-0007 — signing key deprecation deadline check
        Err(e) => {
            log::warn!(
                "Invalid GRPC_SIGNING_KEY_DEPRECATION_DEADLINE_UTC value: {} ({})",
                deadline_raw,
                e
            );
            false
        }
    }
}

pub fn resolve_trusted_signer_ids() -> Vec<String> {
    std::env::var("GRPC_TRUSTED_SIGNER_IDS")
        .ok()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn should_enforce_trusted_signers() -> bool {
    matches!(
        std::env::var("GRPC_ENFORCE_TRUSTED_SIGNERS"),
        Ok(v) if v.eq_ignore_ascii_case("1")
            || v.eq_ignore_ascii_case("true")
            || v.eq_ignore_ascii_case("yes")
            || v.eq_ignore_ascii_case("on")
    )
}

pub fn is_trusted_signer(source_node_id: &str) -> bool {
    let trusted = resolve_trusted_signer_ids();
    if trusted.is_empty() {
        return !should_enforce_trusted_signers();
    }
    trusted.iter().any(|id| id == source_node_id)
}

pub fn get_sync_security_diagnostics() -> AppResult<crate::models::SyncSecurityDiagnostics> {
    use crate::models::SyncSecurityDiagnostics;

    let has_app_key_env = match std::env::var("GRPC_APP_KEY") {
        Ok(s) => s.trim().starts_with("AGE-SECRET-KEY-1"),
        Err(_) => false,
    };

    let has_package_signing_key_env = match std::env::var("GRPC_PACKAGE_SIGNING_KEY") {
        Ok(s) => {
            use base64::engine::general_purpose::STANDARD;
            use base64::Engine;
            STANDARD
                .decode(s.trim())
                .map(|b| b.len() == 32)
                .unwrap_or(false)
        }
        Err(_) => false,
    };

    let is_prod = is_production_mode();

    Ok(SyncSecurityDiagnostics {
        production_mode: is_prod,
        has_app_key_env,
        has_package_signing_key_env,
        bootstrap_would_fail: is_prod && (!has_app_key_env || !has_package_signing_key_env),
        active_signing_key_id: resolve_active_signing_key_id().unwrap_or_default(),
        accepted_verification_key_ids: resolve_accepted_verification_key_ids(),
        deprecated_signing_key_ids: std::env::var("GRPC_DEPRECATED_SIGNING_KEY_IDS")
            .ok()
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        deprecation_deadline_utc: std::env::var("GRPC_SIGNING_KEY_DEPRECATION_DEADLINE_UTC").ok(),
        enforce_trusted_signers: should_enforce_trusted_signers(),
        trusted_signer_ids: resolve_trusted_signer_ids(),
    })
}

#[cfg(test)]
mod key_resolution_tests {
    use super::*;
    use crate::errors::AppError;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const VALID_APP_KEY: &str =
        "AGE-SECRET-KEY-1KTYK6RVLN5TAPE7VF6FQQSKZ9HWWCDSKUGXXNUQDWZ7XXT5YK5LSF3UTKQ";

    fn b64(bytes: &[u8]) -> String {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        STANDARD.encode(bytes)
    }

    fn clear_key_env() {
        std::env::remove_var("GRPC_ENV");
        std::env::remove_var("GRPC_APP_KEY");
        std::env::remove_var("GRPC_PACKAGE_SIGNING_KEY");
        std::env::remove_var("GRPC_ACTIVE_SIGNING_KEY_ID");
    }

    fn valid_sign_env() -> String {
        b64(&[42u8; 32])
    }

    // ---- resolve_app_encryption_key_impl ----

    #[test]
    fn app_key_valid_env_returns_trimmed_in_both_modes() {
        for allow in [true, false] {
            let got =
                resolve_app_encryption_key_impl(Some(format!("  {}  ", VALID_APP_KEY)), allow)
                    .expect("valid key accepted");
            assert_eq!(got, VALID_APP_KEY);
        }
    }

    #[test]
    fn app_key_malformed_prefix_is_validation_error_in_both_modes() {
        for allow in [true, false] {
            let err =
                resolve_app_encryption_key_impl(Some("NOT-AN-AGE-KEY".into()), allow).unwrap_err();
            assert!(
                matches!(err, AppError::Validation(_)),
                "malformed key must be a validation error, got {err:?}"
            );
        }
    }

    #[test]
    fn app_key_missing_with_dev_fallback_returns_embedded_key() {
        let got = resolve_app_encryption_key_impl(None, true).expect("dev fallback allowed");
        assert_eq!(got, DEV_AGE_KEY);
    }

    #[test]
    fn app_key_missing_without_dev_fallback_is_configuration_error() {
        let err = resolve_app_encryption_key_impl(None, false).unwrap_err();
        assert!(
            matches!(err, AppError::Configuration(_)),
            "missing key must be a configuration error, got {err:?}"
        );
    }

    // ---- resolve_package_signing_key_32_impl ----

    #[test]
    fn signing_key_valid_env_returns_bytes_in_both_modes() {
        let b64 = valid_sign_env();
        for allow in [true, false] {
            let got = resolve_package_signing_key_32_impl(Some(b64.clone()), allow)
                .expect("valid key accepted");
            assert_eq!(got, [42u8; 32]);
        }
    }

    #[test]
    fn signing_key_invalid_base64_is_validation_error_in_both_modes() {
        for allow in [true, false] {
            let err = resolve_package_signing_key_32_impl(Some("!!!not-base64!!!".into()), allow)
                .unwrap_err();
            assert!(
                matches!(err, AppError::Validation(_)),
                "invalid base64 must be a validation error, got {err:?}"
            );
        }
    }

    #[test]
    fn signing_key_wrong_length_is_validation_error_in_both_modes() {
        let short = b64(&[1u8; 16]);
        for allow in [true, false] {
            let err = resolve_package_signing_key_32_impl(Some(short.clone()), allow).unwrap_err();
            assert!(
                matches!(err, AppError::Validation(_)),
                "wrong key size must be a validation error, got {err:?}"
            );
        }
    }

    #[test]
    fn signing_key_missing_with_dev_fallback_returns_zero_key_only_in_debug() {
        let got = resolve_package_signing_key_32_impl(None, true).expect("dev fallback allowed");
        assert_eq!(got, [0u8; 32]);
    }

    #[test]
    fn signing_key_missing_without_dev_fallback_is_configuration_error_and_never_zero_key() {
        let err = resolve_package_signing_key_32_impl(None, false).unwrap_err();
        // Release semantics: missing key must fail closed (configuration error).
        // No code path in release may return the all-zero dev fallback key.
        assert!(
            matches!(err, AppError::Configuration(_)),
            "missing key must be a configuration error, got {err:?}"
        );
    }

    // ---- validate_production_security_environment ----

    #[test]
    fn validate_skips_when_not_production() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_ENV", "development");
        assert!(validate_production_security_environment().is_ok());
    }

    #[test]
    fn validate_rejects_missing_app_key_in_production() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_ENV", "production");
        let err = validate_production_security_environment().unwrap_err();
        assert!(
            err.contains("GRPC_APP_KEY") && err.starts_with("Configuration error:"),
            "missing app key must be a configuration error, got: {err}"
        );
    }

    #[test]
    fn validate_rejects_missing_signing_key_in_production() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_ENV", "production");
        std::env::set_var("GRPC_APP_KEY", VALID_APP_KEY);
        let err = validate_production_security_environment().unwrap_err();
        assert!(
            err.contains("GRPC_PACKAGE_SIGNING_KEY") && err.starts_with("Configuration error:"),
            "missing signing key must be a configuration error, got: {err}"
        );
    }

    #[test]
    fn validate_rejects_malformed_signing_key_in_production() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_ENV", "production");
        std::env::set_var("GRPC_APP_KEY", VALID_APP_KEY);
        std::env::set_var("GRPC_PACKAGE_SIGNING_KEY", "not-base64!!");
        let err = validate_production_security_environment().unwrap_err();
        assert!(
            err.starts_with("Validation error:"),
            "malformed key must be a validation error, got: {err}"
        );
    }

    #[test]
    fn validate_rejects_wrong_size_signing_key_in_production() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_ENV", "production");
        std::env::set_var("GRPC_APP_KEY", VALID_APP_KEY);
        std::env::set_var("GRPC_PACKAGE_SIGNING_KEY", b64(&[0u8; 16]));
        let err = validate_production_security_environment().unwrap_err();
        assert!(
            err.contains("exactly 32 bytes") && err.starts_with("Validation error:"),
            "wrong key size must be a validation error, got: {err}"
        );
    }

    #[test]
    fn validate_accepts_valid_keys_in_production() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_ENV", "production");
        std::env::set_var("GRPC_APP_KEY", VALID_APP_KEY);
        std::env::set_var("GRPC_PACKAGE_SIGNING_KEY", valid_sign_env());
        assert!(validate_production_security_environment().is_ok());
    }

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

    #[test]
    fn public_resolve_package_signing_key_honors_env_first() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_PACKAGE_SIGNING_KEY", valid_sign_env());
        assert_eq!(
            resolve_package_signing_key_32().expect("env key used"),
            [42u8; 32]
        );
    }

    // ---- pre-existing signing-key-id resolution ----

    #[test]
    fn active_signing_key_missing_env_is_none() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        assert_eq!(resolve_active_signing_key_id(), None);
    }

    #[test]
    fn active_signing_key_empty_or_whitespace_is_none() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_ACTIVE_SIGNING_KEY_ID", "");
        assert_eq!(resolve_active_signing_key_id(), None);
        std::env::set_var("GRPC_ACTIVE_SIGNING_KEY_ID", "   ");
        assert_eq!(resolve_active_signing_key_id(), None);
    }

    #[test]
    fn active_signing_key_present_trimmed() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_key_env();
        std::env::set_var("GRPC_ACTIVE_SIGNING_KEY_ID", "  prod-key-1  ");
        assert_eq!(
            resolve_active_signing_key_id().as_deref(),
            Some("prod-key-1")
        );
    }
}
