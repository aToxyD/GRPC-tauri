# ADR-003: All Reproducible Reports Produce Byte-Identical Output

## Status
Accepted

## Context

The Algerian Civil Protection food-service management system generates operational and fiscal reports at the unit, WILAYA, and national levels. These reports include daily consumption reports (breakfast, lunch, dinner sections with beneficiary counts and FIFO-based cost allocation), monthly summaries, fiscal-year snapshots, and WILAYA oversight rollups.

Fiscal integrity requires that given the same input data and the same database state, a report produces byte-identical output. Any variation in report output — whether from non-deterministic ordering, floating-point rounding differences, wall-clock timestamps embedded in output, or database query plan changes — breaks the ability to verify report correctness across nodes, invalidates cross-WILAYA comparison, and undermines audit credibility. Non-reproducible reports also prevent meaningful regression testing: a test cannot assert an exact expected output if the report contains non-deterministic elements.

## Decision

Reports that declare `is_reproducible() = true` must produce byte-identical output for identical input and identical database state. The following constraints apply to all reproducible reports:

1. **Deterministic ordering**: All queries in the reporting layer use explicit `ORDER BY` clauses with composite keys that include a unique tiebreaker (`id ASC` as the final sort column). No report query relies on natural table order, insertion order, or query-plan-dependent ordering.

2. **No wall-clock dependency**: Reproducible report code never calls `Utc::now()`, `SystemTime::now()`, or `Instant::now()`. Temporal data in reports comes exclusively from persisted columns (e.g., `date`, `fiscal_year`, `created_at` from the database row). Wall-clock timestamps for report metadata (e.g., `generated_at`) are acceptable only if the report type explicitly documents them as non-reproducible metadata.

3. **Deterministic floating-point formatting**: All monetary values are formatted using a fixed-precision rounding strategy (typically 2 decimal places, `round()` via `(val * 100.0).round() / 100.0` as seen in `DailyReportMeal::compute_meal_average`). Floating-point operations use the same evaluation path regardless of platform or CPU features.

4. **No external state**: Reproducible reports depend only on database state accessible through the repository layer. File system state, environment variables, and network state are never used. The reporting cache (`reporting/cache/`) stores intermediate results but always produces consistent output from consistent input.

5. **Pure computation**: Business logic in the reporting layer (averages, totals, cost allocation from FIFO portions) is computed using pure functions. The `simulate_fifo_consumption` engine in `src-tauri/src/domain/fifo_engine.rs` is shared between the consumption writing path and the report preview path, guaranteeing identical cost calculations.

The `DailyConsumptionSummary`, `DailyReportResult`, `MonthlySummary`, and `WilayaReportSummary` types all follow these constraints.

## Consequences

**Easier:**
- Regression testing: tests can assert exact report output byte-for-byte against a golden file or in-memory buffer, catching unintended changes.
- Cross-node verification: the same database state on two different units produces identical WILAYA-level rollups, enabling automated consistency checks during sync.
- Fiscal audit: external auditors can independently reproduce any report from the underlying data and verify the results exactly.
- Cache validity: the reporting cache (`compute_or_get_cached_helper`) can be validated by recomputing the report and comparing byte output.

**Harder:**
- Adding a new field to a report requires ensuring it is deterministic (no wall-clock values, no un-ordered aggregations).
- Reports that intentionally include non-reproducible metadata must clearly document this and cannot declare `is_reproducible() = true`.
- The reporting layer (`src-tauri/src/application/reporting/`) is prohibited from owning transactions (Rule 50) and from mutating state (Rule 51), which constrains how report caching and invalidation are designed.
- Reproducible report tests must seed the database with a fixed state rather than relying on fixtures that may vary across test runs.

## Compliance

Enforced by:

- Architecture Rule 49 (`check_arch.ts`): no SQL mutations in `reporting/` — reports are read-only.
- Architecture Rule 50: no transaction ownership in `reporting/` — reports must not commit state.
- Architecture Rule 51: no mutation calls on repositories from `reporting/`.
- Architecture Rule 77: no `Utc::now()` in `reporting/` — use `SystemTime` for access tracking only, with explicit acknowledgment of non-reproducibility.
- A reproducibility test suite (`src/tests/integration/reproducibility.test.ts` or equivalent) that runs each reproducible report twice against the same seeded database, asserts byte-identical output, and runs in CI.
- FIFO layer ordering (ADR-001) and domain event ordering (ADR-002) provide the foundation for deterministic cost allocation.
- SQL queries in the reporting layer use explicit `ORDER BY` clauses; repository methods default to keyset pagination (Rule 85 prohibits `OFFSET` in sync integrity paths, and the same principle applies to report queries).
