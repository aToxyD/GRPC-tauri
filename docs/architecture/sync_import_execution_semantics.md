# Sync Import Execution Semantics

## Overview

This document describes the execution semantics of the sync import pipeline for Phase 5.C of the GRPC-tauri platform. It covers replay safety, rollback guarantees, ordering guarantees, idempotency semantics, deterministic conflict handling, fiscal import atomicity, audit guarantees, and reconciliation invariants.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Import Pipeline Flow                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  SyncImportRequest                                          │
│       │                                                     │
│       ▼                                                     │
│  ┌──────────────┐  (outside transaction)                    │
│  │   Validation  │  read-only, pure checks                  │
│  │    Service    │  wraps ValidationGate + ReplayDetector    │
│  └──────┬───────┘                                           │
│         │ passed?                                            │
│         ├── No ──► ┌──────────────┐                         │
│         │          │  Rejection   │  opens transaction       │
│         │          │   Handler    │  emits SyncConflictDetected│
│         │          └──────────────┘  commits (audit only)    │
│         │                                                    │
│         │ Yes                                                │
│         ▼                                                    │
│  ┌──────────────┐  (single transaction boundary)             │
│  │  Execution   │  with_event_persistence                    │
│  │   Service    │  - double-check replay detection           │
│  │              │  - apply mutations                         │
│  │              │  - emit SyncPackageImported                │
│  │              │  - commit atomically                       │
│  └──────────────┘                                            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Replay Safety

### Principles

1. **Detection BEFORE mutation**: Every import MUST pass replay detection before any mutation is applied. The detection uses `ReplayDetector` from `sync_integrity::replay` with fully deterministic state.

2. **Two-phase detection**:
   - Phase 1 (outside transaction): Quick check via `SyncImportValidationService::validate()` — accesses current DB state to determine if the package was already imported.
   - Phase 2 (inside transaction): Double-check after transaction open. This prevents TOCTOU races and ensures the rejection audit is in the same transaction.

3. **Audit on rejection**: When replay is detected, the rejection MUST be audited via `DomainEvent::SyncConflictDetected` in the SAME transaction. The transaction commits the audit event without applying any mutations.

### Enforcement

- `check_arch.ts` Rule 88: No mutation before replay detection
- `check_arch.ts` Rule 93: Replay rejection must audit
- `check_arch.ts` Rule 96: No `chrono::Utc::now` in sync execution path

## Rollback Guarantees

### Transactional boundaries

- Validation runs OUTSIDE any transaction (read-only)
- Execution runs INSIDE a single transaction via `db.with_event_persistence()`
- No nested transactions: `db.with_event_persistence()` is called exactly once per import
- On failure: the transaction is aborted. No partial state is committed
- On success: the transaction commits, atomically persisting both state changes and domain events

### Failure modes

| Failure | Behavior | Rollback |
|---------|----------|----------|
| Validation failure | Returns error before transaction | N/A |
| Replay detected in transaction | Emits audit event, returns Ok (not Err) | No rollback — audit is committed |
| Mutation error | Returns Err | Full rollback |
| Event emission error | Returns Err | Full rollback |
| Commit failure | Automatic rollback by SQLite | Full rollback |

### Fiscal guarantees

Fiscal imports (kind=`fiscal`) MUST be all-or-nothing:
- No partial fiscal transition application
- If any fiscal mutation fails, the entire import is rolled back
- Audits of fiscal imports are atomic with the import state

## Ordering Guarantees

### Event ordering

Events emitted during execution follow the `(transaction_id, sequence_number)` ordering:
- `transaction_id` is a UUID generated once per `with_event_persistence` call
- `sequence_number` is monotonic starting at 1, assigned by `EventContext::emit()`
- Events are persisted inside the transaction in emission order
- Queries MUST order by `(transaction_id ASC, sequence_number ASC)` — never by `created_at`

### Emission order within a fiscal import

1. `SyncConflictDetected` (if replay detected — before any mutation)
2. State mutations (e.g., fiscal transition application)
3. `SyncPackageImported` (after all mutations succeed)

