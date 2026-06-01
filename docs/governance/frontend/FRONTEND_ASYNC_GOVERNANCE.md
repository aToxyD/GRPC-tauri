# Frontend Async Governance

**Status:** Target Architecture

**Version:** v1.2.0

---

## Async Governance Philosophy

All asynchronous operations in the frontend must be race-safe, lifecycle-bound, and cancellable. Unmanaged async operations create stale-state corruption, memory leaks, and unpredictable user experiences.

**Core principles:**

1. **Lifecycle-bound** — Every timer, listener, and async operation must be bound to a `RuntimeScope` that auto-cleans on disposal.
2. **Stale-response protected** — Every async response must be verified as still-relevant before updating state.
3. **Controlled concurrency** — No unbounded concurrent requests. Loading guards prevent overlapping operations.
4. **Cancellation support** — Long-running operations must support cancellation via `AbortSignal`.
5. **No zombie callbacks** — After component unmount or scope disposal, no callback may update state.

---

## RuntimeScope Requirements

### What is RuntimeScope?

`RuntimeScope` (defined in `src/lib/runtimeCleanup.ts`) is a lifecycle container that tracks timers, intervals, and event listeners. When the scope is disposed, all tracked resources are automatically cleaned up.

```typescript
interface RuntimeScope {
    readonly signal: AbortSignal;
    isAlive(): boolean;
    setTimeout(fn: () => void, ms: number): number;
    setInterval(fn: () => void, ms: number): number;
    addListener(
        target: Window | Document | HTMLElement,
        event: string,
        handler: Function,
        options?: AddEventListenerOptions
    ): void;
    dispose(): void;
}
```

### Requirements

1. **Every page** must create a `RuntimeScope` in `onMount` and dispose it in `onDestroy` (FE-121).
2. **Every component** that uses timers or listeners must use `RuntimeScope` (FE-123, FE-124).
3. **Library code** (`src/lib/*.ts`) that uses timers or listeners must accept and use a `RuntimeScope` parameter (FE-149).
4. **`createOperation()` and `createOperationGuard()`** must receive scope for lifecycle-safe async operations (FE-122).

---

## Timer Ownership Rules

| Timer Type | Rule | Enforcement |
|-----------|------|------------|
| `window.setTimeout` | Forbidden outside RuntimeScope | FE-123 |
| `window.setInterval` | Forbidden outside RuntimeScope | FE-123 |
| `scope.setTimeout` | Allowed — tracked in scope | — |
| `scope.setInterval` | Allowed — tracked in scope | — |
| Module-level timer IDs | Forbidden — must use scope | FE-150 |

### Rules

1. Raw `window.setTimeout` and `window.setInterval` are forbidden in all page, component, and lib files.
2. All timers must be created through `scope.setTimeout()` or `scope.setInterval()`.
3. Timer callbacks are automatically wrapped with `isAlive()` checks by RuntimeScope.
4. On scope disposal, all active timers and intervals are automatically cleared.
5. No module-level variables may store timer IDs — the scope owns timer lifecycle.

---

## Listener Ownership Rules

| Listener Type | Rule | Enforcement |
|--------------|------|------------|
| `document.addEventListener` | Forbidden outside RuntimeScope | FE-124 |
| `window.addEventListener` | Forbidden outside RuntimeScope | FE-124 |
| `scope.addListener` | Allowed — tracked in scope | — |
| Module-level handler refs | Forbidden — must use scope | FE-150 |

### Rules

1. Raw `addEventListener` is forbidden in all page, component, and lib files.
2. All event listeners must be registered through `scope.addListener()`.
3. Listener callbacks are automatically wrapped with `isAlive()` checks by RuntimeScope.
4. On scope disposal, all registered listeners are automatically removed.
5. Touch/scroll/wheel listeners should use `{ passive: true }` for performance (FE-151).

---

## Cancellation Requirements

1. **Every IPC contract function** should accept an optional `AbortSignal` parameter for long-running operations (FE-127).
2. **Pages must use `createOperation` or `createOperationGuard`** for async data fetching (FE-125).
3. **Stale response protection** — After every `await`, verify `scope.isAlive()` before updating state (FE-128).
4. **No overlapping projection requests** — In-flight loading guards prevent duplicate requests (FE-126).

---

## Stale-Response Protection

### Two-Layer Protection Model

```
┌──────────────────────────────────────────┐
│ Layer 1: RuntimeScope callback wrapper   │
│                                          │
│ scope.setInterval(() => {                │
│     if (!scope.isAlive()) return;  ←─────│── Prevents callback execution
│     fetchData().then(...)                │    if scope is dead
│ });                                      │
└──────────────────────────────────────────┘
                        │
                        ▼
┌──────────────────────────────────────────┐
│ Layer 2: Stale-response guard after await│
│                                          │
│ fetchData().then(result => {             │
│     if (!scope.isAlive()) return;  ←─────│── Prevents state update
│     state = result;                      │    if scope died during request
│ });                                      │
└──────────────────────────────────────────┘
```

### Implementation Pattern

```typescript
// @category Projection
let data = $state<DataType | null>(null);
const op = createOperation({ scope });

async function loadData() {
    await op.run(async () => {
        const result = await fetchData();
        if (!scope.isAlive()) return;  // Stale-response guard
        data = result;
    });
}
```

---

## Lifecycle-Safe Updates

### Pattern for Page Components

