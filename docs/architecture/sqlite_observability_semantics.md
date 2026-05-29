# SQLite Observability Semantics

## Overview

The `sqlite_observability` module provides a deterministic, read-only
observability layer for SQLite diagnostics. It is an infrastructure-level
module with no business logic, no repository imports, and no state mutation.

## Design Principles

1. **No Mutation** — Every function is read-only. No INSERT/UPDATE/DELETE
   is executed. No WAL checkpoints are triggered. No schema changes are made.
2. **Determinism** — Same input produces same output. No wall-clock
   dependency in core parsing/computation. `snapshot_order` is the only
   non-deterministic input, provided by the caller.
3. **Pure Parsing** — PRAGMA output parsing is a pure function of the
   returned string. No connection state is read during parsing.
4. **Bounded Memory** — Slow query history is bounded by configurable
   capacity. No unbounded growth.
5. **No Background Threads** — All diagnostics are collected synchronously
   on demand. No periodic monitoring.
6. **No Telemetry Backend** — Results are returned as structs for the caller
   to consume. No export, no external service.

## Module Structure

```
sqlite_observability/
  mod.rs           — Module re-exports
  query_plan.rs    — EXPLAIN QUERY PLAN parsing and diagnostics
  slow_query.rs    — Slow query recording and categorization
  wal.rs           — WAL file and page count metrics
  integrity.rs     — PRAGMA integrity_check / quick_check parsing
  metrics.rs       — Page growth, connection, and memory metrics
  diagnostics.rs   — Aggregated SqliteDiagnosticsSnapshot
```

## Determinism Guarantees

| Component | Deterministic | Source of Determinism |
|-----------|-------------|----------------------|
| QueryPlan parsing | Yes | Pure function of input rows; SQL hash is stable hash |
| IndexRecommendation | Yes | Derived from scan detection; no heuristics with randomness |
| SlowQueryRecord | Yes | Ordering by `recorded_at_order` counter |
| WalMetrics | Yes | Pure construction from PRAGMA values |
| Integrity parsing | Yes | Line-by-line parsing; no time dependency |
| SqliteDiagnosticsSnapshot | Yes | All inputs are caller-provided; same inputs produce same snapshot |
| Snapshot ID | Yes | Hex-encoded `snapshot_order` — caller controls ordering |

## Observability Boundaries

The module observes but does not intervene:

- Reads PRAGMA output but never sets PRAGMA values
- Parses EXPLAIN QUERY PLAN but never rewrites SQL
- Detects full table scans but never creates indexes
- Identifies checkpoint eligibility but never executes checkpoints
- Records slow queries but never terminates them
- Parses integrity check output but never repairs corruption

## No-Mutation Guarantees

The module NEVER:

- Executes INSERT, UPDATE, or DELETE statements
- Creates, alters, or drops indexes
- Executes WAL checkpoint operations
- Modifies database schema
- Writes to the filesystem
- Spawns threads or background tasks

## Future Extensibility (Phase 6.B)

Planned extensions that maintain the above invariants:

1. **WAL Checkpoint Execution** — New `wal_checkpoint.rs` module with
   explicit `checkpoint()` function. Separate from `wal.rs` observability.
   Caller controls when to execute.
2. **Second Read Connection** — Evaluation-only in Phase 6.C. The
   observability layer will expose connection-level metrics to inform
   the decision.
3. **Scheduled Integrity Checks** — A new scheduler service above the
   observability layer. The parsing remains pure.
