# ADR-007: All Runtime Evaluation Functions Are Pure

## Status
Accepted

## Context

The `sqlite_runtime` and `sqlite_runtime_review` layers in the Algerian Civil Protection food-service management system are responsible for database health assessment, integrity verification, and operational diagnostics. These functions — `sqlite_runtime()`, `sqlite_runtime_review()`, and their callees — are invoked during startup, periodic health checks, and pre-backup validation.

Because these functions evaluate database state and runtime health, they must not themselves produce side effects that would alter database state, depend on wall-clock time, open connections outside the single-writer pattern, or exhibit non-deterministic behavior. A non-pure evaluation function could: corrupt the very state it is evaluating, produce false positives or negatives because of timing-dependent behavior, bypass the single-connection discipline (ADR-005) by opening secondary connections, or report non-reproducible diagnostics that cannot be compared across nodes.

The system's audit and oversight requirements demand that health evaluations are themselves auditable: two evaluations of the same database state at different times must produce identical results. If an evaluation function depends on `Utc::now()`, a background thread scheduler, or a mutable counter, its output is inherently non-reproducible.

## Decision

All runtime evaluation functions (`sqlite_runtime`, `sqlite_runtime_review`, and any function they call) are pure: they have no side effects, open no connections during evaluation, depend on no wall-clock time, and are deterministic.

The purity contract requires:

1. **No side effects**: Evaluation functions do not execute `INSERT`, `UPDATE`, `DELETE`, or any mutation on the database. They do not write to the filesystem, modify global state, or emit domain events. The `sqlite_runtime_review` layer is explicitly checked for mutation calls (Rule 121).

2. **No connections opened during evaluation**: Evaluation operates on the single shared connection obtained through `AppState` (ADR-005). No temporary or secondary connections are opened except in `backup_validation.rs` which has an explicit exemption (Rule 110). The `sqlite_runtime_review` layer is explicitly checked for `Connection::open` (Rule 120).

3. **No wall-clock dependency**: Evaluation functions do not call `Utc::now()`, `SystemTime::now()`, or `Instant::now()`. Temporal context comes exclusively from database row timestamps or from an explicit reference time passed as a parameter. Rules 115 (runtime) and 122 (runtime review) enforce this.

4. **No background threads or schedulers**: Evaluation functions run on the calling thread. They do not spawn threads (Rule 108 for runtime, Rule 116 for review), enter polling loops (Rule 109 for runtime, Rule 118 for review), or schedule deferred work.

5. **No async execution**: Evaluation functions are synchronous. Rule 117 enforces that no async runtime constructs appear in `sqlite_runtime_review/`.

6. **Input determinism**: Given the same database connection (at the same state) and the same explicit parameters, an evaluation function always returns the same output. There is no internal mutable state across calls — no counters, accumulators, or caches.

The `sqlite_runtime` layer (checkpoint advice, integrity_runner, idle_checkpoint policies, runtime_metrics) and `sqlite_runtime_review` layer (review evaluation) both operate under this purity contract. The `IntegrityExecutionPolicy` enum defines what actions are taken based on evaluation results, but the evaluation itself is a pure assessment.

## Consequences

**Easier:**
- Reproducible diagnostics: two evaluations of the same database state always produce identical results, enabling delta-based monitoring and cross-node comparison.
- Auditable health checks: evaluation results can be logged as audit events without concern that the evaluation itself modified state.
- Safe invocation at any time: pure evaluation can be called during backup, during sync import, or during report generation without risk of interfering with the primary operation.
- Testing simplicity: pure functions are tested without mocking time, without thread synchronization, and without special setup beyond providing a database connection.

**Harder:**
- Wall-clock measurements (query duration, checkpoint age) must be recorded as metadata separate from the evaluation result, or passed as an explicit parameter.
- Any evaluation that conceptually needs timing information (e.g., "how long since last checkpoint") must compute this from persisted timestamps in the database, not from `Utc::now()`.
- The purity constraint limits the observability layer's ability to report "current wall-clock time" — all temporal data must be traceable to database content.
- If a future evaluation requires a side effect (e.g., "repair a corrupted index"), that effect must be implemented as a separate explicit action invoked by an orchestrator, not as part of the evaluation function.

## Compliance

Enforced by:

- Rule 115: no `Utc::now()` in `sqlite_runtime/` — runtime evaluation must not depend on wall-clock time.
- Rule 116: no `std::thread::spawn` in `sqlite_runtime_review/` — no background threads during review evaluation.
- Rule 117: no `async fn`, `await`, `tokio::`, `futures::`, or `Async` in `sqlite_runtime_review/` — evaluation is synchronous.
- Rule 118: no `loop { }`, `while true`, or `for (;;)` in `sqlite_runtime_review/` — no polling or scheduler loops.
- Rule 121: no `INSERT`, `UPDATE`, `DELETE`, `.execute()`, or `.prepare()` in `sqlite_runtime_review/` — review must not mutate state.
- Rule 122: no `Utc::now()` in `sqlite_runtime_review/` — review evaluation is deterministic with no wall-clock dependency.
- Rule 110: no secondary `Connection::open` in `sqlite_runtime/` outside `backup_validation.rs`.
- Rule 120: no `Connection::open` in `sqlite_runtime_review/` — review must not open its own write connections.
- All evaluation functions reside in `src-tauri/src/infrastructure/sqlite_runtime/` and `sqlite_runtime_review/`, with documented purity contracts and test coverage that verifies determinism.
