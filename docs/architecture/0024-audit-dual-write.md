# ADR-004: Audit Schema Evolution Uses Additive-Only Dual-Write

## Status
Accepted

## Context

The audit subsystem in the Algerian Civil Protection food-service management system must serve both legacy and modern consumers across the WILAYA hierarchy. The original `audit_log` schema used flat columns (`user_id`, `username`, `action`, `entity_type`, `entity_id`, `old_value`, `new_value`, etc.) with a free-form `metadata` JSON field. As the system evolved to support fiscal lifecycle events, sync conflict tracking, and structured governance queries, the flat schema became insufficient: querying for fiscal-year-specific audit events required JSON parsing of the `metadata` field, and the absence of typed actor/target columns made WILAYA-level oversight queries slow and error-prone.

A full schema migration (ALTER TABLE DROP COLUMN, column renames, type changes) would break backward compatibility with existing deployments across dozens of Civil Protection units. Many units run offline for extended periods; a destructive migration would prevent safe sync of audit data between nodes on different schema versions. The system needs to evolve the audit schema without breaking existing readers, writers, or sync protocols.

## Decision

Audit schema evolution uses additive-only dual-write. The approach has three pillars:

1. **Structured columns added gradually**: Migration `004_audit_schema_evolution.sql` adds new columns as NULLABLE with defaults: `event_type`, `actor_id`, `target_type`, `target_id`, `fiscal_year`, `before_snapshot`, `after_snapshot`, `node_id`, `details`. These columns coexist with all legacy flat columns. No existing column is altered or dropped.

2. **Dual-write on insertion**: The `NewAuditEntry` struct in `src-tauri/src/domain/audit.rs` carries both the legacy flat fields (which continue to be populated) and the new structured fields (`event_type: Option<String>`, `actor_id: Option<String>`, etc., up to `details: Option<String>`). The `AuditRepository::insert_audit_log()` INSERT statement writes all 25 columns in a single row, ensuring both representations are populated atomically. The `build_details_from_entry()` function also produces a canonical JSON `details` blob from the legacy entry fields, preserving backward compatibility for readers that parse the `details` column.

3. **Gradual reader migration**: The `to_audit_event()` function in `src-tauri/src/domain/audit.rs` implements a three-tier reconstruction strategy:
   - Tier 1: If `event_type` is populated (new structured row), reconstruct `AuditEvent` directly from structured columns.
   - Tier 2: If only `details` JSON is present, parse it to populate the structured fields.
   - Tier 3: For fully legacy rows, reconstruct from flat columns via `build_details_json()`.

   This tiered approach means existing deployments retain full read capability for all audit data regardless of which columns are populated. New readers can consume structured columns immediately; old readers continue to work via the legacy paths indefinitely.

## Consequences

**Easier:**
- Zero-downtime schema evolution: no destructive ALTER on production databases, no sync protocol breakage between nodes on different schema versions.
- Incremental adoption: services can be migrated to populate structured columns one at a time; the dual-write ensures data is always available in at least one representation.
- Backward compatibility: any existing audit reader continues to work without modification. The `AuditEvent` projection layer transparently handles all three row formats.
- Offline safety: units that sync infrequently can upgrade at their own pace; the additive schema is always forward-compatible.

**Harder:**
- INSERT statements are wider (25 columns vs. 16 legacy columns), increasing storage slightly and making the repository code more verbose.
- The `to_audit_event()` function has three code paths, increasing testing surface. Each tier must be tested for correctness and round-trip fidelity.
- Developers adding a new structured column must remember to update the dual-write in `NewAuditEntry`, the INSERT statement, and the `to_audit_event()` reconstruction, plus the `AuditEventRow` struct.
- Legacy flat columns remain populated indefinitely — they cannot be removed. This means ongoing maintenance cost for code paths that are no longer the primary representation.
- The `details` JSON column is populated alongside structured columns, meaning the same data may be stored twice (once in structured columns, once serialized into `details`). This is an explicit trade-off for backward compatibility.

## Compliance

Enforced by:

- Rule 69 in `scripts/check_arch.ts`: no destructive audit migrations (`DROP TABLE audit_log`, `ALTER TABLE ... DROP COLUMN`). Any migration affecting the `audit_log` table must be additive only.
- Rule 70: every `INSERT INTO audit_log` must include the `event_type` column. The architectural checker scans the INSERT statement in `audit.rs` to verify that structured columns are present.
- Rule 71: no direct `INSERT INTO audit_log` outside `AuditRepository`. All audit writes flow through the repository, ensuring the dual-write INSERT is always used.
- Rule 72: audit readers must not depend on unstable ordering — `ORDER BY timestamp` must include an `id` tiebreaker (`ORDER BY timestamp, id ASC`).
- Rule 73: no `UPDATE` or `DELETE` on `audit_log` outside the cleanup path (`delete_older_than`). Mutations to audit data are prohibited to preserve the audit trail's immutability.

Migration `004_audit_schema_evolution.sql` in `src-tauri/src/db/migrations/` is the source of truth for the additive schema evolution. Any future audit column addition must follow the same pattern.
