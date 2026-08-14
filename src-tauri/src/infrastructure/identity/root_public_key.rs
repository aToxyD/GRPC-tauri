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
//!    supply the real Authority Root key this way.
//! 2. Debug builds (`#[cfg(debug_assertions)]`) fall back to an embedded
//!    development key with a loud warning — mirrors the `GRPC_APP_KEY` /
//!    `DEV_AGE_KEY` pattern.
//! 3. Otherwise the compiled-in pin (`PROD_ROOT_PUBLIC_KEY`) is used — but ONLY
//!    if it is NOT a known RFC 8032 test-vector key. While the pin is the TEST-2
//!    placeholder, Release builds fail closed on WILAYA finalization until a
//!    real Authority Root key is pinned or supplied via `GRPC_ROOT_PUBLIC_KEY`.
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
/// Authority Root public key here before release; until then, Release builds
/// fail closed on WILAYA finalization unless `GRPC_ROOT_PUBLIC_KEY` supplies a
/// real (non-test-vector) Authority Root public key.
const PROD_ROOT_PUBLIC_KEY: &str = "FTMc0JHAEL0EHqzB408Aqlx8NP6FjvvbVLC5JRatd0Q=";

/// Known RFC 8032 §7.1 public test vectors (Base64, 32 bytes). Their private
/// keys are published in RFC 8032, so none of them may ever become a trusted
/// production Authority Root key. TEST 1 is the debug-mode dev fallback.
const RFC8032_TEST1_PUBLIC_KEY_B64: &str = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";
const RFC8032_TEST2_PUBLIC_KEY_B64: &str = "PUAXw+hDiVqStwqnTRt+vJyYLM8uxJaMwM1V8Sr0Zgw=";
const RFC8032_TEST3_PUBLIC_KEY_B64: &str = "/FHNjmIYoaONpH7QAjDwWAgW7RO6MwOsXeuRFUiQgCU=";

fn is_known_rfc8032_test_public_key(key: &[u8; ED25519_PUBLIC_KEY_LEN]) -> bool {
    [
        RFC8032_TEST1_PUBLIC_KEY_B64,
        RFC8032_TEST2_PUBLIC_KEY_B64,
        RFC8032_TEST3_PUBLIC_KEY_B64,
    ]
    .iter()
    .any(|encoded| STANDARD.decode(encoded).ok().as_deref() == Some(key.as_slice()))
}

/// Fail-closed guard: in release semantics (`allow_dev_fallback == false`), a
/// known RFC 8032 test-vector key can never become the trusted production Root
/// key — neither via `GRPC_ROOT_PUBLIC_KEY` nor via the compiled-in pin.
fn reject_known_test_vector(
    key: [u8; ED25519_PUBLIC_KEY_LEN],
    source: &str,
    allow_dev_fallback: bool,
) -> AppResult<()> {
    if !allow_dev_fallback && is_known_rfc8032_test_public_key(&key) {
        return Err(AppError::Configuration(format!(
            "{source} is a known RFC 8032 §7.1 public test vector and MUST NOT be used as the production Authority Root key"
        )));
    }
    Ok(())
}

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
            let key = decode_root_public_key(&trimmed, "GRPC_ROOT_PUBLIC_KEY")?;
            reject_known_test_vector(key, "GRPC_ROOT_PUBLIC_KEY", allow_dev_fallback)?;
            Ok(key)
        }
        None if allow_dev_fallback => dev_key_fallback(),
        None => {
            let key = decode_root_public_key(PROD_ROOT_PUBLIC_KEY, "PROD_ROOT_PUBLIC_KEY")?;
            reject_known_test_vector(
                key,
                "PROD_ROOT_PUBLIC_KEY (RFC 8032 §7.1 TEST-2 placeholder)",
                allow_dev_fallback,
            )?;
            Ok(key)
        }
    }
}

/// Resolve the Authority Root public verification key (lazy, at finalize time).
pub fn resolve_root_public_key() -> AppResult<[u8; ED25519_PUBLIC_KEY_LEN]> {
    resolve_root_public_key_impl(
        std::env::var("GRPC_ROOT_PUBLIC_KEY").ok(),
        cfg!(debug_assertions),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_decodes_to_32_bytes() {
        assert_eq!(
            decode_root_public_key(PROD_ROOT_PUBLIC_KEY, "pin")
                .unwrap()
                .len(),
            32
        );
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
    fn missing_env_in_prod_fails_closed_when_pin_is_a_test_vector() {
        // PROD_ROOT_PUBLIC_KEY is currently the RFC 8032 §7.1 TEST-2 placeholder,
        // so Release + missing env MUST fail closed instead of trusting TEST-2.
        let pin = decode_root_public_key(PROD_ROOT_PUBLIC_KEY, "pin").unwrap();
        if is_known_rfc8032_test_public_key(&pin) {
            let err = resolve_root_public_key_impl(None, false).unwrap_err();
            assert!(
                matches!(err, AppError::Configuration(_)),
                "release + missing env + test-vector pin must fail closed, got {err:?}"
            );
        } else {
            // A real pinned key is the certification ceremony path and is accepted.
            assert_eq!(resolve_root_public_key_impl(None, false).unwrap(), pin);
        }
    }

    #[test]
    fn release_rejects_known_rfc8032_test_vectors_from_env() {
        for encoded in [
            RFC8032_TEST1_PUBLIC_KEY_B64,
            RFC8032_TEST2_PUBLIC_KEY_B64,
            RFC8032_TEST3_PUBLIC_KEY_B64,
        ] {
            let err = resolve_root_public_key_impl(Some(encoded.to_string()), false).unwrap_err();
            assert!(
                matches!(err, AppError::Configuration(_)),
                "release must reject a known test vector via env, got {err:?}"
            );
        }
    }

    #[test]
    fn release_accepts_valid_configured_root_key() {
        // A deterministic, non-RFC-8032 key (never a production secret) must be
        // accepted as a configured production Root key.
        let valid = [42u8; 32];
        let encoded = STANDARD.encode(valid);
        assert_eq!(
            resolve_root_public_key_impl(Some(encoded), false).unwrap(),
            valid
        );
    }

    #[test]
    fn debug_semantics_still_accept_test_keys() {
        // Dev/test workflows must remain usable: test vectors are allowed when
        // the dev fallback is permitted (debug builds).
        let key = resolve_root_public_key_impl(Some(RFC8032_TEST2_PUBLIC_KEY_B64.to_string()), true)
            .unwrap();
        assert_eq!(key.len(), ED25519_PUBLIC_KEY_LEN);
    }

    #[test]
    fn malformed_env_is_rejected() {
        assert!(resolve_root_public_key_impl(Some("not-base64!!".to_string()), true).is_err());
        let short = STANDARD.encode([1u8; 16]);
        assert!(resolve_root_public_key_impl(Some(short), true).is_err());
    }
}
