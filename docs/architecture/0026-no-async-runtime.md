# ADR-006: The System Uses No Async Runtime

## Status
Accepted

> **Amendment (ADR-0043):** This ADR is **partially superseded** by
> `docs/architecture/0043-backup-async-exception.md` for the two backup command handlers
> only — `create_backup` and `restore_backup` in `src-tauri/src/commands/backup.rs` use
> `tauri::async_runtime::spawn_blocking` for blocking backup/restore file I/O (decision
> items 1–2 above). All other Rust code remains fully synchronous.

## Context

The Algerian Civil Protection food-service management system is a desktop application built on Tauri. Desktop application development in Rust commonly uses async runtimes (tokio, async-std, smol) to handle concurrent I/O, background tasks, and non-blocking GUI interactions. However, the system's architecture prioritizes deterministic execution, auditability, and operational simplicity over async-induced throughput.

The core operations — FIFO consumption, fiscal-year closure, sync package import, report generation — are CPU-bound database transactions that do not benefit from async concurrency. Introducing an async runtime would add complexity (pin management, future combinators, cancellation safety), complicate the single-writer SQLite access pattern (ADR-005), and introduce non-determinism risks from async task scheduling that could affect reproducible reporting (ADR-003).

The system's offline-first deployment model means there is no network request to a central server that would benefit from non-blocking I/O. Sync is package-based and occurs as discrete operations, not streaming connections. The frontend (Svelte/TypeScript) already handles UI-level concurrency; the Rust backend operates as a synchronous command processor invoked by Tauri's `invoke` mechanism.

## Decision

The system uses no async runtime. All Rust code is synchronous. Specifically:

1. **No tokio, async-std, smol, or futures dependency**: `Cargo.toml` does not include any async runtime crate. The `futures` crate is not used. The `async` keyword does not appear in any Rust source file.

2. **All functions are synchronous**: Every `fn` in the codebase is a regular synchronous function. No `async fn`, no `.await` calls, no `Future` implementations. Tauri command handlers are synchronous — they return `Result<T, E>` directly, not `Result<impl Future<Output = T>, E>`.

3. **The `sqlite_runtime_review` layer enforces synchronous execution**: Rule 117 in `scripts/check_arch.ts` scans all files under `src-tauri/src/infrastructure/sqlite_runtime_review/` for `async fn`, `await`, `tokio::`, `futures::`, or `Async` and flags any occurrence as an error.

4. **Thread spawning is also prohibited**: Rules 101, 108, and 116 forbid `std::thread::spawn` in the observability, runtime, and runtime-review layers respectively. The system does not use background threads for polling, scheduling, or concurrent work.

5. **Background operations use the Tauri event system**: Long-running or periodic operations (idle checkpoint, backup scheduling) are driven by the Tauri frontend via timers and event listeners, not by background threads in Rust. The frontend creates runtime scopes via `createRuntimeScope` (enforced by Rules 41, 42, 44) that respect the Tauri lifecycle.

## Consequences

**Easier:**
- Deterministic execution: no async task scheduler can interleave operations non-deterministically. The same input always produces the same execution path.
- Simplified error handling: synchronous code eliminates the need for `Box<dyn Future>`, pin projections, or cancellation-safe patterns. Error propagation uses standard `Result` and `?`.
- Lower compilation overhead: no async runtime dependency reduces compile time and binary size.
- Single-writer SQLite is naturally synchronous: the `Mutex` lock is held for the duration of a synchronous operation, which is predictable and easy to reason about.

**Harder:**
- Network I/O, if ever needed (e.g., direct HTTP sync instead of package-based sync), would require either a synchronous HTTP client (blocking the mutex) or an external process.
- CPU-bound report generation blocks the Tauri command thread for the duration of the computation. For very large fiscal-year rollups this could cause UI unresponsiveness if not offloaded to the frontend with progress reporting.
- No built-in timeout mechanism for database operations: a long-running query blocks the mutex indefinitely. Timeout must be implemented at the SQLite level (`PRAGMA busy_timeout`) or via Tauri command timeout.
- The frontend must manage its own async lifecycle (Svelte reactive state, `createRuntimeScope`), which means the async/sync boundary is the Tauri IPC bridge rather than within the Rust code.

## Compliance

Enforced by:

- Rule 117 in `scripts/check_arch.ts`: scans `sqlite_runtime_review/` for `async fn`, `await`, `tokio::`, `futures::`, `Async`. An error is raised if any are present.
- Rule 108: no `std::thread::spawn` in `sqlite_runtime/` (prevents background threads in runtime).
- Rule 101: no `std::thread::spawn` in `sqlite_observability/`.
- Rule 116: no `std::thread::spawn` in `sqlite_runtime_review/`.
- Absence of `tokio`, `async-std`, `smol`, `futures` in `Cargo.toml` — any addition of these crates would be caught in code review and requires a new ADR with justification.
- All Tauri `#[tauri::command]` functions are synchronous `fn` returning `Result<T, E>`.
