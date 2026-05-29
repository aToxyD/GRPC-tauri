# Sync Replay & Transactional Guarantees

## Overview

This document specifies the replay safety and transactional guarantees of the sync import pipeline. It documents how replay detection, transactional atomicity, and deterministic ordering interact to ensure the system's accounting integrity.

## Replay Safety Guarantees

### Deterministic Replay Detection

Every import package carries a globally unique `package_id`. The system tracks all applied package IDs in the `applied_sync_packages` table.

**Replay detection flow:**

1. **Outside transaction**: `SyncImportValidationService::validate()` calls `ReplayDetector::check_all()` which checks:
   - `package_id` not in applied set
   - `transition_id` not in applied transitions set
   - `transaction_id` not in seen transactions set

2. **Inside transaction**: Same checks run again to prevent TOCTOU races between validation and execution.

3. **State-based detection**: All inputs to `ReplayDetector` are provided as immutable state slices (`AppliedPackages`, `AppliedTransitions`, `SeenTransactions`). The detector is a pure function — no mutation.

### Replay Rejection Protocol

```
Validation (outside txn):
  └─ ReplayDetector.check_all() → NoConflict → proceed
  └─ ReplayDetector.check_all() → Conflict → open txn for audit

Transaction:
  └─ ReplayDetector.check_all() → Conflict (double-check)
  └─ EventContext::emit(SyncConflictDetected { ... })
  └─ Commit (audit persisted, no state mutations)
```

Key properties:
- Replay rejection audits are in the SAME transaction as detection
- No partial state is ever committed on replay
- The `ConflictId` is deterministic (SHA-256 hash), enabling cross-node correlation

## Transactional Guarantees

### Single Transaction Boundary

The execution service enforces a strict single-transaction boundary:

```rust
db.with_event_persistence(|ctx: &mut EventContext<'_>| {
    // All operations inside this closure share one SQLite transaction
    // Either all succeed and commit, or all fail and rollback
})
```

**What this guarantees:**
- All state mutations and event emissions are atomic
- No partial import application
- No orphaned events (events never outlive their causal mutations)
- The `transaction_id` (UUID) links all events from the same import

**What this prevents:**
- Nested transaction boundaries (no `with_transaction` inside `with_event_persistence`)
- Hidden savepoints (SQLite savepoints are never created by the import pipeline)
- Cross-service mutation chains (the execution service owns the entire mutation scope)

### Rollback Semantics

| Situation | Behavior | Data Integrity |
|-----------|----------|---------------|
| Validation fails (outside txn) | Return error, no txn opened | No state change |
| Replay detected inside txn | Emit audit event, commit (not rollback) | Audit persisted; no mutations |
| Mutation error | Transaction rollback | No state change, no events |
| Event emission error | Transaction rollback | No state change, no events |
| Commit fails | SQLite rolls back automatically | No state change |
| Crash during commit | SQLite WAL recovery on restart | Either fully committed or fully rolled back |

### Fiscal Import Atomicity

Fiscal packages (those containing fiscal transitions or year-scoped mutations) MUST execute as all-or-nothing:

1. **Pre-validation**: Check fiscal year is open and within allowed range
2. **Transaction open**: Begin SQLite transaction
3. **Replay double-check**: Verify no transition in the package has been applied
4. **Apply fiscal mutations**: Each transition is recorded via `FiscalTransitionRepository`
5. **Emit event**: `SyncPackageImported` with `kind: "fiscal"`
6. **Commit**: All-or-nothing

If any fiscal mutation fails (e.g., transition already applied, fiscal year closed), the entire transaction rolls back.

## Event Ordering Guarantees

### Within-Transaction Ordering

```
transaction_id = uuid_v4 (generated once per db.with_event_persistence)

sequence_number 1: SyncConflictDetected  (if replay detected)
sequence_number 2: [mutation 1]
sequence_number 3: [mutation 2]
sequence_number 4: SyncPackageImported    (after all mutations)
```

Properties:
- `sequence_number` is strictly monotonic within each transaction
- Event ordering reflects the logical order of operations
- `SyncPackageImported` is ALWAYS the last event in a successful import
- `SyncConflictDetected` is ALWAYS the first event (if emitted)

### Cross-Transaction Ordering

Events from different transactions are ordered by `(transaction_id, sequence_number)`:
- `transaction_id` is a UUID — ordering is by its u128 representation
- Within the same transaction, `sequence_number` determines order
- Never order by `created_at` (insufficient resolution)

