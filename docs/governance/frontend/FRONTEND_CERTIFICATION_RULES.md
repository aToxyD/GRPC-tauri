# Frontend Certification Rules — v1.2.0

**Status:** Target Architecture

**Version:** v1.2.0

**Rule Range:** FE-100 through FE-151

---

## Certification Philosophy

Frontend governance must be machine-enforceable. Manual code review alone is insufficient to maintain architectural boundaries. Rules are verified by `scripts/check_arch.ts` and enforced in CI.

**Rule categories:**

| Category | Rules | Focus |
|----------|-------|-------|
| State/Category | FE-100–FE-110 | State variable classification and ownership |
| IPC/Contract | FE-111–FE-120 | Contract file structure and isolation |
| Async/Safety | FE-121–FE-130 | RuntimeScope, cancellation, stale-response |
| Import/Structure | FE-131–FE-140 | File import boundaries and organization |
| Projection/Purity | FE-141–FE-148 | Domain computation prohibition |
| Timer/Scope | FE-149–FE-151 | Timer and listener governance |

---

## Rule Categories

### State/Category — Rules FE-100 through FE-110

These rules enforce the four-category state model (Projection, UI, Session, Transient).

| Rule | Severity | Description |
|------|----------|-------------|
| FE-100 | Error | Every `$state()` declaration must have a `// @category` marker |
| FE-101 | Error | Projection State must use types from contracts/backend DTOs, never local types |
| FE-102 | Error | Component/page files must not perform domain arithmetic on projection values |
| FE-103 | Error | Direct `invoke()` only permitted in contract files |
| FE-104 | Error | Direct `@tauri-apps/` import only permitted in contract files and tauri.ts |
| FE-105 | Error | Every `writable()`/`readable()` store must have a `// @category` marker |
| FE-106 | Error | UI State must not hold backend DTO types |
| FE-107 | Warning | Transient State should be reset on submit |
| FE-108 | Error | Session State only in `src/lib/session.ts`, must use `writable()` |
| FE-109 | Warning | Projection to Transient assignment triggers semantic copy warning |
| FE-110 | Error | No `any` type on state category declarations |

### IPC/Contract — Rules FE-111 through FE-120

These rules govern contract file structure and isolation.

| Rule | Severity | Description |
|------|----------|-------------|
| FE-111 | Error | Contract file must exist per domain |
| FE-112 | Error | Contract file must export at least one `safeInvoke` wrapper |
| FE-113 | Error | `safeInvoke` only permitted in contract files and tauri.ts |
| FE-114 | Error | Contract functions must have explicit return types |
| FE-115 | Warning | Contract function names should match IPC command convention |
| FE-116 | Error | No cross-contract imports between `.contract.ts` files |
| FE-117 | Error | Contract DTOs must be defined in same contract file or shared barrel |
| FE-118 | Error | Platform API wrappers must not be duplicated in contract files |
| FE-119 | Warning | Contract files must not re-export `@deprecated` commands |
| FE-120 | Error | Contract files must not import Svelte runtime |

### Async/Safety — Rules FE-121 through FE-130

These rules ensure race-safe async operations.

| Rule | Severity | Description |
|------|----------|-------------|
| FE-121 | Error | Every page must call `createRuntimeScope()` and dispose in `onDestroy` |
| FE-122 | Warning | `createOperation`/`createOperationGuard` should receive RuntimeScope |
| FE-123 | Error | No raw `setTimeout`/`setInterval` outside RuntimeScope |
| FE-124 | Error | No raw `addEventListener` outside RuntimeScope |
| FE-125 | Error | All async data-fetching must use `createOperation` or stale-response guard |
| FE-126 | Warning | No overlapping projection requests (loading guard missing) |
| FE-127 | Warning | Long-running contract functions should accept `AbortSignal` |
| FE-128 | Error | `run()` and `guard()` must check `scope.isAlive()` before state updates |
| FE-129 | Error | Polling must have cancellation mechanism in lifecycle hook |
| FE-130 | Warning | Double-subscribe pattern should use `get()` instead |

### Import/Structure — Rules FE-131 through FE-140

These rules enforce file import boundaries.

| Rule | Severity | Description |
|------|----------|-------------|
| FE-131 | Error | Pages must not import from other domain pages |
| FE-132 | Error | Components must not import from pages |
| FE-133 | Error | Pages must only use shared UI components from `lib/components/ui/` |
| FE-134 | Warning | Domain-named files outside contract directory suggest leakage |
| FE-135 | Error | Importing from `preview.ts` is forbidden (being removed) |
| FE-136 | Error | All `.contract.ts` files must be in `src/lib/contracts/` |
| FE-137 | Error | No barrel file circular imports |
| FE-138 | Error | Components must not import IPC functions directly |
| FE-139 | Warning | Pages should import from contract barrel, not specific contracts |
| FE-140 | Warning | Contract files must be statically imported |

### Projection/Purity — Rules FE-141 through FE-148

These rules prohibit domain computation in the frontend.

| Rule | Severity | Description |
|------|----------|-------------|
| FE-141 | Error | Dividing projection values to compute averages is forbidden |
| FE-142 | Error | Multiplying quantity by unit cost in frontend is forbidden |
| FE-143 | Error | Summing backend cost fields in frontend is forbidden |
| FE-144 | Warning | Local `.filter()` with business logic should be backend-provided |
| FE-145 | Error | All consumption arithmetic must come from backend projections |
| FE-146 | Error | Any locally computed variable named `*Average*` or `*Avg*` is a violation |
| FE-147 | Error | Reassembling backend projections into a new summary is forbidden |
| FE-148 | Error | Multi-step `$derived` chain producing business value is forbidden |

