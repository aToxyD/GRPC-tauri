# Frontend IPC Governance

**Status:** Target Architecture

**Version:** v1.2.0

---

## IPC Governance Principles

1. **Centralized Transport** — All frontend/backend communication flows through a governed typed transport layer.
2. **Typed Contracts** — Every IPC command has a typed request and typed response. No raw `invoke()` in pages or components.
3. **Domain Ownership** — IPC commands are organized by domain into per-domain contract files.
4. **Contract Isolation** — Contract files are pure TypeScript with no Svelte dependencies.
5. **Validation Boundary** — Contract files may perform format-level validation but must not duplicate backend business validation.
6. **Drift Prevention** — Contract files must be verifiable against backend IPC command signatures.

---

## Centralized Transport Architecture

```
┌─────────────────────────────────────────────────┐
│                 Pages/Components                  │
│  (import from contract barrel or types barrel)   │
└──────────────────────┬──────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────┐
│           src/lib/contracts/index.ts             │
│              (barrel re-export)                  │
└──────┬──────────┬──────────┬──────────┬─────────┘
       │          │          │          │
       ▼          ▼          ▼          ▼
┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐
│consumpt.│ │inventory│ │session  │ │ ...     │
│.contract│ │.contract│ │.contract│ │         │
└────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘
     │           │           │           │
     └───────────┼───────────┼───────────┘
                 │           │
                 ▼           ▼
        ┌────────────────────────┐
        │    src/lib/tauri.ts    │
        │  (safeInvoke +         │
        │   platform wrappers)   │
        └────────────┬───────────┘
                     │
                     ▼
        ┌────────────────────────┐
        │  @tauri-apps/api/core  │
        │     (invoke)           │
        └────────────┬───────────┘
                     │
                     ▼
        ┌────────────────────────┐
        │    Backend IPC         │
        │    Commands            │
        └────────────────────────┘
```

---

## Contract Organization Model

### Directory Structure

```
src/lib/
├── tauri.ts                    # Platform API wrappers + safeInvoke (~50 lines)
├── types.ts                    # Barrel re-export for all contract DTOs
├── contracts/
│   ├── index.ts                # Barrel re-export of all contracts
│   ├── consumption.contract.ts
│   ├── inventory.contract.ts
│   ├── distribution.contract.ts
│   ├── report.contract.ts
│   ├── session.contract.ts
│   ├── dashboard.contract.ts
│   ├── program.contract.ts
│   ├── beneficiary.contract.ts
│   ├── user.contract.ts
│   ├── audit.contract.ts
│   ├── observability.contract.ts
│   ├── fiscal.contract.ts
│   ├── sync.contract.ts
│   └── backup.contract.ts
```

### File Naming Convention

- Format: `{domain}.contract.ts`
- Suffix `.contract.ts` enables glob-based rule enforcement
- Shared cross-domain types remain in `src/lib/types.ts` barrel

---

## Domain Contract Structure

Each contract file must export:

### 1. Domain-Specific DTOs

Types used by the domain's IPC commands, defined inline in the contract file:

```typescript
// src/lib/contracts/consumption.contract.ts
export interface DailyReportInput { ... }
export interface DailyReportResult { ... }
export interface DailyFifoConsumptionPreview { ... }
```

### 2. Command Wrappers

Typed async functions wrapping `safeInvoke`:

```typescript
export async function createDailyReport(
    input: DailyReportInput,
    unitId?: string
): Promise<DailyReportResult> {
    return await safeInvoke('create_daily_report', { input, unitId });
}

export async function getDailyConsumption(
    date: string
): Promise<DailyConsumptionView | null> {
    return await safeInvoke('get_daily_consumption', { date });
}
```

### 3. Event Listeners

Domain-specific Tauri event listeners:

```typescript
export async function onConsumptionUpdated(
    callback: (data: unknown) => void
): Promise<() => void> {
    // Use Tauri listen() for domain-specific events
}
```

### 4. Validation Helpers (if needed)

Format-only validation, never business rules:

```typescript
export function isValidMealDate(date: string): boolean {
    return /^\d{4}-\d{2}-\d{2}$/.test(date);
}
```

---

## DTO Governance

### Current State

- `src/lib/types.ts`: ~860 lines, 100+ interfaces/types in a single file
- All types are co-located regardless of domain ownership

