# Domain Ownership Registry

**Status:** Active — Phase 4A

**Version:** v1.0.0

---

## Purpose

Single source of truth for frontend domain ownership. Defines which contract owns
each domain, which pages belong to each domain, allowed imports, and forbidden imports.

---

## Domain Map

### consumption

| Property | Value |
|----------|-------|
| Owner Contract | `consumption.contract.ts` |
| Owned Pages | `ConsumptionPage.svelte` |
| Owned Projection Types | `DailyReportResult`, `DailyFifoConsumptionPreview`, `DailyConsumptionView`, `DailyReport`, `MealSectionResult`, `DailyConsumptionSummary` |
| Allowed Imports | consumption contract, session contract (`getSettings`) |
| Cross-Domain Exceptions | `listProducts` (inventory), `checkStockAvailability` (inventory) |
| Forbidden Imports | All other domain contracts |

---

### inventory

| Property | Value |
|----------|-------|
| Owner Contract | `inventory.contract.ts` |
| Owned Pages | `ProductsPage.svelte`, `StockPage.svelte`, `UnitDashboard.svelte`, `UnitsPage.svelte`, `UnitInventoryPage.svelte` |
| Owned Projection Types | `Product`, `Unit`, `InventoryStock`, `StockCheckResult`, `StockMovement`, `StockSummary`, `InventoryProductView`, `InventoryStockPageView`, `UnitInventoryView`, `UnitMonthlySnapshot`, `ComputeSnapshotResult` |
| Allowed Imports | inventory contract, session contract (`getSettings`) |
| Cross-Domain Exceptions | `exportProductsPackage` (sync), `importProductsPackage` (sync), `exportStockMovementsPackage` (sync), `importStockMovementsPackage` (sync), `exportUnitNodePackage` (sync), `listSupplierOrders` (orders) |
| Forbidden Imports | All other domain contracts |

---

### orders

| Property | Value |
|----------|-------|
| Owner Contract | `orders.contract.ts` |
| Owned Pages | `OrdersPage.svelte` |
| Owned Projection Types | `SupplierOrder`, `SupplierOrderItem` |
| Allowed Imports | orders contract, session contract (`getSettings`) |
| Cross-Domain Exceptions | `listProducts` (inventory) |
| Forbidden Imports | All other domain contracts |

---

### report

| Property | Value |
|----------|-------|
| Owner Contract | `report.contract.ts` |
| Owned Pages | `UnitReportsPage.svelte`, `WilayaReportsPage.svelte` |
| Owned Projection Types | `MonthlySummary`, `WilayaReportList`, `ReportType` |
| Allowed Imports | report contract, session contract (`getSettings`) |
| Cross-Domain Exceptions | `listUnits` (inventory), `listDailyReports` (consumption), `getDailyReport` (consumption), `exportDailyReportPackage` (sync), `exportMonthlySummaryPackage` (sync) |
| Forbidden Imports | All other domain contracts |

---

### fiscal

| Property | Value |
|----------|-------|
| Owner Contract | `fiscal.contract.ts` |
| Owned Pages | `FiscalManagementPage.svelte`, `FiscalDiagnosticsPage.svelte` |
| Owned Projection Types | `FiscalYearStatus`, `FiscalClosurePreview`, `FiscalClosureApplyResult`, `FiscalTransitionHistoryEntry`, `FiscalPackageRegistryEntry`, `CloseFiscalYearRequest`, `CloseFiscalYearResponse` |
| Allowed Imports | fiscal contract, session contract (`getSettings`) |
| Cross-Domain Exceptions | `listProducts` (inventory), `getSystemHealth` (observability) |
| Forbidden Imports | All other domain contracts |

---

### sync

| Property | Value |
|----------|-------|
| Owner Contract | `sync.contract.ts` |
| Owned Pages | `SyncPage.svelte` |
| Owned Projection Types | `SyncExportResult`, `SyncImportResult`, `DailyReportImportResult`, `UnitNodePackageImportResult`, `StockMovementsImportResult` |
| Allowed Imports | sync contract, session contract (`getSettings`) |
| Cross-Domain Exceptions | `listUnits` (inventory), `login` (user) |
| Forbidden Imports | All other domain contracts |

---

### backup

| Property | Value |
|----------|-------|
| Owner Contract | `backup.contract.ts` |
| Owned Pages | `BackupPage.svelte` |
| Owned Projection Types | `BackupInfo` |
| Allowed Imports | backup contract, session contract (`getSettings`) |
| Cross-Domain Exceptions | None |
| Forbidden Imports | All other domain contracts |

---

### dashboard

