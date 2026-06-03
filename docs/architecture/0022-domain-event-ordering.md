# ADR-002: Domain Events Are Ordered by (transaction_id ASC, sequence_number ASC)

## Status
Accepted

## Context

Domain events are the backbone of sync replication, fiscal replay, and audit trail reconstruction in the Algerian Civil Protection food-service management system. Events are emitted by application services during transactional operations (stock movements, fiscal-year transitions, sync imports, inventory corrections) and persisted to the `domain_events` table.

The system must guarantee that replay of domain events across different nodes produces identical state. If events are ordered by `created_at` — a wall-clock timestamp that can drift between nodes, be affected by clock corrections, or collide under high concurrency — same-origin event streams could replay in different orders on different nodes. This would cause irreconcilable state divergence between units in the same WILAYA, break fiscal closure consistency, and invalidate the sync protocol's causal ordering guarantees.

## Decision

Domain events are ordered by `(transaction_id ASC, sequence_number ASC)`. This ordering is authoritative and enforced at every level:

1. **Event emission**: `EventBuffer` in `src-tauri/src/domain/events/mod.rs` assigns monotonically increasing `sequence_number` values starting at 1 within each transaction. Every event in the same buffer shares the same `transaction_id` (a UUIDv4 assigned at buffer creation). The `(transaction_id, sequence_number)` pair is immutable after assignment.

2. **Validation**: `validate_replay_ordering()` in `src-tauri/src/domain/events/mod.rs` checks that a slice of `StoredEvent` respects the canonical ordering. An independent function `validate_transaction_sequences()` verifies that sequence numbers within each transaction are contiguous with no gaps.

3. **Storage and query**: `DomainEventRepository` queries events with `ORDER BY transaction_id ASC, sequence_number ASC`. No query on the `domain_events` table uses `ORDER BY created_at` alone.

4. **Architecture rule enforcement**: Rule 47 in `scripts/check_arch.ts` scans all Rust source files for `ORDER BY created_at` on the `domain_events` table and flags it as an error. The only exceptions are explicit `[arch:allow-created-at]` tags for legacy compatibility queries.

The `StoredEvent` struct carries `sequence_number`, `transaction_id`, `category`, and the `DomainEvent` body. The `(transaction_id, sequence_number)` pair is the event's identity within the ordered event stream.

## Consequences

**Easier:**
- Deterministic replay: given the same set of events in the same transaction scope, any node produces byte-identical replay output. This is critical for sync package application across the WILAYA hierarchy.
- Gap detection: contiguous sequence numbering makes it trivial to detect missing events (`validate_transaction_sequences` catches non-contiguous sequences immediately).
- Transactional atomicity: the `EventBuffer` is created per transaction and dropped on rollback — events outside a committed transaction never enter the ordered stream.
- Cross-node sync: causal ordering is preserved regardless of wall-clock differences between sources.

**Harder:**
- `created_at` cannot be used as a shorthand ordering column in domain event queries, even though it is present on the table as an informational field.
- Debugging and log inspection are slightly harder because event timeline must be inferred from `transaction_id` order rather than wall-clock time.
- Sequence number allocation requires the `EventBuffer` to track state per transaction, adding a small memory overhead during long-running transactional scopes.

## Compliance

Enforced by:

- `validate_replay_ordering()` and `validate_transaction_sequences()` in `src-tauri/src/domain/events/mod.rs` — called in tests and optionally in production as a pre-commit invariant check.
- Rule 47 in `scripts/check_arch.ts` — automated CI check that `ORDER BY created_at` is never used on the `domain_events` table without an `[arch:allow-created-at]` exemption.
- `DomainEventRepository` in `src-tauri/src/repositories/domain_events.rs` — all queries use `ORDER BY transaction_id ASC, sequence_number ASC`.
- Unit tests in `events/mod.rs` (`validate_replay_ordering_accepts_correct_order`, `validate_replay_ordering_rejects_reversed_transaction`, `buffer_sequence_is_contiguous`).

Any use of `ORDER BY created_at` on `domain_events` is caught by Rule 47 in CI and rejected.
