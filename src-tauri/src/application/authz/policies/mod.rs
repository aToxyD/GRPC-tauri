//! Authorization policy router.
//!
//! DEFAULT: **deny**. All actions must be explicitly handled.
//! No wildcard fallthrough anywhere in the policy tree.

use super::{Action, AuthorizationError, Principal, ResourceContext};
use crate::models::{NodeType, UserRole};

mod reports;
mod system;

/// B8 (ADR-0045) pipeline selection for `identity_access` imports.
///
/// The role decision is an authorization decision and therefore owned by the
/// authz layer — commands must not branch on roles directly. Only UNIT-node
/// sessions resolve to a path: Admin keeps the AdminOnly pipeline (the
/// regular, post-provisioning re-import), User is admitted ONLY through the
/// fail-closed first-import pipeline (predicates enforced inside the import
/// transaction). WILAYA nodes and any other session are denied outright.
pub enum IdentityAccessImportPath {
    AdminOnly,
    FirstImportBootstrap,
}

pub fn resolve_identity_access_import_path(
    role: &UserRole,
    node_type: NodeType,
) -> Result<IdentityAccessImportPath, AuthorizationError> {
    match (node_type, role) {
        (NodeType::Unit, UserRole::Admin) => Ok(IdentityAccessImportPath::AdminOnly),
        (NodeType::Unit, UserRole::User) => Ok(IdentityAccessImportPath::FirstImportBootstrap),
        _ => Err(AuthorizationError::InsufficientPermissions),
    }
}

