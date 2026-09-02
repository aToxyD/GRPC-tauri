import { readFileSync } from "fs";

export class FileCache {
  private store = new Map<string, string>();
  private hitCount = 0;
  private missCount = 0;

  get(path: string): string {
    if (this.store.has(path)) {
      this.hitCount++;
      return this.store.get(path)!;
    }
    this.missCount++;
    const content = readFileSync(path, "utf-8");
    this.store.set(path, content);
    return content;
  }

  exists(path: string): boolean {
    try {
      this.get(path);
      return true;
    } catch {
      return false;
    }
  }

  get hits() {
    return this.hitCount;
  }

  get misses() {
    return this.missCount;
  }

  get files() {
    return this.store.size;
  }
}

export const DOMAIN_REGISTRY: Record<string, {
  contract: string;
  pages: string[];
  functions: string[];
  crossDomainExceptions: string[];
}> = {
  consumption: {
    contract: "consumption.contract.ts",
    pages: ["ConsumptionPage"],
    functions: ["createDailyReport", "previewDailyConsumptionFifo", "createMealConsumption", "getDailyReport", "listDailyReports", "getDailyConsumption", "calculateMealCost", "calculateMealRate", "recordConsumption"],
    crossDomainExceptions: ["listProducts", "checkStockAvailability"],
  },
  inventory: {
    contract: "inventory.contract.ts",
    pages: ["ProductsPage", "StockPage", "UnitDashboard", "UnitsPage", "UnitInventoryPage"],
    functions: ["createProduct", "updateProduct", "deleteProduct", "getProduct", "listProducts", "createUnit", "getUnit", "listUnits", "updateUnit", "deleteUnit", "getStock", "getAllStocks", "checkStockAvailability", "getCurrentStock", "getInventoryFifoView", "computeUnitInventorySnapshot", "getUnitInventoryView", "getAvailableReportMonths", "exportUnitInventoryExcel", "getStockMovements", "getStockSummary", "calculateProductPriceWithTva", "exportProductsExcel", "exportStockMovementsExcel", "getOrders"],
    crossDomainExceptions: ["exportProductsPackage", "importProductsPackage", "exportStockMovementsPackage", "importStockMovementsPackage", "exportUnitNodePackage", "listSupplierOrders"],
  },
  orders: {
    contract: "orders.contract.ts",
    pages: ["OrdersPage"],
    functions: ["createSupplierOrder", "confirmOrder", "updateSupplierOrder", "deleteSupplierOrder", "getSupplierOrder", "getSupplierOrderItems", "listSupplierOrders", "createOrder"],
    crossDomainExceptions: ["listProducts"],
  },
  report: {
    contract: "report.contract.ts",
    pages: ["UnitReportsPage", "WilayaReportsPage"],
    functions: ["getMonthlySummary", "listFiscalYears", "listWilayaReports", "generateReports", "getReportData", "exportDailyReportExcel", "exportMonthlySummaryExcel", "exportAllUnitsMonthlyStatusExcel"],
    crossDomainExceptions: ["listUnits", "listDailyReports", "getDailyReport", "exportDailyReportPackage", "exportMonthlySummaryPackage"],
  },
  fiscal: {
    contract: "fiscal.contract.ts",
    pages: ["FiscalManagementPage", "FiscalDiagnosticsPage"],
    functions: ["closeFiscalYear", "getFiscalYearStatus", "exportFiscalClosurePackage", "previewFiscalClosurePackage", "applyFiscalClosurePackage", "getFiscalTransitionHistory", "listFiscalPackageRegistry", "updateFiscalPackageRetentionStatus", "getAdvancedDiagnosticsBundle", "verifyInventoryIntegrity", "createFiscalOperationalSnapshot"],
    crossDomainExceptions: ["listProducts", "getSystemHealth"],
  },
  sync: {
    contract: "sync.contract.ts",
    pages: ["SyncPage", "SettingsPage"],
    functions: ["exportProductsPackage", "exportDailyReportPackage", "exportMonthlySummaryPackage", "exportUnitNodePackage", "exportStockMovementsPackage", "importProductsPackage", "importDailyReportPackage", "importUnitNodePackage", "importMonthlySummaryPackage", "importStockMovementsPackage", "setFleetAdminPassword"],
    crossDomainExceptions: ["listUnits", "getSettings", "login"],
  },
  backup: {
    contract: "backup.contract.ts",
    pages: ["BackupPage"],
    functions: ["createBackup", "listBackups", "issueOperationExecutionToken", "restoreBackup"],
    crossDomainExceptions: [],
  },
  dashboard: {
    contract: "dashboard.contract.ts",
    pages: ["SystemHealthPage"],
    functions: ["getBuildInfo", "getRecentTelemetry"],
    crossDomainExceptions: ["getSystemHealth", "getSyncHealth"],
  },
  observability: {
    contract: "observability.contract.ts",
    pages: ["AuditIntegrityPage", "SystemHealthPage", "SyncTopologyPage", "ConflictCenterPage"],
    functions: ["getAuditChainStatus", "getAuditHealth", "getSystemHealth", "getSyncHealth", "getConflictSummary", "listSyncConflicts", "resolveSyncConflict"],
    crossDomainExceptions: [],
  },
  audit: {
    contract: "audit.contract.ts",
    pages: ["AuditLogPage"],
    functions: ["getAuditLog", "getAuditStats", "getUserActivity", "exportAuditLogExcel", "cleanupAuditLogs"],
    crossDomainExceptions: [],
  },
  metrics: {
    contract: "metrics.contract.ts",
    pages: ["UnitStatisticsPage", "WilayaStatisticsPage", "WilayaDashboard"],
    functions: ["getLoginMetrics", "getSystemMetrics", "getSyncSecurityDiagnostics", "syncPreflightCheck"],
    crossDomainExceptions: ["listUnits", "listProducts", "getSecurityStatus"],
  },
  session: {
    contract: "session.contract.ts",
    pages: ["LoginPage", "WilayaNodeSetupPage"],
    functions: ["logout", "checkSession", "getCurrentUser", "touchSession", "getSettings", "configureAsWilaya", "isConfigured"],
    crossDomainExceptions: ["login", "importUnitNodePackage"],
  },
  user: {
    contract: "user.contract.ts",
    pages: ["LoginPage"],
    functions: ["login"],
    crossDomainExceptions: ["getSecurityStatus"],
  },
  security: {
    contract: "security.contract.ts",
    pages: ["AppSecurityPage"],
    functions: ["getSecurityStatus", "initializeAppKey", "unlockAppKey", "exportAppKeyBackupToPath"],
    crossDomainExceptions: ["login"],
  },
  procurement: {
    contract: "procurement.contract.ts",
    pages: ["SuppliersPage", "ContractsPage"],
    functions: [
      "createSupplier", "updateSupplier", "setSupplierActive", "associateSupplierWithUnit", "disassociateSupplierFromUnit",
      "createContract", "addContractProduct", "setContractProductAgreedPrice", "acceptContract", "activateContract", "endContract", "cancelContract",
      "releaseContractAllocation", "revokeContractAllocationRelease",
      "setFiscalTaxPolicy", "getFiscalTaxPolicy", "listFiscalTaxPolicies",
      "getSupplier", "listSuppliers", "listUnitSuppliers", "getContract", "listContracts", "getContractProducts", "listContractAllocations", "listAllocationExceptions",
    ],
    crossDomainExceptions: ["listProducts", "listUnits"],
  },
};

export const UNIVERSAL_ALLOWED = ["getSettings"];
