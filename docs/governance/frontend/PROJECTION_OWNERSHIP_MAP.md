# Projection Ownership Map

**Status:** Active — Phase 4A

**Version:** v1.0.0

---

## Purpose

Every backend projection type must have a known owner contract, IPC source, and
set of consuming pages. This map enables projection leakage detection and domain
isolation enforcement (FE-152).

---

## Projection Map

### Authentication & User

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `LoginRequest` | user.contract.ts | `—` (input type) | LoginPage |
| `LoginResponse` | user.contract.ts | `login` | LoginPage |
| `User` | user.contract.ts | `get_current_user`, `login` | Sidebar, session.ts |

---

### Session & Configuration

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `SessionStatus` | session.contract.ts | `check_session` | session.ts |
| `Settings` | session.contract.ts | `get_settings`, `configure_as_wilaya`, `is_configured` | All pages (19) |

---

### Product & Unit

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `Product` | inventory.contract.ts | `create_product`, `get_product`, `list_products` | ProductsPage, OrdersPage, ConsumptionPage, WilayaDashboard, FiscalManagementPage |
| `CreateProductRequest` | inventory.contract.ts | `—` (input type) | ProductsPage |
| `UpdateProductRequest` | inventory.contract.ts | `—` (input type) | ProductsPage |
| `Unit` | inventory.contract.ts | `create_unit`, `get_unit`, `list_units` | UnitsPage, SyncPage, WilayaDashboard, WilayaReportsPage, UnitInventoryPage |

---

### Stock & Inventory

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `InventoryStock` | inventory.contract.ts | `get_stock`, `get_all_stocks`, `get_current_stock` | UnitDashboard, StockPage |
| `StockCheckResult` | inventory.contract.ts | `check_stock_availability` | ConsumptionPage (via components) |
| `StockMovement` | inventory.contract.ts | `get_stock_movements` | StockPage, WilayaReportsPage |
| `StockMovementFilters` | inventory.contract.ts | `—` (input type) | StockPage |
| `StockMovementResponse` | inventory.contract.ts | `get_stock_movements` | StockPage |
| `StockSummary` | inventory.contract.ts | `get_stock_summary` | StockPage |
| `InventoryLayerView` | inventory.contract.ts | `get_inventory_fifo_view` | StockPage |
| `InventoryProductView` | inventory.contract.ts | `get_inventory_fifo_view` | StockPage |
| `InventoryStockPageView` | inventory.contract.ts | `get_inventory_fifo_view` | StockPage, inventory.contract.ts |
| `UnitMonthlySnapshot` | inventory.contract.ts | `compute_unit_inventory_snapshot`, `get_unit_inventory_view` | UnitInventoryPage |
| `UnitInventoryView` | inventory.contract.ts | `get_unit_inventory_view` | UnitInventoryPage |
| `ComputeSnapshotResult` | inventory.contract.ts | `compute_unit_inventory_snapshot` | UnitInventoryPage |

---

### Orders

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `SupplierOrder` | orders.contract.ts | `list_supplier_orders`, `get_supplier_order`, `create_supplier_order`, `update_supplier_order` | OrdersPage, UnitDashboard, inventory.contract.ts |
| `SupplierOrderItem` | orders.contract.ts | `get_supplier_order_items` | OrdersPage |
| `CreateOrderRequest` | orders.contract.ts | `—` (input type) | OrdersPage, orders.contract.ts |
| `UpdateOrderRequest` | orders.contract.ts | `—` (input type) | orders.contract.ts |
| `OrderItemInput` | orders.contract.ts | `—` (input type) | OrdersPage |

---

