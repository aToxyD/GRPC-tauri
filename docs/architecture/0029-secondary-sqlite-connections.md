# ADR-0029: Secondary SQLite Connection Policy

## Status

Accepted — 2026-05-29

## Context

The single-writer SQLite topology (ADR-0025) uses one primary `Mutex<Option<Connection>>`. However, 3 production sites open secondary connections for backup/restore/integrity operations. These must be formally governed.

## Decision

Secondary SQLite connections are permitted only for:

1. **Backup file integrity verification** — opening a detached backup file to run `PRAGMA integrity_check` or `PRAGMA quick_check`
2. **Backup destination** — creating a temporary SQLite database to receive the SQLite Backup API output
3. **Restore metadata verification** — reading fiscal year metadata from a backup file before applying a restore

### Restrictions

- Connections must be ephemeral (open, use, close within the same function)
- Connections must not be stored in application state
- Connections must not be shared across threads
- Connections to the primary database file must use WAL mode
- Connections to backup files are read-only (except the backup temp destination)
- All secondary connection sites must be registered in the ADR exception registry

### Prohibited

- Read query offloading to a second connection
- Background write connections
- Connection pools
- Cross-thread connection sharing

## Consequences

1. All secondary connection sites are documented, governed, and auditable
2. The single-writer guarantee is preserved — secondary connections never write to the primary database
3. The `infrastructure/db/` and `infrastructure/backup/` directories are the only allowed locations for secondary connections
