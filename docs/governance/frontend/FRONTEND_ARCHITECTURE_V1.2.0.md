# Frontend Architecture — v1.2.0

**Status:** Target Architecture

**Version:** v1.2.0

**Applies To:** All frontend code under `src/`

---

## Architecture Goals

1. **Projection-Driven Rendering** — All business-semantic values are computed by the backend and consumed as projections by the frontend.
2. **Backend Authority** — Business truth exists exclusively in the backend. The frontend is a governed operational projection layer.
3. **Enforceable Governance** — Architecture rules are machine-verifiable via static analysis. Manual review alone is insufficient.
4. **Race-Safe Async** — All asynchronous operations are lifecycle-bound, cancellable, and protected against stale responses.
5. **Zero Semantic Duplication** — No FIFO, accounting, KPI, or fiscal logic exists in the frontend.
6. **Deterministic UX** — Identical inputs and identical backend state produce identical rendered output.

---

## Frontend Identity

The frontend is a **governed operational projection layer**.

### Allowed Responsibilities

| Category | Examples |
|----------|----------|
| Rendering | Components, layouts, visual rendering |
| Navigation | Route management, page transitions |
| User workflows | Form collection, multi-step wizards |
| Projection visualization | Tables, charts, dashboards sourced from backend |
| Audit visualization | Audit logs, chain integrity display |
| Operational UX | Loading states, error states, progress indicators |
| Session experience | Login/logout, activity tracking |
| Presentation formatting | Number formatting, date formatting, RTL layout |

### Forbidden Responsibilities

| Category | Examples |
|----------|----------|
| Accounting calculations | Cost allocation, FIFO computations |
| KPI calculations | Consumption per beneficiary, stock coverage days |
| Inventory valuation | Layer costing, unit cost derivation |
| Fiscal calculations | Fiscal year closure metrics, budget computations |
| Recommendation generation | Operational suggestions, risk scoring |
| Forecasting logic | Consumption prediction, stock projections |
| Authorization decisions | Access control enforcement, permission evaluation |
| Business rule ownership | Validation rules, business state machines |

---

## Feature Boundaries

The frontend is organized by domain features under `src/features/{domain}/`. Each feature owns:

- **Pages** — Route-level components that compose the feature
- **Components** — Feature-specific UI components
- **Contracts** — IPC contract consumption (via `src/lib/contracts/`)
- **State** — Feature-local UI and Transient state (never Projection state ownership)

### Domain List

| Domain | Contract File | Primary Pages |
|--------|--------------|---------------|
| Consumption | `consumption.contract.ts` | ConsumptionPage |
| Inventory | `inventory.contract.ts` | StockPage, UnitInventoryPage |
| Distribution | `distribution.contract.ts` | OrdersPage |
| Report | `report.contract.ts` | WilayaReportsPage, UnitReportsPage |
| Session | `session.contract.ts` | (app-wide, via App.svelte) |
| Dashboard | `dashboard.contract.ts` | WilayaDashboard, UnitDashboard |
| Program | `program.contract.ts` | (future) |
| Beneficiary | `beneficiary.contract.ts` | (future) |
| User | `user.contract.ts` | LoginPage |
| Audit | `audit.contract.ts` | AuditLogPage, AuditIntegrityPage |
| Observability | `observability.contract.ts` | SystemHealthPage, SyncTopologyPage, ConflictCenterPage |
| Fiscal | `fiscal.contract.ts` | FiscalManagementPage, FiscalDiagnosticsPage |
| Sync | `sync.contract.ts` | SyncPage |
| Backup | `backup.contract.ts` | BackupPage |

---

## Layer Boundaries

The frontend consists of exactly three layers:

### Presentation Layer

**Location:** `src/components/`, `src/features/*/components/`, `src/lib/components/ui/`

**Responsibilities:**
- Component rendering
- Layout composition
- Visual formatting
- Display transformations (e.g., number locale formatting)
- Transient UI state management (modal visibility, active tab)

**Constraints:**
- Must not import from `@tauri-apps/*` directly
- Must not call `invoke()` or `safeInvoke()` directly
- Must receive all business data via props or slot projection from Application Layer

### Application Layer

**Location:** `src/lib/contracts/`, `src/lib/tauri.ts`

**Responsibilities:**
- Typed IPC command dispatch
- Event listener registration (Tauri events)
- Contract validation
- Query orchestration
- Command orchestration

**Constraints:**
- Contract files are pure TypeScript — no Svelte imports
- All IPC calls go through `safeInvoke()` in `tauri.ts`
- No domain computation or business logic

