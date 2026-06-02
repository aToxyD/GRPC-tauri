//! Authentication Commands
//!
//! Login, logout, password management, and session handling
//! Strictly follows Clean Architecture: Commands -> Services -> Repositories -> DB

use crate::application::services::{
    AuditService, AuditTxService, LoginPolicy, OperationalSessionService, SessionEndReason,
    SettingsService, UserService,
};
use crate::commands::common::{
    db_mut_or_command_error, user_ctx_from_parts, user_ctx_from_session,
};
use crate::commands::types::AppState;
use crate::domain::audit::AuditAction;
use crate::domain::session::CurrentSession;
use crate::errors::{into_command_error, AppError, ValidationError};
use crate::models::{LoginRequest, LoginResponse, SessionStatus, User};
use tauri::State;

/// User login with rate limiting and audit logging
#[tauri::command]
pub fn login(state: State<AppState>, request: LoginRequest) -> Result<LoginResponse, String> {
    // Check rate limiting
    let rate_limiter = state.rate_limiter.lock().map_err(|e| {
        into_command_error(AppError::Internal(format!(
            "Failed to lock rate limiter: {}",
            e
        )))
    })?;

    if !rate_limiter.is_allowed(&request.username) {
        let remaining_secs = rate_limiter
            .get_remaining_lockout_secs(&request.username)
            .unwrap_or(300);
        log::warn!(
            target: "grpc::auth",
            "login rate limited: username={} remaining_secs={}",
            request.username,
            remaining_secs
        );
        return Err(into_command_error(AppError::Validation(
            ValidationError::InvalidFormat {
                field: "login".to_string(),
                message: format!(
                    "عدد المحاولات تجاوز الحد. انتظر {} دقيقة",
                    remaining_secs / 60
                ),
            },
        )));
    }
    drop(rate_limiter);

    // Get database
    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let password_port = state.password_port.as_ref();
    let user = UserService::new(db.executor(), password_port)
        .get_user_by_username(&request.username)
        .map_err(into_command_error)?;

    if let Some(user) = user {
        // Node-bound password verification
        let valid = password_port
            .verify_password(&request.password, &user.node_id, &user.password_hash)
            .map_err(|e| {
                into_command_error(AppError::Internal(format!(
                    "Password verification failed: {}",
                    e
                )))
            })?;

        if valid {
            // Reset rate limiter on success
            if let Ok(rl) = state.rate_limiter.lock() {
                rl.record_success(&request.username);
            }

            // Get settings
            let settings = SettingsService::new(db.executor())
                .get_settings()
                .map_err(into_command_error)?;
            let requires_configuration = !settings.configured;

            // Enforce login authorization policy
            if let Err(e) = LoginPolicy::check_login_allowed(&settings.node_type, &user.role) {
                return Err(into_command_error(AppError::BusinessLogic(e)));
            }

            // Create session
            let session =
                CurrentSession::new(user.id.clone(), user.username.clone(), user.role.clone());
            let session_id = session.session_id.clone();
            if let Ok(mut current_session) = state.current_session.lock() {
                *current_session = Some(session);
            }

            // Log successful login (Atomic Audit) + operational session metadata
            let user_ctx = user_ctx_from_parts(&user.id, &user.username, Some(&session_id));
            if let Err(e) =
                AuditTxService::execute_with_audit(db, AuditAction::Login, &user_ctx, |tx| {
                    OperationalSessionService::new(tx.executor).begin_session(
                        &session_id,
                        &user.id,
                        &user.username,
                    )
                })
            {
                log::error!(
                    target: "grpc::audit",
                    "AUDIT WRITE FAILED [login] user={} err={:?}",
                    &user.username,
                    e
                );
            }

            return Ok(LoginResponse {
                success: true,
                user: Some(user),
                message: "تم تسجيل الدخول بنجاح".to_string(),
                requires_configuration,
            });
        }
    }

    // Log failed login (Non-transactional)
    if let Err(e) = AuditService::new(db.executor()).log_failure(
        "SYSTEM",
        &request.username,
        AuditAction::Login,
        crate::domain::audit::EntityType::User,
        None,
        "بيانات الدخول غير صحيحة",
        None,
    ) {
        log::error!(
            target: "grpc::audit",
            "AUDIT WRITE FAILED [login_failure] user={} err={:?}",
            &request.username,
            e
        );
    }

    log::warn!(
        target: "grpc::auth",
        "login failed: username={}",
        request.username
    );

    // Record failed attempt
    if let Ok(rl) = state.rate_limiter.lock() {
        rl.record_failure(&request.username);
    }

    Ok(LoginResponse {
        success: false,
        user: None,
        message: "اسم المستخدم أو كلمة المرور غير صحيحة".to_string(),
        requires_configuration: false,
    })
}

