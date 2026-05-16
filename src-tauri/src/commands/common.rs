use crate::application::authz::{self, Action, Principal, ResourceContext};
use crate::application::services::UserContext;
use crate::domain::session::CurrentSession;
use crate::errors::{into_command_error, AppError, ValidationError};

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
