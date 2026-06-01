# Frontend State Governance

**Status:** Target Architecture

**Version:** v1.2.0

---

## State Governance Philosophy

All frontend state must belong to exactly one category. State categories define ownership, lifecycle, and permitted transformations. This classification is mandatory for every state variable in the frontend.

**Core principles:**

1. **Explicit categorization** — Every `$state()` and `writable()` declaration must carry a `@category` marker.
2. **No hidden mutable state** — All mutable state must be declared and categorized.
3. **No domain state in frontend** — Backend-owned data (Projection State) may not be used for domain computation.
4. **Category purity** — State may not change category during its lifetime.
5. **Ownership enforcement** — Each category has specific permitted declaration locations.

---

## State Ownership Model

| Layer | Owns Categories |
|-------|----------------|
| `src/lib/contracts/` | Projection (returns DTOs) |
| `src/lib/session.ts` | Session State |
| `src/lib/components/ui/` | UI State (component internal) |
| `src/pages/*.svelte` | All four (marked accordingly) |
| `src/components/**/*.svelte` | UI State, Transient State |
| Feature components | UI State, Transient State (receive Projection via props) |

---

## Projection State

**Definition:** Data that originates from the backend. The frontend is a consumer only.

**Category Marker:** `// @category Projection`

**Rules:**
- Must be typed using backend-provided DTOs (imported from contract files or types barrel)
- Must never be mutated locally except by reassignment from a new backend response
- Must never be used as input to domain computations (averages, costs, FIFO allocations)
- Cannot be assigned to Transient state (no semantic copy)
- Can be filtered/sorted for display purposes only (visual organization, not business logic)

**Lifecycle:**
- Created: On page mount via IPC command call
- Updated: On user-triggered refresh or event listener callback
- Destroyed: On page unmount (data may be cached for re-mount)

**Persistence:** In-memory only. Never persisted to localStorage. Cache via backend only.

**Examples:**
```typescript
// @category Projection
let products = $state<Product[]>([]);

// @category Projection
let dailyView = $state<DailyConsumptionView | null>(null);

// @category Projection
let fifoPreview = $state<DailyFifoConsumptionPreview | null>(null);
```

**Anti-pattern:**
```typescript
// ❌ Projection used in domain computation
let mealAverage = totalBeneficiaries > 0 ? totalCost / totalBeneficiaries : 0;

// ❌ Projection reassembled into new summary object
function dailySummaryFromFormsAndFifo(...): DailyConsumptionSummary {
    // manually constructs summary from individual fields
}
```

---

## UI State

**Definition:** Presentation-only state that controls what the user sees.

**Category Marker:** `// @category UiState`

**Rules:**
- Must not hold backend DTO types
- Can include: modal visibility, active tab, selected item ID, expanded/collapsed state, pagination page number
- Can be derived from Projection state for display purposes only
- Can be persisted in sessionStorage for UX continuity
- Cannot participate in any arithmetic that produces a business value

**Lifecycle:**
- Created: On component initialization with default values
- Updated: On user interaction (click, input, navigation)
- Destroyed: On component unmount (unless persisted to sessionStorage)

**Persistence:** May persist to `sessionStorage` for UX continuity. Never to `localStorage`.

**Examples:**
```typescript
// @category UiState
let showModal = $state(false);

// @category UiState
let activeMeal = $state<MealType>('breakfast');

// @category UiState
let selectedUnitId = $state('');

// @category UiState
let searchProduct = $state('');

// @category UiState
let success = $state('');

// @category UiState
let activeMealLabel = $derived(
    MEAL_OPTIONS.find((m) => m.id === activeMeal)?.label ?? activeMeal
);
```

**Anti-pattern:**
```typescript
// ❌ UI State holding backend DTO
// @category UiState
let products: Product[] = $state([]);  // Product is a Projection type
```

---

## Session State

**Definition:** Authenticated user context and session lifecycle data.

**Category Marker:** `// @category SessionState`

