//! System-level authorization policies.
//! All actions in this module default-deny.

use crate::models::UserRole;

use super::{Action, AuthorizationError, Principal, ResourceContext};

/// Enforces Admin-only or per-user-self access for system-level actions.
/// Every arm is explicit — the catch-all denies.
pub fn authorize_system(
    principal: &Principal,
    action: Action,
    resource: &ResourceContext,
) -> Result<(), AuthorizationError> {
    match action {
        Action::AdminOnly
        | Action::ConfigureAsWilaya
        | Action::ReadImportAudit
        | Action::ReadAuditLog => {
            if principal.role != UserRole::Admin {
                return Err(AuthorizationError::RequiresAdmin);
            }
            Ok(())
        }
        Action::ReadSystemMetrics | Action::ViewSystemHealth | Action::ManageSyncConflicts => {
            if principal.role != UserRole::Admin {
                return Err(AuthorizationError::RequiresAdmin);
            }
            Ok(())
        }
        Action::ReadUserActivity => {
            // Admin may read any user's activity; non-admin may only read their own.
            if principal.role == UserRole::Admin {
                return Ok(());
            }
            if let Some(target_uid) = resource.target_user_id() {
                if target_uid == principal.user_id {
                    return Ok(());
                }
            }
            Err(AuthorizationError::RequiresAdmin)
        }
        // Any other action routed here is an implementation error — deny.
        _ => Err(AuthorizationError::InsufficientPermissions),
    }
}

/// Verifies that the caller holds an authenticated session.
/// Enforces node-level restrictions:
/// - WILAYA: Admin only for all actions.
/// - UNIT: Admin + User (Everyone).
pub fn authorize_authenticated(
    principal: &Principal,
    resource: &ResourceContext,
) -> Result<(), AuthorizationError> {
    match resource {
        ResourceContext::WilayaNode => {
            if principal.role != UserRole::Admin {
                return Err(AuthorizationError::RequiresAdmin);
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