### Target State

- Types are distributed to their owning contract files
- `src/lib/types.ts` becomes a barrel re-export only:

```typescript
// src/lib/types.ts (after split)
export type {
    MealType, DailyReportInput, DailyReportResult,
    DailyConsumptionView, DailyFifoConsumptionPreview,
    // ... consumption types
} from './contracts/consumption.contract';

export type {
    InventoryStock, StockCheckResult,
    // ... inventory types
} from './contracts/inventory.contract';

export type {
    User, LoginRequest, LoginResponse
} from './contracts/user.contract';
// ... etc
```

### Remaining Shared Types

These cross-domain types remain in the `types.ts` barrel:

| Type | Used By |
|------|---------|
| `Settings` | session, dashboard, consumption |
| `NodeConfiguration` | session, dashboard |
| `ProgressInfo` | sync, backup |
| `Notification` | session, components |
| `BuildInfo` | system health, about |
| `TelemetryEvent` | system health, diagnostics |
| `XlsxExportResult` | audit, inventory, export |

---

## Runtime Validation Strategy

### Current State

- No runtime validation of IPC responses
- TypeScript type assertions are the only safeguard
- Contract drift is undetectable at runtime

### Target State

- Runtime schema validation using a lightweight library (Zod/Valibot)
- Responses validated against schemas at the contract boundary
- Validation failures logged via telemetry and surfaced as user-friendly errors
- Contract verification tests that validate schemas against backend command signatures

---

## Contract Verification Requirements

1. **Type Safety** — Every exported function must have explicit return type (no inferred `Promise<any>`).
2. **No Cross-Contract Imports** — Contract files must not import from other `.contract.ts` files.
3. **No Svelte Imports** — Contract files must not import from `svelte`.
4. **Platform API Isolation** — Platform wrappers (dialog, window, dpi) remain only in `tauri.ts`.
5. **Function/Command Naming** — Contract function names should match their IPC command string (snake_case convention).
6. **No Deprecated Re-exports** — Functions marked `@deprecated` must not be re-exported from the barrel.

---

## Domain List and Type Allocation

