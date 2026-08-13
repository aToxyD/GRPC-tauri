//! Authorization policy router.
//!
//! DEFAULT: **deny**. All actions must be explicitly handled.
//! No wildcard fallthrough anywhere in the policy tree.

use super::{Action, AuthorizationError, Principal, ResourceContext};

mod reports;
mod system;

/// Top-level authorization dispatcher.
/// Every Action variant MUST appear in exactly one arm — the compiler enforces exhaustiveness.
pub fn authorize(
    principal: &Principal,
    action: Action,
    resource: &ResourceContext,
) -> Result<(), AuthorizationError> {
    match action {
        // ── Report / export / import actions ──────────────────────────────
        Action::ReadDailyReports
        | Action::ManageDailyReports
        | Action::ReadMonthlySummary
        | Action::ReadWilayaReports
        | Action::ExportProducts
        | Action::ExportDailyReport
        | Action::ExportMonthlySummary
        | Action::ImportProductsPackage
        | Action::ImportDailyReportPackage
        | Action::ImportMonthlySummaryPackage
        | Action::ExportStockMovements
        | Action::ExportStockMovementsPackage
        | Action::ImportStockMovementsPackage
        | Action::ImportStockMovements
        | Action::ImportTrustPackage
        | Action::ImportRegistryPackage => reports::authorize_reports(principal, action, resource),

        // ── Strict system / admin-only actions ───────────────────────────
        Action::ReadImportAudit | Action::ConfigureAsWilaya | Action::ReadAuditLog => {
            system::authorize_system(principal, action, resource)
        }

        // ── Core domain management (Admin-only) ──────────────────────────
        Action::AdminOnly | Action::ManageProducts | Action::ManageInventory => {
            system::authorize_system(principal, Action::AdminOnly, resource)
        }

        // ── Unit management (Wilaya node-scoped authority) ───────────────
        // Unit CRUD and UNIT bootstrap export are Wilaya-side operations.
        // This closes the previous ManageUnits gap: a UNIT admin may no
        // longer manage units through the AdminOnly-only arm.
        Action::ManageUnits => {
            if let ResourceContext::WilayaNode = resource {
                system::authorize_system(principal, Action::AdminOnly, resource)
            } else {
                Err(AuthorizationError::InsufficientPermissions)
            }
        }

        // ── Actions requiring authentication (any valid session) ──────────
        // These must still require a valid session — enforced by the command
        // dispatcher (`authorize_command`) before this function is reached.
        // Any anonymous/unauthenticated call never reaches here.
        Action::AuthenticatedOnly
        | Action::ManageBackups
        | Action::ReadProducts
        | Action::ReadOrders
        | Action::ManageOrders
        | Action::ReadInventory
        | Action::ReadSystemMetrics
        | Action::ReadUnits => system::authorize_authenticated(principal, resource),

        // ── Per-user activity (self or admin) ────────────────────────────
        Action::ReadUserActivity => system::authorize_system(principal, action, resource),

        // ── Observability & conflict management (Admin-only) ─────────────
        Action::ViewSystemHealth | Action::ManageSyncConflicts => {
            system::authorize_system(principal, Action::AdminOnly, resource)
        }

        // ── Fiscal lifecycle ──────────────────────────────────────────────
        Action::CloseFiscalYearAuthority => {
            // Only Wilaya admins may execute the *authority* to close fiscal years.
            if let ResourceContext::WilayaNode = resource {
                system::authorize_system(principal, Action::AdminOnly, resource)
            } else {
                Err(AuthorizationError::InsufficientPermissions)
            }
        }
        Action::ApplyFiscalTransition => {
            // Any authenticated user on the Unit node can apply a Wilaya-authorized transition.
            if let ResourceContext::UnitNode { .. } = resource {
                system::authorize_authenticated(principal, resource)
            } else {
                Err(AuthorizationError::InsufficientPermissions)
            }
        }

        // ── Identity credential rotation (B7) ──────────────────────────────
        // Same node-scoped posture as authenticated actions: WILAYA rotation /
        // UNIT CSR signing require a WILAYA Admin; UNIT rotation requires any
        // valid session on the UNIT node.
        Action::RotateCredential | Action::ReissueCredential => {
            system::authorize_authenticated(principal, resource)
        }

        // ── UNIT bootstrap CSR signing (SEC-004-01) ─────────────────────────
        // WILAYA-node Admin-only: the WILAYA signing key is the trust decision
        // for the UNIT identity ceremony. UNIT nodes and WILAYA non-admin users
        // are denied before any policy evaluation.
        Action::SignUnitIdentityRequest => {
            if let ResourceContext::WilayaNode = resource {
                system::authorize_system(principal, Action::AdminOnly, resource)
            } else {
                Err(AuthorizationError::InsufficientPermissions)
            }
        }

        // ── Identity & Access Synchronization (B8) ─────────────────────────
        // Account management and package export are Wilaya-side authorities:
        // the Wilaya is the single source of truth for the two synced accounts.
        Action::ManageAccountSync | Action::ExportIdentityAccessPackage => {
            if let ResourceContext::WilayaNode = resource {
                system::authorize_system(principal, Action::AdminOnly, resource)
            } else {
                Err(AuthorizationError::InsufficientPermissions)
            }
        }
        // Package import is a UNIT-side apply (one-way Wilaya→UNIT, no reverse path).
        // SEC-003-06-b: UNIT Admin-only — applying a WILAYA-issued account-sync
        // package carries credential-overwrite authority and must not be reachable
        // by a non-admin role on the UNIT node.
        Action::ImportIdentityAccessPackage => {
            if let ResourceContext::UnitNode { .. } = resource {
                system::authorize_system(principal, Action::AdminOnly, resource)
            } else {
                Err(AuthorizationError::InsufficientPermissions)
            }
        }

        // ── Licensing management (ADR-0042 / B7-licensing) ─────────────────
        // Status is readable by any authenticated user; installing the trust
        // anchor and importing licenses is an Admin authority on any node
        // (per-node licenses are imported directly on the node they bind to).
        Action::ReadLicensingStatus => system::authorize_authenticated(principal, resource),
        Action::ManageLicensing => system::authorize_system(principal, Action::AdminOnly, resource),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::UserRole;

    fn principal(role: UserRole) -> Principal {
        Principal {
            user_id: "u1".to_string(),
            username: "operator".to_string(),
            role,
            session_id: Some("s1".to_string()),
        }
    }

    #[test]
    fn import_identity_access_package_requires_unit_admin() {
        let unit_node = ResourceContext::UnitNode {
            unit_id: "unit-a".to_string(),
        };

        assert!(authorize(
            &principal(UserRole::Admin),
            Action::ImportIdentityAccessPackage,
            &unit_node,
        )
        .is_ok());

        let denied = authorize(
            &principal(UserRole::User),
            Action::ImportIdentityAccessPackage,
            &unit_node,
        );
        assert!(
            matches!(denied, Err(AuthorizationError::RequiresAdmin)),
            "UNIT User must be denied identity_access import, got: {denied:?}"
        );
    }

    #[test]
    fn import_identity_access_package_is_never_allowed_on_wilaya() {
        let denied = authorize(
            &principal(UserRole::Admin),
            Action::ImportIdentityAccessPackage,
            &ResourceContext::WilayaNode,
        );
        assert!(denied.is_err());
    }

    #[test]
    fn export_identity_access_package_remains_wilaya_admin_only() {
        let ok = authorize(
            &principal(UserRole::Admin),
            Action::ExportIdentityAccessPackage,
            &ResourceContext::WilayaNode,
        );
        assert!(ok.is_ok());

        let denied = authorize(
            &principal(UserRole::User),
            Action::ExportIdentityAccessPackage,
            &ResourceContext::WilayaNode,
        );
        assert!(denied.is_err());

        let denied_unit = authorize(
            &principal(UserRole::Admin),
            Action::ExportIdentityAccessPackage,
            &ResourceContext::UnitNode {
                unit_id: "unit-a".to_string(),
            },
        );
        assert!(denied_unit.is_err());
    }
}