```svelte
<script lang="ts">
import { onMount, onDestroy } from 'svelte';
import { createRuntimeScope } from '$lib/runtimeCleanup';
import { createOperation } from '$lib/operationGuard';
import { getData } from '$lib/contracts';

const scope = createRuntimeScope();
const op = createOperation({ scope });

// @category Projection
let data = $state<DataType | null>(null);

onMount(() => {
    loadData();
});

onDestroy(() => {
    scope.dispose();
});

async function loadData() {
    await op.run(async () => {
        const result = await getData();
        if (!scope.isAlive()) return;
        data = result;
    });
}
</script>
```

### Pattern for Library Code (e.g., session.ts)

```typescript
let activeScope: RuntimeScope | null = null;

export function startSessionMonitoring(scope: RuntimeScope): void {
    stopSessionMonitoring();  // Dispose previous scope if any
    activeScope = scope;

    scope.setInterval(async () => {
        const status = await checkSession();
        if (!scope.isAlive()) return;  // Stale-response guard
        if (status) handleSessionStatus(status);
    }, CHECK_INTERVAL);
}

export function stopSessionMonitoring(): void {
    if (activeScope) {
        activeScope.dispose();
        activeScope = null;
    }
}
```

---

## Session Monitoring Design

The session monitoring system (`src/lib/session.ts`) is the highest-priority target for async governance migration.

### Current Violations

| Violation | Line | Risk |
|-----------|------|------|
| Raw `window.setInterval` | 129 | Zombie interval after unmount |
| Raw `window.setTimeout` | 187 | Race condition on activity debounce |
| Raw `document.addEventListener` | 195 | Listener accumulation on restart |
| Module-level `activityTimeout` | 62 | Re-entrancy hazard |
| Module-level `activityHandler` | 63 | Orphaned handler references |
| `sessionState` mixed categories | 55 | Session + UI + Transient in one store |

### Target Design

```
App.svelte (onMount)
    │
    ├── const scope = createRuntimeScope()
    ├── onDestroy(() => scope.dispose())
    │
    └── bootstrapSession(scope)
          │
          ├── getCurrentUser()
          ├── setCurrentUser(user)
          └── startSessionMonitoring(scope)
                │
                ├── scope.setInterval(…)     // Session check (60s)
                ├── scope.setTimeout(…)      // Initial check
                └── scope.addListener(…)     // Activity tracking
```

Key changes:
- `session.ts` accepts `RuntimeScope` as parameter
- `activeScope: RuntimeScope | null` replaces `activityTimeout`, `activityHandler`, `checkInterval`
- `scope.dispose()` atomically clears all timers, intervals, and listeners
- Two-layer stale-response protection (scope wrapper + response guard)
- `sessionState` store split into separate Session and UI state

---

## Race-Condition Findings Summary

The async audit identified 21 race-condition risks across the codebase:

| Risk Area | Count | Severity | Primary Locations |
|-----------|-------|----------|-------------------|
| Raw timers/listeners | 4 | Critical | `session.ts` (setInterval, setTimeout, addEventListener) |
| Stale projection responses | 9 | High | `ConsumptionPage.svelte`, preview flows, dashboard |
| Uncontrolled polling | 5 | Medium | Session check, auto-refresh patterns |
| Double-subscribe patterns | 3 | Low | `session.ts` getSessionInfo(), `get()` usage |

### Critical Risks

| ID | File | Lines | Risk | Mitigation |
|----|------|-------|------|------------|
| R1 | `session.ts` | 129 | Untracked setInterval — zombie timer after unmount | `scope.setInterval` |
| R2 | `session.ts` | 187 | Untracked setTimeout — fires after scope disposed | `scope.setTimeout` |
| R3 | `session.ts` | 195 | addEventListener — accumulated listeners on restart | `scope.addListener` |
| R4 | `session.ts` | 62-63 | Module-level mutable state — re-entrancy hazard | Eliminate variables, use scope |

### High Risks

| ID | File | Risk | Mitigation |
|----|------|------|------------|
| R5 | `ConsumptionPage.svelte` | Stale FIFO preview overwrites user edits | Operation guard + stale-response check |
| R6 | `preview.ts` | Computed values use stale projection data | Backend computed projections |
| R7 | `UnitDashboard.svelte` | Filtered arrays stale after data refresh | Backend pre-filtered endpoint |

---

## Async Anti-Pattern Catalog

### Critical Anti-Patterns

| Anti-Pattern | Example | Fix |
|-------------|---------|-----|
| Raw timer in lib code | `window.setInterval(...)` in session.ts | `scope.setInterval(...)` |
| Module-level mutable timer ID | `let activityTimeout: number \| null` | Store in scope, not module variable |
| Listener without scope | `document.addEventListener(...)` | `scope.addListener(document, ...)` |
| No stale-response check | `fetch().then(r => state = r)` | `if (!scope.isAlive()) return` |

### High Anti-Patterns

| Anti-Pattern | Example | Fix |
|-------------|---------|-----|
| Async without loading guard | `async function load() { data = await fetch(); }` | `await op.run(async () => { ... })` |
| No cancellation support | Long-running command without AbortSignal | Add AbortSignal parameter |
| Overlapping requests | Same fetch fired while in-flight | Check `op.isRunning()` |
| Promise without lifecycle | `onMount(() => fetch())` without cleanup | Guard with scope.isAlive() |

### Medium Anti-Patterns

| Anti-Pattern | Example | Fix |
|-------------|---------|-----|
| Double-subscribe pattern | `store.subscribe(callback)()` | `get(store)` |
| Uncontrolled polling | `setInterval(fetch, 5000)` without max/stop | `scope.setInterval` + cleanup |
| Async in `$:` reactive | `$: load(id)` — fires on every change | Explicit invocation with guard |
| Missing passive event option | `touchstart` listener without `{ passive: true }` | Add `{ passive: true }` |