### Timer/Scope — Rules FE-149 through FE-151

These rules are specific to timer and listener governance.

| Rule | Severity | Description |
|------|----------|-------------|
| FE-149 | Error | `session.ts` must use `createRuntimeScope` for all timers and listeners |
| FE-150 | Error | No module-level mutable timer or listener references |
| FE-151 | Warning | Touch/scroll/wheel listeners should use `{ passive: true }` |

---

## Enforcement Strategy

### Static Analysis (`scripts/check_arch.ts`)

Rules are implemented as file pattern + regex + validation function checks in the existing `check_arch.ts` framework. Each rule:

1. Scans files matching a glob pattern
2. Applies a regex to detect violations
3. Applies a validation function to filter false positives
4. Reports violations grouped by file
5. Returns error exit code if violations found

### Rule Implementation Categories

| Implementation Type | Rules | Approach |
|-------------------|-------|----------|
| Regex check | FE-100, FE-103, FE-104, FE-105, FE-113, FE-120, FE-123, FE-124, FE-130, FE-149 | Line-level regex match with skip patterns |
| File existence | FE-111, FE-136 | Glob pattern file existence check |
| AST parse | FE-102, FE-141, FE-142, FE-143, FE-146, FE-148 | Parse `<script>` content for arithmetic expressions |
| Import graph | FE-116, FE-131, FE-132, FE-137, FE-138, FE-139 | Track import statements and verify against allowlist |
| Content scan | FE-121, FE-129, FE-144, FE-147, FE-150 | Multi-line content patterns |

---

## CI Integration Strategy

### Pipeline Addition

```yaml
# .github/workflows/frontend-architecture.yml
name: Frontend Architecture Governance
on: [pull_request]
jobs:
  check-fe:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v1
      - run: bun install
      - run: bun run check:fe
```

### NPM Script

```json
{
    "scripts": {
        "check:fe": "bun run scripts/check_arch.ts",
        "check:all": "bun run check:fe && bun run check:arch"
    }
}
```

### Gating

- `check:fe` must exit with code 0 for merge approval
- Warnings are informational (not gating) but must be tracked
- Violations block merge immediately
- ADR exceptions must be registered in `adr_exception_registry.md` to bypass specific rules

---

## Certification Gates

Gate 1 — **State Governance**
- All `$state()` and `writable()` declarations have `@category` marker
- No Projection Purity violations
- Session State only in `session.ts`

Gate 2 — **IPC Governance**
- All domains have contract files
- No direct `invoke()` outside contracts
- Contract functions have typed returns

Gate 3 — **Async Safety**
- All pages have `createRuntimeScope`
- No raw timers or listeners
- Stale-response guards in place

Gate 4 — **Import Structure**
- No cross-domain page imports
- Components don't import IPC functions
- Contract files are pure TS

Gate 5 — **Projection Purity**
- No domain computation in components
- All business values from backend
- No local filtering of business data

---

## Failure Conditions

| Condition | Result |
|-----------|--------|
| Any FE-100–FE-151 Error violation | CI fails, merge blocked |
| ADR exception expired (>90 days) | CI fails until renewed |
| New state variable without category marker | Rule FE-100 violation |
| Direct `invoke()` in page/component | Rule FE-103 violation |
| Raw `setInterval` in lib code | Rule FE-123/FE-149 violation |
| Domain arithmetic in component | Rule FE-102/FE-141 violation |

---

## Governance Debt Classification

### Critical Debt

| Pattern | Rules | Recognition |
|---------|-------|-------------|
| Business logic in UI | FE-102, FE-141–FE-148 | Arithmetic on backend values |
| Accounting/FIFO in frontend | FE-142, FE-143, FE-145 | Cost multiplication/summation |
| Semantic duplication | FE-147, FE-148 | Reassembling backend summaries |
| Backend authority violation | FE-101, FE-106 | Domain state in frontend |

### High Debt

| Pattern | Rules | Recognition |
|---------|-------|-------------|
| DTO drift | FE-111, FE-112, FE-114 | Missing contract files or types |
| Hidden mutable state | FE-150 | Module-level timer/listener variables |
| Race conditions | FE-121, FE-123, FE-124, FE-125 | Missing RuntimeScope or stale guards |
| Authorization duplication | FE-104 | Backend auth logic in frontend |

### Medium Debt

| Pattern | Rules | Recognition |
|---------|-------|-------------|
| Missing runtime validation | FE-114, FE-115 | Untyped contract returns |
| Contract verification gaps | FE-111, FE-117 | Missing domain contracts |
| Unmanaged UI complexity | FE-131, FE-132, FE-138 | Cross-domain component coupling |

---

## Frontend Certification Process

### Pre-Certification Checklist

- [ ] All FE-100 through FE-151 rules implemented in `scripts/check_arch.ts`
- [ ] All migration items M1–M14 complete
- [ ] Zero violations on `bun run check:fe`
- [ ] Zero violations on `bun run check:arch`
- [ ] All ADR-0031 through ADR-0037 documented
- [ ] All ADR exceptions registered with review dates
- [ ] CI pipeline includes `check:fe` job
- [ ] CI pipeline blocks merge on violations

### Certification Validation

```
bun run check:fe --verbose
```
Expected output: `✅ Frontend governance: PASS (0 violations, 0 warnings)`

### Post-Certification

- Governance debt tracked in `adr_exception_registry.md`
- 90-day review cycle for all ADR exceptions
- Architecture review required for new state categories
- Rule refinement allowed via ADR process only
