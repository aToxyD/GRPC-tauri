# Cross-Unit Benchmark Reproducibility Guarantees

## Scope

This document covers the reproducibility guarantees of the cross-unit benchmark layer (`application/oversight/benchmarks/`). These benchmarks produce statistical distributions of KPI values across a population of units.

## Reproducibility Contract

All benchmarks produce byte-identical output when given the same database state and the same fiscal year.

### What is guaranteed

| Property | Guarantee | Rationale |
|----------|-----------|-----------|
| **Deterministic ranking** | Same input values produce same rank order and percentile scores | Explicit `PartialOrd` tie-breaking by `unit_id` ascending |
| **Stable serialization** | `serde_json` output is identical for identical distributions | Field order is fixed by struct definition; no `HashMap` or nondeterministic containers |
| **Cross-run stability** | Two benchmarks on identical DB state produce identical `BenchmarkDistribution` | No wall-clock time, no randomness, no nondeterministic iteration |
| **Tie stability** | Identical metric values in same DB state yield identical ranks | `PercentileRank::compute` is deterministic; tie-breaking by `unit_id` is stable |

### What is NOT guaranteed

| Property | Reason | Mitigation |
|----------|--------|------------|
| **Cross-DB-instance stability** | Two DB instances with semantically equivalent but different row IDs produce different `unit_id` ordering | Tie-breaking uses `unit_id` — if IDs differ, rank may differ for tied values |
| **Cross-version stability** | Future report `version()` bumps may change per-unit KPI values | Report version changes require ADR + new file per Phase 2.X reproducibility rules |

## Deterministic Ranking Semantics

### Sorting

1. Units are sorted by their KPI numeric value **descending** (highest value = rank 1)
2. For values that are equal within `f64::EPSILON`, `unit_id` is used as tiebreaker, sorted **ascending**
3. Sort is stable — same inputs always produce same order

### Rank assignment

- Rank is 1-based (best-performing unit = rank 1)
- Tied values receive the **same rank** (e.g., two tied units at rank 1)
- If N units share rank R, the next distinct value receives rank R+N (gap ranking)

### Percentile rank computation

Percentile rank of value *v* within the population is:

```
rank = (count_below + 0.5 × count_equal) / total × 100
```

Where:
- `count_below` = number of units with value strictly less than *v*
- `count_equal` = number of units with value equal to *v* (within `f64::EPSILON`)
- `total` = total number of units
- Single-value population: rank = 50.0
- Empty population: rank = 0.0

### Distribution statistics

| Statistic | Method |
|-----------|--------|
| **min** | Minimum numeric value in population |
| **max** | Maximum numeric value in population |
| **median (p50)** | Nearest-rank percentile at 50% |
| **p95** | Nearest-rank percentile at 95% |

Nearest-rank formula: `rank = ceil(p / 100 × N)` → value at 1-based index `rank`.

## Aggregation Semantics

### Per-unit KPI computation

Each benchmark:
1. Queries all unit IDs via `ReportsContext::unit_ids()`
2. Creates a unit-scoped context via `ReportsContext::with_unit_id(uid)`
3. Computes the underlying KPI using the unit-scoped context
4. Collects the KPI's numeric value (skipping units where the KPI returns `MetricValue::None`)
5. Builds a `BenchmarkDistribution` from the collected values

### Unit-scoped report filtering

When a `ReportsContext` has a unit_id set (via `with_unit_id`), the `fiscal_year_summary()` method passes `unit_id` to `FiscalYearSummaryReport`, which:
- Filters `stock_movements` by `unit_id`
- Filters `daily_reports` by `unit_id`
- Filters `fifo_stock_layers` by `unit_id`

This ensures per-unit KPI values are computed from that unit's data only.

### Rounding

- KPI numeric values are rounded at the KPI level (e.g., `Amount` values to 2 decimal places)
- Distribution statistics inherit the KPI's precision — no additional rounding is applied at the benchmark level
- `f64::EPSILON` is used for tie detection to avoid precision issues

## Empty-handling

| Scenario | Behavior |
|----------|----------|
| No units exist | `unit_count: 0`, empty `values`, `min/max/median/p95` all `None` |
| Units exist but no KPI data | Skipped — only units with non-`None` KPI values are included |
| Single unit with data | `median == p95 == min == max == that unit's value` |
| All units produce same KPI value | All tied at rank 1; `median == min == max` |

## Enforcement

The following architecture rules apply to benchmark code:

- **Rule 58**: No SQL in `oversight/benchmarks/`
- **Rule 59**: No mutations in `oversight/benchmarks/`
- **Rule 60**: No transaction ownership in `oversight/benchmarks/`
- **Rule 61**: No repository imports in `oversight/benchmarks/`
- **Rule 62**: No `rusqlite` in `oversight/benchmarks/`

Benchmark `compute()` takes only a `&ReportsContext` — all data access goes through typed context methods.
