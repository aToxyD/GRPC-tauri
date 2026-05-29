# ADR-005: Single Mutex Serializes All Database Access — Single-Writer Pattern

## Status
Accepted

## Context

The Algerian Civil Protection food-service management system runs as a Tauri desktop application on offline-first nodes deployed across multiple WILAYA-level units. SQLite is the embedded database engine. SQLite itself supports multiple readers or a single writer at any moment, but the system must enforce this at the application level to prevent concurrency bugs caused by Rust's ownership model interacting with the database handle.

In early iterations, `rusqlite::Connection` objects were created ad-hoc in multiple places, leading to subtle bugs: stale connections after WAL checkpoint operations, simultaneous write attempts causing `SQLITE_BUSY` errors, and inconsistent database state when one code path wrote through one connection while another read through a different connection that had not yet seen the write. The offline-first sync protocol and fiscal-year closure operations are particularly sensitive to write atomicity — a partially committed fiscal transition is unrecoverable.

The system needs a single, well-known access point for all database operations that serializes reads and writes through a mutex, ensuring that only one database operation is in flight at any time.

## Decision

All database access is serialized through a single `Mutex<Option<Connection>>` field in `AppState`:

```
pub struct AppState {
    pub db: Arc<Mutex<Option<Database>>>,
    // ...
}
```

Key properties of this pattern:

1. **Single connection**: Exactly one `Database` (which wraps a `rusqlite::Connection`) exists at any time. The connection is acquired via `state.get_db()` which returns a `MutexGuard<Option<Database>>`. All repository operations execute within this guard scope.

2. **Connection lifecycle**: The connection is created at application startup, stored in `AppState`, and only replaced during operations that require exclusive access (database backup via `take_db()` / `set_db()`, schema migration with pre-migration backup). The `take_db()` method extracts the connection from the `Option`, and `set_db()` returns it after the exclusive operation completes.

3. **No second write connection**: The `sqlite_runtime` modules (checkpoint, integrity_runner, idle_checkpoint, runtime_metrics, policies) are explicitly prohibited from opening secondary connections. Only `backup_validation.rs` within `sqlite_runtime` is allowed to open temporary connections for backup verification purposes.

4. **Read connection evaluated but not implemented**: A dedicated read-only connection was evaluated as an alternative to reduce mutex contention but was not implemented. The offline-first workload profile (single user per node, infrequent concurrent operations) does not justify the complexity of a read-connection pool. If read contention becomes a bottleneck in future deployments, a read-only connection can be added following the same single-writer discipline.

5. **Transaction ownership**: Transactions are owned by application services, not by repositories. The `with_transaction`, `with_event_context`, and `with_event_persistence` methods manage the transaction lifecycle on the single connection. Nested transactions are prohibited (Rule 89). Reporting and oversight layers are explicitly forbidden from owning transactions (Rules 50, 55, 60, 65).

## Consequences

**Easier:**
- Eliminates `SQLITE_BUSY` errors caused by concurrent write attempts on different connections.
- Guarantees write atomicity: a `MutexGuard` ensures that transactional boundaries are not interleaved.
- Simplifies connection lifecycle: there is exactly one connection to monitor, checkpoint, and back up.
- Makes take-backup semantics safe: `take_db()` extracts the connection exclusively, and `set_db()` returns it after backup completes.

**Harder:**
- Concurrent read operations block behind write operations (and vice versa). For the offline-first deployment model this is acceptable, but a future multi-threaded frontend with background sync could experience contention.
- Long-running write transactions (e.g., fiscal-year closure that touches dozens of tables) block all other database access for the duration.
- The `sqlite_runtime` modules must operate on the single connection rather than opening their own, which constrains how integrity checks and checkpoint operations are implemented.
- The `read connection evaluated but not implemented` caveat means the architecture has a known gap: if a read connection is ever added, all repository code must be audited to ensure read-only queries use the read connection and writes use the write connection.

## Compliance

Enforced by:

- Rule 110 in `scripts/check_arch.ts`: no `Connection::open` or `Connection::open_with_flags` in `sqlite_runtime/` outside `backup_validation.rs`. This prevents accidental secondary connections in the runtime layer.
- Rule 120: no `Connection::open` in `sqlite_runtime_review/` — the review evaluation layer must not open its own write connections.
- The `AppState::get_db()`, `take_db()`, and `set_db()` methods in `src-tauri/src/app/state.rs` are the sole API surface for database access. Commands acquire the database through these methods.
- The `DbExecutor` type in `src-tauri/src/repositories/executor.rs` implements `Copy` and wraps either a `&Connection` or a `&Transaction`, ensuring that all repository operations operate on the same underlying connection.
