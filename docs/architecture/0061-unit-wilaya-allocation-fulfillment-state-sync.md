# ADR 0061: UNIT → WILAYA Allocation-Level Fulfillment State Synchronization

## Decision Status

**ACCEPTED — 2026-09-26.**

This ADR ratifies a new UNIT-issued sync package kind, `contract_fulfillment`, that
propagates the **current cumulative fulfillment state** of a `ContractAllocation`
from the executing UNIT to the WILAYA, and a WILAYA-owned derived projection
`wilaya_executable_remaining`.

| Item | Status |
|------|--------|
| `FulfillmentFact` is an **allocation-level cumulative state snapshot**, not a historical event | **ACCEPTED** |
| Synchronization identity = `allocation_id` + absolute cumulative `fulfilled_quantity` | **ACCEPTED** |
| `order_id` / `order_item_id` / random `fact_id` are **NOT** part of the authoritative identity | **ACCEPTED** |
| Historical per-order fulfillment causality is **NOT** reconstructed (intentional) | **ACCEPTED** |
| `fulfilled_quantity` is denominated in **purchase units** | **ACCEPTED** |
| Unit triple carried as a **fail-closed cross-check** only | **ACCEPTED** |
| Fiscal attribution = `contract_allocations.fiscal_year`, cross-check only, never an application input | **ACCEPTED** |
| Fulfillment resolves against the **original** `allocation_id` regardless of `entitlement_state` | **ACCEPTED** |
| Export = **complete current set** of eligible allocation states (deliberate; no watermark, no window) | **ACCEPTED** |
| Idempotency = content-derived `package_id` + guarded monotone absolute-state `SET` | **ACCEPTED** |
| Import is **all-or-nothing**, fail-closed, validated entirely before any mutation | **ACCEPTED** |
| `wilaya_executable_remaining = contracted − fulfilled − released` on the WILAYA projection | **ACCEPTED** |
| `effective_remaining` semantics **unchanged** and out of scope | **ACCEPTED** |
| `reserved_quantity` **never** included in the WILAYA value | **ACCEPTED** |
| Reuse `IdentitySignedExportService` + existing V3 crypto; no parallel subsystem | **ACCEPTED** |
| No schema change, no migration, no new table, no new column, no V4 | **ACCEPTED** |

## Date

2026-09-26 — accepted by the governance reviewer (ADR-0061).

## Owner

Architecture / Security / Sync

## Context

ADR-0055 made `Contract` + `ContractAllocation` the entitlement authority and gave
the WILAYA authoritative read access to allocation state through the single
WILAYA-owned projection `Action::ReadContractProjection`. ADR-0059/0060 fixed the
ContractCatalog producer direction (WILAYA → UNIT). What did **not** exist is the
reverse propagation of *execution* state: UNIT order confirmation increments
`contract_allocations.fulfilled_quantity` on the UNIT, and the WILAYA's
`contract_allocations.fulfilled_quantity` stays `0` forever.

`upsert_sync_allocation` cannot carry it, by design: its `ON CONFLICT` clause omits
`fulfilled_quantity` and `reserved_quantity`, and the WILAYA refuses ContractCatalog
import entirely. This is intentional column ownership, not a gap to be closed by
widening ContractCatalog.

Two further facts shaped this decision and are recorded because they constrain it.

**Fact 1 — at the WILAYA, `reserved_quantity` and `fulfilled_quantity` are
structurally `0`.** Every production writer of either column sits behind
`resolve_order_unit`, which rejects order creation on any non-UNIT node. Therefore
the WILAYA's current `effective_remaining` already equals `contracted − released`:
the visible symptom is **not** an over-subtraction of `reserved_quantity`, it is
that `fulfilled_quantity` never reflects execution. Adding an explicit WILAYA value
is consequently a **defensive/clarifying** measure that makes the semantics robust
rather than resting on a negative invariant.

**Fact 2 — the repository does not persist a durable historical fulfillment
ledger.** See §"Why the event model was rejected". Per-order attribution is an
inference, not authoritative state.

## Decision

### 1. `FulfillmentFact` is an allocation-level cumulative state

> **`FulfillmentFact` represents the current cumulative fulfillment state of a
> `ContractAllocation`, as known by the UNIT at export time.**

It is a **state snapshot**, not an event. The export emits **one fact per
allocation** whose current `fulfilled_quantity > 0`.

