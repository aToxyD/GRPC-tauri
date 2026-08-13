//! Identity bootstrap, rotation, & Challenge–Response commands (B5 / B7).
//!
//! RFC 2026-08-04-node-identity-trust §3.3–3.7, §3.12 / ADR-0038.
//!
//! Thin IPC handlers — dispatch + minimal I/O only, no business logic.
//! Bootstrap commands are PRE-AUTH: they run before any session exists, and
//! the offline Root flow means no command here ever touches a Root key — only
//! the operator-facing request/finalize files. Rotation commands (§3.3 / §3.12)
//! are POST-AUTH: they gate on `Action::RotateCredential` /
//! `Action::ReissueCredential` and delegate all decisions to the
//! `IdentityRotationCoordinator`.

use crate::application::authz::Action;
use crate::application::services::identity_challenge_service::CHALLENGE_RATE_LIMIT_KEY;
use crate::application::services::{
    AuditService, FinalizeUnitProvisionResult, IdentityBootstrapStatusService,
    IdentityChallengeService, IdentityProvisioningService, IdentityRotationCoordinator,
    IdentityTrustAnchorService, InstallWilayaCertificateResult, RotationFinalizeOutcome,
    RotationOperation, RotationPlan, SignedUnitRotation,
};
use crate::commands::common::{
    adminkey_provider, db_mut_or_command_error, db_ref_or_command_error, node_key_store,
};
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::domain::audit::{AuditAction, EntityType};
use crate::domain::identity::{
    ChallengeMessage, IdentityBootstrapState, IdentityCertificate, SubjectType,
};
use crate::domain::session::CurrentSession;
use crate::errors::{into_command_error, AppError};
use crate::models::{LoginResponse, User};
use tauri::State;

