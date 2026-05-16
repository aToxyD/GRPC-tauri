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
use crate::application::services::{MaintenanceBlockedOperation, SettingsService};
use crate::domain::session::CurrentSession;
use crate::errors::{AppError, AuthenticationError};
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

/// Verify user is authenticated (session exists and not expired)
pub(crate) fn require_authenticated(state: &AppState) -> Result<CurrentSession, AppError> {
    let session = state.get_session()?;
    if session.is_expired() {
        return Err(AppError::Authentication(
            AuthenticationError::SessionExpired,
        ));
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
                    ResourceContext::UnitScope {
                        unit_id: uid.to_string(),
                    }
                } else {
                    let db_guard = state.get_db()?;
                    let db = db_guard
                        .as_ref()
                        .ok_or_else(|| AppError::Internal("Database not available".into()))?;
                    let current_unit = SettingsService::new(db.executor())
                        .get_current_unit_id()
                        .unwrap_or(None);

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