/// Admin-Access import pipeline selection for `admin_access` imports
/// (ADR-0051 §8 — Accepted 2026-08-22).
///
/// Parallel to [`resolve_identity_access_import_path`]: only UNIT-node
/// sessions resolve to a path. Admin keeps the AdminOnly pipeline; User is
/// admitted ONLY through the fail-closed first-import pipeline whose
/// predicates (`anchor_installed`, `anchor_is_issuer`, `no_active_admin`) are
/// enforced inside the import transaction. There is deliberately NO target
/// binding predicate — the package is fleet-wide and carries no target.
/// WILAYA nodes and any other session are denied outright.
pub fn resolve_admin_access_import_path(
    role: &UserRole,
    node_type: NodeType,
) -> Result<IdentityAccessImportPath, AuthorizationError> {
    match (node_type, role) {
        (NodeType::Unit, UserRole::Admin) => Ok(IdentityAccessImportPath::AdminOnly),
        (NodeType::Unit, UserRole::User) => Ok(IdentityAccessImportPath::FirstImportBootstrap),
        _ => Err(AuthorizationError::InsufficientPermissions),
    }
}

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
        | Action::ImportRegistryPackage
        | Action::ExportContractCatalogPackage
        | Action::ImportContractCatalogPackage => {
            reports::authorize_reports(principal, action, resource)
        }

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
        Action::ManageAccountSync => {
            if let ResourceContext::WilayaNode = resource {
                system::authorize_system(principal, Action::AdminOnly, resource)
            } else {
                Err(AuthorizationError::InsufficientPermissions)
            }
        }
        // ── Admin-Only B8 Account Synchronization (ADR-0051) ────────────────
        // Export is a WILAYA-side Admin-only authority: the fleet `admin`
        // credential state is WILAYA-owned. There is NO unit selector in the
        // export path — the package is fleet-wide by construction.
        Action::ExportAdminAccessPackage => {
            if let ResourceContext::WilayaNode = resource {
                system::authorize_system(principal, Action::AdminOnly, resource)
            } else {
                Err(AuthorizationError::RequiresWilayaNode)
            }
        }
        // Import post-bootstrap is a UNIT-side Admin-only apply: the payload
        // carries credential-overwrite authority for the canonical `admin`
        // account only. The first-import bootstrap exemption is resolved
        // separately through [`resolve_admin_access_import_path`] and remains
        // fail-closed inside the import transaction.
        Action::ImportAdminAccessPackage => {
            if let ResourceContext::UnitNode { .. } = resource {
                system::authorize_system(principal, Action::AdminOnly, resource)
            } else {
                Err(AuthorizationError::RequiresUnitNode)
            }
        }
        // ── Contract-centric procurement (ADR-0055 / SEC-087-F) ─────────────
        // WILAYA is the single source of truth for procurement authority.
        // Every write/projection authority is WILAYA-node Admin-only.
        Action::ManageSuppliers
        | Action::ManageContracts
        | Action::ApproveContractPrice
        | Action::CloseContract
        | Action::ManageTaxPolicy
        | Action::ReleaseContractAllocation
        | Action::RevokeContractAllocationRelease
        | Action::ExportSuppliers
        | Action::ExportContracts
        | Action::ExportContractAllocations => {
            if let ResourceContext::WilayaNode = resource {
                system::authorize_system(principal, Action::AdminOnly, resource)
            } else {
                Err(AuthorizationError::RequiresWilayaNode)
            }
        }
        // The authoritative contract/supplier entitlement projection is
        // WILAYA-admin to read (it is the projection owner). UNIT nodes are
        // read-only consumers via the ContractCatalog sync, not via this action.
        Action::ReadContractProjection => {
            if let ResourceContext::WilayaNode = resource {
                system::authorize_system(principal, Action::AdminOnly, resource)
            } else {
                Err(AuthorizationError::RequiresWilayaNode)
            }
        }
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
    fn trust_package_import_remains_wilaya_admin_only() {
        // SEC-010: trust distribution is a WILAYA authority duty. A UNIT node
        // must NEVER be able to import a trust package (Unit stores stay
        // unreachable through this path).
        let ok = authorize(
            &principal(UserRole::Admin),
            Action::ImportTrustPackage,
            &ResourceContext::WilayaNode,
        );
        assert!(ok.is_ok());

        let denied_user = authorize(
            &principal(UserRole::User),
            Action::ImportTrustPackage,
            &ResourceContext::WilayaNode,
        );
        assert!(
            matches!(denied_user, Err(AuthorizationError::RequiresAdmin)),
            "WILAYA non-admin must be denied trust import, got: {denied_user:?}"
        );

        let denied_unit = authorize(
            &principal(UserRole::Admin),
            Action::ImportTrustPackage,
            &ResourceContext::UnitNode {
                unit_id: "unit-a".to_string(),
            },
        );
        assert!(
            matches!(denied_unit, Err(AuthorizationError::RequiresWilayaNode)),
            "UNIT nodes must never import trust packages, got: {denied_unit:?}"
        );

        let denied_unit_scope = authorize(
            &principal(UserRole::Admin),
            Action::ImportTrustPackage,
            &ResourceContext::UnitScope {
                unit_id: "unit-a".to_string(),
            },
        );
        assert!(denied_unit_scope.is_err());
    }

    #[test]
    fn resolve_identity_access_import_path_unit_routes_by_role() {
        assert!(matches!(
            resolve_identity_access_import_path(&UserRole::Admin, NodeType::Unit),
            Ok(IdentityAccessImportPath::AdminOnly)
        ));
        assert!(matches!(
            resolve_identity_access_import_path(&UserRole::User, NodeType::Unit),
            Ok(IdentityAccessImportPath::FirstImportBootstrap)
        ));
        assert!(resolve_identity_access_import_path(&UserRole::Admin, NodeType::Wilaya).is_err());
        assert!(resolve_identity_access_import_path(&UserRole::User, NodeType::Wilaya).is_err());
    }

    #[test]
    fn export_admin_access_package_is_wilaya_admin_only() {
        // ADR-0051 §8: export is a WILAYA-side Admin-only authority with NO
        // unit selector anywhere in the path.
        assert!(authorize(
            &principal(UserRole::Admin),
            Action::ExportAdminAccessPackage,
            &ResourceContext::WilayaNode,
        )
        .is_ok());

        let denied_user = authorize(
            &principal(UserRole::User),
            Action::ExportAdminAccessPackage,
            &ResourceContext::WilayaNode,
        );
        assert!(
            matches!(denied_user, Err(AuthorizationError::RequiresAdmin)),
            "WILAYA non-admin must be denied admin_access export, got: {denied_user:?}"
        );

        let denied_unit = authorize(
            &principal(UserRole::Admin),
            Action::ExportAdminAccessPackage,
            &ResourceContext::UnitNode {
                unit_id: "unit-a".to_string(),
            },
        );
        assert!(
            matches!(denied_unit, Err(AuthorizationError::RequiresWilayaNode)),
            "UNIT nodes must never export admin_access, got: {denied_unit:?}"
        );
    }

    #[test]
    fn import_admin_access_package_requires_unit_admin_post_bootstrap() {
        let unit_node = ResourceContext::UnitNode {
            unit_id: "unit-a".to_string(),
        };

        assert!(authorize(
            &principal(UserRole::Admin),
            Action::ImportAdminAccessPackage,
            &unit_node,
        )
        .is_ok());

        let denied = authorize(
            &principal(UserRole::User),
            Action::ImportAdminAccessPackage,
            &unit_node,
        );
        assert!(
            matches!(denied, Err(AuthorizationError::RequiresAdmin)),
            "UNIT User must be denied admin_access import post-bootstrap, got: {denied:?}"
        );

        let denied_wilaya = authorize(
            &principal(UserRole::Admin),
            Action::ImportAdminAccessPackage,
            &ResourceContext::WilayaNode,
        );
        assert!(denied_wilaya.is_err());
    }

    #[test]
    fn resolve_admin_access_import_path_unit_routes_by_role() {
        // ADR-0051 §8: same routing shape as identity_access — Admin keeps the
        // AdminOnly pipeline; User reaches ONLY the fail-closed first-import
        // pipeline; WILAYA sessions are denied outright.
        assert!(matches!(
            resolve_admin_access_import_path(&UserRole::Admin, NodeType::Unit),
            Ok(IdentityAccessImportPath::AdminOnly)
        ));
        assert!(matches!(
            resolve_admin_access_import_path(&UserRole::User, NodeType::Unit),
            Ok(IdentityAccessImportPath::FirstImportBootstrap)
        ));
        assert!(resolve_admin_access_import_path(&UserRole::Admin, NodeType::Wilaya).is_err());
        assert!(resolve_admin_access_import_path(&UserRole::User, NodeType::Wilaya).is_err());
    }

    #[test]
    fn procurement_write_actions_are_wilaya_admin_only() {
        // ADR-0055 §3.9: suppliers, contracts, agreed-price approval, closing,
        // TVA policy, and obligation release/revoke are WILAYA authorities.
        // UNIT nodes and WILAYA non-admins are denied before any evaluation.
        let write_actions = [
            Action::ManageSuppliers,
            Action::ManageContracts,
            Action::ApproveContractPrice,
            Action::CloseContract,
            Action::ManageTaxPolicy,
            Action::ReleaseContractAllocation,
            Action::RevokeContractAllocationRelease,
            Action::ExportContractCatalogPackage,
            Action::ExportSuppliers,
            Action::ExportContracts,
            Action::ExportContractAllocations,
        ];

        for action in write_actions {
            assert!(
                authorize(
                    &principal(UserRole::Admin),
                    action,
                    &ResourceContext::WilayaNode,
                )
                .is_ok(),
                "WILAYA Admin must be authorized for {action:?}"
            );

            let denied_unit = authorize(
                &principal(UserRole::Admin),
                action,
                &ResourceContext::UnitNode {
                    unit_id: "unit-a".to_string(),
                },
            );
            assert!(
                matches!(denied_unit, Err(AuthorizationError::RequiresWilayaNode)),
                "UNIT nodes must never {action:?}, got: {denied_unit:?}"
            );

            let denied_user = authorize(
                &principal(UserRole::User),
                action,
                &ResourceContext::WilayaNode,
            );
            assert!(
                matches!(denied_user, Err(AuthorizationError::RequiresAdmin)),
                "WILAYA non-admin must never {action:?}, got: {denied_user:?}"
            );
        }
    }

    #[test]
    fn read_contract_projection_is_wilaya_admin_only() {
        assert!(authorize(
            &principal(UserRole::Admin),
            Action::ReadContractProjection,
            &ResourceContext::WilayaNode,
        )
        .is_ok());

        assert!(matches!(
            authorize(
                &principal(UserRole::Admin),
                Action::ReadContractProjection,
                &ResourceContext::UnitNode {
                    unit_id: "unit-a".to_string(),
                },
            ),
            Err(AuthorizationError::RequiresWilayaNode)
        ));
    }

    #[test]
    fn contract_catalog_import_is_operational_on_unit_nodes() {
        // ADR-0055 / SEC-087-F ContractCatalog sync: the UNIT node applies the
        // WILAYA projection operationally (any role), mirroring
        // ImportProductsPackage. WILAYA non-admins stay denied.
        let unit_node = ResourceContext::UnitNode {
            unit_id: "unit-a".to_string(),
        };
        let unit_scope = ResourceContext::UnitScope {
            unit_id: "unit-a".to_string(),
        };

        assert!(authorize(
            &principal(UserRole::Admin),
            Action::ImportContractCatalogPackage,
            &unit_node,
        )
        .is_ok());
        assert!(authorize(
            &principal(UserRole::User),
            Action::ImportContractCatalogPackage,
            &unit_scope,
        )
        .is_ok());

        assert!(matches!(
            authorize(
                &principal(UserRole::User),
                Action::ImportContractCatalogPackage,
                &ResourceContext::WilayaNode,
            ),
            Err(AuthorizationError::RequiresAdmin)
        ));
    }
}