**Rules:**
- Must only be defined in `src/lib/session.ts`
- Must use Svelte writable stores (`writable()`), not `$state()`
- Can be read by any component
- Must not be written by components (only by session.ts functions)
- Can be persisted to localStorage for session recovery
- Must be cleared on logout

**Lifecycle:**
- Created: On successful login / app bootstrap
- Updated: On session check responses (periodic or activity-triggered)
- Destroyed: On logout, session timeout, or app shutdown

**Persistence:** May persist to `localStorage` (JWT token, user preferences). Must clear on logout.

**Examples:**
```typescript
// src/lib/session.ts
// @category SessionState
export const currentUser = writable<User | null>(null);

// @category SessionState
export const isAuthenticated = writable<boolean>(false);
```

**Anti-pattern:**
```typescript
// ❌ SessionState in a page file
// @category SessionState  ❌ only allowed in session.ts
let currentUser = $state<User | null>(null);

// ❌ Mixed categories in a single store
export const sessionState = writable({
    isActive: false,     // Session State ✅
    lastCheck: null,     // UiState ❌
    warningShown: false, // UiState ❌
    checkInterval: null  // Transient ❌
});
```

---

## Transient State

**Definition:** Temporary form input, interaction buffers, and operation-local data.

**Category Marker:** `// @category TransientState`

**Rules:**
- Must be reset to defaults after submit/complete
- Can be derived from Projection state only for editing purposes (edit buffers)
- Must never be used to compute domain values directly
- Must be stored as strings for raw form inputs (user has not yet committed)
- Must be validated before being sent to backend (format/UX validation only)

**Lifecycle:**
- Created: When form or interaction starts (page mount, modal open)
- Updated: On every user keystroke or interaction
- Destroyed: On form submit, cancel, or component unmount

**Persistence:** Never persisted. Reset on unmount.

**Examples:**
```typescript
// @category TransientState
let supplierName = $state('');

// @category TransientState
let referenceNumber = $state('');

// @category TransientState
let date = $state(new Date().toISOString().split('T')[0]);

// @category TransientState
let mealForms = $state<Record<MealType, MealFormState>>(emptyMealForms());
```

**Anti-pattern:**
```typescript
// ❌ Transient used in domain computation
// ❌ parseBeneficiaryCounts + mealItemsFromForm → domain calculation

// ❌ Not reset on submit
// @category TransientState
let formData = $state({ name: '' });
function handleSubmit() {
    // ... submit logic ...
    // ❌ formData is never reset to initial state
}
```

---

## State Transition Rules

| From \ To | Projection | UI State | Session State | Transient |
|-----------|-----------|----------|---------------|-----------|
| **Projection** | ✅ Same response replaced | ✅ Display derive | ❌ Forbidden | ⚠️ Edit buffer only |
| **UI State** | ❌ Forbidden | ✅ Allowed | ❌ Forbidden | ❌ Forbidden |
| **Session State** | ❌ Forbidden | ✅ Read for display | ✅ Allowed | ❌ Forbidden |
| **Transient** | ❌ Forbidden | ❌ Forbidden | ❌ Forbidden | ✅ Allowed |

### Key Restrictions

- **Projection → Transient:** Only allowed for edit buffers where the user may modify data; must be reset on cancel.
- **Transient → Backend:** Allowed (form submission sends Transient data as IPC arguments).
- **Session → UI:** Read-only access for display (e.g., showing username).
- **Projection → UI:** `$derived` for rendering transformations only (e.g., formatting a date).

---

## State Anti-Patterns

### Critical Violations

| Anti-Pattern | Example | Risk |
|-------------|---------|------|
| Domain computation | `mealAverage = totalCost / totalBeneficiaries` | Semantic duplication, backend authority violation |
| Hidden mutable state | Module-level `let activityTimeout: number \| null` | Lifecycle leak, race condition |
| Mixed state categories | `sessionState` store with Session + UI + Transient | Unnecessary re-renders, obscured ownership |
| Projection → Transient assignment | `editBuffer = products[0]` | Semantic reinterpretation risk |

