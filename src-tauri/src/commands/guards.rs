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
        let db_guard = state.get_db().map_err(|_| {
            AppError::Authentication(AuthenticationError::SessionNotFound)
        })?;
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
        return Err(AppError::Authentication(AuthenticationError::UserNotFound {
            username: session.username,
        }));
    }

    Ok(session)
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

    // ── Licensing enforcement gate (ADR-0042 §5) ────────────────────────────
    // Persistence-derived trigger: dormant until an active trust anchor is
    // installed; then globally active for non-exempt actions, fail-closed.
    // Database unavailability or a missing node key ⇒ deny.
    let gate_result = {
        let db_guard = state.get_db().map_err(|e| {
            log::warn!(target: "grpc::licensing", "gate: database unavailable: {e}");
            AppError::Authorization(AuthorizationError::LicenseRequired)
        })?;
        let Some(db) = db_guard.as_ref() else {
            return Err(AppError::Authorization(AuthorizationError::LicenseRequired));
        };
        crate::application::licensing::enforcement::enforce(
            db,
            crate::commands::common::resolve_node_public_key(),
            action,
        )
    };
    if let Err(e) = gate_result {
        log::warn!(
            target: "grpc::licensing",
            "licensing gate denied: action={action:?} err={e}"
        );
        return Err(AppError::Authorization(e));
    }

    Ok((session, settings))
}
