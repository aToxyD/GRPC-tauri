# ADR-001: FIFO Consumption Order Is Deterministic

## Status
Accepted

## Context

The Algerian Civil Protection food-service management system must guarantee that identical input data always produces identical FIFO consumption order and cost allocation. Non-deterministic FIFO behavior would cause inventory valuation discrepancies between nodes in the same WILAYA, undermine fiscal-year closure integrity, and break the reproducibility requirements for audit and oversight reporting.

The system tracks inventory through FIFO (First-In-First-Out) layers, where each supplier order receipt or opening balance creates a layer with a received-at timestamp, unit cost, and remaining quantity. When consumption occurs — whether for meal preparation (breakfast, lunch, dinner) across Civil Protection units — the system must decide which layers to consume from and in what order. Without a strict deterministic ordering rule, the same sequence of inbound stock events could produce different cost allocations depending on database insertion order, query plan variance, or index fragmentation.

## Decision

FIFO consumption order is defined as `received_at ASC, id ASC`. Within the same received-at timestamp, the `id` field (UUID, assigned at layer creation) acts as a deterministic tiebreaker. This ordering is enforced at three levels:

1. **Domain model invariant**: `FifoOrderStable` in `src-tauri/src/domain/invariants/fifo.rs` checks that every consumption operation respects `received_at ASC, id ASC`. The invariant verifies that no newer layer is consumed while an older layer retains positive remaining quantity. Violations produce `FifoOrderStableViolation` records.

2. **Pure consumption engine**: `simulate_fifo_consumption` in `src-tauri/src/domain/fifo_engine.rs` is a pure function operating on an ordered slice of `FifoLayerRow` tuples. It never queries the database or depends on external state. The ordering of the input slice is established before the function is called.

3. **Repository ordering**: `FifoLayerRepository` in `src-tauri/src/repositories/fifo_layers.rs` always queries layers with `ORDER BY received_at ASC, id ASC`. The SQL-level ordering guarantees that the input to the simulation engine is already in canonical FIFO order.

The `ConsumerLayerPortion` output carries `layer_id`, `quantity`, `unit_cost`, and `total_cost`. Every consumed item in a meal section traces back to exactly one FIFO layer, and the portion vector is itself ordered from oldest to newest consumed layer.

## Consequences

**Easier:**
- Reproducible cost allocation: the same set of layers and the same consumption quantity always produces identical portion vectors and total costs, enabling deterministic report output.
- Audit verifiability: auditors in the WILAYA oversight chain can independently replay consumption and verify that the correct layers were consumed in the correct order.
- Cross-node consistency: sync packages replayed on different nodes produce identical FIFO layer impact, eliminating a class of sync-induced valuation drift.
- Simplified testing: the pure `simulate_fifo_consumption` function is tested in isolation without database setup.

**Harder:**
- Any future ordering change (e.g., adding a custom layer priority field) would require coordinated changes across all three enforcement levels and a migration of existing consumption records.
- The `id` tiebreaker means that within the same microsecond, consumption order is determined by UUID lexical order, which is arbitrary but deterministic.
- Performance of FIFO layer queries depends on the `(received_at, id)` composite index being present and maintained.

## Compliance

Enforced by:

- `FifoOrderStable` domain invariant in `src-tauri/src/domain/invariants/fifo.rs` — checked via `check_fifo_consumption_order()` which is called in the `FifoEngine` application service after every consumption operation.
- Pure function contract of `simulate_fifo_consumption` in `src-tauri/src/domain/fifo_engine.rs` — no side effects, no database access, no wall-clock dependency.
- The `src-tauri/src/domain/accounting/fifo.rs` model defines `ConsumedLayerPortion` and the layer row type; layer source classification (`Order` vs `Opening`) preserves ordering semantics across fiscal-year boundaries.
- FIFO layer ordering tests in both `fifo_engine.rs` (`consumes_oldest_layer_first`, `spans_multiple_layers`) and `fifo.rs` invariants (`oldest_layer_consumed_first_is_valid`, `skipping_older_layer_triggers_violation`).

Any violation of `received_at ASC, id ASC` ordering in consumption is caught as an invariant violation before the consumption transaction commits.
