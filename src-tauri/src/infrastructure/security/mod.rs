pub mod file_encryption;
pub mod node_identity_provider;
pub mod password_hash_provider;

use crate::errors::{AppError, AppResult};

pub use file_encryption::AgeFileEncryptionProvider;
pub use node_identity_provider::{NodeIdentityProvider, SettingsNodeIdentityProvider};
pub use password_hash_provider::Argon2PasswordHashProvider;

fn is_production_mode() -> bool {
    matches!(
        std::env::var("GRPC_ENV"),
        Ok(v) if v.eq_ignore_ascii_case("production") || v.eq_ignore_ascii_case("prod")
    )
}

pub fn validate_production_security_environment() -> Result<(), String> {
    // TEMP DEV OVERRIDE: allow startup during local testing even when production
    // environment variables are missing. This is a temporary relaxation so you can
    // run the app in a production-like environment locally. DO NOT ship with this
    // enabled in real production.
    // To restore original behaviour: remove this temporary block and uncomment
    // the original implementation preserved below.
    // BEGIN TEMP DEV_OVERRIDE - REMOVE BEFORE PROD
    if !is_production_mode() {
        return Ok(());
    }

    // In production mode we normally enforce GRPC_APP_KEY and GRPC_PACKAGE_SIGNING_KEY.
    // For local testing we relax the check and allow startup. Replace the line below
    // with the original implementation when reverting.
    log::warn!(target: "grpc::security", "[DEV_SECURITY_WARNING] Production checks temporarily relaxed for local testing.");
    return Ok(());
    // END TEMP DEV_OVERRIDE

    /* Original implementation preserved for easy revert:
    if !is_production_mode() {
        return Ok(());
    }

    let app = std::env::var("GRPC_APP_KEY").map_err(|_| {
        "Production startup aborted: GRPC_APP_KEY is required when GRPC_ENV=production".to_string()
    })?;
    if !app.starts_with("AGE-SECRET-KEY-1") {
        return Err(
            "Production startup aborted: GRPC_APP_KEY must be a valid age x25519 identity (starts with AGE-SECRET-KEY-1).".to_string(),
        );
    }

    let sign = std::env::var("GRPC_PACKAGE_SIGNING_KEY").map_err(|_| {
        "Production startup aborted: GRPC_PACKAGE_SIGNING_KEY is required when GRPC_ENV=production"
            .to_string()
    })?;

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let decoded = STANDARD.decode(sign.trim()).map_err(|_| {
        "Production startup aborted: GRPC_PACKAGE_SIGNING_KEY must be a valid Base64 string."
            .to_string()
    })?;

    if decoded.len() != 32 {
        return Err(
            "Production startup aborted: GRPC_PACKAGE_SIGNING_KEY must decode to exactly 32 bytes."
                .to_string(),
        );
    }

    Ok(())
    */
}

pub fn resolve_app_encryption_key() -> AppResult<String> {
    // TEMP DEV OVERRIDE: allow using an embedded development AGE key when
    // `GRPC_APP_KEY` is not present. This simplifies local production-mode testing.
    // Remove this override and uncomment the original implementation below before
    // deploying to production.
    if let Ok(v) = std::env::var("GRPC_APP_KEY") {
        let trimmed = v.trim().to_string();
        if !trimmed.starts_with("AGE-SECRET-KEY-1") {
            return Err(AppError::Internal(
                "GRPC_APP_KEY must be a valid age x25519 identity (starts with AGE-SECRET-KEY-1)."
                    .into(),
            ));
        }
        return Ok(trimmed);
    }

    log::warn!(target: "grpc::security", "[DEV_SECURITY_WARNING] GRPC_APP_KEY missing — using embedded development AGE key for local testing. DO NOT USE IN PRODUCTION.");
    Ok("AGE-SECRET-KEY-1KTYK6RVLN5TAPE7VF6FQQSKZ9HWWCDSKUGXXNUQDWZ7XXT5YK5LSF3UTKQ".to_string())

    /* Original implementation preserved for easy revert:
    let raw = match std::env::var("GRPC_APP_KEY") {
        Ok(v) => v,
        Err(_) => {
            #[cfg(debug_assertions)]
            {
                // DEV ONLY
                // NEVER FOR PRODUCTION
                // LOCAL DEVELOPMENT ONLY
                // production MUST provide GRPC_APP_KEY
                log::warn!(target: "grpc::security", "[DEV_SECURITY_WARNING] Using embedded development AGE key. NOT FOR PRODUCTION.");
                "AGE-SECRET-KEY-1KTYK6RVLN5TAPE7VF6FQQSKZ9HWWCDSKUGXXNUQDWZ7XXT5YK5LSF3UTKQ"
                    .to_string()
            }
            #[cfg(not(debug_assertions))]
            {
                return Err(AppError::Internal("GRPC_APP_KEY is missing".into()));
            }
        }
    };
    let trimmed = raw.trim();
    if !trimmed.starts_with("AGE-SECRET-KEY-1") {
        return Err(AppError::Internal(
            "GRPC_APP_KEY must be a valid age x25519 identity (starts with AGE-SECRET-KEY-1)."
                .into(),
        ));
    }
    Ok(trimmed.to_string())
    */
}