For a successful import, the order is always: mutations → `SyncPackageImported`.

## Idempotency Semantics

### Package-level idempotency

- `package_id` is the unique identifier for each sync package
- `SyncAppliedPackagesRepository::insert_if_new()` uses `INSERT OR IGNORE` — second insert with same `package_id` returns `false`
- If `insert_if_new()` returns `false`, the import is treated as a duplicate and rejected with `BusinessLogicError::DuplicateSyncPackage`
- The `duplicate_package` check via `ReplayDetector` catches duplicates before transaction open (Phase 1)

### Replay idempotency

- Same `package_id` in the same DB state → same rejection result
- Deterministic `ConflictId` (SHA-256 of `(package_id, conflict_type, evidence)`) ensures reproducible conflict identities

## Deterministic Conflict Handling

### Conflict resolution policies

| Conflict Type | Resolution Policy | Auto/Manual |
|--------------|-------------------|-------------|
| `DuplicatePackage` | `SkipIdempotent` | Auto |
| `StaleImport` | `RejectWithAudit` | Auto |
| `ReplayAttempt` | `RejectWithAudit` | Auto |
| `SequenceGap` | `ManualResolutionRequired` | Manual |
| `DivergentStockState` | `ManualResolutionRequired` | Manual |
| `ConflictingInventoryMutation` | `ManualResolutionRequired` | Manual |

### Policy enforcement

- `SyncConflictResolutionService::resolve()` applies deterministic policies
- NO silent auto-merge for manual-resolution conflicts
- All resolutions are logged via `log::info!` with conflict ID, type, and resolution

## Fiscal Import Atomicity

### All-or-nothing execution

Fiscal packages follow the same transactional model as other packages:
1. Validation (outside transaction)
2. Transaction open
3. Double-check replay detection
4. Apply fiscal mutations (e.g., fiscal transitions)
5. Emit `SyncPackageImported` (with `kind: "fiscal"`)
6. Commit

If any step within the transaction fails, the entire operation rolls back — including fiscal state changes and event emissions.

### Invariant protection

- Fiscal transitions are tracked via `applied_fiscal_transitions` set (UNIQUE enforced at application layer)
- Transition replay is detected by `ReplayDetector::check_transition()`
- Fiscal year validation via `ValidationGate::validate_fiscal_year()` ensures year bounds

## Audit Guarantees

### Events emitted during import

| Scenario | Events Emitted |
|----------|---------------|
| Successful import | `SyncPackageImported` |
| Replay rejected | `SyncConflictDetected` |
| Transaction failure | None (rolled back) |

### Audit invariants

- Every import attempt is recorded as a domain event
- Rejection audits are in the same transaction as the detection
- Audit events are immutable once committed
- Audit events carry deterministic conflict IDs for cross-reference

## Reconciliation Invariants

### Operational sequence continuity

- Operational packages (kind=`operational`) MUST maintain sequence continuity
- `SequenceValidator::check_sequence_continuity()` verifies `incoming == last_applied + 1`
- Sequence gaps cause `SequenceGap` conflict and `ManualResolutionRequired`
- First operational import MUST have `incoming_sequence == 1`

### Stock state reconciliation

- Divergent stock state detection compares Merkle-like fingerprints between incoming package and current state
- Stock state fingerprints are deterministic for identical histories
- Divergent fingerprints require manual resolution

## Key Files

| File | Responsibility |
|------|---------------|
| `application/services/sync_import_execution_service.rs` | Transactional import orchestration |
| `application/services/sync_import_validation_service.rs` | Read-only validation, wraps ValidationGate |
| `application/services/sync_conflict_resolution_service.rs` | Deterministic conflict resolution policies |
| `application/services/sync_import_models.rs` | Import request/result model types |
| `application/sync_integrity/validation.rs` | ValidationGate — pure pre-import checks |
| `application/sync_integrity/replay.rs` | ReplayDetector — deterministic replay detection |
| `application/sync_integrity/conflicts.rs` | ConflictDetector — pure conflict detection |
