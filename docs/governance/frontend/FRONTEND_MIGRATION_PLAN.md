# Frontend Migration Plan — v1.2.0

**Status:** Active

**Version:** v1.2.0

---

## Executive Summary

v1.2.0 migrates the frontend from its current operational-but-unhardened state to a governance-certified frontend architecture. The migration is organized into 4 phases with 25 migration items (M1–M25) plus enforcement items (E1–E10).

**Mission:** Eliminate semantic leakage, establish state governance, and create an enforceable frontend architecture aligned with backend governance guarantees.

**Certification gate:** All M1–M14 complete, all FE-100+ rules passing with zero violations.

---

## Migration Goals

| Goal | Measured By |
|------|-------------|
| Eliminate raw timer/listener patterns | Zero FE-123/FE-124/FE-149 violations |
| Add state category markers to all declarations | Zero FE-100/FE-105 violations |
| Remove domain computations from preview.ts | Zero FE-141–FE-148 violations |
| Split monolithic tauri.ts into per-domain contracts | FE-111–FE-120 all passing |
| Move dashboard filtering to backend | FE-144 zero violations |
| Enforce all rules via CI | `check:fe` passes in CI |

---

## Phase 1: Critical — Timer/Lifecycle Safety (M1–M4)

**Phase:** 1
**Name:** RuntimeScope Hardening
**Objective:** Eliminate all raw timer and listener patterns that create race-condition risks
**Risk level:** Critical
**Estimated effort:** Small

### Items

| ID | File | Change | Verification |
|----|------|--------|-------------|
| M1 | `src/lib/session.ts:129` | Replace `window.setInterval(...)` with `scope.setInterval(...)` | FE-149, FE-123 |
| M2 | `src/lib/session.ts:187` | Replace `window.setTimeout(...)` with `scope.setTimeout(...)` | FE-149, FE-123 |
| M3 | `src/lib/session.ts:63,195` | Replace `document.addEventListener(...)` and module-level `activityHandler` with `scope.addListener(...)` | FE-149, FE-124 |
| M4 | `src/lib/session.ts:62,147-160` | Remove module-level `activityTimeout`, `activityHandler`, `checkInterval` from store. Make session.ts accept `RuntimeScope` parameter. | Manual review, FE-150 |

**Files affected:** `src/lib/session.ts`, `src/App.svelte`, `src/tests/unit/session.test.ts`

**Dependencies:** None (self-contained)

**Parallelization:** M1–M3 simultaneous; M4 depends on M1–M3

---

## Phase 2: High Priority — State Governance + Projection Purity (M5–M14)

**Phase:** 2
**Name:** State Category Marking and Domain Computation Elimination
**Objective:** Apply state category markers to all `$state()` declarations and eliminate domain computations in preview.ts and ConsumptionPage
**Risk level:** High
**Estimated effort:** Large

### Items

| ID | File | Change | Verification |
|----|------|--------|-------------|
| M5 | All 24 pages + components | Add `// @category` marker to every `$state()` declaration | FE-100, FE-101, FE-105 |
| M6 | `src/lib/session.ts:55` | Split `sessionState` store into separate Session State and UI State categories | FE-108 |
| M7 | `src/components/consumption/preview.ts:47-51` | Delete `mealPreviewFromFifo` — backend must return `meal_average` | FE-141, FE-145, FE-146 |
| M8 | `src/components/consumption/preview.ts:65-95` | Delete `productFifoCostsForMeal` — backend must return `unit_cost` | FE-142, FE-145 |
| M9 | `src/components/consumption/preview.ts:97-122` | Delete `summaryFromFifoPreview` — backend must return full `DailyConsumptionSummary` | FE-143, FE-147 |
| M10 | `src/components/consumption/preview.ts:125-161` | Delete `dailySummaryFromFormsAndFifo` — backend must compute summary | FE-147, FE-148 |
| M11 | `src/components/consumption/preview.ts:163-197` | Move `summaryFromSaved` to use backend-computed averages only | FE-145 |
| M12 | `src/components/consumption/preview.ts:199-233` | Delete deprecated `computeMealPreview`, `computeDailySummary` | FE-135 |
| M13 | `src/pages/ConsumptionPage.svelte:71-90` | Replace `displaySummary` derivation with backend-only summary | FE-147, FE-148 |
| M14 | `src/pages/ConsumptionPage.svelte` | Replace local computation imports with backend projection only | FE-141, FE-145 |