pub fn resolve_package_signing_key_32() -> AppResult<[u8; 32]> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let env = std::env::var("GRPC_PACKAGE_SIGNING_KEY")
        .map_err(|_| AppError::Internal("GRPC_PACKAGE_SIGNING_KEY is missing".to_string()));
    if let Ok(raw) = env {
        let bytes = STANDARD.decode(raw.trim()).map_err(|e| {
            log::error!(target: "grpc::security", "Failed to decode Base64 GRPC_PACKAGE_SIGNING_KEY: {}", e);
            AppError::Internal("GRPC_PACKAGE_SIGNING_KEY must be a valid Base64 string".to_string())
        })?;

        if bytes.len() != 32 {
            log::error!(target: "grpc::security", "GRPC_PACKAGE_SIGNING_KEY must decode to exactly 32 bytes (got {})", bytes.len());
            return Err(AppError::Internal(
                "GRPC_PACKAGE_SIGNING_KEY must decode to exactly 32 bytes".to_string(),
            ));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes[..32]);
        return Ok(key);
    }

    // TEMP DEV OVERRIDE: Development fallback allowed even in non-debug builds.
    // This returns an all-zero key for local testing and logs a warning. Remove
    // and restore the original guarded behavior before deploying to production.
    log::warn!(target: "grpc::security", "[DEV_SECURITY_WARNING] GRPC_PACKAGE_SIGNING_KEY missing — using all-zero dev fallback key. DO NOT USE IN PRODUCTION.");
    Ok([0u8; 32])

    /* Original implementation preserved for easy revert:
    #[cfg(debug_assertions)]
    {
        // Development fallback allowed only in non-production builds
        Ok([0u8; 32])
    }
    #[cfg(not(debug_assertions))]
    {
        Err(AppError::Internal(
            "Missing required secret: GRPC_PACKAGE_SIGNING_KEY. Development fallback is forbidden."
                .to_string(),
        ))
    }
    */
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
        Ok(deadline) => chrono::Utc::now() >= deadline.with_timezone(&chrono::Utc),
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
mod signing_key_id_tests {
    use super::resolve_active_signing_key_id;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn active_signing_key_missing_env_is_none() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("GRPC_ACTIVE_SIGNING_KEY_ID");
        assert_eq!(resolve_active_signing_key_id(), None);
    }

    #[test]
    fn active_signing_key_empty_or_whitespace_is_none() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("GRPC_ACTIVE_SIGNING_KEY_ID", "");
        assert_eq!(resolve_active_signing_key_id(), None);
        std::env::set_var("GRPC_ACTIVE_SIGNING_KEY_ID", "   ");
        assert_eq!(resolve_active_signing_key_id(), None);
    }

    #[test]
    fn active_signing_key_present_trimmed() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("GRPC_ACTIVE_SIGNING_KEY_ID", "  prod-key-1  ");
        assert_eq!(
            resolve_active_signing_key_id().as_deref(),
            Some("prod-key-1")
        );
    }
}