| Property | Value |
|----------|-------|
| Owner Contract | `dashboard.contract.ts` |
| Owned Pages | `SystemHealthPage.svelte` |
| Owned Projection Types | `BuildInfo`, `TelemetryEvent` |
| Allowed Imports | dashboard contract, session contract (`getSettings`) |
| Cross-Domain Exceptions | `getSystemHealth` (observability), `getSyncHealth` (observability) |
| Forbidden Imports | All other domain contracts |

---

### observability

| Property | Value |
|----------|-------|
| Owner Contract | `observability.contract.ts` |
| Owned Pages | `AuditIntegrityPage.svelte`, `SystemHealthPage.svelte`, `SyncTopologyPage.svelte`, `ConflictCenterPage.svelte` |
| Owned Projection Types | `AuditChainStatus`, `AuditHealthReport`, `SystemHealthReport`, `SyncNodeHealth`, `ConflictSummary`, `SyncConflict` |
| Allowed Imports | observability contract, session contract (`getSettings`) |
| Cross-Domain Exceptions | None |
| Forbidden Imports | All other domain contracts |

---

### audit

| Property | Value |
|----------|-------|
| Owner Contract | `audit.contract.ts` |
| Owned Pages | `AuditLogPage.svelte` |
| Owned Projection Types | `AuditEntry`, `AuditFilters`, `AuditLogResponse`, `AuditStats`, `UserActivitySummary`, `OperationCount`, `DailyOperationCount` |
| Allowed Imports | audit contract, session contract (`getSettings`) |
| Cross-Domain Exceptions | None |
| Forbidden Imports | All other domain contracts |

---

### metrics

| Property | Value |
|----------|-------|
| Owner Contract | `metrics.contract.ts` |
| Owned Pages | `UnitStatisticsPage.svelte`, `WilayaStatisticsPage.svelte`, `WilayaDashboard.svelte` |
| Owned Projection Types | `LoginMetrics`, `SystemMetrics`, `SyncSecurityDiagnostics`, `SyncPreflightCheck` |
| Allowed Imports | metrics contract, session contract (`getSettings`) |
| Cross-Domain Exceptions | `listUnits` (inventory), `listProducts` (inventory), `getSecurityStatus` (security) |
| Forbidden Imports | All other domain contracts |

---

### session

| Property | Value |
|----------|-------|
| Owner Contract | `session.contract.ts` |
| Owned Pages | `LoginPage.svelte`, `WilayaNodeSetupPage.svelte` |
| Owned Projection Types | `SessionStatus`, `Settings` |
| Allowed Imports | session contract, user contract (`login`) |
| Cross-Domain Exceptions | `importUnitNodePackage` (sync), `login` (user) |
| Forbidden Imports | All other domain contracts |

---

### user

| Property | Value |
|----------|-------|
| Owner Contract | `user.contract.ts` |
| Owned Pages | `LoginPage.svelte` |
| Owned Projection Types | `LoginRequest`, `LoginResponse`, `User` |
| Allowed Imports | user contract, session contract |
| Cross-Domain Exceptions | `getSecurityStatus` (security) |
| Forbidden Imports | All other domain contracts |

---

### security

| Property | Value |
|----------|-------|
| Owner Contract | `security.contract.ts` |
| Owned Pages | `AppSecurityPage.svelte` |
| Owned Projection Types | `AppKeyStatusDto`, `AppKeyInitializeResultDto`, `AppKeyUnlockResultDto` |
| Allowed Imports | security contract, session contract (`getSettings`) |
| Cross-Domain Exceptions | `login` (user — route navigation to `/login`) |
| Forbidden Imports | All other domain contracts |

---

### platform

| Property | Value |
|----------|-------|
| Owner Contract | `platform.contract.ts` |
| Owned Pages | None |
| Owned Projection Types | None |
| Allowed Imports | None (no IPC wrappers — direct Tauri plugin calls) |
| Cross-Domain Exceptions | None |
| Forbidden Imports | All other domain contracts |

---

## Universal Cross-Domain Functions

The following contract functions may be imported by any page regardless of domain:

| Function | Contract | Justification |
|----------|----------|---------------|
| `getSettings` | session.contract.ts | Session/configuration is cross-cutting |

---

## Import Graph Validation (FE-152)

The domain registry is enforced by FE-152 in `check_arch.ts`. Each page's contract imports
are validated against the allowed/cross-domain lists above. Violations produce an ERROR.

## Architecture References

- [Projection Purity Lockdown](./PROJECTION_PURITY_LOCKDOWN.md)
- [Projection Ownership Map](./PROJECTION_OWNERSHIP_MAP.md)
- [Frontend Certification v3](./FRONTEND_CERTIFICATION_v3.md)