### High Violations

| Anti-Pattern | Example | Risk |
|-------------|---------|------|
| UI State holding backend types | `// @category UiState let products: Product[]` | Type system false security |
| SessionState outside session.ts | `// @category SessionState in any page` | Category ownership violation |
| No category marker | `let x = $state(...)` without comment | Governance invisibility |

### Medium Violations

| Anti-Pattern | Example | Risk |
|-------------|---------|------|
| `any` type on state | `let x: any = $state(...)` | Type safety violation |
| No reset on submit | Transient state not cleared after form submit | Stale form data |

---

## Findings from State Audit

A comprehensive audit of the codebase inventoried approximately 260 state variables across 24 pages, 6 components, 15 UI lib components, and 18 lib TS files.

### Ownership Violations (V1–V24)

24 instances of frontend code computing or holding backend-domain values.

| Severity | Count | Examples |
|----------|-------|----------|
| Critical | 3 | `preview.ts` computing `mealAverage`, `costPerBeneficiary`, `dailyCostSummary` |
| High | 8 | Local filtering in `UnitDashboard.svelte`, reactive derivations in `ConsumptionPage.svelte` |
| Medium | 13 | Mixed category stores, unmarked state declarations |

### Domain-State Leaks (D1–D9)

9 instances where domain state is held in the frontend.

| ID | File | Finding |
|----|------|---------|
| D1 | `session.ts:55` | `sessionState` mixes Session, UI, and Transient categories |
| D2 | `preview.ts:143` | `bAvg` computed from projection values |
| D3 | `preview.ts:147` | `lAvg` computed from projection values |
| D4 | `preview.ts:151` | `dAvg` computed from projection values |
| D5 | `preview.ts:156-158` | `dailyCostSummary` assembled from individual cost fields |
| D6 | `preview.ts:87` | `lineTotal` multiplies quantity by FIFO unit cost |
| D7 | `UnitDashboard.svelte:36` | `confirmedOrders` filter — business rule in frontend |
| D8 | `UnitDashboard.svelte:37` | `lowStockItems` filter — threshold in frontend |
| D9 | `ConsumptionPage.svelte` | `dailySummary` assembled from form fragments |

### Race-Condition Risks (R1–R21)

21 instances of unmanaged async operations.

| Risk Area | Count | Primary Files |
|-----------|-------|--------------|
| Raw timers/listeners | 4 | `session.ts` (setInterval, setTimeout, addEventListener) |
| Stale projection responses | 9 | `ConsumptionPage.svelte`, preview flows |
| Uncontrolled polling | 5 | Session check, auto-refresh patterns |
| Double-subscribe patterns | 3 | `session.ts` getSessionInfo(), store reads |

---

## Governance Requirements

1. **Every `$state()` declaration** must have a `// @category` marker (FE-100).
2. **Every `writable()`/`readable()` declaration** must have a `// @category` marker (FE-105).
3. **Projection State** must use types from contracts/backend DTOs, never local types (FE-101).
4. **UI State** must not hold backend DTO types (FE-106).
5. **Session State** must only be defined in `src/lib/session.ts` using `writable()` (FE-108).
6. **Transient State** must be reset on submit (FE-107).
7. **No `any` type** on state declarations (FE-110).
8. **Projection → Transient assignment** triggers a warning (FE-109).
9. **State category markers** must be preserved during refactoring.
10. **New state variables** must be reviewed for correct category classification.

---

## Verification Approach

1. **Static Analysis** (`scripts/check_arch.ts`): Rules FE-100 through FE-110 verify category markers and ownership boundaries.
2. **Manual Review**: Architect reviews that category classification is semantically correct (a human judgment).
3. **CI Gate**: Zero tolerance on FE-100–FE-110 violations.
4. **ADR Exception**: Any state that does not fit a category requires ADR approval.
