# ADR: Domain Event Lifecycle

## الحالة
مقبول

## السياق
Fiscal operations — closures, FIFO consumption, inventory corrections, state transitions — produce side effects that downstream consumers (audit log, sync replication, integrity checks, reporting projections) depend on. Without formal event semantics, these side effects are scattered across service methods, invoked ad-hoc, with no ordering guarantee, no replay contract, and no isolation between success and failure paths.

A Domain Event model must define *when*, *how*, and *in what order* events are emitted, persisted, and delivered — without introducing async infrastructure, middleware, or distributed concepts.

## القرار

### 1. Event Emission Semantics

Events are emitted **by application services only**, at a single point in the workflow: **after domain logic succeeds and before the transaction commits**.

Emission is **synchronous and in-process**. The emitter:
1. Constructs the event struct from domain operation output
2. Appends it to an in-memory `Vec<DomainEvent>` on the active transactional scope
3. Does NOT persist, dispatch, or notify at emission time

No repository, command handler, or domain function emits events.

### 2. Transaction-Scoped Ordering

Every event carries:
- `transaction_id: Uuid` — assigned once per `transactional()` call, identical for all events emitted inside that transaction
- `sequence_number: u64` — monotonically increasing counter scoped to the transaction, assigned in emission order

The authoritative ordering key is `(transaction_id, sequence_number)`.

This pair is deterministically ordered for replay and reconstruction: any two events from any transaction can be compared first by `transaction_id` and then by `sequence_number`.

### 3. Sequence-Number Assignment Timing

The counter is a `u64` stored on the transactional scope, initialized to `1` when the transaction begins. Each call to `emit(event)` increments it atomically (within the same thread) and assigns the pre-increment value.

Assignment happens **before** the event is appended — the sequence number reflects emission order, not persistence order. This guarantees that replay order matches original emission order regardless of SQLite page-level reordering.

No gap is possible: the sequence is contiguous from `1..=N` for a transaction that emits `N` events. Gaps indicate a logic defect or partial write corruption and invalidate replay for that transaction.

### 4. Commit / Rollback Behavior

**On commit:**
1. The scope persists all `N` events to `domain_events` in a single `INSERT` batch, ordered by `sequence_number`.
2. After the SQLite `COMMIT` succeeds, the scope dispatches each event to its registered subscribers (see §6).
3. If dispatch of any event fails, the error is logged and isolated — **no rollback of the committed transaction** (see §10).

**On rollback:**
1. The in-memory event buffer is discarded.
2. No events are persisted.
3. No dispatch occurs.
4. The `transaction_id` is abandoned and MUST NOT be reused.

`sequence_number` resets to `1` on the next transaction. There is no monotonic global sequence — only `(transaction_id, sequence_number)` pairs.

### 5. Persistence Timing

Events are persisted **exactly once**, as part of the originating transaction's commit, in the same SQLite `BEGIN` / `COMMIT` boundary as the domain data they describe.

The event table is a regular SQLite table with no triggers, no foreign-key cascades, and no post-commit hooks. Persistence is:
- **Atomic**: either all events in the transaction are written or none are.
- **Consistent**: event rows match exactly the emitted set.
- **Isolated**: no other transaction sees uncommitted events.
- **Durable**: after `COMMIT`, events survive crash recovery.

### 6. Dispatch Timing

Dispatch occurs **after commit succeeds**, synchronously, in the same thread, before the `transactional()` closure returns. Events are delivered to subscribers **in `sequence_number` order** — the same order they were emitted.

Each subscriber is called sequentially. Subscriber execution order for the same event type is deterministic but not externally specified — subscribers must be commutative with respect to side effects.

### 7. Replay Guarantees

Replaying events from `domain_events` produces an **identical sequence** to the original emission:

- `ORDER BY transaction_id, sequence_number` yields deterministic ordering.
- For a fixed starting state and the same event stream, replay produces identical derived state.
- Replay does NOT re-execute authoritative domain mutations or invariant validation — it re-applies side-effect projections (audit, sync, reporting) to reconstruct projections, derived state, and audit views.

This guarantees:
- **FIFO replay**: event stream mirrors FIFO consumption order exactly.
- **Audit reconstruction**: every fiscal operation is traceable to the events it produced.
- **Sync correctness**: sync packages derived from events have deterministic content.

### 8. Authoritative Ordering Rules

| Aspect | Rule |
|--------|------|
| Global order | `(transaction_id ASC, sequence_number ASC)` |
| Cross-transaction ordering | `transaction_id` Uuid v7 provides temporal ordering |
| Intra-transaction ordering | `sequence_number` — assigned at emission time, contiguous |
| Tie-breaking | Impossible by construction: `(transaction_id, sequence_number)` is unique |
| Replay order | Must match original emission order exactly |

### 9. Best-Effort Subscriber Semantics

Subscribers are **best-effort**: a subscriber failure (panic, error return, timeout) does NOT roll back the transaction. Post-commit dispatch has already committed the events.

Guarantees:
- Events are durably persisted first.
- Each subscriber is invoked at most once per event for a given dispatch attempt.
- If a subscriber fails, the error is logged to `structured_log` with the event's `(transaction_id, sequence_number)` for manual reconciliation.
- No retry mechanism exists at this layer — reconciliation is an operational concern.
- Downstream consumers (sync, audit) are expected to tolerate temporary gaps and catch up via periodic reconciliation queries.

### 10. Failure Isolation Expectations

| Failure location | Effect | Recovery |
|-----------------|--------|----------|
| Emission (pre-commit) | Transaction fails → rollback → buffer discarded | Caller retries operation |
| Persistence (during commit) | SQLite atomicity: all-or-nothing | Standard crash recovery |
| Dispatch (post-commit) | Event committed but subscriber(s) missed it | Logged; operational reconciliation |
| Subscriber panic | Single subscriber lost; others unaffected | Logged; process restarts |

Subscribers MUST NOT throw uncaught panics. If a subscriber panics:
- The panic propagates to the `transactional()` caller.
- Already-delivered subscribers are not rolled back (events are committed).
- Undelivered subscribers are never called for this dispatch attempt.
- On restart, the subscriber may reconcile by scanning for missed events since its last known `(transaction_id, sequence_number)`.

## النتائج المترتبة

- Events are a persistent, ordered, replayable side-effect record — not an in-memory notification mechanism
- `(transaction_id, sequence_number)` is the single source of truth for event ordering across all consumers
- Subscriber failure never corrupts domain state
- Deterministic replay enables FIFO correctness, audit reconstruction, and sync verification
- Synchronous in-process model avoids async complexity while preserving clear semantics
- No EventBus, no middleware, no distributed concepts — only SQLite transactions + ordered dispatch