**Files affected:**
- `src/pages/ConsumptionPage.svelte` (lines 34-42 imports, lines 71-106 derived state)
- `src/components/consumption/preview.ts` (entire file — most functions deleted)
- `src/components/consumption/DailySummaryPanel.svelte` (simplification)
- `src/components/consumption/MealSection.svelte` (uses MealPreview, MealFifoCosts)
- All 24 pages for state marker annotations

**Dependencies:** None on Phase 1 (can run in parallel). Backend requires update to `DailyFifoConsumptionPreview` DTO.

**Parallelization:**
- M5 (state markers) runs in parallel with M7–M14
- M7–M12 (preview.ts) sequential within group
- M13–M14 (ConsumptionPage) depend on M7–M12

---

## Phase 3: Medium Priority — Contract Splitting + Dashboard Refactoring (M15–M25)

**Phase:** 3
**Name:** Contract Organization and Dashboard Purity
**Objective:** Split monolithic files into per-domain contracts and move dashboard filtering to backend
**Risk level:** Medium
**Estimated effort:** Large

### Items

| ID | File | Change | Verification |
|----|------|--------|-------------|
| M15 | `src/lib/contracts/` | Create directory structure and `index.ts` barrel | FE-111, FE-136 |
| M16 | `src/lib/contracts/` | Create `session.contract.ts`, `user.contract.ts` | FE-112, FE-114 |
| M17 | `src/lib/contracts/` | Create `consumption.contract.ts` | FE-112 |
| M18 | `src/lib/contracts/` | Create `inventory.contract.ts`, `distribution.contract.ts`, `report.contract.ts` | FE-112 |
| M19 | `src/lib/contracts/` | Create `dashboard.contract.ts`, `audit.contract.ts`, `observability.contract.ts` | FE-112 |
| M20 | `src/lib/contracts/` | Create `fiscal.contract.ts`, `sync.contract.ts`, `backup.contract.ts` | FE-112 |
| M21 | `src/lib/tauri.ts` | Strip to ~50 lines (platform API wrappers + safeInvoke only) | FE-113 |
| M22 | `src/lib/types.ts` | Convert to barrel re-export — move types to contract files | FE-117 |
| M23 | `src/pages/UnitDashboard.svelte:36-37` | Remove local `$: confirmedOrders` and `$: lowStockItems` — use backend endpoint | FE-144 |
| M24 | Backend (Rust) | Add `get_dashboard_stats` IPC command returning pre-filtered data | Manual review |
| M25 | All pages | Update import paths from `'../lib/tauri'` to `'../lib/contracts'` | FE-131, FE-139 |

**Files affected:**
- `src/lib/tauri.ts` (649→50 lines)
- `src/lib/types.ts` (860→barrel)
- 14 new contract files
- `src/pages/UnitDashboard.svelte`
- All 24 pages (import updates)
- Backend: new `get_dashboard_stats` command

**Dependencies:**
- M15–M22 after M5 (state markers)
- M23–M24 depend on backend `get_dashboard_stats`
- M25 depends on M16–M22

**Parallelization:**
- M16–M20 parallel (each file independent)
- M21–M22 parallel with M16–M20
- M23 independent of M15–M22
- M24 (backend) independent of all frontend work

---

## Phase 4: Enforcement — Static Analysis Rules + CI Gate (E1–E10)