| Domain | Contract File | Types | Commands |
|--------|--------------|-------|----------|
| Session | `session.contract.ts` | `SessionStatus` | `login`, `logout`, `checkSession`, `getCurrentUser`, `touchSession`, `changePassword` |
| User | `user.contract.ts` | `User`, `LoginRequest`, `LoginResponse` | Shared with session |
| Consumption | `consumption.contract.ts` | `MealType`, `DailyReportInput`, `DailyReportResult`, `DailyReportMeal`, `DailyReportMealItem`, `MealSectionInput`, `MealSectionResult`, `DailyConsumptionSummary`, `DailyConsumptionView`, `DailyFifoConsumptionPreview`, `MealFifoPreview`, `ProductFifoPreview`, `ConsumptionItemInput`, `ConsumedLayerPortion` | `createDailyReport`, `previewDailyConsumptionFifo`, `getDailyConsumption`, `listDailyReports`, `getDailyReport`, `listFiscalYears` |
| Inventory | `inventory.contract.ts` | `InventoryStock`, `StockCheckResult`, `InventoryStockPageView`, `InventoryProductView`, `InventoryLayerView`, `UnitMonthlySnapshot`, `UnitInventoryView`, `ComputeSnapshotResult` | `getStock`, `getAllStocks`, `checkStockAvailability`, `getInventoryFifoView`, `computeUnitInventorySnapshot`, `getUnitInventoryView`, `getAvailableReportMonths`, `getCurrentStock` |
| Distribution | `distribution.contract.ts` | `SupplierOrder`, `SupplierOrderItem`, `CreateOrderRequest`, `UpdateOrderRequest`, `OrderItemInput`, `StockMovement`, `StockMovementFilters`, `StockMovementResponse`, `StockSummary`, `StockMovementType` | `createSupplierOrder`, `confirmOrder`, `updateSupplierOrder`, `deleteSupplierOrder`, `getSupplierOrder`, `getSupplierOrderItems`, `listSupplierOrders`, `createOrder`, `getOrders`, `getStockMovements`, `getStockSummary` |
| Report | `report.contract.ts` | `DailyReport`, `MonthlySummary`, `DailyReportResult`, `WilayaReportList`, `ReportType` | `listWilayaReports`, `getMonthlySummary`, `generateReports`, `getReportData`, `calculateMealCost`, `calculateMealRate`, `calculateProductPriceWithTva` |
| Dashboard | `dashboard.contract.ts` | Dashboard-specific projection DTOs | `get_dashboard_stats` |
| Audit | `audit.contract.ts` | `AuditEntry`, `AuditFilters`, `AuditLogResponse`, `AuditStats`, `UserActivitySummary`, `OperationCount`, `DailyOperationCount` | `getAuditLog`, `getAuditStats`, `getUserActivity`, `exportAuditLogExcel`, `cleanupAuditLogs` |
| Observability | `observability.contract.ts` | `AuditChainStatus`, `AuditHealthReport`, `AuditAnomaly`, `SystemHealthReport`, `SyncNodeHealth`, `ComponentHealth`, `BackupHealth`, `SyncHealth`, `HealthStatus`, `ConflictSummary`, `SyncConflict`, `ConflictResolutionSuggestion` | `getAuditChainStatus`, `getAuditHealth`, `getSystemHealth`, `getSyncHealth`, `getConflictSummary`, `listSyncConflicts`, `resolveSyncConflict` |
| Fiscal | `fiscal.contract.ts` | `FiscalYearStatus`, `FiscalClosurePreview`, `FiscalClosureApplyResult`, `FiscalClosurePackage`, `FiscalTransitionHistoryEntry`, `FiscalPackageRegistryEntry`, `AuthorizedExecutionWindow`, `CloseFiscalYearRequest`, `CloseFiscalYearResponse` | `closeFiscalYear`, `getFiscalYearStatus`, `previewFiscalClosurePackage`, `applyFiscalClosurePackage`, `getFiscalTransitionHistory`, `listFiscalPackageRegistry`, `updateFiscalPackageRetentionStatus`, `exportFiscalClosurePackage`, `getAdvancedDiagnosticsBundle`, `verifyInventoryIntegrity`, `createFiscalOperationalSnapshot` |
| Sync | `sync.contract.ts` | `SyncExportResult`, `SyncImportResult`, `DailyReportImportResult`, `UnitNodePackageImportResult`, `StockMovementsImportResult`, `SyncSecurityDiagnostics`, `SyncPreflightCheck` | `exportProductsPackage`, `exportDailyReportPackage`, `exportMonthlySummaryPackage`, `exportUnitNodePackage`, `exportStockMovementsPackage`, `importProductsPackage`, `importDailyReportPackage`, `importUnitNodePackage`, `importMonthlySummaryPackage`, `importStockMovementsPackage`, `syncPreflightCheck`, `getSyncSecurityDiagnostics` |
| Backup | `backup.contract.ts` | `BackupInfo` | `createBackup`, `listBackups`, `restoreBackup`, `issueOperationExecutionToken` |
| Program | `program.contract.ts` | Minimal | (future) |
| Beneficiary | `beneficiary.contract.ts` | Minimal | (future) |

---

## IPC Anti-Patterns

| Anti-Pattern | Violation | Correct Approach |
|-------------|-----------|-----------------|
| Direct `invoke()` in pages | FE-103 | Use typed contract wrappers |
| Direct `@tauri-apps/` import in pages | FE-104 | Import from contract barrel |
| `safeInvoke()` outside contracts | FE-113 | Call from contract file only |
| Missing return types on contract functions | FE-114 | Explicit `Promise<Type>` annotation |
| Contract files importing Svelte | FE-120 | Contracts are pure TS |
| Cross-contract imports | FE-116 | Use shared types via barrel |
| Platform API in contract files | FE-118 | Keep in `tauri.ts` |
| Re-exporting deprecated commands | FE-119 | Remove or inline deprecated paths |

---

## Future Generated-Contract Direction

The preferred long-term approach is **generated contracts**:

1. **Schema Definition** — Backend IPC commands define their types in a schema language (Zod/Valibot).
2. **Code Generation** — Frontend contract files are generated from backend schemas during build.
3. **Runtime Validation** — Schemas are embedded to validate IPC responses at runtime.
4. **Contract Verification Tests** — Generated contracts include tests that verify schema alignment.

This direction is preferred but not required for v1.2.0. The immediate target is manually maintained typed contracts with runtime validation, with a migration path to generated contracts in a future release.
