# Reproducibility Guarantees

## Contract

Any report where `is_reproducible() == true` must produce byte-identical data output when given the same input and the same database state. The full `ReportEnvelope` (including `metadata.computed_at`) is NOT byte-identical because `computed_at` is a wall-clock timestamp.

Reproducibility is tested in the integration test suite: run once on a fixed snapshot, compare the `data` JSON, run again, assert byte-identical.

## What Makes a Report Reproducible

A reproducible report:

1. **Reads only from the database** via `DbExecutor` — no network, no filesystem, no external APIs
2. **Uses no wall-clock time** in its computation (only `computed_at` in metadata, which is excluded from reproducibility assertion)
3. **Uses no random values** — no `Uuid::new_v4()`, no random sampling
4. **Uses deterministic ordering** — all SQL queries have explicit `ORDER BY` with tiebreaker columns
5. **Uses round-half-to-even** for monetary rounding via `round_money()`
6. **Serializes identically** — same serde representation for identical data

## Deterministic Ordering

Every SQL query in reporting/ uses an explicit `ORDER BY` clause with at least one tiebreaker column:

| Report | ORDER BY clause |
|--------|----------------|
| FiscalYearSummary | N/A (aggregation-only) |
| InventoryValuation | `ORDER BY p.name COLLATE NOCASE` |
| StockMovementLedger | `ORDER BY sm.timestamp ASC, sm.id ASC` |

No query uses `ORDER BY created_at` alone (which could produce non-deterministic ordering with identical timestamps).

## Deterministic Rounding

```
round_money(value: f64) -> f64
```

Rounding is round-half-even (standard `f64::round()` behavior), applied:
- After every financial aggregation step
- After computing weighted averages
- To exactly 2 decimal places

## Historical Immutability

Historical reports (those referencing a closed fiscal year) must remain reproducible indefinitely. This is guaranteed by:

- Closed fiscal years receive no new mutations.
- FIFO layers for closed years are immutable after year closure.
- Stock movements for closed years are immutable.

### Versioning on Semantic Change

Any system change that would alter a historical report output is a breaking change requiring:

1. A new report `version()` number
2. An ADR documenting the semantic shift
3. A migration notice in the changelog

If a bugfix corrects a historical report's computation, both versions must coexist:
- V1 (buggy, original) preserved as-is with a deprecation notice
- V2 (corrected) available for new reports
- The system must document which version was in effect at any given time

## Cache and Reproducibility

- A cache miss produces the exact same result as a cache hit (minus computation time).
- Cache entries for closed fiscal years are never invalidated.
- Cache entries for open fiscal years are invalidated via DomainEvents, never by a general "clear all".

## Testing

Every report where `is_reproducible() == true` has at least one reproducibility integration test:

```rust
#[test]
fn test_reproducibility() {
    let output1 = Report::compute(&db1, &input).unwrap();
    let json1 = serde_json::to_string(&output1.data).unwrap();
    let output2 = Report::compute(&db2, &input).unwrap();  // identical db state
    let json2 = serde_json::to_string(&output2.data).unwrap();
    assert_eq!(json1, json2, "Report {} data is not reproducible", Report::slug());
}
```

The test creates two independent empty databases and verifies that the report data JSON is byte-identical.

### Test Suite

The reproducibility test suite (`tests/reporting_reproducibility_tests.rs`) includes:

| Test | What it verifies |
|------|-----------------|
| `inventory_valuation_is_deterministic_on_identical_db` | Byte-identical data on identical empty DB state |
| `fiscal_year_summary_computes_on_empty_db_without_error` | Error handling for missing year |
| `inventory_valuation_computes_on_empty_db` | Empty result on empty DB |
| `stock_movement_ledger_computes_on_empty_db` | Empty result on empty DB |
| `report_output_is_serializable` | Serde round-trip |
| `round_money_basic_cases` | Deterministic rounding |
| `cache_key_deterministic` | Cache key identity |
| `all_report_slugs_are_unique` | Slug uniqueness |
| `metadata_contains_snapshot_source` | Metadata integrity |

## Upgrade Resilience

Re-running a historical report after a system upgrade must preserve historical meaning. This is verified by:

1. Running the existing integration test suite against the historical snapshot database
2. All `is_reproducible() == true` report tests must pass before and after upgrade
3. A reproducibility test failure is a hard CI failure — same severity as a compilation error

## Enforcement

| Mechanism | What it enforces |
|-----------|-----------------|
| `check_arch.ts` Rule 49-52 | No mutations, no transactions, no side effects in reporting/ |
| Rust type system | `Report` trait with explicit `is_reproducible()` |
| Integration tests | Byte-identical output for identical inputs |
| CI pipeline | All reproducibility tests must pass |
| Historical versioning | Semantic changes require version increment |
