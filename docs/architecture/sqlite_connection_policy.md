# SQLite Connection Policy — GRPC-Tauri

## Architecture Overview

The system uses a **single-writer SQLite topology** (WAL mode). One primary `Mutex<Option<Connection>>` in `app/state.rs` owns all production database access. Secondary connections are created only for documented backup/restore/integrity operations.

---

## Primary Connection

| Property | Value |
|----------|-------|
| Location | `app/state.rs` — `Mutex<Option<Connection>>` |
| Type | Read-write |
| WAL mode | Active (`PRAGMA journal_mode=wal`) |
| Concurrency | Serialized via Mutex |
| Lifetime | Process lifetime |

---

## Secondary Connection Policy

### Allowed Use-Cases

| Use-Case | File | Type | Connection Type | Rationale |
|----------|------|------|----------------|-----------|
| **Backup file validation** | `infrastructure/db/integrity.rs` | `Connection::open(path)` | Read-only (PRAGMA integrity_check) | Opens a detached backup file for integrity verification; not the primary database |
| **Pre-backup file validation** | `infrastructure/backup/sqlite_backup_adapter.rs` (line 167) | `Connection::open(path)` | Read-only (PRAGMA quick_check) | Validates that a file is a valid SQLite database before backup operation |
| **Backup destination** | `infrastructure/backup/sqlite_backup_adapter.rs` (line 474) | `Connection::open(temp_path)` | Read-write (temporary) | Creates a temporary destination database for SQLite Backup API; cleaned up after operation |
| **Restore metadata check** | `infrastructure/db/metadata.rs` | `Connection::open_with_flags(path, READ_ONLY)` | Read-only | Reads metadata from a backup file to verify restore safety |

### Prohibited Use-Cases

| Use-Case | Why Prohibited |
|----------|---------------|
| Offloading read queries to a second connection | Risk of stale reads; adds complexity without measurable benefit at current load |
| Background write connections | Violates single-writer guarantee; risk of WAL contention |
| Connection pools | SQLite is not designed for connection pools; unnecessary overhead |

### Threading Guarantees

1. Secondary connections are used **synchronously and ephemerally** — opened, used, closed within the same function
2. No secondary connection outlives its calling function
3. No secondary connection is shared across threads
4. No secondary connection is stored in application state

### WAL Assumptions

1. **Primary connection** uses WAL journal mode
2. Secondary connections to the **primary database file** must also use WAL mode
3. Secondary connections to **backup/restore files** open self-contained files that are not under WAL
4. Concurrent readers on the primary file (WAL allows concurrent reads) are not used by design — all reads go through the Mutex

### Enforcement

- `check_arch.ts` Rules 110 and 120 enforce that `sqlite_runtime/` and `sqlite_runtime_review/` do not open connections
- `infrastructure/db/` and `infrastructure/backup/` are the only allowed directories for secondary connections
- All secondary connection sites must be documented in the ADR exception registry

---

## Connection Inventory

| File | Line | Connection | Purpose | Allowed |
|------|------|-----------|---------|---------|
| `app/state.rs` | — | `Mutex<Option<Connection>>` | Primary production connection | ✅ Yes — primary |
| `infrastructure/db/integrity.rs` | 7 | `Connection::open(path)` | Integrity check on backup file | ✅ Yes — backup operation |
| `infrastructure/db/metadata.rs` | 9 | `Connection::open_with_flags(path, READ_ONLY)` | Read metadata from backup file | ✅ Yes — restore operation |
| `infrastructure/backup/sqlite_backup_adapter.rs` | 167 | `Connection::open(path)` | Pre-backup SQLite file validation | ✅ Yes — backup operation |
| `infrastructure/backup/sqlite_backup_adapter.rs` | 262 | `Connection::open_with_flags(...)` | Open source DB for backup | ✅ Yes — backup operation |
| `infrastructure/backup/sqlite_backup_adapter.rs` | 474 | `Connection::open(temp_path)` | Temporary destination for SQLite Backup API | ✅ Yes — backup operation |
| `infrastructure/backup/sqlite_backup_adapter.rs` | 525 | `Connection::open_with_flags(...)` | Open source for backup with page size | ✅ Yes — backup operation |
| `infrastructure/sqlite_runtime/backup_validation.rs` | 25 | `Connection::open_with_flags(...)` | Backup validation connection | ✅ Yes — backup validation |

---

## Policy Violations

Adding a new secondary connection site requires:
1. ADR update to this document
2. Entry in the ADR exception registry
3. Architecture review and approval