**Phase:** 4
**Name:** Automated Governance Enforcement
**Objective:** Implement and activate all FE-100 through FE-151 rules in the CI pipeline
**Risk level:** Medium
**Estimated effort:** Medium

### Items

| ID | File | Change | Verification |
|----|------|--------|-------------|
| E1 | `scripts/check_arch.ts` | Implement FE-100 through FE-110 (State/Category) | Self-verifying |
| E2 | `scripts/check_arch.ts` | Implement FE-111 through FE-120 (IPC/Contract) | Self-verifying |
| E3 | `scripts/check_arch.ts` | Implement FE-121 through FE-130 (Async/Safety) | Self-verifying |
| E4 | `scripts/check_arch.ts` | Implement FE-131 through FE-140 (Import/Structure) | Self-verifying |
| E5 | `scripts/check_arch.ts` | Implement FE-141 through FE-148 (Projection/Purity) | Self-verifying |
| E6 | `scripts/check_arch.ts` | Implement FE-149 through FE-151 (Timer/Scope) | Self-verifying |
| E7 | `package.json` | Add `"check:fe": "bun run scripts/check_arch.ts"` script | Manual |
| E8 | `.github/workflows/` | Add CI job `frontend-architecture` that runs `check:fe` | CI log |
| E9 | `docs/architecture/` | Create ADR-0031 (done); ADR-0032–0037 CANCELLED — not implemented in v1.2.0 | Manual review |
| E10 | `docs/architecture/adr_exception_registry.md` | Register ADR exceptions for false positives | Manual review |

**Files affected:**
- `scripts/check_arch.ts` (add ~52 rule blocks)
- `.github/workflows/` (CI configuration)
- `package.json` (NPM scripts)
- `docs/architecture/` (ADR documents)

**Dependencies:**
- E1 depends on M5 (state markers applied)
- E2 depends on M15-M22 (contracts exist)
- E3 depends on M1-M4 (Phase 1)
- E4 independent (import structure)
- E5 depends on M7-M14 (domain computations removed)
- E6 depends on M1-M4

**Parallelization:**
- E1–E6 parallel (independent verification logic)
- E7–E8 depend on E1–E6
- E9–E10 parallel with implementation

---

## M1–M25 Summary Table

| ID | Phase | Priority | File | Risk |
|----|-------|----------|------|------|
| M1 | 1 | Critical | `session.ts:129` | High |
| M2 | 1 | Critical | `session.ts:187` | High |
| M3 | 1 | Critical | `session.ts:63,195` | High |
| M4 | 1 | Critical | `session.ts:62,147-160` | High |
| M5 | 2 | High | All pages + components | Medium |
| M6 | 2 | High | `session.ts:55` | Medium |
| M7 | 2 | High | `preview.ts:47-51` | High |
| M8 | 2 | High | `preview.ts:65-95` | High |
| M9 | 2 | High | `preview.ts:97-122` | High |
| M10 | 2 | High | `preview.ts:125-161` | High |
| M11 | 2 | High | `preview.ts:163-197` | Medium |
| M12 | 2 | High | `preview.ts:199-233` | Low |
| M13 | 2 | High | `ConsumptionPage.svelte:71-90` | High |
| M14 | 2 | High | `ConsumptionPage.svelte` | High |
| M15 | 3 | Medium | `src/lib/contracts/` | Low |
| M16 | 3 | Medium | `session.contract.ts` | Medium |
| M17 | 3 | Medium | `consumption.contract.ts` | Medium |
| M18 | 3 | Medium | `inventory/distribution/report.contract` | Medium |
| M19 | 3 | Medium | `dashboard/audit/observability.contract` | Medium |
| M20 | 3 | Medium | `fiscal/sync/backup.contract` | Medium |
| M21 | 3 | Medium | `tauri.ts` | Medium |
| M22 | 3 | Medium | `types.ts` | Medium |
| M23 | 3 | Medium | `UnitDashboard.svelte:36-37` | Medium |
| M24 | 3 | Medium | Backend (Rust) | Medium |
| M25 | 3 | Medium | All pages (imports) | Medium |

