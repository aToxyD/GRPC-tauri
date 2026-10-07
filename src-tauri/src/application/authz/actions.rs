#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // Reports
    ReadDailyReports,
    ManageDailyReports,
    ReadMonthlySummary,
    ReadWilayaReports,
    ExportProducts,
    ExportDailyReport,
    ExportMonthlySummary,
    ImportProductsPackage,
    ImportDailyReportPackage,
    ImportMonthlySummaryPackage,
    ReadImportAudit,
    ReadAuditLog,
    ExportStockMovements,
    ExportStockMovementsPackage,
    ImportStockMovementsPackage,
    ImportStockMovements,
    ImportTrustPackage,
    ImportRegistryPackage,

    // Core Domain
    ManageProducts,
    ReadProducts,
    ManageOrders,
    ReadOrders,
    ManageInventory,
    ReadInventory,
    ManageUnits,
    ReadUnits,
    ReadUserActivity,

    // System/Config
    ConfigureAsWilaya,
    ReadSystemMetrics,

    // Observability & Operations
    ViewSystemHealth,
    ManageSyncConflicts,

    // Generic
    ManageBackups,
    AdminOnly,
    AuthenticatedOnly,

    // Fiscal lifecycle
    CloseFiscalYearAuthority,
    ApplyFiscalTransition,

    // Identity credential rotation (B7)
    RotateCredential,
    ReissueCredential,

    // UNIT bootstrap CSR signing (RFC §3.12 D2, WILAYA side)
    // SEC-004-01: WILAYA-node Admin-only signing authority for the UNIT
    // identity bootstrap ceremony. Distinct from `RotateCredential` so the
    // bootstrap trust decision is never conflated with rotation.
    SignUnitIdentityRequest,

    // Identity & Access Synchronization (B8)
    ManageAccountSync,

    // Admin-Only B8 Account Synchronization (ADR-0051 — Accepted 2026-08-22)
    ExportAdminAccessPackage,
    ImportAdminAccessPackage,

    // Canonical local UNIT operator self password change (ADR-0063 §6 / D3).
    // Scoped by policy to UNIT node + username `user` + role `User`; the target
    // is always the authenticated principal, never a caller-supplied id.
    ChangeOwnPassword,

    // Canonical local UNIT operator password reset by the local admin
    // (ADR-0063 §7.1 / D6). Scoped by policy to UNIT node + the canonical
    // local `admin` (`BOOTSTRAP_ADMIN_USERNAME`); the target is derived
    // server-side from the local UNIT code — never a caller-supplied username
    // or unit id.
    ResetLocalUnitUserPassword,

    // Contract-centric procurement (ADR-0055 / SEC-087-F)
    ManageSuppliers,
    ManageContracts,
    ApproveContractPrice,
    CloseContract,
    ManageTaxPolicy,
    ReadContractProjection,
    ReleaseContractAllocation,
    RevokeContractAllocationRelease,
    ExportContractCatalogPackage,
    ImportContractCatalogPackage,

    // UNIT local read of its own offline ContractCatalog entitlement projection.
    // Distinct from `ReadContractProjection` (WILAYA-only projection owner): this
    // is a UNIT-node-scoped, read-only consumer read over locally imported rows.
    ReadUnitEntitlements,

    // Contract-centric procurement Excel exports (ADR-0055 / SEC-087-F)
    ExportSuppliers,
    ExportContracts,
    ExportContractAllocations,

    // Allocation-level cumulative fulfillment state synchronization
    // (ADR-0061, kind `contract_fulfillment`).
    //
    // `Export…` is a UNIT-node capability (the UNIT owns the fulfillment
    // state); `Import…` is a WILAYA-node capability (the WILAYA converges its
    // projection from the authenticated UNIT state). They are distinct actions
    // so neither direction can be authorized by the other's policy.
    ExportContractFulfillment,
    ImportContractFulfillment,
}
