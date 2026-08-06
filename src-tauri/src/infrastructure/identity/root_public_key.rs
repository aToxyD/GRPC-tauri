//! Authority Root public verification key (B5).
//!
//! RFC 2026-08-04-node-identity-trust §3.6 / ADR-0038.
//!
//! Offline bootstrap ("Root signs fully offline"): the finalized WILAYA
//! certificate carries a signature by the Authority Root. This module resolves
//! the Root public key used to verify that signature at `finalize` time.
//!
//! Resolution order:
//! 1. `GRPC_ROOT_PUBLIC_KEY` (Base64, exactly 32 bytes) — production fleets
//!    override the compiled-in pin when the Authority Root key is rotated.
//! 2. Debug builds (`#[cfg(debug_assertions)]`) fall back to an embedded
//!    development key with a loud warning — mirrors the `GRPC_APP_KEY` /
//!    `DEV_AGE_KEY` pattern.
//! 3. Otherwise the compiled-in pin (`PROD_ROOT_PUBLIC_KEY`) is used.
//!
//! Resolved LAZILY by the finalize command, NOT by
//! `validate_production_security_environment()`: existing fleets that never use
//! offline bootstrap are unaffected.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::errors::{AppError, AppResult};

const ED25519_PUBLIC_KEY_LEN: usize = 32;

/// Compiled-in Authority Root verification key (Base64, 32 bytes).
///
/// RFC 8032 §7.1 TEST 2 vector placeholder. Certification MUST pin the real
/// Authority Root public key here before release; until then, production
/// finalizations fail closed unless `GRPC_ROOT_PUBLIC_KEY` is supplied.
const PROD_ROOT_PUBLIC_KEY: &str = "PUAXw+hDiVqStwqnTRt+vJyYLM8uxJaMwM1V8Sr0Zgw=";

/// Debug-only embedded Root key (RFC 8032 §7.1 TEST 1). Reachable ONLY in
/// debug builds; release builds must supply `GRPC_ROOT_PUBLIC_KEY` or the pin.
#[cfg(debug_assertions)]
const DEV_ROOT_PUBLIC_KEY: &str = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";

#[cfg(debug_assertions)]
fn dev_key_fallback() -> AppResult<[u8; ED25519_PUBLIC_KEY_LEN]> {
    log::warn!(target: "grpc::identity", "[DEV_SECURITY_WARNING] GRPC_ROOT_PUBLIC_KEY missing — using embedded development Root key. NOT FOR PRODUCTION.");
    decode_root_public_key(DEV_ROOT_PUBLIC_KEY, "DEV_ROOT_PUBLIC_KEY")
}

#[cfg(not(debug_assertions))]
fn dev_key_fallback() -> AppResult<[u8; ED25519_PUBLIC_KEY_LEN]> {
    Err(AppError::Internal(
        "DEV_ROOT_PUBLIC_KEY fallback is unavailable in release builds".to_string(),
    ))
}

fn decode_root_public_key(raw: &str, source: &str) -> AppResult<[u8; ED25519_PUBLIC_KEY_LEN]> {
    let decoded = STANDARD
        .decode(raw.trim())
        .map_err(|e| AppError::Internal(format!("{source}: invalid Base64: {e}")))?;
    decoded.try_into().map_err(|v: Vec<u8>| {
        AppError::Internal(format!(
            "{source}: expected {ED25519_PUBLIC_KEY_LEN} bytes, got {}",
            v.len()
        ))
    })
}

/// Shared core so both debug and release semantics are testable in one binary.
fn resolve_root_public_key_impl(
    env_raw: Option<String>,
    allow_dev_fallback: bool,
) -> AppResult<[u8; ED25519_PUBLIC_KEY_LEN]> {
    match env_raw {
        Some(raw) => {
            let trimmed = raw.trim().to_string();
            decode_root_public_key(&trimmed, "GRPC_ROOT_PUBLIC_KEY")
        }
        None if allow_dev_fallback => dev_key_fallback(),
        None => decode_root_public_key(PROD_ROOT_PUBLIC_KEY, "PROD_ROOT_PUBLIC_KEY"),
    }
}

/// Resolve the Authority Root public verification key (lazy, at finalize time).
pub fn resolve_root_public_key() -> AppResult<[u8; ED25519_PUBLIC_KEY_LEN]> {
    resolve_root_public_key_impl(std::env::var("GRPC_ROOT_PUBLIC_KEY").ok(), cfg!(debug_assertions))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_decodes_to_32_bytes() {
        assert_eq!(decode_root_public_key(PROD_ROOT_PUBLIC_KEY, "pin").unwrap().len(), 32);
    }

    #[test]
    fn env_override_wins() {
        let key = resolve_root_public_key_impl(
            Some("11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=".to_string()),
            true,
        )
        .unwrap();
        assert_eq!(
            key,
            decode_root_public_key("11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=", "env").unwrap()
        );
    }

    #[test]
    fn debug_fallback_used_only_when_allowed() {
        let key = resolve_root_public_key_impl(None, true).unwrap();
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn missing_env_in_prod_uses_pin() {
        let key = resolve_root_public_key_impl(None, false).unwrap();
        assert_eq!(key, decode_root_public_key(PROD_ROOT_PUBLIC_KEY, "pin").unwrap());
    }

    #[test]
    fn malformed_env_is_rejected() {
        assert!(resolve_root_public_key_impl(Some("not-base64!!".to_string()), true).is_err());
        let short = STANDARD.encode([1u8; 16]);
        assert!(resolve_root_public_key_impl(Some(short), true).is_err());
    }
}
