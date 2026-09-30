use crate::application::authz::{self, Action, Principal, ResourceContext};
use crate::application::services::UserContext;
use crate::domain::session::CurrentSession;
use crate::errors::{into_command_error, AppError, AppResult, ValidationError};
use crate::infrastructure::identity::{AdminKeyProvider, NodeKeyStore};
use crate::infrastructure::security::AppKeyStore;

/// Resolved on-disk identity data dir shared by the node key store, the
/// `.adminkey` provider, and the `appkey.age` store (ADR-0062): the single
/// resolver in `infrastructure/identity/data_dir.rs` owns this decision.
pub(crate) fn default_data_dir() -> crate::errors::AppResult<std::path::PathBuf> {
    crate::infrastructure::identity::identity_data_dir()
}

/// Node key store provider. ADR-0062 §2 is fail-closed: a rejected
/// `GRPC_IDENTITY_DATA_DIR` is propagated, never substituted with another
/// directory, so no provisioning path can write identity material elsewhere.
pub(crate) fn node_key_store() -> AppResult<NodeKeyStore> {
    Ok(NodeKeyStore::new(default_data_dir()?))
}

/// `.adminkey` provider. Propagates the ADR-0062 resolver error like
/// [`node_key_store`].
pub(crate) fn adminkey_provider() -> AppResult<AdminKeyProvider> {
    Ok(AdminKeyProvider::new(default_data_dir()?))
}

/// `appkey.age` store provider (ADR-0041) sharing the GRPC data directory.
/// Propagates the ADR-0062 resolver error like [`node_key_store`].
pub(crate) fn appkey_store() -> AppResult<AppKeyStore> {
    Ok(AppKeyStore::new(default_data_dir()?))
}

pub fn principal_from_session(session: &CurrentSession) -> Principal {
    Principal {
        user_id: session.user_id.clone(),
        username: session.username.clone(),
        role: session.user_role.clone(),
        session_id: Some(session.session_id.clone()),
    }
}

pub fn user_ctx_from_session(session: &CurrentSession) -> UserContext {
    UserContext::new(
        &session.user_id,
        &session.username,
        Some(&session.session_id),
    )
}

pub fn user_ctx_from_parts<'a>(
    user_id: &'a str,
    username: &'a str,
    session_id: Option<&'a str>,
) -> UserContext {
    UserContext::new(user_id, username, session_id)
}

pub fn authorize_or_command_error(
    principal: &Principal,
    action: Action,
    resource: &ResourceContext,
) -> Result<(), String> {
    authz::authorize(principal, action, resource)
        .map_err(|e| into_command_error(AppError::Authorization(e)))
}

pub fn db_closed_command_error() -> String {
    into_command_error(AppError::Internal("Database is closed".to_string()))
}

pub fn db_ref_or_command_error<T>(db: Option<&T>) -> Result<&T, String> {
    db.ok_or_else(db_closed_command_error)
}

pub fn db_mut_or_command_error<T>(db: Option<&mut T>) -> Result<&mut T, String> {
    db.ok_or_else(db_closed_command_error)
}

pub fn db_mut_or_app_error<T>(db: Option<&mut T>) -> Result<&mut T, AppError> {
    db.ok_or_else(|| AppError::Internal("Database is closed".to_string()))
}

pub fn decrypt_utf8_content_or_command_error(
    crypto_port: &crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider,
    encrypted_bytes: &[u8],
    decrypt_error_prefix: &str,
    utf8_error_prefix: &str,
) -> Result<String, String> {
    let decrypted_bytes = crypto_port.decrypt_data(encrypted_bytes).map_err(|e| {
        into_command_error(AppError::Internal(format!(
            "{}: {}",
            decrypt_error_prefix, e
        )))
    })?;
    String::from_utf8(decrypted_bytes).map_err(|e| {
        into_command_error(AppError::Internal(format!("{}: {}", utf8_error_prefix, e)))
    })
}

