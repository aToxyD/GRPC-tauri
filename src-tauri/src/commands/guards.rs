//! Authorization Guards (Security Layer)
//!
//! Node/session **role** checks (`require_admin`, `require_wilaya_node`, …) are **transitional**:
//! prefer [`crate::application::authz`] (`Principal`, `Action`, `ResourceContext`, `authorize`)
//! for resource decisions in commands. Guards remain for shared session expiry and node-type gates
//! until fully migrated.
//!
//! All guards follow the pattern:
//! - Check authentication first
//! - Check authorization second
//! - Return Result with AppError on failure

use crate::app::state::AppState;
use crate::application::authz::{authorize, Action, Principal, ResourceContext};
use crate::application::services::{MaintenanceBlockedOperation, UserService};
use crate::domain::session::CurrentSession;
use crate::errors::{AppError, AuthenticationError, AuthorizationError};
use crate::models::Settings;

/// Macro for cleaner guard usage
#[macro_export]
macro_rules! guard {
    ($state:expr, authenticated) => {
        $crate::commands::guards::require_authenticated($state)
    };
    ($state:expr, authorize, $action:expr) => {
        $crate::commands::guards::authorize_command($state, $action, None)
    };
    ($state:expr, authorize, $action:expr, $unit_id:expr) => {
        $crate::commands::guards::authorize_command($state, $action, Some($unit_id))
    };
}

/// Reject operational mutations while explicit maintenance mode is active.
pub fn require_maintenance_allows(
    state: &AppState,
    operation: MaintenanceBlockedOperation,
) -> Result<(), AppError> {
    state.maintenance.assert_allows(operation)
}

/// Verify user is authenticated (session exists and not expired), then
/// revalidate the underlying user (SEC-003-08).
///
/// Revalidation is the SINGLE chokepoint for session staleness: a session must
/// not survive user deletion, disablement (`deleted = 1`), or a role change
/// (a demoted Admin loses Admin-only commands immediately). On revalidation
/// failure the stale session is invalidated and the request fails closed.
pub(crate) fn require_authenticated(state: &AppState) -> Result<CurrentSession, AppError> {
    let session = state.get_session()?;
    if session.is_expired() || session.is_absolutely_expired() {
        return Err(AppError::Authentication(
            AuthenticationError::SessionExpired,
        ));
    }

    // SEC-003-08: revalidate against the users repository on every call.
    // `get_user_by_id` deliberately returns disabled rows (unlike the login
    // lookup), so existence, `deleted`, and `role` are all checkable here.
    let valid = {
        let db_guard = state
            .get_db()
            .map_err(|_| AppError::Authentication(AuthenticationError::SessionNotFound))?;
        let Some(db) = db_guard.as_ref() else {
            return Err(AppError::Authentication(
                AuthenticationError::SessionNotFound,
            ));
        };
        let user = UserService::new(db.executor(), state.password_port.as_ref())
            .get_user_by_id(&session.user_id)
            .map_err(|_| AppError::Authentication(AuthenticationError::SessionNotFound))?;
        match user {
            Some(u) => !u.deleted && u.role == session.user_role,
            None => false,
        }
    };

    if !valid {
        log::warn!(
            target: "grpc::authz",
            "session revalidation failed: user_id={} username={} (user missing, disabled, or role changed); invalidating session",
            session.user_id,
            session.username
        );
        if let Ok(mut s) = state.current_session.lock() {
            *s = None;
        }
        return Err(AppError::Authentication(
            AuthenticationError::UserNotFound {
                username: session.username,
            },
        ));
    }

    Ok(session)
}

/// ADR-0063 §5 — the **closed** allowlist of commands that may proceed while
/// the forced credential state (`users.must_change_password = true`) is active.
///
/// Closed by construction: membership here is an ADR-scoped decision, never a
/// convenience decision. The list is keyed on registered IPC command names
/// (`invoke.message.command()`), not on [`Action`] variants — the actions
/// `AuthenticatedOnly` and `AuthenticatedOnly`-adjacent variants are shared by
/// commands that must stay blocked.
pub const FORCED_STATE_ALLOWED_COMMANDS: &[&str] = &[
    // Session establishment / teardown / inspection
    "login",
    "logout",
    "get_current_user",
    "check_session",
    "touch_session",
    // The only way out of the forced state (ADR-0063 §6)
    "change_own_password",
    // `admin_access` import must stay open until initialization completes
    "import_admin_access_package",
    // Rendering prerequisites for the change screen
    "get_settings",
    "is_configured",
];

