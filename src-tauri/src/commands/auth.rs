//! Authentication Commands
//!
//! Login, logout, and session handling
//! Strictly follows Clean Architecture: Commands -> Services -> Repositories -> DB

use crate::application::services::{
    AuditService, AuditTxService, IdentityAuthenticationPolicy, OperationalSessionService,
    SessionEndReason, SessionEstablishmentService, UserService,
};
use crate::commands::common::{
    adminkey_provider, db_mut_or_command_error, user_ctx_from_parts,
};
use crate::commands::types::AppState;
use crate::domain::audit::AuditAction;
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

    // Identity auth gate (RFC 2026-08-04 / ADR-0038): the password path is
    // permitted ONLY while no ACTIVE ADMIN identity exists. The decision comes
    // exclusively from `IdentityAuthenticationPolicy` (a security fact — ACTIVE
    // ADMIN cert + `.adminkey` present) — never from the derived bootstrap state.
    //
    // B6-B: this gate is now PERMANENT. The temporary `GRPC_LEGACY_AUTH` override
    // (Introduced B6-A) is removed and MUST NOT survive. Nodes without an ACTIVE
    // ADMIN identity (UNIT local users created from the `.unit` package, and
    // unprovisioned nodes) keep Application User Authentication; WILAYA nodes
    // with an ACTIVE ADMIN identity route exclusively to Challenge–Response.
    let password_allowed =
        IdentityAuthenticationPolicy::password_login_allowed(db, &adminkey_provider())
            .map_err(into_command_error)?;
    if !password_allowed {
        log::warn!(
            target: "grpc::auth",
            "password login rejected: node has an ACTIVE ADMIN identity; Challenge–Response is mandatory"
        );
        return Ok(LoginResponse {
            success: false,
            user: None,
            message: "عليك تسجيل الدخول باستخدام المفتاح الإداري".to_string(),
            requires_configuration: false,
            identity_challenge_required: true,
        });
    }

    let password_port = state.password_port.as_ref();
    let user = UserService::new(db.executor(), password_port)
        .get_user_by_username(&request.username)
        .map_err(into_command_error)?;

    if let Some(user) = user {
        // Identity-only credential guard (B5): an EMPTY password hash means the
        // account authenticates exclusively through Challenge–Response
        // (`.adminkey`). The password path rejects it explicitly — a malformed
        // `PasswordHash::new("")` would otherwise surface as an internal error.
        if user.password_hash.is_empty() {
            log::warn!(
                target: "grpc::auth",
                "password login rejected: user={} has an identity-only credential (empty password hash)",
                request.username
            );
        } else {
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

                // Shared session path: settings + login policy + atomic Login audit +
                // operational begin_session (single source of truth, ADR-0038/B3).
                let established =
                    SessionEstablishmentService::establish(db, &user, None, "password")
                        .map_err(into_command_error)?;
                if let Ok(mut current_session) = state.current_session.lock() {
                    *current_session = Some(established.session);
                }

                // Observability (B5–B6): the password path is now the permanent
                // Application User Authentication for nodes without an ACTIVE
                // ADMIN identity (UNIT local users / unprovisioned nodes).
                log::warn!(
                    target: "grpc::auth",
                    "PASSWORD_LOGIN: user={} auth_method=password (Application User Authentication — node has no ACTIVE ADMIN identity)",
                    request.username
                );

                return Ok(LoginResponse {
                    success: true,
                    user: Some(user),
                    message: "تم تسجيل الدخول بنجاح".to_string(),
                    requires_configuration: established.requires_configuration,
                    identity_challenge_required: false,
                });
            }
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
        identity_challenge_required: false,
    })
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

    // Read from login-time snapshot.
    // Role/user changes take effect on next login by design.
    let snapshot = &session.user_snapshot;
    let user = User {
        id: snapshot.id.clone(),
        username: snapshot.username.clone(),
        password_hash: String::new(),
        role: snapshot.role.clone(),
        created_at: snapshot.created_at,
        node_id: String::new(),
    };

    Ok(Some(user))
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