### Query Guarantees

All event queries from the sync import pipeline:
- Use `ORDER BY transaction_id ASC, sequence_number ASC`
- Never use `ORDER BY created_at` without a `[arch:allow-created-at]` tag
- Never use `OFFSET` pagination — use keyset pagination with `(transaction_id, sequence_number)`

## Idempotency Guarantees

### Package-Level Idempotency

```
First import:  INSERT OR IGNORE applied_sync_packages → true  → apply mutations
Second import: INSERT OR IGNORE applied_sync_packages → false → reject as duplicate
```

The `insert_if_new()` method uses SQLite's `INSERT OR IGNORE`:
- Returns `true` if the row was inserted (first time)
- Returns `false` if the row already exists (duplicate)

### Deterministic Rejection

Same input + same DB state → same output:
- Same `package_id` → same `ConflictId` (SHA-256)
- Same `ConflictId` → same `ResolutionPolicy`
- Same `package_id` + same DB state → same import result

This is verified by integration tests that run the same import twice on the same DB and assert identical results.

## Conflict Resolution Determinism

### Resolution by Conflict Type

| Conflict Type | Resolution | Rationale |
|--------------|------------|-----------|
| `DuplicatePackage` | Skip (idempotent) | Package already applied; no-op is safe |
| `StaleImport` | Reject with audit | Older state should not overwrite newer state |
| `ReplayAttempt` | Reject with audit | Fiscal transitions must not replay |
| `SequenceGap` | Manual resolution | Operator must decide gap handling |
| `DivergentStockState` | Manual resolution | Divergence requires investigation |
| `ConflictingInventoryMutation` | Manual resolution | Overlapping windows require operator judgment |

### No Silent Auto-Merge

Conflicts requiring manual intervention (`ManualResolutionRequired`) are NEVER auto-resolved. The system:
- Logs the conflict with full metadata
- Returns `ConflictResolutionOutcome { resolution: ManualResolutionRequired }`
- Does NOT attempt to merge or reconcile automatically

## Audit Guarantees

### Events in Import Lifecycle

| Phase | Event | Description |
|-------|-------|-------------|
| Replay detected | `SyncConflictDetected` | Logged with conflict_id, type, package_id, details |
| Import succeeded | `SyncPackageImported` | Logged with package_id, kind, source_node_id |
| Transaction failed | (none) | No events — entire operation rolled back |

### Audit Invariant

```
For every import attempt:
  - If validation fails outside txn: no audit (no txn opened)
  - If replay detected: SyncConflictDetected is committed atomically
  - If import succeeds: SyncPackageImported is committed atomically with mutations
  - If transaction fails: nothing is committed
```

### Deterministic Audit IDs

All audit-relevant identifiers (`ConflictId`, `transaction_id`) are deterministic:
- `ConflictId` = SHA-256 of `(package_id, conflict_type, evidence)`
- `transaction_id` = UUID v4 (generated once per transaction scope)

## Key Invariants

### Invariant 1: No Mutation Before Detection

Replay detection MUST complete and pass before any mutation is applied. Enforced by:
- `check_arch.ts` Rule 88
- Two-phase detection (outside + inside transaction)
- `insert_if_new()` called only after both detection phases pass

### Invariant 2: Single Transaction

Exactly one `with_event_persistence` call per import. Enforced by:
- `check_arch.ts` Rule 89
- The execution service's `execute_import()` method structure

### Invariant 3: No Wall-Clock Time

Deterministic execution requires no wall-clock time dependency. Enforced by:
- `check_arch.ts` Rule 96
- All sync services use deterministic sequencing, not timestamps

### Invariant 4: No Raw SQL

All persistence goes through repositories. Enforced by:
- `check_arch.ts` Rule 90
- Existing SQL boundary rules (Rules 1, 81)

### Invariant 5: Fiscal All-or-Nothing

Fiscal imports are never partially applied. Enforced by:
- Single transaction boundary
- Pre-validation of fiscal year bounds
- Replay detection for fiscal transitions

## Files

| File | Purpose |
|------|---------|
| `docs/architecture/sync_import_execution_semantics.md` | Full execution semantics reference |
| This file | Replay & transactional guarantees specification |
| `application/services/sync_import_execution_service.rs` | Implementation of transactional import |
| `application/sync_integrity/replay.rs` | Deterministic replay detector |
| `application/sync_integrity/validation.rs` | Validation gate |
