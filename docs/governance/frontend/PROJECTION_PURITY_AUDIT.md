# Projection Purity Audit Report

Generated: 2026-06-01
Phase: 2C — Pre-escalation baseline

## Summary

| Rule | Total | Real Violations | False Positives | Governance Exceptions |
|------|-------|-----------------|-----------------|----------------------|
| FE-141 | 3 | 0 | 3 | 0 |
| FE-142 | 0 | 0 | 0 | 0 |
| FE-143 | 1 | 1 | 0 | 0 |
| FE-146 | 6 | 0 | 6 | 0 |
| FE-148 | 0 | 0 | 0 | 0 |

## FE-141: Division on projection values

### 1. SyncTopologyPage.svelte:201
- **Expression**: `Math.min((t.count/summary.total)*100,100)`
- **Classification**: False positive
- **Rationale**: UI percentage bar width calculation. Division is used for proportional display scaling (count/total ratio for progress bars), not business computation. The values `count` and `total` are conflict statistics from the backend, rendered as visual proportions.

### 2. SyncTopologyPage.svelte:216
- **Expression**: `Math.min((s.count/summary.total)*100,100)`
- **Classification**: False positive
- **Rationale**: Same as above — identical UI percentage bar rendering for severity breakdowns.

### 3. UnitDashboard.svelte:131
- **Expression**: `<tr class="... stock.quantity < 10 ...">`
- **Classification**: False positive
- **Rationale**: HTML template comparison (`<` operator in ternary for CSS class), not division. Regex false match on `<` character in template syntax.

## FE-142: Multiplication on projection values

No findings.

## FE-143: Addition reconstructing aggregate costs

### 1. OrdersPage.svelte:401
- **Expression**: `orderItems.reduce((sum, item) => sum + item.total_cost, 0).toFixed(2)`
- **Classification**: Real violation
- **Rationale**: Frontend recomputes the total order amount by summing individual item `total_cost` values. The backend already provides the total — this duplicates backend semantics.
- **Recommendation**: Add a `totalAmount` field to the order response from the backend, or consume an existing projection. Remove the frontend reduce computation.

## FE-146: Average recomputation

### 1. ConsumptionPage.svelte:105 (type definition)
- **Expression**: `type ActiveMealSummary = { totalBeneficiaries: number; totalCost: number; mealAverage: number }`
- **Classification**: False positive
- **Rationale**: TypeScript type definition only — no computation. The field `mealAverage` is mapped directly from backend projection `m.meal_average`.

### 2. ConsumptionPage.svelte:116
- **Expression**: `mealAverage: m.meal_average`
- **Classification**: False positive
- **Rationale**: Direct field mapping from backend projection value. No computation.

### 3. ConsumptionPage.svelte:124
- **Expression**: `mealAverage: activeFifoMeal.meal_average`
- **Classification**: False positive
- **Rationale**: Direct field mapping from backend projection value. No computation.

### 4. MealSection.svelte:9
- **Expression**: `type ActiveMealSummary = { totalBeneficiaries: number; totalCost: number; mealAverage: number }`
- **Classification**: False positive
- **Rationale**: TypeScript type definition only — no computation.

### 5. MealSummaryCard.svelte:4
- **Expression**: `type SummaryCardPreview = { totalBeneficiaries: number; totalCost: number; mealAverage: number }`
- **Classification**: False positive
- **Rationale**: TypeScript type definition only — no computation.

### 6. telemetry.ts:154
- **Expression**: `const avg =`
- **Classification**: False positive
- **Rationale**: Telemetry timing calculation (`avg` is latency average, not business data). Operations telemetry is infrastructure, not domain logic.

## Escalation Recommendations

For Phase 3 projection purity escalation to ERROR:

1. **FE-143 — OrdersPage.svelte:401**: Must be fixed before escalation. The `reduce` sum of `total_cost` is a real semantic duplication.
2. **FE-141**: Current regex is too broad — matches `<` in HTML templates and percentage display math. Needs regex refinement before escalation.
3. **FE-146**: Current regex matches field names in type definitions and telemetry. Needs regex refinement or explicit allow-list for type-only references.
4. **FE-142, FE-148**: No findings, ready for escalation as-is.