### Consumption & Daily Reports

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `MealType` | consumption.contract.ts | `—` (enum) | ConsumptionPage, MealSection, MealProductsTable, MealTabs, preview.ts |
| `DailyReport` | consumption.contract.ts | `list_daily_reports`, `get_daily_report`, `create_daily_report` | WilayaReportsPage, UnitReportsPage |
| `DailyReportMeal` | consumption.contract.ts | `get_daily_report` | WilayaReportsPage, UnitReportsPage |
| `DailyReportMealItem` | consumption.contract.ts | `get_daily_report` | WilayaReportsPage, UnitReportsPage |
| `MealSectionResult` | consumption.contract.ts | `get_daily_report`, `create_daily_report` | consumption.contract.ts |
| `DailyReportInput` | consumption.contract.ts | `—` (input type) | ConsumptionPage |
| `DailyReportResult` | consumption.contract.ts | `create_daily_report`, `get_daily_report`, `create_meal_consumption` | WilayaReportsPage, UnitReportsPage, DailyReportModal |
| `DailyConsumptionSummary` | consumption.contract.ts | `create_daily_report`, `preview_daily_consumption_fifo`, `get_daily_consumption` | ConsumptionPage, DailySummaryPanel |
| `DailyConsumptionView` | consumption.contract.ts | `get_daily_consumption` | ConsumptionPage |

---

### FIFO Preview

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `ConsumedLayerPortion` | consumption.contract.ts | `preview_daily_consumption_fifo` | (via ProductFifoPreview) |
| `ProductFifoPreview` | consumption.contract.ts | `preview_daily_consumption_fifo` | MealSection, MealProductsTable, ConsumptionPage |
| `MealFifoPreview` | consumption.contract.ts | `preview_daily_consumption_fifo` | (via DailyFifoConsumptionPreview) |
| `DailyFifoConsumptionPreview` | consumption.contract.ts | `preview_daily_consumption_fifo` | ConsumptionPage |

---

### Monthly Reports

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `MonthlySummary` | report.contract.ts | `get_monthly_summary` | WilayaReportsPage, UnitReportsPage |
| `WilayaReportList` | report.contract.ts | `list_wilaya_reports` | WilayaReportsPage |
| `ReportType` | report.contract.ts | `—` (type alias) | report.contract.ts |

---

### Export

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `SyncExportResult` | sync.contract.ts | various `export_*` | sync.contract.ts |
| `XlsxExportResult` | report.contract.ts / inventory.contract.ts / audit.contract.ts | various `export_*` | report.contract.ts, inventory.contract.ts, audit.contract.ts |

---

### Sync & Import

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `SyncImportResult` | sync.contract.ts | `import_products_package` | sync.contract.ts |
| `DailyReportImportResult` | sync.contract.ts | `import_daily_report_package`, `import_monthly_summary_package` | sync.contract.ts |
| `UnitNodePackageImportResult` | sync.contract.ts | `import_unit_node_package` | sync.contract.ts |
| `StockMovementsImportResult` | sync.contract.ts | `import_stock_movements_package` | sync.contract.ts |

---

### Backup

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `BackupInfo` | backup.contract.ts | `create_backup`, `list_backups` | BackupPage |

---

### Metrics & Monitoring

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `LoginMetrics` | metrics.contract.ts | `get_login_metrics` | WilayaStatisticsPage, UnitStatisticsPage |
| `SystemMetrics` | metrics.contract.ts | `get_system_metrics` | WilayaStatisticsPage, UnitStatisticsPage, WilayaDashboard |
| `SyncSecurityDiagnostics` | metrics.contract.ts | `get_sync_security_diagnostics` | metrics.contract.ts |
| `SyncPreflightCheck` | metrics.contract.ts | `sync_preflight_check` | WilayaStatisticsPage, WilayaDashboard |

---

### Audit

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `AuditEntry` | audit.contract.ts | `get_audit_log`, `get_user_activity` | AuditLogPage |
| `AuditFilters` | audit.contract.ts | `—` (input type) | AuditLogPage |
| `AuditLogResponse` | audit.contract.ts | `get_audit_log` | audit.contract.ts |
| `AuditStats` | audit.contract.ts | `get_audit_stats` | AuditLogPage |
| `UserActivitySummary` | audit.contract.ts | `get_audit_stats` | (via AuditStats) |
| `OperationCount` | audit.contract.ts | `get_audit_stats` | (via AuditStats) |
| `DailyOperationCount` | audit.contract.ts | `get_audit_stats` | (via AuditStats) |

---

