//! Identity bootstrap & Challenge–Response commands (B5).
//!
//! RFC 2026-08-04-node-identity-trust §3.6–3.7 / ADR-0038.
//!
//! Thin IPC handlers — dispatch + minimal I/O only, no business logic. All
//! bootstrap commands are PRE-AUTH: they run before any session exists. The
//! offline Root flow ("Root signs fully offline") means no command here ever
//! touches a Root key — only the operator-facing request/finalize files.

use crate::application::services::{
    IdentityBootstrapStatusService, IdentityChallengeService, IdentityProvisioningService,
};
use crate::commands::common::{db_mut_or_command_error, db_ref_or_command_error};
use crate::commands::types::AppState;
use crate::domain::identity::{ChallengeMessage, IdentityBootstrapState, IdentityCertificate};
use crate::errors::{into_command_error, AppError};
use crate::infrastructure::identity::{AdminKeyProvider, NodeKeyStore};
use crate::models::{LoginResponse, User};
use tauri::State;

/// Default on-disk GRPC data dir (`dirs::data_dir()/GRPC`) shared by the node
/// key store and the `.adminkey` provider.
pub(crate) fn default_data_dir() -> crate::errors::AppResult<std::path::PathBuf> {
    AdminKeyProvider::default_data_dir()
}

pub(crate) fn node_key_store() -> NodeKeyStore {
    let dir = default_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("GRPC"));
    NodeKeyStore::new(dir)
}

pub(crate) fn adminkey_provider() -> AdminKeyProvider {
    let dir = default_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("GRPC"));
    AdminKeyProvider::new(dir)
}

fn challenge_service(state: &AppState) -> IdentityChallengeService {
    IdentityChallengeService::with_default_verifier(
        state.identity_challenge.clone(),
        adminkey_provider(),
        state.password_port.clone(),
    )
}

/// Derived bootstrap state for the local node (pure projection).
#[tauri::command]
pub fn get_identity_status(state: State<AppState>) -> Result<IdentityBootstrapState, String> {
    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    IdentityBootstrapStatusService::compute(db, &node_key_store(), &adminkey_provider())
        .map_err(into_command_error)
}

/// Begin offline WILAYA bootstrap: generate the node keypair, persist the node
/// secret, and write the UNSIGNED certificate (CSR) to `request_file_path` for
/// the operator to carry to the Authority Root. Nothing else is persisted.
#[tauri::command]
pub fn begin_wilaya_provision(
    state: State<AppState>,
    request_file_path: String,
) -> Result<IdentityCertificate, String> {
    crate::domain::validation::validate_file_path(&request_file_path, &["json"])
        .map_err(into_command_error)?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let subject_id = uuid::Uuid::new_v4();
    let request = IdentityProvisioningService::new(db)
        .generate_wilaya_request(subject_id, &node_key_store())
        .map_err(into_command_error)?;
    let json = serde_json::to_string_pretty(&request)
        .map_err(|e| into_command_error(AppError::Internal(format!("CSR serialization failed: {e}"))))?;
    std::fs::write(&request_file_path, json)
        .map_err(|e| into_command_error(AppError::Io(e)))?;

    Ok(request)
}

/// Finalize offline WILAYA bootstrap with the Root-signed certificate read from
/// `cert_file_path`. Idempotent: an identical re-presentation is a no-op.
#[tauri::command]
pub fn finalize_wilaya_provision(
    state: State<AppState>,
    cert_file_path: String,
) -> Result<crate::application::services::FinalizeWilayaProvisionResult, String> {
    crate::domain::validation::validate_file_path(&cert_file_path, &["json"])
        .map_err(into_command_error)?;

    let json = std::fs::read_to_string(&cert_file_path)
        .map_err(|e| into_command_error(AppError::Io(e)))?;
    let signed_cert: IdentityCertificate = serde_json::from_str(&json)
        .map_err(|e| into_command_error(AppError::FileFormat(format!("Malformed signed certificate file: {e}"))))?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let now = chrono::Utc::now().to_rfc3339();
    IdentityProvisioningService::new(db)
        .finalize_wilaya_provision(&signed_cert, &node_key_store(), &now)
        .map_err(into_command_error)
}

/// Issue the FIRST ADMIN key bound to `subject_username`, protecting the
/// portable `.adminkey` with `passphrase`. Links the `users` row additively.
#[tauri::command]
pub fn issue_first_admin_key(
    state: State<AppState>,
    subject_username: String,
    passphrase: String,
) -> Result<IdentityCertificate, String> {
    if passphrase.is_empty() {
        return Err(into_command_error(AppError::Validation(
            crate::errors::ValidationError::Required {
                field: "passphrase".into(),
            },
        )));
    }
    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let now = chrono::Utc::now().to_rfc3339();
    IdentityProvisioningService::new(db)
        .issue_first_admin_key(
            &subject_username,
            &passphrase,
            &node_key_store(),
            &adminkey_provider(),
            &now,
        )
        .map_err(into_command_error)
}

/// Begin a one-shot Challenge–Response login.
#[tauri::command]
pub fn begin_challenge(state: State<AppState>) -> Result<ChallengeMessage, String> {
    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    challenge_service(&state)
        .begin_with_db(db)
        .map_err(into_command_error)
}

/// Complete a Challenge–Response login with the `.adminkey` passphrase.
/// The backend decrypts the key and signs the challenge in Rust — the frontend
/// only ever passes the passphrase.
#[tauri::command]
pub fn complete_challenge(
    state: State<AppState>,
    session_id: String,
    passphrase: String,
) -> Result<LoginResponse, String> {
    if passphrase.is_empty() {
        return Err(into_command_error(AppError::Validation(
            crate::errors::ValidationError::Required {
                field: "passphrase".into(),
            },
        )));
    }
    let parsed_session_id = uuid::Uuid::parse_str(&session_id).map_err(|e| {
        into_command_error(AppError::Internal(format!("Invalid challenge session id: {e}")))
    })?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let established = challenge_service(&state)
        .complete_with_passphrase(db, &parsed_session_id, &passphrase)
        .map_err(into_command_error)?;

    let session = established.session.clone();
    if let Ok(mut current_session) = state.current_session.lock() {
        *current_session = Some(established.session);
    }

    let user = User {
        id: session.user_snapshot.id.clone(),
        username: session.user_snapshot.username.clone(),
        password_hash: String::new(),
        role: session.user_snapshot.role.clone(),
        created_at: session.user_snapshot.created_at,
        node_id: String::new(),
    };

    Ok(LoginResponse {
        success: true,
        user: Some(user),
        message: "تم تسجيل الدخول بنجاح".to_string(),
        requires_configuration: established.requires_configuration,
    })
}
