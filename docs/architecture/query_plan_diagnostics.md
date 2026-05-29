# Query Plan Diagnostics

## Overview

Query plan diagnostics wrap SQLite's `EXPLAIN QUERY PLAN` output into
deterministic, structured Rust types for inspection, alerting, and index
recommendation.

## Parsing Semantics

SQLite's `EXPLAIN QUERY PLAN` returns four columns:

| Column | Type | Description |
|--------|------|-------------|
| `id` | INTEGER | Node ID |
| `parent` | INTEGER | Parent node ID (0 for root) |
| `notused` | INTEGER | Unused (always 0) |
| `detail` | TEXT | Human-readable plan description |

## Scan Detection

A step is classified as a full table scan when `detail` contains the word
"SCAN" but does NOT contain:

- "USING INDEX"
- "USING COVERING INDEX"

Steps containing "SEARCH" with an index reference are NOT classified as
scans.

## Severity Classification

Severity is computed from the ratio of scan steps to total steps:

| Ratio | Severity |
|-------|----------|
| 0% | None |
| >0% to 25% | Low |
| >25% to 50% | Medium |
| >50% to 75% | High |
| >75% | Critical |

## Index Recommendations

Index recommendations are generated heuristically for each detected full
table scan. Recommendations include:

- The table being scanned
- The reason (always `FullTableScan` in Phase 6.A)
- An estimated impact severity

In Phase 6.A, column-level recommendations are not generated (requires
SQL parsing in Phase 6.C+).

## Determinism

- Same EXPLAIN output → same `QueryPlan` (verified by `PartialEq` + tests)
- `sql_hash` uses a stable hash of the SQL string
- No random or time-based fields in any diagnostic type
- All collections are ordered by construction order

## Limitations (Phase 6.A)

- No SQL parsing for column-level recommendations
- No historical query plan trend tracking
- No automatic EXPLAIN execution (caller must execute EXPLAIN and pass rows)
- No index creation or schema modification
- No query plan comparison across versions