### State Layer

**Location:** Throughout the frontend, governed by state categories

**Responsibilities:**
- Projection State — consumed from backend, never mutated locally
- UI State — presentation-only visibility and interaction state
- Session State — authenticated user context (only in `src/lib/session.ts`)
- Transient State — form inputs and interaction buffers

**Constraints:**
- No Domain Layer — domain ownership belongs exclusively to the backend
- All state must belong to exactly one category
- State transitions follow governed rules (see FRONTEND_STATE_GOVERNANCE.md)

---

## Folder Structure

```
src/
├── lib/
│   ├── tauri.ts                    # Platform API wrappers + safeInvoke
│   ├── types.ts                    # Barrel re-export for contract DTOs
│   ├── contracts/
│   │   ├── index.ts                # Barrel re-export
│   │   ├── consumption.contract.ts
│   │   ├── inventory.contract.ts
│   │   ├── distribution.contract.ts
│   │   ├── report.contract.ts
│   │   ├── session.contract.ts
│   │   ├── dashboard.contract.ts
│   │   ├── program.contract.ts
│   │   ├── beneficiary.contract.ts
│   │   ├── user.contract.ts
│   │   ├── audit.contract.ts
│   │   ├── observability.contract.ts
│   │   ├── fiscal.contract.ts
│   │   ├── sync.contract.ts
│   │   └── backup.contract.ts
│   ├── session.ts                  # Session state management
│   ├── runtimeCleanup.ts           # RuntimeScope lifecycle management
│   ├── operationGuard.ts           # Operation guard pattern
│   ├── errors.ts                   # Error formatting
│   ├── permissions.ts              # Advisory frontend permissions
│   ├── notifications.ts           # Notification system
│   ├── telemetry.ts               # Telemetry system
│   └── components/ui/             # Shared UI components
├── features/
│   ├── consumption/               # Feature-based organization (target)
│   ├── inventory/
│   ├── distribution/
│   ├── ...
├── pages/                         # Current flat pages (migration target)
├── components/                    # Feature components (migration target)
└── App.svelte                     # Root component
```

---

## Dependency Rules

### Import Boundaries

| Source → Target | Allowed? | Notes |
|----------------|----------|-------|
| Page → Contract barrel | ✅ | Preferred import path |
| Page → Specific `.contract.ts` | ⚠️ Warning | Prefer barrel import |
| Component → Contract barrel | ❌ Error | Components receive data via props |
| Component → Page | ❌ Error | Reverse dependency |
| Contract → Svelte | ❌ Error | Contracts are pure TS |
| Contract → Another contract | ❌ Error | Use shared types via barrel |
| Contract → `tauri.ts` | ✅ | For `safeInvoke` only |
| Page → Another page | ❌ Error | Cross-domain imports forbidden |

### IPC Invocation Rules

- `invoke()` and `safeInvoke()` may only appear in `src/lib/contracts/*.contract.ts` and `src/lib/tauri.ts`
- Direct `@tauri-apps/` imports only permitted in contract files and `tauri.ts`
- Platform API imports (`@tauri-apps/plugin-dialog`, `@tauri-apps/api/window`, etc.) only in `tauri.ts`

---

## Frontend Authority Boundaries

| Authority | Owner | Frontend Role |
|-----------|-------|---------------|
| Accounting semantics | Backend | Render projection |
| FIFO semantics | Backend | Render projection |
| KPI computation | Backend | Render projection |
| Inventory valuation | Backend | Render projection |
| Fiscal metrics | Backend | Render projection |
| Authorization | Backend | Advisory UI hints only |
| Recommendation generation | Backend | Render projection |
| Forecasting | Backend | Render projection |
| Presentation formatting | Frontend | Full ownership |
| User interaction | Frontend | Full ownership |
| Form collection | Frontend | Ownership, validated by backend |
| Session UX | Frontend | Ownership, session data from backend |

---

## Projection-Driven Architecture Principles

1. **If the backend can compute it, the frontend must render it.**
2. **No semantic computation** — any value that represents a business concept must originate from the backend.
3. **No reinterpretation** — projections must be rendered as received. The frontend may format for display but must not re-derive meaning.
4. **No reassembly** — the frontend must not reconstruct summary objects from individual projection fields; the backend provides complete projections.
5. **Display filtering only** — array filtering in the frontend must be for visual organization only, not business logic.

---

## Backend Authority Integration

The backend exclusively owns:

