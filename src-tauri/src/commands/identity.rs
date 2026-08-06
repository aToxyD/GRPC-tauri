//! Identity bootstrap & Challenge–Response commands (B5).
//!
//! RFC 2026-08-04-node-identity-trust §3.6–3.7 / ADR-0038.
//!
//! Thin IPC handlers — dispatch + minimal I/O only, no business logic. All
//! bootstrap commands are PRE-AUTH: they run before any session exists. The
//! offline Root flow ("Root signs fully offline") means no command here ever
//! touches a Root key — only the operator-facing request/finalize files.

use crate::application::services::{
    FinalizeUnitProvisionResult, IdentityBootstrapStatusService, IdentityChallengeService,
    IdentityProvisioningService, IdentityTrustAnchorService, InstallWilayaCertificateResult,
};
use crate::commands::common::{
    adminkey_provider, db_mut_or_command_error, db_ref_or_command_error, node_key_store,
};
use crate::commands::types::AppState;
use crate::domain::identity::{
    ChallengeMessage, IdentityBootstrapState, IdentityCertificate, SubjectType,
};
use crate::errors::{into_command_error, AppError};
use crate::models::{LoginResponse, User};
use tauri::State;

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

/// Begin UNIT bootstrap: resolve the LOCAL `subject_id` (the node's own `units`
/// row), generate the UNIT keypair, persist the node secret, and write the
/// UNSIGNED UNIT certificate (CSR) to `request_file_path` for the operator to
/// carry to the WILAYA node. Nothing else is persisted.
#[tauri::command]
pub fn begin_unit_provision(
    state: State<AppState>,
    request_file_path: String,
) -> Result<IdentityCertificate, String> {
    crate::domain::validation::validate_file_path(&request_file_path, &["json"])
        .map_err(into_command_error)?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let service = IdentityProvisioningService::new(db);
    let subject_id = service.resolve_local_unit_subject_id().map_err(into_command_error)?;
    let request = service
        .generate_identity_request(SubjectType::Unit, subject_id, &node_key_store())
        .map_err(into_command_error)?;
    let json = serde_json::to_string_pretty(&request)
        .map_err(|e| into_command_error(AppError::Internal(format!("CSR serialization failed: {e}"))))?;
    std::fs::write(&request_file_path, json)
        .map_err(|e| into_command_error(AppError::Io(e)))?;

    Ok(request)
}

/// WILAYA side: sign a UNIT CSR (RFC §3.12 D2). The CSR MUST be an unsigned
/// UNIT certificate; the `subject_id` MUST match a known local unit (validated,
/// never overridden); the ACTIVE local WILAYA signs it.
#[tauri::command]
pub fn sign_unit_identity_request(
    state: State<AppState>,
    request_json: String,
) -> Result<IdentityCertificate, String> {
    let request: IdentityCertificate = serde_json::from_str(&request_json)
        .map_err(|e| into_command_error(AppError::FileFormat(format!("Malformed UNIT CSR: {e}"))))?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    IdentityProvisioningService::new(db)
        .sign_unit_identity_request(&request, &node_key_store())
        .map_err(into_command_error)
}

/// Finalize UNIT bootstrap with the WILAYA-signed certificate read from
/// `cert_file_path`. The issuer is resolved via the certificate's
/// `issuer_identity_id` and verified (exists + ACTIVE + WILAYA). Idempotent:
/// an identical re-presentation is a no-op.
#[tauri::command]
pub fn finalize_unit_provision(
    state: State<AppState>,
    cert_file_path: String,
) -> Result<FinalizeUnitProvisionResult, String> {
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
        .finalize_unit_provision(&signed_cert, &node_key_store(), &now)
        .map_err(into_command_error)
}

/// Install (or idempotently re-confirm) the ACTIVE WILAYA certificate as the
/// UNIT's LOCAL TRUST ANCHOR (strict two-step flow, RFC §3.12). Reads the
/// Root-signed WILAYA certificate from `cert_file_path`; never bundles it.
#[tauri::command]
pub fn install_wilaya_certificate(
    state: State<AppState>,
    cert_file_path: String,
) -> Result<InstallWilayaCertificateResult, String> {
    crate::domain::validation::validate_file_path(&cert_file_path, &["json"])
        .map_err(into_command_error)?;

    let json = std::fs::read_to_string(&cert_file_path)
        .map_err(|e| into_command_error(AppError::Io(e)))?;
    let signed_cert: IdentityCertificate = serde_json::from_str(&json)
        .map_err(|e| into_command_error(AppError::FileFormat(format!("Malformed certificate file: {e}"))))?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let now = chrono::Utc::now().to_rfc3339();
    IdentityTrustAnchorService::new(db)
        .install_wilaya_certificate(&signed_cert, &now)
        .map_err(into_command_error)
}
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
        identity_challenge_required: true,
    })
}
