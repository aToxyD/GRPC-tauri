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
//! | ExportContractFulfillment‡   | ✗                  | ✗                      | ✓                | ✓                    | ✗            |
//! | ImportContractFulfillment‡   | ✓ (wilaya admin)   | ✗                      | ✗                | ✗                    | ✗            |
//!
//! *Unit Sync Exports: ExportDailyReport, ExportMonthlySummary, ExportStockMovementsPackage.
//! *Wilaya Sync Imports: ImportDailyReportPackage, ImportMonthlySummaryPackage, ImportStockMovementsPackage.
//! †Trust/Registry Imports: ImportTrustPackage, ImportRegistryPackage.
//! ‡Allocation-level cumulative fulfillment state (ADR-0061): UNIT-owned state,
//! exported by the source UNIT — a UNIT operational sync export, so any
//! authenticated role on the local UNIT node is allowed — and converged by the
//! destination WILAYA, whose import remains WILAYA-admin.

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
        | Action::ImportContractCatalogPackage
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

        // ContractCatalog fleet export (ADR-0055 / SEC-087-F): the catalog is a
        // WILAYA-owned projection — only the WILAYA node may emit it. UNIT nodes
        // are read-only consumers via the sync apply path, never producers.
        Action::ExportContractCatalogPackage => {
            if principal.role != UserRole::Admin {
                return Err(AuthorizationError::RequiresAdmin);
            }
            match resource {
                ResourceContext::WilayaNode => Ok(()),
                ResourceContext::UnitNode { .. } | ResourceContext::UnitScope { .. } => {
                    Err(AuthorizationError::RequiresWilayaNode)
                }
                _ => Err(AuthorizationError::InsufficientPermissions),
            }
        }

        // ----------------------------------------------------------------
        // Allocation-level cumulative fulfillment state sync (ADR-0061).
        //
        // The direction of authority decides the arm: the UNIT owns the
        // fulfillment state and is the only producer, the WILAYA is the only
        // convergence target.
        //
        // `ExportContractFulfillment` is a UNIT **operational synchronization
        // export** — the same class as `ExportDailyReport`,
        // `ExportMonthlySummary` and `ExportStockMovementsPackage` — so it is
        // allowed for every authenticated role on the local UNIT node, in line
        // with the `Unit Sync Exports` row of the matrix above. It exports only
        // the node's OWN `contract_allocations.fulfilled_quantity` state, the
        // source node identity is resolved server-side from local settings
        // (`resolve_export_source_node_id`) and never supplied by the caller,
        // and the package is signed by the node identity rather than by the
        // user. It therefore grants no authority over contract master data:
        // creating/modifying suppliers or contracts, approving prices or
        // entitlements, TVA policy, and obligation release remain Admin-only
        // and WILAYA-only (`mod.rs` procurement arm, ADR-0055).
        //
        // The action stays strictly distinct from `ImportContractFulfillment`,
        // which is the WILAYA-side convergence direction and keeps its own
        // Admin-only policy below. Neither direction can be authorized by the
        // other's policy, and both are refused on the wrong node type rather
        // than silently downgraded.
        // ----------------------------------------------------------------
        Action::ExportContractFulfillment => match resource {
            ResourceContext::UnitNode { .. } | ResourceContext::UnitScope { .. } => Ok(()),
            ResourceContext::WilayaNode | ResourceContext::Global => {
                Err(AuthorizationError::RequiresUnitNode)
            }
            _ => Err(AuthorizationError::InsufficientPermissions),
        },

        Action::ImportContractFulfillment => {
            if principal.role != UserRole::Admin {
                return Err(AuthorizationError::RequiresAdmin);
            }
            match resource {
                ResourceContext::WilayaNode | ResourceContext::Global => Ok(()),
                ResourceContext::UnitNode { .. } | ResourceContext::UnitScope { .. } => {
                    Err(AuthorizationError::RequiresWilayaNode)
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
