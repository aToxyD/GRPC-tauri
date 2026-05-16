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
}
