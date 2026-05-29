# Audit Dual-Write Semantics

## Overview

Every audit write populates three representations simultaneously:

1. **Legacy flat columns** — `action`, `entity_type`, `entity_id`, `old_value`, `new_value`, `user_id`, `username`, `session_id`, `status`, `metadata`
2. **Structured columns** — `event_type`, `actor_id`, `target_type`, `target_id`, `fiscal_year`, `before_snapshot`, `after_snapshot`, `node_id`
3. **Details JSON** — a single `details` column containing a JSON object with all legacy fields

## Write Path

### Service Layer (`AuditService::log_success` / `log_failure`)

```
entry = NewAuditEntry {
    // Flat columns (legacy — always populated)
    action:         action.as_str(),
    entity_type:    entity_type.as_str(),
    user_id:        user_id,
    username:       username,
    old_value:      old_value.to_string(),
    // ...

    // Structured columns (new — always populated in service)
    event_type:     audit_action_to_event_type(&action).as_str(),
    actor_id:       Some(user_id),
    target_type:    Some(entity_type.as_str()),
    target_id:      entity_id,
    fiscal_year:    None,   // set by fiscal-scoped callers
    before_snapshot: old_value,
    after_snapshot:  new_value,
    node_id:         None,  // set by sync-scoped callers
};
entry.details = build_details_from_entry(&entry);
```

The `details` JSON is computed AFTER the flat fields are populated, ensuring it is a faithful snapshot of the legacy representation.

### Repository Layer (`AuditRepository::insert_audit_log`)

The INSERT statement writes all 25 columns:

```sql
INSERT INTO audit_log (
    id, user_id, username, action, entity_type, entity_id, entity_name,
    old_value, new_value, session_id, timestamp, status, error_message, metadata,
    previous_hash, entry_hash,
    event_type, actor_id, target_type, target_id, fiscal_year,
    before_snapshot, after_snapshot, node_id, details
) VALUES (?1, ?2, ..., ?25)
```

## Read Path

### Reconstruction Priority

`to_audit_event()` implements a three-tier reconstruction strategy:

```
if event_type IS NOT NULL:
    → Use structured columns directly
elif details IS NOT NULL:
    → Parse details JSON and extract fields
else:
    → Reconstruct from flat columns (legacy row)
```

### New Readers

Call `fetch_event_rows()` or `fetch_event_rows_keyset()` → get `AuditEventRow` → call `to_audit_event()` → get `AuditEvent`.

### Legacy Readers

Call `fetch_entries()` (unchanged) → get `AuditEntryDbRow` → map to `AuditEntry` (unchanged).

## Deterministic Ordering

### Pagination

New keyset-paginated queries use:

```sql
ORDER BY timestamp ASC, id ASC
```

The `id` tiebreaker guarantees deterministic ordering even when multiple rows share the same timestamp (which is possible with sub-second operations).

### Legacy Methods

Existing OFFSET-based methods (`fetch_entries`, `fetch_entries_iter`) retain their original `ORDER BY timestamp` clauses. These are grandfathered and not subject to the tiebreaker requirement.

## Integrity

### Hash Chain

The existing `compute_entry_hash()` function hashes only the flat legacy fields — NOT the structured columns or details JSON. This ensures:

- **Backward compatibility** — existing chain verification works on all rows, regardless of when they were written
- **Semantic correctness** — structured columns are derived views of the flat data; including them in the hash would create circular dependency
- **Determinism** — the hash remains stable across schema upgrades

### Chain Verification

`verify_audit_chain_streaming()` continues to work unchanged. It reads flat columns only via `ORDER BY rowid ASC`, which is unaffected by the new columns.

## Enforcement

Architecture rules in `check_arch.ts` enforce:

- **Rule 70**: Every `INSERT INTO audit_log` in the repository must include `event_type` in its column list
- **Rule 71**: No direct `INSERT INTO audit_log` outside `AuditRepository`
- **Rule 73**: No `UPDATE`/`DELETE` on `audit_log` outside the explicit cleanup path