```
┌──────────────────────────────────────────┐
│              BACKEND                     │
│                                          │
│  ┌─────────┐  ┌──────────┐  ┌────────┐  │
│  │ Domain  │  │ Service  │  │  IPC   │  │
│  │  Logic  │  │   Layer  │  │Commands│  │
│  └────┬────┘  └────┬─────┘  └───┬────┘  │
│       │            │            │        │
│  ┌────▼────────────▼────────────▼────┐   │
│  │       Projection Factory         │   │
│  │  (computes all business values)  │   │
│  └────────────────┬─────────────────┘   │
│                   │                     │
└───────────────────┼─────────────────────┘
                    │ IPC (typed contracts)
┌───────────────────┼─────────────────────┐
│  FRONTEND         │                     │
│                   ▼                     │
│  ┌─────────────────────────────────┐    │
│  │     Contract Layer              │    │
│  │  (typed wrappers, no logic)     │    │
│  └───────────────┬─────────────────┘    │
│                  │                      │
│  ┌───────────────▼─────────────────┐    │
│  │     State / Component Layer     │    │
│  │  (renders projections only)     │    │
│  └─────────────────────────────────┘    │
└──────────────────────────────────────────┘
```

---

## Architecture Diagrams

### Layer Architecture

```
┌────────────────────────────────────────────────────┐
│              PRESENTATION LAYER                     │
│  ┌──────────┐ ┌──────────┐ ┌──────────────────┐    │
│  │  Pages   │ │Components│ │  UI lib/components│   │
│  │(routing) │ │ (feature)│ │  (shared widgets) │    │
│  └────┬─────┘ └────┬─────┘ └────────┬─────────┘    │
│       │            │                │              │
├───────┼────────────┼────────────────┼──────────────┤
│       │            │                │              │
│  ┌────▼────────────▼────────────────▼──────────┐   │
│  │           APPLICATION LAYER                  │   │
│  │  ┌──────────────────────────────────────┐   │   │
│  │  │        src/lib/contracts/            │   │   │
│  │  │  (typed IPC wrappers per domain)     │   │   │
│  │  └──────────────────────────────────────┘   │   │
│  └───────────────────┬──────────────────────────┘   │
│                      │                              │
│  ┌───────────────────▼──────────────────────────┐   │
│  │              STATE LAYER                      │   │
│  │  ┌──────────┐ ┌────────┐ ┌────────┐ ┌──────┐ │   │
│  │  │Projection│ │  UI    │ │Session │ │Trans.│ │   │
│  │  │  State   │ │ State  │ │ State  │ │State │ │   │
│  │  └──────────┘ └────────┘ └────────┘ └──────┘ │   │
│  └───────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────┘
```

### Data Flow

```
User Action
    │
    ▼
┌──────────────┐
│  Page/Comp   │ (Presentation Layer)
│  calls       │
│  contract fn │
└──────┬───────┘
       │ typed request
       ▼
┌──────────────┐
│  Contract    │ (Application Layer)
│  safeInvoke  │
└──────┬───────┘
       │ IPC invoke
       ▼
┌──────────────┐
│  Backend     │
│  processes,  │
│  computes,   │
│  returns     │
│  projection  │
└──────┬───────┘
       │ typed response
       ▼
┌──────────────┐
│  Contract    │
│  returns     │
│  typed data  │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│  Page/Comp   │
│  renders     │
│  projection  │
└──────────────┘
```

---

## Governance Enforcement

Frontend architecture is enforced through:

1. **Static Analysis** — `scripts/check_arch.ts` with rules FE-100 through FE-151
2. **CI Pipeline** — `check:fe` job blocks merges on governance violations
3. **State Category Audit** — Every `$state()` declaration must carry a `@category` marker
4. **Contract Isolation** — IPC call patterns are checked for compliance
5. **Timer/Lifecycle Audit** — RuntimeScope usage is verified across all pages and lib code

See FRONTEND_CERTIFICATION_RULES.md for the complete rule specification.

---

## Relationship to v1.1.0

v1.1.0 achieved backend governance certification. v1.2.0 extends the same governance guarantees to the frontend layer:

| Aspect | v1.1.0 Backend | v1.2.0 Frontend |
|--------|----------------|-----------------|
| Determinism | Certified | Projection consumption only |
| Auditability | Certified | Audit visualization |
| Reproducibility | Certified | Reproducible rendering |
| Replay safety | Certified | Stale-response protection |
| Fiscal correctness | Certified | No fiscal logic |
| Architecture enforcement | 125+ rules | 52 FE rules (FE-100–FE-151) |