---

## Dependency Graph

```
Phase 1 (M1-M4) ───────────────┬────────────────────────────────
                                │
Phase 2 (M5) ───────────────────┼─────────── State Markers
Phase 2 (M6) ───────────────────┤
Phase 2 (M7-M14) ──────────────┤           Preview.ts cleanup
                                │
Phase 3 (M15-M22) ─────────────┼─────────── Contract creation
Phase 3 (M23-M24) ─────────────┤           Dashboard backend
Phase 3 (M25) ─────────────────┤           Import updates
                                │
Phase 4 (E1) ──────────────────┤           State rules
Phase 4 (E2) ──────────────────┤           Contract rules
Phase 4 (E3) ──────────────────┤           Async rules
Phase 4 (E4) ──────────────────┼─────────── Import rules
Phase 4 (E5) ──────────────────┤           Purity rules
Phase 4 (E6) ──────────────────┤           Timer rules
Phase 4 (E7-E10) ──────────────┘           CI gate + ADRs
```

---

## Rollout Strategy

### Parallel Tracks

| Track | Items | Team |
|-------|-------|------|
| A: Timer Safety | M1–M4, E6 | Full-stack |
| B: State Markers | M5–M6, E1 | Frontend architect |
| C: Projection Purity | M7–M14, E5 | Backend (API) + Frontend (cleanup) |
| D: Contract Splitting | M15–M22, M25, E2 | Frontend TS specialist |
| E: Dashboard Backend | M23–M24 | Backend specialist |
| F: Import Rules | E4 | Frontend architect |
| G: Async Rules + CI | E3, E7–E10 | DevOps / Full-stack |

### Recommended Schedule

| Week | Focus | Parallel Tracks |
|------|-------|-----------------|
| Week 1 | Timer Safety + Backend API changes | A + C (independent files) |
| Week 2 | State Markers + Contracts + Dashboard | B + D + E (all independent) |
| Week 3 | Rules implementation + CI | F + G + remaining E items |
| Week 4 | CI gate activation, ADR docs, exceptions | E7–E10 |

---

## Risk Prioritization

| Risk | Phase | Severity | Mitigation |
|------|-------|----------|------------|
| Backend changes not ready for M7-M14 | 2 | High | Temporary ADR exception marking preview.ts as deprecated while backend work completes |
| False positives in FE rules (E1-E6) | 4 | Medium | Register each as ADR exception with 90-day review; refine rule regex |
| Import path rename drift (M25) | 3 | Medium | Use automated codemod script to update all imports |
| Session.ts API change breaks consumers (M4) | 1 | High | Add backwards-compatible overload; deprecate old signature |
| Contract barrel circular imports | 3 | Medium | Use `madge` or `dependency-cruiser` to detect cycles pre-merge |
| Test breakage from RuntimeScope | 1 | Medium | `scope.setInterval` wraps native `window.setInterval` — fake timers still work |

---

## Definition of Completion

v1.2.0 migration is complete when:

1. **Phase 1 complete** — No raw timers/listeners in session.ts (M1-M4)
2. **Phase 2 complete** — All 24+ pages have state category markers (M5) — zero FE-100 violations
3. **Phase 2 complete** — preview.ts domain computations eliminated (M7-M14) — zero FE-141 through FE-148 violations
4. **Phase 3 complete** — Contract files created, tauri.ts and types.ts refactored (M15-M22)
5. **Phase 3 complete** — Dashboard filtering moved to backend (M23-M24)
6. **Phase 4 active** — All FE-100 through FE-151 rules present in `scripts/check_arch.ts` with zero violations in CI
7. **ADRs documented** — ADR-0031 accepted; ADR-0032–0037 DEFERRED (not implemented in v1.2.0 baseline)
8. **Architecture check passes** — `bun run check:arch` returns zero violations, zero warnings
