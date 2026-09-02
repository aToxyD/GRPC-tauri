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
}
