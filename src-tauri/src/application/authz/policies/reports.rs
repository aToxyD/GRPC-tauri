//! Authorization policy for all report, export, and import actions.
//!
//! DEFAULT: **deny**. Every action must match an explicit arm — no wildcard fallthrough.
//!
//! Matrix:
//! | Action                       | WilayaNode (admin) | WilayaNode (non-admin) | UnitNode (admin) | UnitNode (non-admin) | Global/Other |
//! |------------------------------|--------------------|------------------------|------------------|----------------------|--------------|
//! | ReadWilayaReports            | ✓                  | ✗                      | ✗                | ✗                    | ✗            |
//! | ReadDailyReports             | ✓                  | ✗                      | ✓                | ✓                    | ✗            |
//! | ManageDailyReports           | ✓ (wilaya admin)   | ✗                      | ✓                | ✓                    | ✗            |
//! | ReadMonthlySummary           | ✓                  | ✗                      | ✓                | ✓                    | ✗            |
//! | ExportProducts               | ✓                  | ✗                      | ✗                | ✗                    | ✗            |
//! | ImportProductsPackage        | ✓                  | ✗                      | ✓                | ✓                    | ✗            |
//! | Unit Sync Exports*           | ✓ (wilaya admin)   | ✗                      | ✓                | ✓                    | ✗            |
//! | Wilaya Sync Imports*         | ✓                  | ✗                      | ✗                | ✗                    | ✗            |
//! | Trust/Registry Imports†      | ✓                  | ✗                      | ✗                | ✗                    | ✗            |
//! | ImportStockMovements         | ✓                  | ✗                      | ✓                | ✓                    | ✗            |
//! | ReadAuditLog                 | ✓ (wilaya admin)   | ✗                      | ✓ (unit admin)   | ✗                    | ✗            |
//!
//! *Unit Sync Exports: ExportDailyReport, ExportMonthlySummary, ExportStockMovementsPackage.
//! *Wilaya Sync Imports: ImportDailyReportPackage, ImportMonthlySummaryPackage, ImportStockMovementsPackage.
//! †Trust/Registry Imports: ImportTrustPackage, ImportRegistryPackage.

use crate::models::UserRole;

use super::{Action, AuthorizationError, Principal, ResourceContext};

/// Authorizes report/export/import actions.
/// Every arm is **explicit**; the catch-all denies.
pub fn authorize_reports(
    principal: &Principal,
    action: Action,
    resource: &ResourceContext,
) -> Result<(), AuthorizationError> {
    match action {
        // ----------------------------------------------------------------
        // Wilaya-only: Admin on a Wilaya node
        // ----------------------------------------------------------------
        Action::ReadWilayaReports => {
            if principal.role != UserRole::Admin {
                return Err(AuthorizationError::RequiresAdmin);
            }
            match resource {
                ResourceContext::WilayaNode => Ok(()),
                _ => Err(AuthorizationError::RequiresWilayaNode),
            }
        }

        // ----------------------------------------------------------------
        // Read-only report views: Admin on Wilaya, any role on UnitNode
        // ----------------------------------------------------------------
        Action::ReadDailyReports | Action::ReadMonthlySummary => match resource {
            ResourceContext::WilayaNode => {
                if principal.role != UserRole::Admin {
                    return Err(AuthorizationError::RequiresAdmin);
                }
                Ok(())
            }
            ResourceContext::UnitNode { .. } | ResourceContext::UnitScope { .. } => Ok(()),
            // Global scope requires admin (e.g. bootstrap queries)
            ResourceContext::Global => {
                if principal.role != UserRole::Admin {
                    return Err(AuthorizationError::RequiresAdmin);
                }
                Ok(())
            }
            // All other resource scopes: deny
            _ => Err(AuthorizationError::InsufficientPermissions),
        },

        // ----------------------------------------------------------------
        // Manage (write) daily reports:
        // - WILAYA: Admin only
        // - UNIT: Admin + User (Everyone)
        // ----------------------------------------------------------------
        Action::ManageDailyReports => match resource {
            ResourceContext::UnitNode { .. } | ResourceContext::UnitScope { .. } => Ok(()),
            ResourceContext::WilayaNode | ResourceContext::Global => {
                if principal.role != UserRole::Admin {
                    return Err(AuthorizationError::RequiresAdmin);
                }
                Ok(())
            }
            _ => Err(AuthorizationError::InsufficientPermissions),
        },

        // ----------------------------------------------------------------
        // Export & import package actions:
        // - WILAYA: Admin only
        // - UNIT: Admin + User (Everyone) for operational packages
        // ----------------------------------------------------------------
        Action::ExportProducts
        | Action::ImportDailyReportPackage
        | Action::ImportMonthlySummaryPackage
        | Action::ImportStockMovementsPackage => {
            // These are Wilaya-side management actions or high-level exports
            if principal.role != UserRole::Admin {
                return Err(AuthorizationError::RequiresAdmin);
            }
            match resource {
                ResourceContext::WilayaNode
                | ResourceContext::UnitNode { .. }
                | ResourceContext::Global => Ok(()),
                _ => Err(AuthorizationError::InsufficientPermissions),
            }
        }

        Action::ImportProductsPackage
        | Action::ExportDailyReport
        | Action::ExportMonthlySummary
        | Action::ExportStockMovements
        | Action::ExportStockMovementsPackage
        | Action::ImportStockMovements => {
            // These are operational actions allowed for everyone on a UNIT node,
            // but still require Admin on a WILAYA node.
            match resource {
                ResourceContext::UnitNode { .. } | ResourceContext::UnitScope { .. } => Ok(()),
                ResourceContext::WilayaNode | ResourceContext::Global => {
                    if principal.role != UserRole::Admin {
                        return Err(AuthorizationError::RequiresAdmin);
                    }
                    Ok(())
                }
                _ => Err(AuthorizationError::InsufficientPermissions),
            }
        }

        // ----------------------------------------------------------------
        // Trust/Registry imports: WILAYA Admin only.
        // Unit nodes are NOT trust distributors — the trust chain is
        // Root → WILAYA → UNIT (RFC 2026-08-04-node-identity-trust §3.4).
        // Trust distribution is a WILAYA authority duty; the registry
        // (fleet-state snapshot) is likewise WILAYA-side state. This is part
        // of the identity trust model, not a transient organizational policy.
        // ----------------------------------------------------------------
        Action::ImportTrustPackage | Action::ImportRegistryPackage => {
            if principal.role != UserRole::Admin {
                return Err(AuthorizationError::RequiresAdmin);
            }
            match resource {
                ResourceContext::WilayaNode | ResourceContext::Global => Ok(()),
                // Unit nodes are not trust distributors (see above).
                ResourceContext::UnitNode { .. } | ResourceContext::UnitScope { .. } => {
                    Err(AuthorizationError::RequiresWilayaNode)
                }
                _ => Err(AuthorizationError::InsufficientPermissions),
            }
        }

        // ----------------------------------------------------------------
        // Any other action routed here is an implementation error — deny.
        // ----------------------------------------------------------------
        _ => Err(AuthorizationError::InsufficientPermissions),
    }
}