```text
FulfillmentFactExportDataset {
    fact_version: u16,               // fail-closed gate — see §4 layer 1
    facts: Vec<FulfillmentFact>,     // ORDER BY allocation_id ASC
}

FulfillmentFact {
    allocation_id:      String        // the synchronization identity
    fulfilled_quantity: f64           // ABSOLUTE cumulative, PURCHASE units, scale-3
    fiscal_year:        i32           // cross-check only — never an application input
    purchase_unit:      Option<i32>   // unit semantics — cross-check only
    conversion_factor:  Option<i32>   // unit semantics — cross-check only
}
```

The envelope carries the existing `SyncPackageMetadata` unchanged, with
`export_mode: None`, `target_node_id: None`, `signature_version: Some(2)`, and a
**content-derived** `package_id = hex(sha256(canonical_json(dataset)))`.

If three supplier orders contributed `+20 / +15 / +25` to allocation A, the export
contains **one** fact (`A → fulfilled_quantity = 60`), not three.

### 2. The authoritative synchronization identity

```text
allocation_id  +  cumulative fulfilled_quantity
```

It is **not** a random event UUID, and **not** a `SupplierOrder` id.

### 3. Historical order attribution is intentionally not reconstructed

The repository cannot prove which portion of an allocation's current cumulative
`fulfilled_quantity` a given order caused:

| # | Finding | Evidence |
|---|---------|----------|
| 1 | `supplier_order_item_allocations` is a **pricing/reservation** record, not a fulfillment ledger (own module doc: *"Persists the authoritative price leg"*) | `repositories/order_allocations.rs:3-7` |
| 2 | It stores no fulfilled-quantity, no confirmation timestamp, no fulfilled flag; `quantity` is written at **Draft creation**, before any fulfillment exists | `001_initial.sql:482-495` |
| 3 | Confirmation never writes the leg; it updates only the allocation row | `order_service.rs:684-685` |
| 4 | A **fulfillment-without-leg** path exists (`try_increment_fulfilled`) with zero production callers and zero test references | `repositories/contracts.rs:738-753` |
| 5 | Legs are hard-`DELETE`d; only a service-layer guard blocks deleting a Confirmed order's legs — there is no DB constraint | `repositories/order_allocations.rs`; guard `domain/validation.rs:773-782` |
| 6 | `supplier_orders.deleted` / `supplier_order_items.deleted` soft-delete flags are independent of leg lifecycle | `001_initial.sql:313-345` |
| 7 | No DB constraint, trigger, or test asserts `sum(confirmed legs) == fulfilled_quantity` | repo-wide |

The leg→allocation link itself *is* reliable: confirmation **hard-fails** when the
re-resolved allocation differs from the leg's recorded `allocation_id`
(`order_service.rs:661-669`). But reliability of a link is not durability of a
ledger, and finding #4 means the equality is not even guaranteed in principle.

**Therefore this feature does not claim, reconstruct, or transmit per-order
fulfillment causality.** The WILAYA receives authoritative *state*, not *history*.
Historical attribution remains available locally on the UNIT through
`supplier_order_item_allocations` + `stock_movements` and is deliberately not
synchronized.

`order_id` and `order_item_id` are consequently **absent from the payload**. At
allocation granularity a single fact may summarize any number of orders, so a
single per-order field could only ever misrepresent the fact set. No
"optional non-authoritative provenance" variant is adopted: at allocation
granularity *any* per-order field implies causation regardless of its label.

### 4. Idempotency — four independent state-based layers

| Layer | Mechanism | Prevents |
|-------|-----------|----------|
| Payload schema | `fact_version` fail-closed gate (mirrors the existing `signature_version` gate) | silent misparse of a future schema |
| Content identity | `package_id` recomputed from the dataset and compared to the declared one | a declared identity that does not describe the content |
| Package replay | content-derived `package_id` → `applied_sync_packages` PK | re-import of an identical fact set |
| **Per-fact effect** | guarded monotone conditional `SET` on the allocation row | re-application of an individual fact |

Layers 1 and 2 are enforced in the **reader** (`contract_fulfillment_from_reader`),
so an unsupported version or a mismatched identity is rejected before the import
pipeline performs any database access. The application-layer validator
(`validate_contract_fulfillment_package_for_import`) re-checks both for use-case
callers that bypass the reader.

