//! Authentication Commands
//!
//! Login, logout, and session handling
//! Strictly follows Clean Architecture: Commands -> Services -> Repositories -> DB

use crate::application::authz::Action;
use crate::application::services::{
    AuditService, AuditTxService, IdentityAuthenticationPolicy, OperationalSessionService,
    SessionEndReason, SessionEstablishmentService, UserService,
};
use crate::commands::common::{db_mut_or_command_error, user_ctx_from_parts};
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::domain::audit::AuditAction;
use crate::errors::{into_command_error, AppError, AuthenticationError, ValidationError};
use crate::models::{LoginRequest, LoginResponse, SessionStatus, User, UserRole};
use tauri::State;

/// User login with rate limiting and audit logging
#[tauri::command]
pub fn login(state: State<AppState>, request: LoginRequest) -> Result<LoginResponse, String> {
    login_impl(&state, request)
}

/// Implementation of [`login`] over a borrowed `AppState`.
///
/// A Tauri `State` cannot be constructed outside a running app, so the password
/// authentication failure path (rate-limit gate → identity policy gate → user
/// lookup → password verification → failed-login audit) is covered by
/// integration tests through this seam. Same pattern and rationale as
/// [`touch_session_impl`].
pub fn login_impl(state: &AppState, request: LoginRequest) -> Result<LoginResponse, String> {
    // Check rate limiting
    let rate_limiter = state.rate_limiter.lock().map_err(|e| {
        into_command_error(AppError::Internal(format!(
            "Failed to lock rate limiter: {}",
            e
        )))
    })?;

    // SEC-003-07: node-wide login breaker — checked FIRST so a distributed
    // brute-force (failures spread across many usernames) cannot evade the
    // per-user limiter. The WILAYA Challenge–Response path never consults it.
    if !rate_limiter.is_global_login_allowed() {
        let remaining_secs = rate_limiter
            .get_global_remaining_lockout_secs()
            .unwrap_or(300);
        log::warn!(
            target: "grpc::auth",
            "login rate limited: node-wide breaker active, remaining_secs={}",
            remaining_secs
        );
        return Err(into_command_error(AppError::Validation(
            ValidationError::InvalidFormat {
                field: "login".to_string(),
                message: format!(
                    "تم تعليق محاولات الدخول مؤقتاً بسبب محاولات فاشلة متعددة. انتظر {} دقيقة",
                    remaining_secs / 60
                ),
            },
        )));
    }

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

    // Identity auth gate (RFC 2026-08-04 / ADR-0038, as amended by ADR-0050):
    // the password path is permitted whenever the local account carries a
    // usable password credential. An ACTIVE ADMIN identity or a `.adminkey`
    // does NOT close it — normal WILAYA Admin login is username + password
    // (B8 fleet credential); Challenge–Response is the recovery / high-
    // assurance path. Accounts with an EMPTY password hash (identity-only
    // ADMIN ceremony) are routed to Challenge–Response; unknown or soft-deleted
    // accounts fall through to the generic invalid-credentials response below
    // (no account enumeration). The decision comes exclusively from
    // `IdentityAuthenticationPolicy` (a security fact) — never from the
    // derived bootstrap state.
    // ADR-0052: username lookups are scoped to the local node identity —
    // `"WILAYA"` on a WILAYA node, the local unit code on a UNIT node. A
    // foreign-scope username (e.g. another unit's shadow operator) is
    // indistinguishable from an unknown account.
    use crate::infrastructure::security::node_identity_provider::NodeIdentityProvider as _;
    let node_scope =
        crate::infrastructure::security::SettingsNodeIdentityProvider::new(db.executor())
            .current_node_id()
            .unwrap_or_else(|_| "WILAYA".to_string());

    let password_allowed =
        IdentityAuthenticationPolicy::password_login_allowed(db, &request.username, &node_scope)
            .map_err(into_command_error)?;
    if !password_allowed {
        log::warn!(
            target: "grpc::auth",
            "password login rejected: user={} has no usable password credential; Challenge–Response is required",
            request.username
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
        .get_user_by_username(&request.username, &node_scope)
        .map_err(into_command_error)?;

    if let Some(user) = user {
        // Identity-only credential guard (B5): an EMPTY password hash means the
        // account authenticates exclusively through Challenge–Response
        // (`.adminkey`). The password path rejects it explicitly — a malformed
        // `PasswordHash::new("")` would otherwise surface as an internal error.
        // Defense-in-depth: the ADR-0050 gate above already routes these
        // accounts to `challenge_required`; this guard survives as a fail-safe.
        if user.password_hash.is_empty() {
            log::warn!(
                target: "grpc::auth",
                "password login rejected: user={} has an identity-only credential (empty password hash)",
                request.username
            );
        } else {
            // Role-based password verification (B8 — Identity & Access
            // Synchronization): the fleet-wide `admin` account verifies in the
            // global admin domain; every other account (UNIT `user`, legacy
            // local users) verifies node-bound. Admin additionally falls back
            // to node-bound verification to cover pre-sync unit-bootstrapped
            // admin hashes that predate the first identity_access package.
            let valid = match user.role {
                UserRole::Admin => {
                    let admin_ok = password_port
                        .verify_admin(&request.password, &user.password_hash)
                        .map_err(|e| {
                            into_command_error(AppError::Internal(format!(
                                "Password verification failed: {}",
                                e
                            )))
                        })?;
                    if admin_ok {
                        true
                    } else {
                        password_port
                            .verify_node(&request.password, &user.node_id, &user.password_hash)
                            .map_err(|e| {
                                into_command_error(AppError::Internal(format!(
                                    "Password verification failed: {}",
                                    e
                                )))
                            })?
                    }
                }
                UserRole::User => password_port
                    .verify_node(&request.password, &user.node_id, &user.password_hash)
                    .map_err(|e| {
                        into_command_error(AppError::Internal(format!(
                            "Password verification failed: {}",
                            e
                        )))
                    })?,
            };

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

                // Observability (ADR-0050): password is the normal login path — WILAYA
                // Admin authenticates with the B8 fleet credential; UNIT local
                // users keep Application User Authentication.
                log::warn!(
                    target: "grpc::auth",
                    "PASSWORD_LOGIN: user={} auth_method=password (Application User Authentication — ADR-0050 normal path)",
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
    //
    // The audit actor MUST be an existing `users.id`: `audit_log.user_id`
    // references `users(id)` with foreign keys enabled, so a non-existent
    // actor aborts the insert (`SQLITE_CONSTRAINT_FOREIGNKEY`, extended 787)
    // and the failure is never recorded. `"system"` is the id of the system
    // user seeded by the canonical fresh-install migration.
    if let Err(e) = AuditService::new(db.executor()).log_failure(
        "system",
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

    // Record failed attempt (per-user bucket + node-wide breaker, SEC-003-07)
    if let Ok(rl) = state.rate_limiter.lock() {
        rl.record_login_failure(&request.username);
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
    get_current_user_impl(&state)
}

/// Borrowed-state seam for [`get_current_user`].
///
/// A Tauri `State` cannot be constructed outside a running app, so the
/// projection's truthful forced-credential state (ADR-0063 §5/F32) is covered
/// by integration tests through this seam. Same pattern and rationale as
/// [`login_impl`] and [`touch_session_impl`].
pub fn get_current_user_impl(state: &AppState) -> Result<Option<User>, String> {
    // `get_session` clones the session out of the lock, so no session lock is
    // held while the database is read below.
    let session = match state.get_session() {
        Ok(session) => session,
        Err(AppError::Authentication(AuthenticationError::SessionNotFound)) => return Ok(None),
        Err(e) => return Err(into_command_error(e)),
    };

    if session.is_expired() || session.is_absolutely_expired() {
        return Ok(None);
    }

    // Read identity/role from the login-time snapshot: role/user changes take
    // effect on next login by design.
    let snapshot = &session.user_snapshot;

    // ADR-0063 §5: `must_change_password` is a persisted credential fact, not
    // a session fact. It is re-read from the database on every call so the
    // frontend always observes the backend's current state — deliberately NOT
    // stored in `UserSnapshot`, which would create a second source of truth.
    // A vanished row reports no user; a database failure fails closed.
    let must_change_password = {
        let db_guard = state.get_db().map_err(into_command_error)?;
        let db = db_guard.as_ref().ok_or_else(|| {
            into_command_error(AppError::Internal("Database is closed".to_string()))
        })?;
        match UserService::new(db.executor(), state.password_port.as_ref())
            .get_user_by_id(&snapshot.id)
        {
            Ok(Some(persisted)) => persisted.must_change_password,
            Ok(None) => return Ok(None),
            Err(e) => return Err(into_command_error(e)),
        }
    };

    let user = User {
        id: snapshot.id.clone(),
        username: snapshot.username.clone(),
        password_hash: String::new(),
        role: snapshot.role.clone(),
        created_at: snapshot.created_at,
        node_id: String::new(),
        deleted: false,
        must_change_password,
    };

    Ok(Some(user))
}

/// ADR-0063 §6 — canonical local UNIT operator self password change.
///
/// Authorization is `Action::ChangeOwnPassword` (UNIT node + canonical
/// operator username + `User` role); the target row is always the authenticated
/// session's own id, never caller-supplied. The password write, the forced-state
/// clear and the `PasswordChange` audit share one transaction (§6.6, §6.8).
/// No rate-limiter call and no session creation/invalidation (§6.7, §6.9).
#[tauri::command]
pub fn change_own_password(
    state: State<AppState>,
    current_password: String,
    new_password: String,
) -> Result<(), String> {
    change_own_password_impl(&state, &current_password, &new_password)
}

/// Borrowed-state seam for [`change_own_password`], same rationale as
/// [`login_impl`]: the failure paths (wrong current password, policy rejection,
/// reuse) are covered by integration tests through this seam.
pub fn change_own_password_impl(
    state: &AppState,
    current_password: &str,
    new_password: &str,
) -> Result<(), String> {
    let (session, _settings) =
        authorize_command(state, Action::ChangeOwnPassword, None).map_err(into_command_error)?;
    // Advance session activity for an authorized mutating command. The login
    // rate limiter is deliberately never consulted here (ADR-0063 §6.7).
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let user_ctx = user_ctx_from_parts(
        &session.user_id,
        &session.username,
        Some(&session.session_id),
    );
    let password_port = state.password_port.as_ref();

    AuditTxService::execute_with_audit(db, AuditAction::PasswordChange, &user_ctx, |tx| {
        UserService::new(tx.executor, password_port).change_own_password(
            &session.user_id,
            current_password,
            new_password,
        )
    })
    .map_err(into_command_error)?;

    Ok(())
}

/// ADR-0063 §7.1 — canonical local UNIT operator password reset by the local
/// admin.
///
/// Authorization is `Action::ResetLocalUnitUserPassword` (UNIT node +
/// canonical local `admin` identity); the target row is derived server-side
/// from the local UNIT code, never caller-supplied. The temporary credential
/// must satisfy the normal policy (`0000` is rejected), is node-bound to the
/// local code (F4), and MUST drive the target into the forced credential
/// state. The password write, the forced-state set and the `PasswordChange`
/// audit share one transaction (§7.1 / §2.3). No rate-limiter call and no
/// session creation/invalidation (§7.1 F29, F30, F32).
#[tauri::command]
pub fn reset_unit_user_password(
    state: State<AppState>,
    temporary_password: String,
) -> Result<(), String> {
    reset_unit_user_password_impl(&state, &temporary_password)
}

/// Borrowed-state seam for [`reset_unit_user_password`], same rationale as
/// [`change_own_password_impl`]: the failure paths (target derivation, policy
/// rejection, node binding) are covered by integration tests through this seam.
pub fn reset_unit_user_password_impl(
    state: &AppState,
    temporary_password: &str,
) -> Result<(), String> {
    let (session, _settings) = authorize_command(state, Action::ResetLocalUnitUserPassword, None)
        .map_err(into_command_error)?;
    // Advance session activity for an authorized mutating command. The login
    // rate limiter is deliberately never consulted here (§7.1 / F29).
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    // Authenticated ADMIN attribution (§7.1), transactionally with the write.
    let user_ctx = user_ctx_from_parts(
        &session.user_id,
        &session.username,
        Some(&session.session_id),
    );
    let password_port = state.password_port.as_ref();

    AuditTxService::execute_with_audit(db, AuditAction::PasswordChange, &user_ctx, |tx| {
        UserService::new(tx.executor, password_port)
            .reset_local_operator_password(temporary_password)
    })
    .map_err(into_command_error)?;

    Ok(())
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
            if s.is_expired() || s.is_absolutely_expired() {
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
    touch_session_impl(&state).map_err(into_command_error)
}

/// SEC-003-08 Part B: session extension requires authentication. Routing through
/// `authorize_command(Action::AuthenticatedOnly)` means an unauthenticated or
/// stale session (missing / disabled / demoted user) is rejected BEFORE the
/// activity timestamp is advanced, so a deleted user's session can no longer be
/// silently kept alive by frontend activity pings.
pub fn touch_session_impl(state: &AppState) -> Result<(), AppError> {
    authorize_command(state, Action::AuthenticatedOnly, None)?;
    state.touch_session();
    Ok(())
}