/// Admin password reset - Wilaya Admin Only
#[tauri::command]
pub fn change_password(
    state: State<AppState>,
    target_user_id: String,
    new_password: String,
) -> Result<(), String> {
    // Validate password
    crate::domain::validation::validate_change_password(&new_password)
        .map_err(into_command_error)?;

    let (session, _settings) = crate::commands::guards::authorize_command(
        &state,
        crate::application::authz::Action::AdminOnly,
        None,
    )
    .map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    // Perform password change (Atomic)
    let user_ctx = user_ctx_from_session(&session);

    let password_port = state.password_port.as_ref();
    AuditTxService::execute_with_audit(db, AuditAction::UpdateUser, &user_ctx, |tx| {
        UserService::new(tx.executor, password_port).change_password(&target_user_id, &new_password)
    })
    .map_err(into_command_error)?;

    Ok(())
}

/// Logout user - MUTATION with audit
#[tauri::command]
pub fn logout(state: State<AppState>) -> Result<bool, String> {
    let session_info = {
        let session = state.current_session.lock().map_err(|e| {
            into_command_error(AppError::Internal(format!("Failed to lock session: {}", e)))
        })?;
        session.as_ref().map(|s| {
            (
                s.user_id.clone(),
                s.username.clone(),
                s.session_id.clone(),
                s.duration_minutes(),
            )
        })
    };

    if let Some((user_id, username, session_id, duration)) = session_info {
        let mut guard = state.get_db().map_err(into_command_error)?;
        let db = db_mut_or_command_error(guard.as_mut())?;

        let user_ctx = user_ctx_from_parts(&user_id, &username, Some(&session_id));
        if let Err(e) = AuditTxService::execute_with_audit(
            db,
            AuditAction::Logout,
            &user_ctx,
            |tx| {
                OperationalSessionService::new(tx.executor)
                    .end_session(&session_id, SessionEndReason::Logout)?;
                log::info!(target: "grpc::auth", "user_logout: user={} duration_min={}", username, duration);
                Ok(())
            },
        ) {
            log::error!(
                target: "grpc::audit",
                "AUDIT WRITE FAILED [logout] user={} err={:?}",
                username,
                e
            );
        }
    }

    // Clear session
    if let Ok(mut session) = state.current_session.lock() {
        *session = None;
    }

    Ok(true)
}

/// Get current authenticated user
#[tauri::command]
pub fn get_current_user(state: State<AppState>) -> Result<Option<User>, String> {
    let session = state.current_session.lock().map_err(|e| {
        into_command_error(AppError::Internal(format!("Failed to lock session: {}", e)))
    })?;

    let session = match session.as_ref() {
        Some(s) => s,
        None => return Ok(None),
    };

    if session.is_expired() {
        return Ok(None);
    }

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let user = UserService::new(db.executor(), state.password_port.as_ref())
        .get_user_by_username(&session.username)
        .map_err(into_command_error)?;

    Ok(user)
}

/// Check session status
#[tauri::command]
pub fn check_session(state: State<AppState>) -> Result<SessionStatus, String> {
    let session = state.current_session.lock().map_err(|e| {
        into_command_error(AppError::Internal(format!("Failed to lock session: {}", e)))
    })?;

    match session.as_ref() {
        None => Ok(SessionStatus {
            is_active: false,
            is_expired: false,
            should_warn: false,
            remaining_minutes: 0,
            username: None,
        }),
        Some(s) => {
            if s.is_expired() {
                Ok(SessionStatus {
                    is_active: false,
                    is_expired: true,
                    should_warn: false,
                    remaining_minutes: 0,
                    username: Some(s.username.clone()),
                })
            } else {
                Ok(SessionStatus {
                    is_active: true,
                    is_expired: false,
                    should_warn: s.should_warn(),
                    remaining_minutes: s.remaining_minutes(),
                    username: Some(s.username.clone()),
                })
            }
        }
    }
}

/// Update session activity (touch)
#[tauri::command]
pub fn touch_session(state: State<AppState>) -> Result<(), String> {
    state.touch_session();
    Ok(())
}