Layer 3, adapted to repository conventions (scale-3 `INTEGER` via
`numeric_row::qty_scaled`, mirroring `contracts.rs:789`):

```sql
UPDATE contract_allocations
   SET fulfilled_quantity = ?absolute,
       version = version + 1
 WHERE id = ?allocation_id
   AND deleted = 0
   AND fulfilled_quantity < ?absolute
   AND (fulfilled_quantity + released_quantity + reserved_quantity) <= contracted_quantity
```

Why this is genuinely per-fact rather than per-package:

- **Idempotent by construction.** The post-state is a function of
  `(?absolute, ?allocation_id)` only; the prior value is irrelevant. Applying a
  fact twice is *indistinguishable* from applying it once.
- **Order-insensitive.** `fulfilled_quantity` is strictly monotone — all three
  writers are `= fulfilled_quantity + ?1` and no decrement writer exists. The
  predicate holds exactly when the destination is behind the fact, so any
  permutation or subset of a fact set converges to the same value.
- **Already-applied is a no-op, not an error.** After applying, the predicate is
  false → 0 rows → classified `AlreadySatisfied`.
- **Gaps self-heal.** Each absolute already includes its predecessors'
  contributions, so an omitted intermediate fact is subsumed by any later one.

**Honest characterization.** The guarantee is *at-least-once delivery +
at-most-once effect + order-insensitive monotone convergence*. It is **not**
"exactly-once application". A fact whose absolute is `≤` the current value is
silently skipped and is **indistinguishable from already-applied**; the mechanism
cannot detect a genuinely lost fact whose successors were also lost. §6 explains
why this is acceptable here and why it is a coupled decision.

### 5. Unit semantics — purchase units, proven from the write path

