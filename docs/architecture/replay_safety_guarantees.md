# Replay Safety Guarantees

## Model

The replay safety model is built on three layers of detection:

```
Layer 1: Package Identity
  └─ package_id uniqueness (SHA-256 of package metadata)
  └─ Check: is this package_id already in the applied set?

Layer 2: Transition Uniqueness
  └─ fiscal transition ID uniqueness
  └─ Check: is this transition_id already in the applied set?

Layer 3: Transaction Replay
  └─ per-transaction sequence tracking
  └─ Check: has this (transaction_id, sequence_number) been seen before?
```

## Guarantees

### 1. Replay Rejection is Deterministic

Given:
- Same `AppliedPackages` set
- Same `AppliedTransitions` set
- Same `SeenTransactions` set
- Same incoming `package_id`

→ Same `ConflictDetectionOutcome` every time.

This is guaranteed by:
- Using `BTreeSet` for deterministic iteration order
- SHA-256 hashing for conflict IDs (no randomness)
- Pure functions with no wall-clock dependency

### 2. Replay Rejection is Auditable

Every rejected replay produces a `SyncConflict::ReplayAttempt` or
`SyncConflict::DuplicatePackage` containing:

- `conflict_id` — deterministic hash for cross-reference
- `package_id` — rejected package identifier
- `source_node_id` / `target_node_id` — topology trace
- `explanation` — human-readable rejection reason
- `evidence` — specific identifiers proving the replay
- `fiscal_scope` — affected fiscal context

### 3. Replay Detection Occurs BEFORE Mutation

The `ValidationGate` in `validation.rs` enforces this ordering:

```
1. check_package_id()        ← replay detection (FIRST)
2. check_transition()        ← transition uniqueness
3. check_transaction()       ← transaction replay
4. check_sequence()          ← sequence continuity
5. [FISCAL SCOPE CHECK]      ← fiscal consistency
6. [MUTATION]                ← only if ALL checks pass
```

If any check in steps 1-5 fails, the mutation (step 6) is never reached.
The `PreImportValidationResult` carries which check failed and why.

### 4. No Best-Effort Deduplication

The system does NOT accept-then-deduplicate. Every import must pass the
full validation gate before any data is written.

Rationale:
- Accounting correctness requires deterministic import acceptance
- Best-effort deduplication can mask replay attacks
- Audit trail must show exactly which packages were accepted and rejected
- Partial deduplication produces inconsistent state across nodes

### 5. Conflict Detection is Transaction-Free

The detection engine does not open transactions. It operates on immutable
state snapshots supplied by the caller. This ensures:

- No nested transaction risks
- No partial state during detection
- Clean separation between detection and execution

## State Inputs

The `ReplayDetector` requires three state collections:

| Collection | Source | Contents |
|-----------|--------|----------|
| `AppliedPackages` | DB query | All previously imported package IDs |
| `AppliedTransitions` | DB query | All applied fiscal transition IDs |
| `SeenTransactions` | DB query | All known transaction IDs or (txn_id, seq) pairs |

These are supplied as `BTreeSet<String>` — deterministic, ordered,
no mutation inside the detector.

## Fiscal Transition Replay

Fiscal transitions are all-or-nothing:

- A transition is uniquely identified by its transition ID
- Once applied, a transition ID is added to `AppliedTransitions`
- Any subsequent import referencing the same transition ID is rejected as a `ReplayAttempt`
- No partial fiscal package application is possible (the transition is either fully applied or not)

## Operational Sequence Continuity

Operational-order imports (consumption, stock movements) are sequentially
dependent:

- Each incoming sequence is checked against the last applied sequence
- If `incoming_sequence == last_applied + 1` → contiguous
- If `incoming_sequence > last_applied + 1` → gap detected
- If `incoming_sequence <= last_applied` → duplicate/replay detected

A gap at the operational level blocks ALL further operational imports
for that source until the gap is resolved.

## Governance Rationale

1. **Determinism over convenience** — A non-deterministic replay detector
   would produce different results on different nodes for the same input,
   making reconciliation impossible.

2. **Auditability over silence** — Silent deduplication hides replay
   attempts. Every rejection is recorded and traceable.

3. **Correctness over performance** — Detection before mutation means
   every write to the database is guaranteed valid. This is more
   important than import throughput.

4. **Explicit over implicit** — All state inputs are explicitly passed.
   No hidden global state. No implicit dependencies.

## Enforcement

Architectural rules (check_arch.ts 81-87):

- Rule 81: No SQL in sync_integrity/ (except repository layer)
- Rule 82: No `chrono::Utc::now()` in sync_integrity/
- Rule 83: No mutation during validation phase
- Rule 84: Replay checks must occur before mutation
- Rule 85: No OFFSET pagination in reconciliation paths
- Rule 86: All sync ordering requires deterministic tiebreakers
- Rule 87: No direct `domain_events` inserts
