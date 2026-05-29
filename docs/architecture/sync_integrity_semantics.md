# Sync Integrity Semantics

## Overview

The `sync_integrity` module provides a governance-grade deterministic sync
integrity layer. It is separate from the `sync` transport module and contains
no infrastructure, SQL, or wall-clock dependencies.

## Design Principles

1. **Determinism** — Same input + same state = same output. Guaranteed by
   SHA-256 conflict IDs, explicit ordering, and pure functions.
2. **No Mutation** — All detection and validation functions are read-only.
   Validation gates run BEFORE any data mutation.
3. **Reproducibility** — Every conflict decision can be exactly reconstructed
   from its inputs. Conflict IDs are hashes of the evidence, not random UUIDs.
4. **Auditability** — Every conflict carries reproducible metadata including
   the responsible package, source/target nodes, fiscal scope, and evidence
   chain.
5. **Governance** — Conflict detection is not optional. The validation gate
   must be called before every import. There is no best-effort deduplication.

## Conflict Types

| Variant | Detection | Fiscal Scope |
|---------|-----------|-------------|
| `DivergentStockState` | Stock fingerprint hash mismatch | Product's fiscal year |
| `ConflictingInventoryMutation` | Overlapping mutation windows for same product from different sources | Current fiscal year |
| `StaleImport` | Incoming sequence <= last applied sequence per source | Package fiscal year |
| `DuplicatePackage` | Package ID already in applied set | Package fiscal year |
| `ReplayAttempt` | Transition ID or transaction ID already applied | Fiscal transition year |
| `SequenceGap` | Incoming sequence != expected next | Import context |

## Conflict Metadata

Every `SyncConflict` carries:

- **`conflict_id`** — Deterministic SHA-256 hash of `(package_id, conflict_type, source_node, evidence)` — NOT a random UUID. Same conflict detected twice produces identical ID.
- **`package_id`** — The package that triggered the conflict.
- **`source_node_id`** / **`target_node_id`** — Replication topology identifiers.
- **`explanation`** — Machine-readable label + human-readable description.
- **`fiscal_scope`** — Affected fiscal year(s), if known.
- **`evidence`** — List of deterministic identifiers proving the conflict (e.g. `sequence:42`, `transition_already_applied:txn-100`).

## Module Structure

```
sync_integrity/
  mod.rs           — Module re-exports
  types.rs         — All domain types (SyncConflict, ConflictMetadata, etc.)
  replay.rs        — Replay detection primitives (package, transition, transaction)
  conflicts.rs     — Conflict detection logic (stale, divergent, conflicting)
  reconciliation.rs — Reconciliation comparers (sequence windows, lineages, fingerprints)
  sequencing.rs    — Sequence validation (gaps, duplicates, continuity)
  validation.rs    — Validation gate (orchestrates pre-import checks)
```

## Deterministic Conflict ID Generation

A `ConflictId` is computed as `SHA-256(concatenated_input)` where the input
includes:

- `package_id`
- `conflict_type_label`
- `source_node_id`
- All evidence strings joined

This guarantees:
- Same conflict → same ID (reproducibility)
- Different input → different ID (collision resistance)
- No wall-clock or random input (determinism)

## Deterministic Ordering

All ordering uses explicit `ORDER BY`-equivalent rules:

1. Transaction comparisons: sort by `(transaction_id ASC, sequence_number ASC)`
2. Fingerprint comparisons: sort by `product_id ASC`
3. Sequence validation: sort ascending by default
4. Fiscal lineage comparisons: preserve insertion order (deterministic by construction)

Never relies on:
- HashMap iteration order
- Timestamps alone
- Insertion order without explicit sequence

## Replay Safety Model

See `replay_safety_guarantees.md` for the full replay safety model.

## Reconciliation Consistency

Reconciliation compares deterministic snapshots:

- **Sequence windows** — Compare by transaction ID and sequence range
- **Fiscal transition lineages** — Compare ordered lists of transition IDs
- **Stock state fingerprints** — Compare per-product Merkle-like hashes
- **Event ordering** — Compare per-transaction sequence arrays

All comparisons are:
- Deterministic (same input = same output)
- Pure (no mutation, no side effects)
- Ordered explicitly (BTreeMap for product-level, BTreeSet for ID-level)

## Testing Requirements

Every test in the module verifies:
1. Deterministic output for same input + same state
2. Different output for different input
3. Serialization round-trip stability
4. No panic on edge cases (empty sequences, zero values)
5. Boundary conditions (sequence at 0, max values, fiscal year edges)