fn challenge_service(state: &AppState) -> IdentityChallengeService {
    IdentityChallengeService::with_default_verifier(
        state.identity_challenge.clone(),
        adminkey_provider(),
        state.password_port.clone(),
        state.rate_limiter.clone(),
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
    let json = serde_json::to_string_pretty(&request).map_err(|e| {
        into_command_error(AppError::Internal(format!("CSR serialization failed: {e}")))
    })?;
    std::fs::write(&request_file_path, json).map_err(|e| into_command_error(AppError::Io(e)))?;

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
    let signed_cert: IdentityCertificate = serde_json::from_str(&json).map_err(|e| {
        into_command_error(AppError::FileFormat(format!(
            "Malformed signed certificate file: {e}"
        )))
    })?;

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
    let subject_id = service
        .resolve_local_unit_subject_id()
        .map_err(into_command_error)?;
    let request = service
        .generate_identity_request(SubjectType::Unit, subject_id, &node_key_store())
        .map_err(into_command_error)?;
    let json = serde_json::to_string_pretty(&request).map_err(|e| {
        into_command_error(AppError::Internal(format!("CSR serialization failed: {e}")))
    })?;
    std::fs::write(&request_file_path, json).map_err(|e| into_command_error(AppError::Io(e)))?;

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
    let request: IdentityCertificate = serde_json::from_str(&request_json).map_err(|e| {
        into_command_error(AppError::FileFormat(format!("Malformed UNIT CSR: {e}")))
    })?;

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
    let signed_cert: IdentityCertificate = serde_json::from_str(&json).map_err(|e| {
        into_command_error(AppError::FileFormat(format!(
            "Malformed signed certificate file: {e}"
        )))
    })?;

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
    let signed_cert: IdentityCertificate = serde_json::from_str(&json).map_err(|e| {
        into_command_error(AppError::FileFormat(format!(
            "Malformed certificate file: {e}"
        )))
    })?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let now = chrono::Utc::now().to_rfc3339();
    IdentityTrustAnchorService::new(db)
        .install_wilaya_certificate(&signed_cert, &now)
        .map_err(into_command_error)
}

/// Authorization action matching the requested rotation operation.
fn rotation_action(operation: RotationOperation) -> Action {
    match operation {
        RotationOperation::Rotate => Action::RotateCredential,
        RotationOperation::ReIssue => Action::ReissueCredential,
    }
}

fn parse_rotation_operation(raw: &str) -> Result<RotationOperation, String> {
    RotationOperation::parse(raw).ok_or_else(|| {
        into_command_error(AppError::Validation(
            crate::errors::ValidationError::InvalidFormat {
                field: "operation".into(),
                message: format!(
                    "Unknown rotation operation '{raw}' (expected ROTATE or RE-ISSUE)"
                ),
            },
        ))
    })
}

/// Post-rotation audit entry (best-effort; never fails the command).
fn log_rotation_audit(
    state: &AppState,
    session: &CurrentSession,
    operation: &RotationOperation,
    certificate: &IdentityCertificate,
) {
    let action = match operation {
        RotationOperation::Rotate => AuditAction::IdentityRotated,
        RotationOperation::ReIssue => AuditAction::IdentityReissued,
    };
    let entity_id = certificate.identity_id.to_string();
    let entity_name = certificate.subject_id.to_string();
    let metadata = serde_json::json!({
        "operation": operation.as_str(),
        "credential_id": certificate.credential_id.to_string(),
        "generation": certificate.generation,
        "subject_type": certificate.subject_type.as_str(),
    });
    if let Ok(guard) = state.get_db() {
        if let Some(db) = guard.as_ref() {
            let _ = AuditService::new(db.executor()).log_success(
                &session.user_id,
                &session.username,
                action,
                EntityType::System,
                Some(&entity_id),
                Some(&entity_name),
                None,
                None,
                Some(&session.session_id),
                Some(metadata),
            );
        }
    }
}

/// Begin a WILAYA credential rotation (B7, §3.3): stage a fresh node key (the
/// ACTIVE key is untouched until finalize) and write the UNSIGNED rotation
/// certificate (CSR) to `request_file_path` for the operator to carry to the
/// Authority Root. `operation` is `ROTATE` (same credential, generation + 1)
/// or `RE-ISSUE` (new credential). Nothing else is persisted.
#[tauri::command]
pub fn begin_wilaya_rotation(
    state: State<AppState>,
    request_file_path: String,
    operation: String,
) -> Result<RotationPlan, String> {
    let operation = parse_rotation_operation(&operation)?;
    authorize_command(&state, rotation_action(operation), None).map_err(into_command_error)?;
    state.touch_session();
    crate::domain::validation::validate_file_path(&request_file_path, &["json"])
        .map_err(into_command_error)?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let plan = IdentityRotationCoordinator::new(db, &node_key_store())
        .begin(SubjectType::Wilaya, operation)
        .map_err(into_command_error)?;
    let json = serde_json::to_string_pretty(&plan.certificate).map_err(|e| {
        into_command_error(AppError::Internal(format!("CSR serialization failed: {e}")))
    })?;
    std::fs::write(&request_file_path, json).map_err(|e| into_command_error(AppError::Io(e)))?;

    Ok(plan)
}

/// Finalize the WILAYA rotation with the Root-signed certificate read from
/// `cert_file_path`. The rotation Trust Package (kind `trust`, signed with the
/// OLD node key) is written to `rotation_package_path` BEFORE the new secret is
/// promoted; on any later failure the old key is restored (no partial state).
/// Idempotent: an identical re-presentation is a ZERO-write no-op.
#[tauri::command]
pub fn finalize_wilaya_rotation(
    state: State<AppState>,
    cert_file_path: String,
    rotation_package_path: String,
) -> Result<RotationFinalizeOutcome, String> {
    let (session, settings) =
        authorize_command(&state, Action::RotateCredential, None).map_err(into_command_error)?;
    state.touch_session();
    crate::domain::validation::validate_file_path(&cert_file_path, &["json"])
        .map_err(into_command_error)?;
    crate::domain::validation::validate_file_path(&rotation_package_path, &["sync"])
        .map_err(into_command_error)?;

    let json = std::fs::read_to_string(&cert_file_path)
        .map_err(|e| into_command_error(AppError::Io(e)))?;
    let signed_cert: IdentityCertificate = serde_json::from_str(&json).map_err(|e| {
        into_command_error(AppError::FileFormat(format!(
            "Malformed signed certificate file: {e}"
        )))
    })?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let outcome = IdentityRotationCoordinator::new(db, &node_key_store())
        .finalize_wilaya(
            &signed_cert,
            std::path::Path::new(&rotation_package_path),
            &settings,
            &state.crypto_port,
        )
        .map_err(into_command_error)?;

    match &outcome {
        RotationFinalizeOutcome::Completed {
            certificate,
            operation,
        } => log_rotation_audit(&state, &session, operation, certificate),
        RotationFinalizeOutcome::AlreadyCompleted { .. } => {}
    }
    Ok(outcome)
}

/// Begin a UNIT credential rotation on the UNIT node itself (B7, §3.3): stage a
/// fresh node key and write the UNSIGNED rotation certificate (CSR) to
/// `request_file_path` for the operator to carry to the WILAYA node.
#[tauri::command]
pub fn begin_unit_rotation(
    state: State<AppState>,
    request_file_path: String,
    operation: String,
) -> Result<RotationPlan, String> {
    let operation = parse_rotation_operation(&operation)?;
    authorize_command(&state, rotation_action(operation), None).map_err(into_command_error)?;
    state.touch_session();
    crate::domain::validation::validate_file_path(&request_file_path, &["json"])
        .map_err(into_command_error)?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let plan = IdentityRotationCoordinator::new(db, &node_key_store())
        .begin(SubjectType::Unit, operation)
        .map_err(into_command_error)?;
    let json = serde_json::to_string_pretty(&plan.certificate).map_err(|e| {
        into_command_error(AppError::Internal(format!("CSR serialization failed: {e}")))
    })?;
    std::fs::write(&request_file_path, json).map_err(|e| into_command_error(AppError::Io(e)))?;

    Ok(plan)
}

/// WILAYA side: sign a UNIT rotation CSR (B7, §3.12 D2, rotation path). The CSR
/// MUST be an unsigned UNIT certificate; the `subject_id` MUST match a known
/// local unit; the ACTIVE local WILAYA signs it. The signed certificate is also
/// recorded as WILAYA-side Issuer Local State (best-effort credential guard).
#[tauri::command]
pub fn sign_unit_rotation_request(
    state: State<AppState>,
    request_json: String,
) -> Result<SignedUnitRotation, String> {
    let (session, _settings) =
        authorize_command(&state, Action::RotateCredential, None).map_err(into_command_error)?;
    state.touch_session();

    let request: IdentityCertificate = serde_json::from_str(&request_json).map_err(|e| {
        into_command_error(AppError::FileFormat(format!(
            "Malformed UNIT rotation CSR: {e}"
        )))
    })?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let result = IdentityRotationCoordinator::new(db, &node_key_store())
        .sign_unit_rotation(&request)
        .map_err(into_command_error)?;
    log_rotation_audit(&state, &session, &result.operation, &result.certificate);
    Ok(result)
}

/// Finalize the UNIT rotation with the WILAYA-signed certificate read from
/// `cert_file_path`. The issuer is resolved via `issuer_identity_id` and
/// verified (exists + ACTIVE + WILAYA). Fail-closed: a rollback or
/// zero-generation conflict aborts. Idempotent: an identical re-presentation is
/// a ZERO-write no-op.
#[tauri::command]
pub fn finalize_unit_rotation(
    state: State<AppState>,
    cert_file_path: String,
) -> Result<RotationFinalizeOutcome, String> {
    let (session, _settings) =
        authorize_command(&state, Action::RotateCredential, None).map_err(into_command_error)?;
    state.touch_session();
    crate::domain::validation::validate_file_path(&cert_file_path, &["json"])
        .map_err(into_command_error)?;

    let json = std::fs::read_to_string(&cert_file_path)
        .map_err(|e| into_command_error(AppError::Io(e)))?;
    let signed_cert: IdentityCertificate = serde_json::from_str(&json).map_err(|e| {
        into_command_error(AppError::FileFormat(format!(
            "Malformed signed certificate file: {e}"
        )))
    })?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let outcome = IdentityRotationCoordinator::new(db, &node_key_store())
        .finalize_unit(&signed_cert)
        .map_err(into_command_error)?;

    match &outcome {
        RotationFinalizeOutcome::Completed {
            certificate,
            operation,
        } => log_rotation_audit(&state, &session, operation, certificate),
        RotationFinalizeOutcome::AlreadyCompleted { .. } => {}
    }
    Ok(outcome)
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
    let parsed_session_id = match uuid::Uuid::parse_str(&session_id) {
        Ok(id) => id,
        Err(e) => {
            // SEC-001-A / AUTH-11: a malformed (non-UUID) session id is a
            // failed completion and counts toward the limiter so it can never
            // bypass the rate limit.
            if let Ok(rl) = state.rate_limiter.lock() {
                rl.record_failure(CHALLENGE_RATE_LIMIT_KEY);
            }
            return Err(into_command_error(AppError::Internal(format!(
                "Invalid challenge session id: {e}"
            ))));
        }
    };

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
        deleted: false,
    };

    Ok(LoginResponse {
        success: true,
        user: Some(user),
        message: "تم تسجيل الدخول بنجاح".to_string(),
        requires_configuration: established.requires_configuration,
        identity_challenge_required: true,
    })
}
