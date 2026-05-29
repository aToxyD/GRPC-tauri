# Audit Schema Evolution

## Status
Implemented — Phase 4, Q2 2026.

## Motivation

The original `audit_log` table stored all audit data in flat, non-queryable columns. While functional for linear audit trails, this schema made governance queries — "show all FiscalEvents in FY 2025", "find all mutations targeting product X" — expensive or impossible without full-table scans.

Phase 4 introduces structured columns alongside the existing flat schema, enabling governance-grade queries while preserving full backward compatibility.

## Schema

### Migration 004

The migration is additive only — no existing columns are modified or removed.

**New columns on `audit_log`:**

| Column | Type | Purpose |
|--------|------|---------|
| `event_type` | TEXT | High-level classification: `UserAction`, `SystemEvent`, `FiscalEvent`, `SyncEvent`, `IntegrityEvent` |
| `actor_id` | TEXT | The user or system principal that performed the action |
| `target_type` | TEXT | The entity type affected (e.g. `Product`, `Unit`, `Financial`) |
| `target_id` | TEXT | The specific entity ID affected |
| `fiscal_year` | INTEGER | Fiscal year scope (if applicable) |
| `before_snapshot` | TEXT (JSON) | State before mutation |
| `after_snapshot` | TEXT (JSON) | State after mutation |
| `node_id` | TEXT | Sync node identifier (for multi-node events) |
| `details` | TEXT (JSON) | Full legacy representation as JSON for backward-compatible readers |

**New indexes:**

| Index | Columns | Purpose |
|-------|---------|---------|
| `idx_audit_target_entity` | `(target_type, target_id, timestamp)` | Entity-scoped queries |
| `idx_audit_actor_timestamp` | `(actor_id, timestamp)` | User-activity queries |
| `idx_audit_fiscal_event` | `(fiscal_year, event_type)` | Fiscal-event aggregation |
| `idx_audit_keyset` | `(timestamp, id)` | Keyset pagination |

### Domain Types

New types in `domain/audit.rs`:

- **`AuditEventType`** — enum: `UserAction`, `SystemEvent`, `FiscalEvent`, `SyncEvent`, `IntegrityEvent`
- **`AuditEvent`** — structured audit record with all governance fields plus legacy `details` JSON
- **`AuditEventRow`** — database row shape for the projection layer (all 25 columns)

### Projection Function: `to_audit_event()`

Reconstruction priority:

1. **Structured columns** — if `event_type` is non-NULL, use all structured columns directly
2. **Details JSON** — if `details` is non-NULL but `event_type` is NULL, reconstruct from the JSON payload
3. **Legacy flat columns** — if neither structured columns nor details are present, reconstruct from the original flat columns (action, entity_type, etc.)

This three-tier approach ensures all rows — regardless of when they were written — produce a complete `AuditEvent`.

### Event Type Mapping

`AuditAction` variants are mapped to `AuditEventType` via `audit_action_to_event_type()`:

| AuditEventType | AuditAction examples |
|----------------|---------------------|
| `UserAction` | Login, CreateProduct, UpdateUnit, DeleteOrder, ResolveConflict, CreateBackup |
| `FiscalEvent` | FiscalYearClosed, FiscalYearArchived, CarryForwardExecuted |
| `SyncEvent` | ImportProducts, ImportNodePackage, UnitNodeImport |
| `IntegrityEvent` | BackupCheckpointFailed, BackupSnapshotIntegrityFailed |

## Dual-Write Semantics

See [audit_dual_write_semantics.md](audit_dual_write_semantics.md).

## Query Optimization

### Keyset Pagination

All new audit query APIs use keyset pagination with `(timestamp, id)` as the pagination key:

```
WHERE (timestamp > ?keyset_ts OR (timestamp = ?keyset_ts AND id > ?keyset_id))
ORDER BY timestamp ASC, id ASC
LIMIT ?limit
```

This provides:
- **Deterministic ordering** — `id` tiebreaker ensures stable results for rows with identical timestamps
- **No OFFSET drift** — keyset pagination is insensitive to insertions/deletions between pages
- **Index-backed** — the `idx_audit_keyset` index covers the pagination key

### Legacy Compatibility

Existing OFFSET-based APIs (`fetch_entries`, `get_audit_entries`) remain unchanged. New code should use `fetch_event_rows_keyset()`.

## Architectural Enforcement

Rules in `check_arch.ts`:

| Rule | Scope | Description |
|------|-------|-------------|
| 69 | Migrations | No destructive audit schema changes (DROP COLUMN, DROP TABLE) |
| 70 | Repository | Every `INSERT INTO audit_log` must include `event_type` (dual-write) |
| 71 | All layers | No direct `audit_log` INSERT outside `AuditRepository` |
| 72 | Repository | New audit queries must use `(timestamp, id)` tiebreaker ordering |
| 73 | Repository | No UPDATE/DELETE on `audit_log` outside cleanup path |

## Future Integrity Roadmap Compatibility

The structured schema is designed to support future integrity hardening:

- **Chain verification** — hash computation remains `(previous_hash, flat_fields)` — structured columns are not included in the hash (they are derived views of the flat data). This ensures backward-compatible chain verification.
- **Merkle sealing** — the deterministic ordering guarantee (`ORDER BY timestamp, id`) creates a stable sequence suitable for future Merkle root computation.
- **Immutability** — no UPDATE/DELETE paths exist outside the explicit `cleanup_old_audit_logs` method.

## Testing

Test file: `tests/audit_schema_evolution_tests.rs` — 15 tests covering:

- Dual-write structured column population and details JSON validity
- Legacy row reconstruction from flat columns only
- Mixed-schema reads (old + new rows)
- Deterministic ordering with timestamp + id tiebreaker
- Keyset pagination across multiple pages
- Reconstruction stability (same row → identical AuditEvent)
- AuditEvent serialization round-trip
- Backward compatibility of legacy OFFSET-based APIs
- Chain verification with dual-write and mixed entries
