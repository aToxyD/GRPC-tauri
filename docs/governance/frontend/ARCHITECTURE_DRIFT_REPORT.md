# Architecture Drift Report

**Generated:** 2026-06-01

**Status:** Baseline — No Drift Detected

**Source:** FE-155 in `check_arch.ts`

---

## Baseline State

| Check | Result |
|-------|--------|
| Orphan Pages | **0** — All pages registered in App.svelte routes |
| Orphan Contracts | **0** — All 14 contracts consumed by at least one page |
| Orphan Projections | **0** — All projection types consumed by at least one page/component |
| Orphan Components | Not checked (requires full tree-shaking analysis) |

---

## Page Inventory (24 pages, all routed)

| Page | Route | Status |
|------|-------|--------|
| LoginPage | `/`, `/login` | Routed ✅ |
| WilayaNodeSetupPage | `/configure` | Routed ✅ |
| WilayaDashboard | `/wilaya`, `/wilaya/dashboard` | Routed ✅ |
| ProductsPage | `/wilaya/products` | Routed ✅ |
| UnitsPage | `/wilaya/units` | Routed ✅ |
| SyncPage | `/wilaya/sync` | Routed ✅ |
| WilayaReportsPage | `/wilaya/reports` | Routed ✅ |
| WilayaStatisticsPage | `/wilaya/statistics` | Routed ✅ |
| UnitInventoryPage | `/wilaya/unit-inventory` | Routed ✅ |
| UnitDashboard | `/unit`, `/unit/dashboard` | Routed ✅ |
| StockPage | `/unit/stock` | Routed ✅ |
| OrdersPage | `/unit/orders` | Routed ✅ |
| ConsumptionPage | `/unit/consumption` | Routed ✅ |
| UnitReportsPage | `/unit/reports` | Routed ✅ |
| UnitStatisticsPage | `/unit/statistics` | Routed ✅ |
| BackupPage | `/backup` | Routed ✅ |
| AuditLogPage | `/audit-log` | Routed ✅ |
| AuditIntegrityPage | `/admin/audit-integrity` | Routed ✅ |
| SystemHealthPage | `/admin/system-health` | Routed ✅ |
| SyncTopologyPage | `/admin/sync-topology` | Routed ✅ |
| ConflictCenterPage | `/admin/conflicts` | Routed ✅ |
| FiscalManagementPage | `/admin/fiscal` | Routed ✅ |
| FiscalDiagnosticsPage | `/admin/diagnostics` | Routed ✅ |
| NotFoundPage | `*` (fallback) | Routed ✅ |

---

## Contract Inventory (14 contracts, all consumed)

| Contract | Consumers | Status |
|----------|-----------|--------|
| consumption.contract.ts | ConsumptionPage, WilayaReportsPage, UnitReportsPage | Consumed ✅ |
| inventory.contract.ts | ProductsPage, StockPage, UnitDashboard, UnitsPage, UnitInventoryPage | Consumed ✅ |
| orders.contract.ts | OrdersPage, UnitDashboard | Consumed ✅ |
| report.contract.ts | UnitReportsPage, WilayaReportsPage | Consumed ✅ |
| fiscal.contract.ts | FiscalManagementPage, FiscalDiagnosticsPage | Consumed ✅ |
| sync.contract.ts | SyncPage, ProductsPage, StockPage, UnitReportsPage, LoginPage, UnitsPage | Consumed ✅ |
| backup.contract.ts | BackupPage | Consumed ✅ |
| dashboard.contract.ts | SystemHealthPage | Consumed ✅ |
| observability.contract.ts | AuditIntegrityPage, SystemHealthPage, SyncTopologyPage, ConflictCenterPage | Consumed ✅ |
| audit.contract.ts | AuditLogPage | Consumed ✅ |
| metrics.contract.ts | WilayaStatisticsPage, UnitStatisticsPage, WilayaDashboard | Consumed ✅ |
| session.contract.ts | All pages (19 use getSettings) | Consumed ✅ |
| user.contract.ts | LoginPage | Consumed ✅ |
| platform.contract.ts | No pages (empty stub — no IPC wrappers) | Stub ✅ |

---

## Drift Thresholds

| Metric | Threshold | Current | Status |
|--------|-----------|---------|--------|
| Orphan pages | 0 | 0 | ✅ |
| Orphan contracts | 0 | 0 | ✅ |
| Cross-contract command duplication | 0 | 0 | ✅ |
| Unused projection fields (computed/derived/helper) | 0 | 0 | ✅ |
| Suppression creep | 0 new since baseline | 0 | ✅ |

---

## Suppression Baseline (FE-149 validated)

See [Projection Purity Baseline](./PROJECTION_PURITY_BASELINE.md) for active suppressions.

---

## Policy

1. Drift report is regenerated on each architecture check (FE-155).
2. Any new orphan produces a WARNING.
3. Any orphan must be resolved within one sprint.
4. Unresolved orphans after two sprints escalate to ERROR.