### Observability & Health

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `AuditChainStatus` | observability.contract.ts | `get_audit_chain_status` | AuditIntegrityPage |
| `AuditAnomaly` | observability.contract.ts | `get_audit_health` | (via AuditHealthReport) |
| `AuditHealthReport` | observability.contract.ts | `get_audit_health` | AuditIntegrityPage |
| `ComponentHealth` | observability.contract.ts | `get_system_health` | (via SystemHealthReport) |
| `BackupHealth` | observability.contract.ts | `get_system_health` | (via SystemHealthReport) |
| `SyncHealth` | observability.contract.ts | `get_system_health` | (via SystemHealthReport) |
| `SystemHealthReport` | observability.contract.ts | `get_system_health` | SystemHealthPage, FiscalDiagnosticsPage |
| `SyncNodeHealth` | observability.contract.ts | `get_sync_health` | SyncTopologyPage, SystemHealthPage |
| `SyncConflict` | observability.contract.ts | `list_sync_conflicts` | ConflictCenterPage |
| `ConflictSummary` | observability.contract.ts | `get_conflict_summary` | SyncTopologyPage, ConflictCenterPage |
| `ConflictResolutionSuggestion` | observability.contract.ts | `list_sync_conflicts` | (via SyncConflict) |
| `SeverityCount` | observability.contract.ts | `get_conflict_summary` | (via ConflictSummary) |
| `TypeCount` | observability.contract.ts | `get_conflict_summary` | (via ConflictSummary) |
| `HealthStatus` | observability.contract.ts | `—` (enum) | SystemHealthPage |

---

### Fiscal

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `FiscalYearStatus` | fiscal.contract.ts | `get_fiscal_year_status` | FiscalManagementPage |
| `FiscalClosurePreview` | fiscal.contract.ts | `preview_fiscal_closure_package` | FiscalManagementPage |
| `FiscalClosureApplyResult` | fiscal.contract.ts | `apply_fiscal_closure_package` | FiscalManagementPage |
| `FiscalTransitionHistoryEntry` | fiscal.contract.ts | `get_fiscal_transition_history` | FiscalManagementPage |
| `FiscalPackageRegistryEntry` | fiscal.contract.ts | `list_fiscal_package_registry` | FiscalManagementPage |
| `FiscalClosurePackage` | fiscal.contract.ts | `export_fiscal_closure_package`, `preview_fiscal_closure_package` | (via preview) |
| `AuthorizedExecutionWindow` | fiscal.contract.ts | `—` (composite type) | (via FiscalClosurePackage) |

---

### Build & Telemetry

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `BuildInfo` | dashboard.contract.ts | `get_build_info` | SystemHealthPage |
| `TelemetryEvent` | dashboard.contract.ts | `get_recent_telemetry` | SystemHealthPage |
| `TelemetryEventType` | dashboard.contract.ts | `—` (enum) | (via TelemetryEvent) |
| `TelemetryOutcome` | dashboard.contract.ts | `—` (enum) | (via TelemetryEvent) |

---

### Notifications (UI-only, not IPC)

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `Notification` | — (UI layer) | `—` | Notifications.svelte, notifications.ts |

---

### Security & App Key

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `AppKeyStatusDto` | security.contract.ts | `get_security_status` | AppSecurityPage, LoginPage |
| `AppKeyInitializeResultDto` | security.contract.ts | `initialize_app_key` | AppSecurityPage |
| `AppKeyUnlockResultDto` | security.contract.ts | `unlock_app_key` | AppSecurityPage |

---

## Ownership Summary

| Domain | Projections | IPC Commands | Consuming Pages |
|--------|-------------|--------------|-----------------|
| inventory | 14 | 25 | 5 |
| consumption | 9 | 8 | 3 |
| observability | 10 | 7 | 4 |
| fiscal | 7 | 11 | 2 |
| audit | 6 | 5 | 1 |
| sync | 4 | 10 | 1 |
| orders | 5 | 7 | 1 |
| report | 3 | 8 | 2 |
| session | 2 | 6 | 2 |
| metrics | 3 | 4 | 3 |
| user | 3 | 2 | 1 |
| backup | 1 | 4 | 1 |
| dashboard | 2 | 2 | 1 |
| security | 3 | 4 | 2 |
| platform | 0 | 0 | 0 |
| **Total** | **~72** | **~102** | **25 pages** |

---

## Projection Leakage Detection

Projection leakage occurs when a page consumes a projection type from a domain
it does not own, without explicit cross-domain exception registration.

FE-152 enforces this by validating each page's contract imports against the
domain ownership registry. Any unregistered cross-domain import produces an ERROR.