`fulfilled_quantity` is **purchase-denominated**. Proof from the live write path,
not assumption: `order_service.rs:685` passes the *purchase* `quantity` to
`try_convert_reserved_to_fulfilled`, and the purchase→consumption conversion
happens strictly **downstream** at `:733` (`ReceiptConversion::to_receipt`,
`domain/units.rs:354-378`). `try_convert_reserved_to_fulfilled` then writes
`fulfilled_quantity = fulfilled_quantity + ?1` with that same scale-3 value.
Corroborated by the SEC-087 schema comment (*"`quantity` is the purchase
quantity"*, `001_initial.sql:336-340`) and `models/order.rs:66-67`.

`contract_allocations` carries **no unit column**; the unit is implicit via
`contract_products.purchase_unit / conversion_factor`. The fact therefore states
`purchase_unit` + `conversion_factor` explicitly and the importer **fails closed**
on mismatch against the destination's `contract_products` row. A `NULL` unit with a
`NULL` factor is treated as factor `1`, consistent with
`OrderUnitSnapshot::effective_factor` (`domain/units.rs:337-340`).

No unit snapshot is added to `contract_allocations`; the authoring-side gap
remains open and is recorded as a known limitation (§7.5).

### 6. Fiscal-year attribution

Attribution is `contract_allocations.fiscal_year`, which ADR-0055 copies from
`contract.fiscal_year` at allocation creation and which is the ordering key of
`idx_contract_allocations_obligation`. Old-obligation fulfilment across a fiscal
boundary is existing, intended behaviour: `domain/pricing/resolver.rs:115` admits
`allocation_fiscal_year <= current_fiscal_year`, so an FY-N order may fulfil an
FY-(N−1) allocation and increments **that older** allocation's `fulfilled_quantity`
— exactly the X → Y accounting ADR-0055 §3.5 exists to serve.

The fact's `fiscal_year` is a **cross-check, never an application input**. A stale
or wrong value can only fail the check; it can never corrupt attribution. No new
fiscal-year concept is introduced.

### 7. Released / inactive allocations

Fulfillment resolves against the **original** `allocation_id`. Resolution filters
`deleted = 0` only and **never** filters on `entitlement_state`. This is already
ratified by ADR-0055:83 — *"`CANCELLED` excludes the allocation from normal
obligation selection; historical fulfillment is never erased"* — and by the
verified permanence of allocation identity (no `DELETE FROM contract_allocations`
exists, `deleted` is never set to `1`, FKs are `ON DELETE RESTRICT`).

`CANCELLED` is the only state writable in production (`contract_service.rs:436,569`);
`ENDED` is unreachable, so the importer accepts every state and stays correct if
that changes. There is no re-keying, no re-parenting, no archival table, and no
archival mechanism.

An allocation that cannot be safely resolved fails the **entire package**,
fail-closed.

### 7a. Allocation ownership is proven locally, not claimed by the payload

The fact carries **no `unit_id`**. Authorization proves that the signer is an
ACTIVE UNIT whose id equals the operator-selected `unit_id` and that the selected
unit belongs to the importing WILAYA — it does **not** prove that the named
`allocation_id` belongs to that unit. Without a second, local check, any
authenticated UNIT of the same WILAYA could name a peer's allocation and drive
its `fulfilled_quantity`.

Ownership is therefore resolved from the destination's own data:
`resolve_allocation_for_fulfillment_sync` returns the allocation's `unit_id`, and
the importer requires `allocation.unit_id == selected_unit_id` before a fact is
applicable. This is a **local read of an existing column**, not a schema change and
not a payload field. A mismatch fails the whole package, fail-closed, exactly like
an unresolvable allocation (§7).

### 8a. An empty fact set is refused on both sides

A UNIT with no recorded fulfillment cannot produce a meaningful package, and the
importer rejects an empty fact set. The producer therefore fails closed rather
than signing, encrypting and persisting an artifact the destination can never
import — and rather than minting a `package_id` that carries no state. This keeps
§8's completeness claim honest: a package that exists always carries the
complete current set.

### 8. Export is the complete current set — a deliberate choice

Every fulfillment export contains the **complete current set** of eligible
allocation fulfillment states. There is no watermark, no export window, no
transport sequence, no issuer sequence, and no per-fact ledger table.

This is deliberate, and it is the second half of a **coupled pair of decisions**:

- §4 removes the per-fact ledger and relies on state-based convergence.
- §8 supplies the completeness that makes state-based convergence safe, because a
  fact's absolute already subsumes its predecessors.

Recording this coupling is mandatory: the absence of a per-fact ledger is
**licensed by** the completeness of the exported set. Any future change that
narrows the exported set must revisit §4 in the same decision.

### 9. `wilaya_executable_remaining`
```text
UNIT operational remaining   effective_remaining         = contracted − fulfilled − released − reserved
WILAYA executable remaining  wilaya_executable_remaining = contracted − fulfilled − released
```

`wilaya_executable_remaining` is owned by the **domain** (a method on
`ContractAllocation`, surfaced on the WILAYA-only `ContractAllocationView`) and is
**never** re-derived in the frontend. `effective_remaining` is **unchanged** and
remains the UNIT ordering gate. `reserved_quantity` is never included in the
WILAYA value, matching Fact 1.

`UnitContractEntitlement` is deliberately **not** given the new field: two
candidate owners for one business fact would violate P2/A1.

## Relation to Existing Documents

| Document | Relation |
|----------|----------|
| ADR-0055 (Contract-Centric Procurement) | **Extended, not amended** — §3.10 ContractCatalog direction, the `upsert_sync_allocation` column-ownership comment, §3.5 old-obligation priority, and the `:83` "historical fulfillment is never erased" clause are all unchanged and relied upon. |
| ADR-0056 (Multi-Supplier Splitting) | **Complied with** — the multi-order/multi-leg reality that makes per-order payload fields unrepresentable (§3.4). |
| ADR-0057 (V3 Schema Gate) | **Complied with** — envelope stays V3, closed `[V3, V3]` window, no V4, `schema_version` unchanged. |
| ADR-0058 (Product Unit Immutability) | **Extended in scope, not in trigger** — the product unit/factor freeze is unchanged and is not extended to contract existence; §5 adds a sync-boundary cross-check instead. |
| ADR-0059 / ADR-0060 (ContractCatalog Export) | **Unchanged** — producer direction stays WILAYA → UNIT. This ADR is the separate, opposite-direction execution-state fact. |
| ADR-0046 / ADR-0051 + `ARCHITECTURE_FREEZE.md` §2.7 | **Amended** — the UNIT-issued kind allowlist gains `contract_fulfillment`, under exactly the ADR-0051 conditions (V2-only, Ed25519, ACTIVE non-expired certificate, WILAYA-only importer, membership + `unit_id` binding). |
| ADR-0009 / ADR-0003 (Canonical JSON, signature v2) | **Complied with** — canonical serialization and `signature_version = 2` unchanged. |
| `ARCHITECTURE_FREEZE.md` §2.4/§2.5 | **Complied with** — no FIFO/accounting change, no wall-clock in the evaluation path, deterministic fact ordering. |
| SEC-056D / SEC-057 / ADR-0053 | **Complied with** — transport sequencing remains permanently retired; the content-derived `package_id` is a payload identity, not a transport sequence, and allocates no ledger entry. |

## Invariants Established

- **I1** `contract_allocations.fulfilled_quantity` is denominated in the contract
  product's **purchase unit**; `contracted_quantity` and `released_quantity` share it.
- **I2** `FulfillmentFact` denotes **state**, never an event. No payload field may
  assert historical per-order causation.
- **I3** The synchronization identity is `allocation_id` + absolute cumulative
  `fulfilled_quantity`. A random UUID is not an identity.
- **I4** At most one fact exists per allocation per package.
- **I5** Idempotency is **state-based**: content-derived `package_id` +
  guarded monotone `SET`. No per-fact ledger, no watermark, no transport sequence.
- **I6** The exported set is **complete**. Narrowing it invalidates I5's safety
  argument and requires a new decision.
- **I7** `fulfilled_quantity` never decreases. Propagation is one-way; a UNIT-side
  decrease is not transmitted and the WILAYA retains the high-water mark.
- **I8** The fiscal year is a cross-check, never an application input.
- **I9** `entitlement_state` governs obligation **selection**, never fulfillment
  **recording**. `deleted = 1` and unresolvable ids fail closed.
- **I9a** A fact may only mutate an allocation owned by the authenticated source
  unit. Ownership is proven from the destination's `contract_allocations.unit_id`,
  never claimed by the payload.
- **I10** The standing guard `fulfilled + released + reserved <= contracted` is
  never relaxed; a guard failure rejects the fact, it does not overwrite.
- **I11** Import is all-or-nothing; no partial package application.
- **I12** `effective_remaining` is frozen and out of scope.
- **I13** `wilaya_executable_remaining` is domain-owned and never re-derived in the
  frontend; `reserved_quantity` is never part of it.

## Rejected Alternatives

| Alternative | Reason rejected |
|-------------|-----------------|
| **Per-order/per-item `FulfillmentFact` with a durable event ledger** | Requires a new table (migration) and would still not be provable — §3 findings #4–#7. Rejected on evidence, not cost. |
| **Delta facts with a resurrected transport sequence** | Transport sequencing is permanently retired (`001_initial.sql:9-13`). A `+=` delta is also not idempotent and cannot converge out of order. |
| **Extending `ContractCatalog` to carry fulfillment** | Inverts a ratified direction (ADR-0055 §3.10), the WILAYA refuses ContractCatalog import entirely, and it would create a dual-authority projection (P2/A2). The `ON CONFLICT` omission of `fulfilled_quantity` proves the column ownership is disjoint by design. |
| **Reusing `stock_movements` as the fulfillment carrier** | Cheapest-looking path and the nearest alternative. Rejected: that package is consumption-unit denominated (`order_service.rs:754`), carries no `allocation_id`, and is inventory-scoped — a second authority over procurement data. |
| **Whole-`SupplierOrder` synchronization** | Would transmit `reserved_quantity`, a UNIT-local transient draft artifact that the sync contract explicitly forbids carrying. |
| **`applied_fiscal_transitions` as the fact ledger** | Rejected as a governance violation: `closed_year`/`opened_year` are `NOT NULL` and fiscal-only, so fulfillment records would require fabricated values. Violates A1/P2. |
| **A `watermark` / `transport_export_sequence` table** | Migration for a performance concern only; correctness is independent of the window under absolute cumulative state. |
| **Adding unit columns to `contract_allocations`** | Schema change on a table ADR-0055/0058 both reference, for a gap the sync-boundary cross-check closes adequately. Deferred, not closed (§7.5). |
| **Modifying `effective_remaining`** | Breaks the UNIT ordering gate and its DB backstop; explicitly excluded by the requirement. |
| **Frontend-only remaining calculation** | Violates A5/F3; FE-141/FE-145 are ERROR and unsuppressible. |
| **Adding the field to `UnitContractEntitlement` as well** | Two candidate owners for one business fact (P2/A1). |
| **Reviving `application/sync_integrity/`** | It is **live**, not dormant — `SyncImportValidationService::validate` enforces the frozen all-or-nothing pre-mutation rule. Nothing to revive. |
| **A parallel export/crypto subsystem** | Forbidden by the requirement and unnecessary: `IdentitySignedExportService` is the existing single producer entry point. |

## Implementation Notes

The only modification to existing production code is threading an **optional
caller-supplied `PackageId`** through `IdentitySignedExportService::build_and_write`,
defaulting to `Uuid::new_v4()` so all five existing kinds behave identically. This
is required because that method hard-codes a fresh UUID and cannot express a
content-derived identity. Identity resolution, signing, encryption, and streaming
are reused unchanged.

Registration of the kind in `DATA_PACKAGE_KINDS` is **mandatory, not cosmetic**:
`commands/import_export.rs:194` gates the entire V2 metadata enforcement block on
membership, so an omitted kind would silently receive no V2 enforcement.

## Known Limitations (accepted)

1. **Unbounded export growth.** The package grows with the total number of
   confirmations; export is O(all confirmations). Revisit only together with a new
   decision on I6.
2. **Lost-fact detection is impossible.** A fact `≤` current is silently skipped
   and indistinguishable from already-applied. Safe only under I6.
3. **One-way propagation.** A UNIT-side *decrease* in `fulfilled_quantity` is never
   transmitted; the WILAYA keeps the high-water mark. Currently unreachable — no
   decrement writer exists and `check_order_is_editable`
   (`domain/validation.rs:773-782`) blocks deleting Confirmed orders.
4. **Same `package_id`, different bytes across re-exports.** `created_at` is inside
   the signed canonical bytes (`package_builder.rs:49-78`), so re-exporting an
   unchanged fact set yields the same `package_id` but a different
   `integrity_hash`/signature/ciphertext. Not a §2.5 violation (different input),
   but the operator UI must surface `package_id` so the WILAYA's replay rejection
   of the second file is explicable.
5. **`contract_allocations` still carries no unit column.** The authoring-side gap
   remains open: ADR-0058 excludes contract existence as a freeze trigger
   (`:228-231`), and `contract_products.purchase_unit` can be permanently `NULL`
   (⇒ factor 1). Documented, not closed.
6. **`entitlement_state = 'ENDED'` is unreachable** in production. The importer
   accepts all states so it remains correct if that changes.
7. **No durable per-order attribution is transmitted.** By design (§3). Operators
   requiring per-order execution detail must read it on the UNIT.

## Testing Obligations

The following must be covered, and the noted gaps genuinely do not exist today
(every existing `fulfilled_quantity` test uses `conversion_factor: Some(1)`, and
`phase5_receipt_conversion_tests.rs:313-363` never asserts `fulfilled_quantity`):

1. Draft order creation does not change any exported fulfillment state.
2. Confirmation changes the UNIT allocation's `fulfilled_quantity`.
3. The export contains allocation-level cumulative state, one fact per allocation.
4. Multiple orders against one allocation produce exactly **one** authoritative
   fact, not multiple misleading cumulative facts.
5. Re-export of an unchanged state produces an identical `package_id`.
6. Importing the same package twice does not increase fulfillment twice.
7. An older absolute state cannot decrease WILAYA fulfillment.
8. Unit mismatch fails closed.
9. Fiscal-year mismatch fails closed.
10. An unknown allocation fails the **whole** package.
11. `effective_remaining` is unchanged (regression assertion).
12. `wilaya_executable_remaining` excludes `reserved_quantity`.
13. The UI displays both values with explicit labels.
14. No frontend arithmetic is introduced.
15. A non-`1` `conversion_factor` allocation is exported and imported correctly.
16. A `CANCELLED` (and `ENDED`) allocation accepts fulfillment; `deleted = 1` does not.
17. A UNIT-signed package is accepted only on a WILAYA importer.
18. `DATA_PACKAGE_KINDS` membership yields V2-metadata enforcement for the new kind.
19. A fact naming **another unit's allocation inside the same WILAYA** fails the
    whole package, and that peer's state is untouched.
20. A UNIT with no recorded fulfillment cannot produce a package, and an empty
    fact set is rejected on import.
21. The reader rejects an unsupported `fact_version` and a `package_id` that does
    not match the dataset, before any database access.
