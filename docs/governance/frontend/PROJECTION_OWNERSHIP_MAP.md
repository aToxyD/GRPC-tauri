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
| `Unit` | inventory.contract.ts | `create_unit`, `get_unit`, `update_unit` | UnitsPage, SyncPage, WilayaDashboard, WilayaReportsPage, UnitInventoryPage |
| `UnitView` | inventory.contract.ts | `list_units` | UnitsPage, SyncPage, WilayaDashboard, WilayaReportsPage, UnitInventoryPage |
| `CreateUnitRequest` **(IMPLEMENTED — ADR-0063 §3)** | inventory.contract.ts | `create_unit`, `update_unit` (input type) | UnitsPage |

> **`CreateUnitRequest` — ratified change (ADR-0063 §3).** The ratified field set is
> `{ code, name }`. The `password` field is **removed**: WILAYA UNIT creation MUST NOT accept a
> caller-supplied password, and `update_unit` no longer rotates a password. The initial
> credential is the server-side bootstrap constant (`ADR-0063` §4), and UNIT `code` becomes
> immutable (`ADR-0063` §9), so `update_unit` retains only the `name` capability.
> **Implementation status: implemented (Slice 1).** The live creation contract no longer
> carries `password`: `CreateUnitRequest { code, name }` (`src/lib/types.ts:99-102`), and
> `src/pages/UnitsPage.svelte` no longer collects or forwards one. The generated FE-160 baseline
> `docs/governance/frontend/baselines/projection-ownership.snapshot.json` was recertified
> accordingly by the implementation phase, so the enforced baseline matches this approved contract.
> This is an approved Slice 1 change, not an unimplemented mutation.
>
> **Field-level contract.** `password` is removed from `CreateUnitRequest`; `code` and `name`
> remain. `update_unit` becomes name-only, because ADR-0063 §9 makes `code` immutable (the
> operator hash's node-binding key is the UNIT code). The input type therefore loses exactly one
> field and gains none.

---

### UNIT Lifecycle Projection (WILAYA Units)

**Status: ratified by ADR-0064 §1 / §3, not yet implemented.** Recorded here as the owning
contract map entry so the projection has exactly one owner before any code exists.

| Projection | Owner Contract | IPC Command | Consuming Pages |
|------------|---------------|-------------|-----------------|
| `UnitView` with `is_exported: bool` **(RATIFIED, NOT YET IMPLEMENTED — U-2 closed by `ADR-0064` §1.1 / D19.1–D19.10)** | inventory.contract.ts | `list_units` | UnitsPage |

> **`EXPORTED` lifecycle.** A WILAYA UNIT is EXPORTED when the WILAYA-side `identity_store`
> holds **any** row with `subject_type = 'UNIT'` and `subject_id = units.id`, regardless of
> status (`ADR-0064` §1). It is a **derived backend projection, never a stored flag**. While
> EXPORTED, `update_unit` and `export_unit_node_package` are **refused by the backend** with a
> fail-closed error; `delete_unit` remains available in every state.
>
> **Owner decision U-2 — closed (`ADR-0064` §1.1 / D19.1–D19.10).** The state is surfaced as the
> backend-derived DTO **`UnitView`** carrying the boolean **`is_exported: bool`**, true iff any
> historical `identity_store` row exists for the UNIT subject. There is **no mutable exported
> flag** and **no lifecycle enum**. The predicate is **status-agnostic**: `ACTIVE`, `REVOKED`,
> `SUPERSEDED`, and `EXPIRED` all satisfy EXPORTED semantics, and `identity_store.status` is
> neither exposed nor reused as a lifecycle state.
>
> **Field-level contract.** `is_exported` is the only field added to the `list_units` return
> type. Projection scope is exactly **`list_units` -> `Vec<UnitView>`**; `create_unit`,
> `get_unit`, and `update_unit` continue returning `Unit`. The frontend **consumes**
> `is_exported` and **must not recompute it** — the backend refusal remains the authoritative
> control, and hiding the two actions in `UnitsPage` is presentation only.
>
> **Implementation status: not implemented.** The live projection is still `Unit`
> (`src/lib/types.ts:82-89`), and `list_units` still returns `Unit`. The generated FE-160
> baseline `docs/governance/frontend/baselines/projection-ownership.snapshot.json` therefore
> intentionally still records the unimplemented shape and is **not** modified here; it must be
> regenerated by the implementation phase, which is the only change that may alter it.

---

### Local UNIT Credential Lifecycle

**Status: ratified by ADR-0063 §6 / §7, not yet implemented.** Recorded here so the two
credential commands have exactly one owning contract before implementation.

| Command (ratified name) | Owner Contract | Backend locus | Consuming Page |
|-------------------------|---------------|---------------|----------------|
| Self password change **(RATIFIED, NOT YET IMPLEMENTED)** | sync.contract.ts | `commands/` + `application/` (ADR-0063 §6) | SettingsPage |
| Local UNIT admin reset of `user` **(RATIFIED, NOT YET IMPLEMENTED)** | sync.contract.ts | `commands/` + `application/` (ADR-0063 §7.1) | SettingsPage |

> **Owner-contract rationale.** Both ratified commands extend the **existing**
> password-command surface, which `sync.contract.ts` already owns (`setFleetAdminPassword`,
> `setUnitUserPassword` — `src/lib/contracts/sync.contract.ts:65-70`), and both are consumed
> from `SettingsPage`, which already hosts the fleet-password control
> (`src/pages/SettingsPage.svelte:8,20,125`). Choosing a different contract file would split
> one credential surface across two owners and violate FE-152 / `AGENTS.md` §2 A2.
>
> The WILAYA-side `set_unit_user_password` keeps its single existing path and is **evolved, not
> duplicated** (`ADR-0063` §7.2). The names above are placeholders recorded for ownership
> traceability only; the final identifiers are an implementation-phase choice and are **not**
> ratified here.

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

> **Counts describe implemented state only.** Entries in this map explicitly marked
> *RATIFIED, NOT YET IMPLEMENTED* (the `UnitView.is_exported` projection and the two local UNIT
> credential commands) are **excluded** from the summary counts, because the projections and
> commands they describe do not exist in `src/lib/` or in `src-tauri/src/commands/` yet. They are
> listed so that each has exactly one registered owner before implementation (`AGENTS.md` §2 A3,
> FE-152). The `CreateUnitRequest` field change is **no longer** among them: it is implemented
> (Slice 1) and covered by the recertified baseline below. The authoritative enforced counts live in
> the generated FE-160 baseline `docs/governance/frontend/baselines/projection-ownership.snapshot.json`,
> which is **not** hand-maintained; regenerating it belongs to the implementation phase.

---

## Projection Leakage Detection

Projection leakage occurs when a page consumes a projection type from a domain
it does not own, without explicit cross-domain exception registration.

FE-152 enforces this by validating each page's contract imports against the
domain ownership registry. Any unregistered cross-domain import produces an ERROR.
