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
        Action::AdminOnly
        | Action::ManageProducts
        | Action::ManageUnits // [arch:allow-manageunits-wilaya] see ADR-0038 (B2 adds WILAYA node-type guard)
        | Action::ManageInventory => {
            system::authorize_system(principal, Action::AdminOnly, resource)
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
    }
}