/// ADR-0063 §5 — central forced-credential-state gate.
///
/// This is one half of the enforcement seam: it lives in this security layer
/// next to [`require_authenticated`] / [`authorize_command`], and is invoked
/// from the repository's single dispatch point
/// (`commands::registry::get_invoke_handler`), which is the only place that
/// covers **every** registered `#[tauri::command]`. Several commands
/// (`get_build_info`, `get_login_metrics`, `generate_reports`, the
/// `calculate_*` family, and the pre-auth commands) never reach
/// [`authorize_command`], so the dispatch seam — not `authorize_command` — is
/// the choke point (ADR-0063 §5 erratum).
///
/// Semantics, all fail-closed except where stated:
/// * allowlisted command → `Ok`
/// * no session → `Ok` (authentication remains the responsibility of the
///   command's own guards)
/// * expired session → `Ok` (same rationale)
/// * session-lock or database-lock lookup error → propagated (fail closed)
/// * missing authenticated user row → `Ok`, so `require_authenticated` performs
///   its normal fail-closed revalidation for the command itself
/// * forced credential state on any other command → `Err(PasswordChangeRequired)`,
///   a real backend command error, never an empty/success result
pub fn enforce_forced_credential_state(state: &AppState, command: &str) -> Result<(), AppError> {
    if FORCED_STATE_ALLOWED_COMMANDS.contains(&command) {
        return Ok(());
    }

    let session = match state.get_session() {
        Ok(session) => session,
        Err(AppError::Authentication(AuthenticationError::SessionNotFound)) => return Ok(()),
        Err(e) => return Err(e),
    };
    if session.is_expired() || session.is_absolutely_expired() {
        return Ok(());
    }

    let db_guard = state.get_db()?;
    let Some(db) = db_guard.as_ref() else {
        return Err(AppError::Internal("Database is closed".to_string()));
    };

    let user = UserService::new(db.executor(), state.password_port.as_ref())
        .get_user_by_id(&session.user_id)?;

    let Some(user) = user else {
        // The row disappeared after session establishment: defer to the
        // command's own `require_authenticated` revalidation, which fails closed
        // and invalidates the stale session.
        return Ok(());
    };

    if user.must_change_password {
        log::warn!(
            target: "grpc::authz",
            "forced credential state refused command: command={} username={}",
            command,
            session.username
        );
        return Err(AppError::Authorization(
            AuthorizationError::PasswordChangeRequired,
        ));
    }

    Ok(())
}

/// Helper to convert session to principal
fn build_principal(session: &CurrentSession) -> Principal {
    Principal {
        user_id: session.user_id.clone(),
        username: session.username.clone(),
        role: session.user_role.clone(),
        session_id: Some(session.session_id.clone()),
    }
}

/// Core command authorization dispatcher
/// This wraps `application::authz::authorize` and handles building the `Principal` and `ResourceContext`.
pub fn authorize_command(
    state: &AppState,
    action: Action,
    target_resource_id: Option<&str>,
) -> Result<(CurrentSession, Settings), AppError> {
    let session = require_authenticated(state)?;
    let principal = build_principal(&session);
    let settings = state.get_settings()?;

    let resource = match action {
        Action::ReadUserActivity => {
            if let Some(uid) = target_resource_id {
                ResourceContext::UserScope {
                    user_id: uid.to_string(),
                }
            } else {
                ResourceContext::Global
            }
        }
        _ => match settings.node_type {
            crate::models::NodeType::Wilaya => {
                if let Some(uid) = target_resource_id {
                    ResourceContext::UnitScope {
                        unit_id: uid.to_string(),
                    }
                } else {
                    ResourceContext::WilayaNode
                }
            }
            crate::models::NodeType::Unit => {
                if let Some(uid) = target_resource_id {
                    // SEC-003-06-a: a UNIT node may only reference its own
                    // local unit identity. The canonical local unit id is the
                    // same `settings.unit_name` used by the `UnitNode`
                    // derivation below; a caller-supplied foreign unit_id is
                    // denied before any policy evaluation.
                    if settings.unit_name.as_deref() != Some(uid) {
                        return Err(AppError::Authorization(
                            AuthorizationError::InsufficientPermissions,
                        ));
                    }
                    ResourceContext::UnitScope {
                        unit_id: uid.to_string(),
                    }
                } else {
                    let current_unit = settings.unit_name.clone();

                    if let Some(current_uid) = current_unit {
                        ResourceContext::UnitNode {
                            unit_id: current_uid,
                        }
                    } else {
                        ResourceContext::Global // Unconfigured unit
                    }
                }
            }
        },
    };

    authorize(&principal, action, &resource).map_err(|e| {
        log::warn!(
            target: "grpc::authz",
            "authorization denied: username={} action={:?} err={}",
            principal.username,
            action,
            e
        );
        AppError::Authorization(e)
    })?;

    Ok((session, settings))
}