pub fn validation_errors_to_command_error(validation_errors: &[String]) -> String {
    into_command_error(AppError::Validation(ValidationError::InvalidFormat {
        field: "content".into(),
        message: validation_errors.join(" | "),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::infrastructure::identity::data_dir::{identity_data_dir, IDENTITY_DATA_DIR_ENV};
    use crate::infrastructure::security::test_support::lock_security_test_env;

    fn temp_fallback_dir() -> std::path::PathBuf {
        std::env::temp_dir().join("GRPC")
    }

    /// The G helpers used to swallow every resolver error via
    /// `unwrap_or_else(|_| temp_dir().join("GRPC"))`. They must return
    /// `AppResult` so a rejected override can never be converted into a store
    /// rooted at the temp fallback.
    #[test]
    fn g_helpers_propagate_the_resolver_result() {
        let _env = lock_security_test_env();
        // The three helpers are thin wrappers: each one must be exactly the
        // resolver result, with no substitution applied on either branch.
        let results: Vec<AppResult<std::path::PathBuf>> = vec![
            node_key_store().map(|s| s.file_path()),
            adminkey_provider().map(|p| p.file_path()),
            appkey_store().map(|s| s.file_path()),
        ];
        for (index, result) in results.into_iter().enumerate() {
            match result {
                Ok(path) => {
                    assert_eq!(
                        path.parent(),
                        Some(identity_data_dir().unwrap().as_path()),
                        "helper {index} must be rooted at the resolver's directory"
                    );
                    assert_ne!(
                        path.parent().unwrap(),
                        temp_fallback_dir(),
                        "helper {index} must never substitute the temp fallback"
                    );
                }
                Err(e) => assert!(
                    matches!(e, AppError::Configuration(_)),
                    "helper {index} surfaced a non-configuration resolver error: {e:?}"
                ),
            }
        }
    }

    /// A relative `GRPC_IDENTITY_DATA_DIR` is a `Configuration` rejection, so no
    /// helper may hand back a temp-rooted store. The resolver caches its first
    /// *successful* resolution, so this asserts the invariant that holds under
    /// either cache state: the result is never the temp fallback.
    #[test]
    fn a_relative_override_never_yields_a_temp_rooted_store() {
        let _env = lock_security_test_env();
        std::env::set_var(IDENTITY_DATA_DIR_ENV, "relative-identity-dir");

        let observed = [
            node_key_store().map(|s| s.file_path()),
            adminkey_provider().map(|p| p.file_path()),
            appkey_store().map(|s| s.file_path()),
        ];

        std::env::remove_var(IDENTITY_DATA_DIR_ENV);

        for (index, result) in observed.into_iter().enumerate() {
            match result {
                // Cold cache: the rejection is propagated verbatim.
                Err(e) => assert!(
                    matches!(e, AppError::Configuration(_)),
                    "helper {index} must propagate the configuration error: {e:?}"
                ),
                // Warm cache (§3): the pinned platform directory is returned.
                // Either way it is never the temp fallback.
                Ok(path) => assert_ne!(
                    path.parent().unwrap(),
                    temp_fallback_dir(),
                    "helper {index} converted a rejected override into the temp fallback"
                ),
            }
        }
    }

    /// The historical fallback is gone from G: an absent environment must not
    /// change the resolved directory, and the three helpers must agree with the
    /// single resolver.
    #[test]
    fn the_temp_fallback_is_absent_from_the_command_helpers() {
        let _env = lock_security_test_env();
        std::env::remove_var(IDENTITY_DATA_DIR_ENV);

        let resolved = identity_data_dir().expect("the platform default is resolvable");
        for (index, result) in [
            node_key_store().map(|s| s.file_path()),
            adminkey_provider().map(|p| p.file_path()),
            appkey_store().map(|s| s.file_path()),
        ]
        .into_iter()
        .enumerate()
        {
            let path = result.unwrap_or_else(|e| {
                panic!("helper {index} must resolve with the env unset: {e:?}")
            });
            assert_eq!(
                path.parent(),
                Some(resolved.as_path()),
                "helper {index} must share the resolver's directory"
            );
        }
    }
}
