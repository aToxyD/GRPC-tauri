# Projection Purity Baseline

**Generated:** 2026-06-01

**Status:** Certified — Phase 3B

**Purpose:** Establish the known, verified, and accepted projection purity state of the frontend at lockdown time. Any new finding after this baseline is a **regression**.

---

## Active Suppressions

| # | Rule | File | Line | Justification | Owner | Classification |
|---|------|------|------|---------------|-------|---------------|
| 1 | FE-141 | `src/pages/SyncTopologyPage.svelte` | 201 | UI percentage bar, not business division | Governance | UI Display |
| 2 | FE-141 | `src/pages/SyncTopologyPage.svelte` | 217 | UI percentage bar, not business division | Governance | UI Display |
| 3 | FE-141 | `src/pages/UnitDashboard.svelte` | 131 | Tailwind CSS opacity class (bg-red-50/50) — not business division | Governance | UI Display |
| 4 | FE-146 | `src/pages/ConsumptionPage.svelte` | 104 | projection from either saved report or live preview | Governance | Type Definition |
| 5 | FE-146 | `src/pages/ConsumptionPage.svelte` | 115 | field rename from backend projection, not average computation | Governance | Field Mapping |
| 6 | FE-146 | `src/pages/ConsumptionPage.svelte` | 124 | field rename from backend projection, not average computation | Governance | Field Mapping |
| 7 | FE-146 | `src/lib/telemetry.ts` | 154 | telemetry latency average (infrastructure metric, not business logic) | Governance | Infrastructure |
| 8 | FE-146 | `src/components/consumption/MealSection.svelte` | 9 | type definition mirroring backend projection shape | Governance | Type Definition |
| 9 | FE-146 | `src/components/consumption/MealSummaryCard.svelte` | 4 | type definition mirroring backend projection shape | Governance | Type Definition |

### Suppression Summary

| Rule | Count | Classification Breakdown |
|------|-------|------------------------|
| FE-141 | 3 | 3 UI Display |
| FE-146 | 6 | 3 Field Mapping, 2 Type Definition, 1 Infrastructure |
| FE-142 | 0 | — |
| FE-143 | 0 | — |
| FE-145 | 0 | — |
| FE-147 | 0 | — |
| FE-148 | 0 | — |

**Total:** 9 suppressions, 0 empty, 0 duplicates, 0 unused (FE-149 validated).

---

## Baseline Violation Counts

| Metric | Value |
|--------|-------|
| FE-141 violations (suppressed) | 3 |
| FE-146 violations (suppressed) | 6 |
| Total projection purity violations | 9 |
| All projection purity violations suppressed | Yes |
| Real semantic computation violations | 0 |
| All violations are false positives | Yes |

---

## Projection-Related Files

All frontend source files that consume or display backend projections:

| File | Projection Data | Usage |
|------|----------------|-------|
| `src/pages/ConsumptionPage.svelte` | Meal preview, FIFO preview | Display backend projections, field renaming |
| `src/pages/OrdersPage.svelte` | SupplierOrder, SupplierOrderItem | Display, total_amount consumed from backend |
| `src/pages/SyncTopologyPage.svelte` | SyncConflictSummary | UI percentage bars (suppressed) |
| `src/pages/UnitDashboard.svelte` | Stock, inventory | Display, CSS styling (suppressed) |
| `src/pages/DashboardPage.svelte` | KPI data | Display-only |
| `src/pages/ReportPage.svelte` | Report data | Display-only |
| `src/pages/ForecastPage.svelte` | Forecast data | Display-only |
| `src/pages/ConsumptionReportPage.svelte` | Consumption report | Display-only |
| `src/components/consumption/MealSection.svelte` | ActiveMealSummary | Type definition (suppressed) |
| `src/components/consumption/MealSummaryCard.svelte` | SummaryCardPreview | Display, type definition (suppressed) |
| `src/lib/telemetry.ts` | Operation durations | Infrastructure metric (suppressed) |

---

## Regression Policy

1. Any new `[arch:allow-fe*]` suppression requires justification text (FE-149).
2. Duplicate adjacent suppressions fail FE-149.
3. Unused suppressions (no matching violation pattern) fail FE-149.
4. Suppression creep is prohibited — new violations must be fixed, not suppressed.
5. Baseline review required when suppressed code is modified.
6. Baseline review scheduled: quarterly or on major frontend refactor.
