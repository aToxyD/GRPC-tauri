# ADR 0056: Multi-Supplier Request Splitting and Portion-Granularity Order Items (SEC-087 Phase 4B)

## Decision Status

**Status: Accepted**

**Date: 2026-09-09**

This ADR is a proposal. It is NOT accepted and does NOT authorize implementation
until separately ratified by the owner. It supersedes — on acceptance — the
single-allocation single-supplier rejection clauses of ADR-0055 §3.5 and §3.6 and
defines the item/allocation-leg cardinality contract for Phase 4B.

## Owner

Architecture

## Reference

- ADR-0055 (`docs/architecture/0055-contract-centric-procurement.md`) — contract-centric
  procurement; **§3.5 (no-partial/no-split sentence) and §3.6 (no-automatic-split) are
  superseded by this decision on the clauses enumerated in §Relationship to ADR-0055**;
  the core "one supplier per SupplierOrder" structural rule is retained.
- Phase 4A (SEC-087) confirmation semantics — locked; unchanged by this ADR
  (`src-tauri/src/application/services/order_service.rs`, `confirm_order_atomic`;
  `src-tauri/src/repositories/orders.rs`, `get_order_items_for_confirmation`).
- ADR-0021 (`docs/architecture/0021-fifo-determinism.md`) — FIFO layer ordering, unchanged.
- ADR-0024 (`docs/architecture/0024-audit-dual-write.md`) — audit dual-write, unchanged.
- ADR-0025 (`docs/architecture/0025-sqlite-single-writer.md`) — single-writer transaction
  boundary, unchanged.
- ADR-0048 (`docs/architecture/0048-fiscal-closure-ed25519-migration.md`) —
  exact `Decimal` numeric semantics, unchanged.
- AGENTS.md — Runtime Tenet 7 (backend source of truth), Critical Isolation Rules,
  Change Control (C1/D1), Layer Map.

## Context

SEC-087 defines contract-centric procurement (ADR-0055). Phase 3 established the
authoritative pricing chain and Phase 4A locked single-supplier order confirmation:

> Each `SupplierOrderItem` has exactly one `SupplierOrderItemAllocation` leg.
> `get_order_items_for_confirmation()` is a row-per-(item, leg) projection without
> aggregation; confirmation re-resolves each recorded leg authoritatively.

Phase 4A's resolution (`resolve_supplier_for_item`) requires one product request to be
fully covered by a single allocation; insufficient coverage rejects the whole order.
This forbids a legitimate operational case: a UNIT requesting a quantity that no single
allocation can cover but multiple allocations (possibly of different suppliers, or of
the same supplier across fiscal years) could cover jointly.

Phase 4B design validation established that the previously assumed design phrase
"one item per (order × product)" is invalid and must be replaced by a
portion-granularity invariant. This ADR records that decision.

## Problem

1. A `CreateOrderRequest` may legitimately span suppliers. ADR-0055 §3.6 currently
   rejects the entire creation in that case and requires separate manual orders.
2. A single supplier may hold multiple eligible allocations for the same product
   (an old outstanding obligation plus a current-year entitlement). A request may need
   to consume several of them jointly; ADR-0055 §3.5 currently forbids splitting.
3. The correct persistence shape for "product P sourced from allocation A and
   allocation B of the same supplier" must not violate the locked Phase 4A invariant
   that every item carries exactly one allocation leg.

## Decision

For a single `CreateOrderRequest`:

1. The request may span multiple suppliers.
2. The system **automatically splits** the request into supplier-specific `SupplierOrder`s.
3. Each resulting `SupplierOrder` has exactly one supplier.
4. There is **at most one resulting `SupplierOrder` per supplier** for one request.
5. Different suppliers therefore produce different `SupplierOrder`s.
6. The same supplier may receive multiple products in the same `SupplierOrder`.
7. The resolver retains **oldest-outstanding-obligation priority**.
8. Resolution may consume **multiple allocations** for the same product.
9. If multiple allocations for the same product belong to the **same supplier**, they
   remain inside the **same** `SupplierOrder` (no extra orders for an identical supplier).
10. **Each allocation portion becomes its own `SupplierOrderItem`.**
11. Each `SupplierOrderItem` has **exactly one** `SupplierOrderItemAllocation` leg.
12. The same `product_id` MAY therefore appear in multiple `SupplierOrderItem` rows
    within the same `SupplierOrder` when those rows represent different allocation portions.
13. A `SupplierOrderItem` MUST NOT have multiple allocation legs.
14. Same-supplier multi-allocation MUST NOT be solved by creating additional
    `SupplierOrder`s for the same supplier.
15. Multiple allocation portions MUST NOT be merged into one item with multiple legs.
16. Portions MUST be **planned** in greedy resolver-priority order — the oldest
    eligible allocation is consumed to its full effective remaining before a younger
    allocation is used; only the final allocation portion may be partial (I6). Portions
    SHOULD also be materialized in that same order for deterministic side-effect
    sequencing, but write/insertion order is NOT a confirmation-correctness invariant
    (see §Confirmation Correctness).
17. This preserves compatibility with the existing Phase 4A confirmation flow.
18. The complete request MUST be planned and proven satisfiable **before any
    persistent write** occurs.
19. All resulting `SupplierOrder`s, items, allocation legs, and reservations are
    created inside the existing single `AuditTxService` transaction.
20. If any part of the request fails, the entire request rolls back.
21. No partial supplier orders may survive.
22. Over-request rejects the entire request.
23. Supplier, allocation, and price decisions remain **backend-derived**; the caller
    cannot choose them.
24. `price_ttc` remains the sole operational order-price authority.
25. `SupplierOrder.fiscal_year` is anchored to the current fiscal year
    (`settings.current_year`) at creation for EVERY resulting `SupplierOrder`,
    including orders consuming older outstanding obligations.
26. `reference_number` is optional and non-unique. If one request creates multiple
    `SupplierOrder`s, the same `reference_number` is copied to every resulting order.
27. No persistent `ProcurementRequest` / request-group aggregate is introduced.
28. No database/schema change is required for Phase 4B.
29. Existing contract-allocation quantity guards remain authoritative.
30. The one-live-contract-per-unit-per-fiscal-year constraint remains unchanged.
31. Supplier `active` status, `unit_suppliers` association, and contract-status
    eligibility remain unchanged in Phase 4B.
32. Cross-unit confirmation hardening remains a separate future security item.
33. Sync behavior remains unchanged because `SupplierOrder`s are UNIT-local; existing
    stock-movement/FIFO consequences continue through existing sync.
34. Phase 4A confirmation logic is NOT redesigned or modified by Phase 4B.
35. The allocation planner MUST consume eligible allocations in resolver priority order
    using the **greedy-drain** rule: the oldest eligible allocation is consumed to its
    FULL effective remaining quantity before any younger allocation is used; only the
    FINAL allocation portion for a product may be partial. A non-greedy split is invalid
    and MUST fail closed (see §Greedy-Drain Allocation Invariant).
36. Confirmation correctness is **order-independent by construction** under the
    greedy-drain allocation invariant and Phase 4A own-reservation netting; it MUST NOT
    depend on any physical row order (see §Confirmation Correctness).
37. The confirmation item query MUST use an explicit deterministic `ORDER BY` based on
    the allocation priority key (`fiscal_year ASC, created_at ASC, id ASC`) with
    `SupplierOrderItem.id` as the final tie-break, and MUST NOT rely on SQLite rowid,
    insertion, index-traversal, join, or query-plan order (see §Deterministic Query Contract).

## Detailed Invariants

| # | Invariant | Nature |
|---|-----------|--------|
| I1 | A `SupplierOrder` contains exactly one supplier | structural (retained from ADR-0055) |
| I2 | At most one `SupplierOrder` per supplier per request | structural (this ADR) |
| I3 | One `SupplierOrderItem` per allocation portion | structural (this ADR) |
| I4 | Every `SupplierOrderItem` has exactly one allocation leg | locked (Phase 4A) |
| I5 | A product may appear in multiple item rows of one order only via distinct portions of the same supplier | structural (this ADR) |
| I6 | Greedy-drain portion selection: oldest eligible allocation consumed to its FULL effective remaining quantity; only the final portion may be partial; non-greedy split invalid and fails closed | locked (this ADR) |
| I6b | Portion materialization order = resolver-priority order (deterministic side-effect sequencing; NOT a correctness mechanism) | operational (this ADR) |
| I7 | Oldest-outstanding-obligation priority | locked (ADR-0055, retained) |
| I8 | Whole-request atomicity; no partial orders survive | locked (this ADR) |
| I9 | Backend-authoritative supplier/allocation/price | locked (ADR-0055, retained) |
| I10 | Current-FY anchor on every resulting order header | locked (this ADR) |
| I11 | Confirmation correctness is order-independent by construction (greedy-drain + Phase 4A own-reservation netting); no reliance on physical row order | locked (this ADR) |
| I12 | Confirmation item query uses explicit deterministic `ORDER BY` on allocation priority key (`fiscal_year ASC, created_at ASC, id ASC`) + `SupplierOrderItem.id` tie-break; no reliance on rowid/insertion/index/join/plan order | locked (this ADR) |

## Allocation-Plan Semantics

- The request boundary stays `CreateOrderRequest { reference_number, items }` with
  `items = [{ product_id, quantity }]`. Duplicate `product_id` input rows are
  **normalized/aggregated** at the request boundary into one requested quantity per
  product before planning. This is distinct from the resolution boundary: a single
  normalized product line may still produce **multiple allocation portions**.
- The resolution boundary is a pure, in-memory allocation plan over the existing
  candidate set (`list_resolution_candidates_full`: `fiscal_year ASC, created_at ASC,
  id ASC`, CANCELLED excluded). Every portion is `(allocation, supplier, fiscal_year,
  quantity, price_ttc)` and satisfies the allocation's effective remaining.
- The plan is complete and fully validated (per-product Σ portions == requested, and
  every portion ≤ its allocation's remaining) before any write (I8).
- Violating determinism: identical inputs and identical persisted state produce an
  identical portion plan (P1 determinism).

## Supplier Grouping Semantics

- Portions are grouped by `supplier_id` only. Fiscal year is NOT part of the group key.
- Each group becomes exactly one `SupplierOrder` (I2). If a product is split across
  allocations of the **same** supplier, all of its portions go to that supplier's single
  order. If a product is split across different suppliers, the portions go to the
  respective per-supplier orders.

## Item / Allocation-Leg Cardinality

The old assumption — **one `SupplierOrderItem` per (SupplierOrder × product)** — is
**superseded**.

The correct invariant is:

> **One `SupplierOrderItem` per allocation portion.**

And, explicitly:

> **A product MAY appear in multiple `SupplierOrderItem` rows within the same
> `SupplierOrder` when the resolver allocates that product across multiple allocations
> belonging to the same supplier.**

And:

> **Every `SupplierOrderItem` has exactly one allocation leg.**

Consequences verified against Phase 4A:

- `get_order_items_for_confirmation()` is a row-per-(item, leg) join with no
  aggregation; multiple item rows for one product simply yield multiple rows, each
  carrying its own quantity, price, and allocation. The only query change is the
  explicit deterministic `ORDER BY` (I12); no aggregation is introduced.
- Confirmation re-resolves each recorded leg independently (quantity, allocation
  identity, supplier identity, price) and converts reservation → fulfillment per row.
  Its `own_reserved` / `converted_own` accounting is keyed by `allocation_id` and
  therefore naturally handles multiple rows touching one allocation.
- Confirmation is order-independent by construction (I11): each recorded allocation's
  own still-reserved quantity is restored for its own re-resolution, conversion
  reserved→fulfilled preserves the allocation's total committed quantity, and under the
  greedy-drain invariant (I6) older allocations cannot be incorrectly stolen by another
  recorded portion. Physical row order therefore never determines which allocation is
  re-resolved.
- `get_order_items_for_confirmation()` additionally applies the deterministic query
  contract (I12) so execution and side-effect sequencing are explicit and reproducible;
  it MUST NOT rely on rowid, insertion, index-traversal, join, or query-plan order.

## Greedy-Drain Allocation Invariant

The allocation planner is a pure, greedy, oldest-first consumer. This is a mandatory
invariant, not an implementation detail:

> For a single product request, the planner MUST consume eligible allocations in
> resolver priority order (`fiscal_year ASC, created_at ASC, id ASC`, CANCELLED
> excluded). Each eligible allocation is consumed to its **full effective remaining
> quantity** before any younger allocation is used. Only the **final** allocation
> portion for the product may be partial (because the request ends there). A
> non-greedy split — e.g., apportioning a request across several partially-used
> allocations — is **invalid and MUST fail closed**.

Rationale: greedy-drain is exactly what makes confirmation order-independent (I11). A
non-greedy split leaves older allocations with positive remaining after their portion
is converted; confirmation re-resolution would then steal the later portion's
allocation and reject the order in every processing order. Greedy-drain makes every
consumed allocation (except possibly the last) show zero effective remaining at
re-resolution until its own recorded portion is restored.

## Confirmation Correctness

> **Confirmation is order-independent by construction under the greedy-drain allocation
> invariant (I6) and Phase 4A own-reservation netting (I11).**

The mechanism, explicitly:

- Each recorded allocation's own still-reserved quantity is restored for its own
  re-resolution: `own_reserved` subtracts exactly this order's still-reserved quantity
  (net of `converted_own`) from the candidate matching the recorded leg, so the
  resolver sees that allocation at its pre-order remaining.
- Conversion reserved→fulfilled preserves the allocation's total committed quantity
  (`fulfilled + reserved` is invariant under `try_convert_reserved_to_fulfilled`), so
  an allocation's effective remaining is identical whether or not its own item was
  already processed.
- Under greedy-drain, any allocation older than a recorded portion is exhausted (or
  self-blocked by its own still-reserved quantity), so it cannot be incorrectly chosen
  by another recorded portion's re-resolution.
- Therefore confirmation **validation does not depend on physical row order**. Reversed
  row order cannot change which allocations are re-selected for a valid greedy-drain
  order.

This is the fundamental correctness mechanism. The explicit query ordering below (I12)
is defense-in-depth and the deterministic execution/write-order contract — it is NOT
the correctness mechanism.

## Deterministic Query Contract

> The confirmation item query MUST use an explicit deterministic `ORDER BY` based on
> the allocation priority key (`contract_allocations.fiscal_year ASC`,
> `contract_allocations.created_at ASC`, `contract_allocations.id ASC`), with
> `SupplierOrderItem.id` as the final tie-break. Confirmation MUST NOT rely on SQLite
> rowid order, insertion order, index-traversal order, join order, or query-plan
> behavior.

- This ordering matches the resolver's candidate priority exactly
  (`list_resolution_candidates_full` sorts by the same key), so the confirmation
  iteration executes in the same deterministic order.
- It provides reproducible side-effect sequencing (stock movements and FIFO layer
  creation follow the confirmation loop order).
- It is semantically neutral for Phase 4A single-portion orders (see §Compatibility
  with Phase 4A) and cannot weaken any Confirmation guard.
- Implementation note: the query joins `contract_allocations` on the recorded
  allocation id solely to apply this ordering; no index, schema, or data change is
  required.

## Same-Supplier Multi-Allocation Example

Current fiscal year = 2027.

Product P:

| Allocation | Supplier | Fiscal year | Remaining | price_ttc |
|------------|----------|-------------|-----------|-----------|
| A | X | 2026 | 200 | 100.00 |
| B | X | 2027 | 300 | 120.00 |

Request: `P = 300`.

Resolution (oldest obligation first):

- A → 200
- B → 100

Result — **ONE** `SupplierOrder` for Supplier X, `fiscal_year = 2027`:

- `SupplierOrderItem` #1: product P, quantity 200, unit_price 100.00, allocation A, one leg.
- `SupplierOrderItem` #2: product P, quantity 100, unit_price 120.00, allocation B, one leg.
- `total_amount = 32000.00`.

Each item has exactly one allocation leg. The product appears in two item rows of the
same order because the two portions belong to the same supplier (I5, I4).

## Different-Supplier Split Example

Allocation A → Supplier X → 200; Allocation B → Supplier Y → 100; request `P = 300`.

Result:

- SupplierOrder X → 200 (item P qty 200, leg allocation A, fiscal_year 2027).
- SupplierOrder Y → 100 (item P qty 100, leg allocation B, fiscal_year 2027).

Both order headers use the current fiscal year (2027).

## Atomicity / Transaction Boundary

- Planning happens entirely **before the write phase** (I8). All headers, items, legs,
  and reservations are written inside the existing single
  `AuditTxService` transaction (ADR-0025 single-writer).
- Any failure — header creation, guarded reservation (`try_increment_reserved` == 0),
  item insertion, leg insertion, audit-log write — aborts the transaction and rolls
  back the whole request (I20, I21).
- No write occurs for which the plan is not already proven satisfiable.

## Quantity and Over-Request Semantics

- Requested quantity per product is the **normalized** sum after duplicate-row
  aggregation.
- The plan may satisfy the request by combining portions across allocations (I8).
- If the total eligible remaining across all allocations is **less than** the requested
  quantity, the **entire request is rejected** (I22) — no partial orders, no silent
  cut-down.
- All quantity arithmetic is exact `Decimal` (ADR-0048); no float epsilon in coverage
  comparisons.

## Price Authority

- `price_ttc` on the contract product is the sole operational order-price authority
  (unchanged from ADR-0055 / SEC-087 Phase 3).
- Each portion carries the `price_ttc` of its own contract product; a portion whose
  allocation lacks a frozen `price_ttc` fails closed.
- Different portions of the same product within one order may therefore legitimately
  carry different `price_ttc` values (different contracts/fiscal years). Per-item
  `unit_price` and `total_cost` snapshots preserve each portion's price historically.
- The header `total_amount` is the exact sum over the order's item lines, converted to
  `f64` only at the header-write boundary.

## Fiscal-Year Semantics

- EVERY resulting `SupplierOrder` is created with `fiscal_year = current fiscal year
  (`settings.current_year`)` (I10), including orders that consume older outstanding
  obligations.
- Old obligations retain priority via the **portion selection order** (I7); the order
  header is never stamped with the obligation's older fiscal year.
- Confirmation anchors on the order's fiscal year and asserts that year is open,
  exactly as in Phase 4A.

## reference_number Semantics

- `reference_number` remains optional and non-unique (no schema change).
- When one request produces multiple `SupplierOrder`s, the **same** `reference_number`
  is copied to every resulting order (I26). The reference identifies the request, not
  an individual order, and carries no uniqueness contract.

## Auditability Decision

- The split outcome is fully reconstructible from persisted `supplier_orders`,
  `supplier_order_items`, and `supplier_order_item_allocations` rows: product, portion
  quantity, supplier, allocation, fiscal year, and price all persist.
- Whole-request atomicity guarantees the all-or-nothing property of the request itself.
- The request→orders grouping correlation is derivable via the shared
  `reference_number` when present and via the transaction boundary; it is not required
  for correctness or auditability.
- **No persistent request-group / `ProcurementRequest` aggregate is introduced** (I27).

## Offline-First / Sync Impact

- `SupplierOrder` artifacts remain UNIT-local; sync ships only the downstream
  `stock_movements` (`reference_type = "Order"`) and FIFO layers
  (`source_type = "ORDER"`) produced at confirmation, plus the WILAYA→UNIT
  `ContractCatalog` projection for contracts — all unchanged.
- Creating N orders in one transaction requires no sync schema, package type, conflict,
  or WILAYA change (I33).

## Security Invariants

- Supplier, allocation, and price remain backend-derived; the caller provides only
  product and quantity (I23).
- All reservation/fulfillment/release writes remain guarded conditional updates on
  `contract_allocations`; the component invariant
  `fulfilled + released + reserved <= contracted` remains authoritative (I29).
- No new caller-controlled surface is introduced.
- Cross-unit confirmation hardening remains a separate, future security item (I32);
  it is not a Phase 4B dependency.

## Database / Schema Decision

- **No schema change** (I28). Table shape, indices, checks, and constraints are
  unchanged.
- Item→leg 1:1 (I4) is application-enforced (one portion = one item = one leg), exactly
  as Phase 4A relies on application control rather than a DB unique constraint.
- The one-live-contract-per-unit-per-fiscal-year constraint is unchanged (I30).
- Supplier `active` status, `unit_suppliers` association, and contract-status
  eligibility rules are unchanged (I31).

## Compatibility with Phase 4A

- `confirm_order_atomic` and the confirmation ALGORITHM in `order_service.rs` are
  unchanged. `get_order_items_for_confirmation()` gains ONLY the explicit deterministic
  `ORDER BY` (I12), which is semantically neutral for Phase 4A single-portion orders.
- The Phase 4A locked invariant — one item, exactly one leg — is preserved for every
  item created under Phase 4B (I4).
- Multi-portions of the same product in one order confirm as independent rows,
  producing one stock movement and one FIFO layer per portion at the portion's own
  unit cost (correct lot costing).
- The SQL `ORDER BY` does not alter any Phase 4A drift, supplier, price, reservation,
  FIFO, stock, transaction, or audit guard; a single-portion order returns a single row
  under any ordering scheme.
- Phase 4A semantics are not redesigned; Phase 4B only widens what `ConfirmOrder` may
  encounter within a single order (I34).

## Consequences

### Positive

- Old obligations across several of a supplier's allocations can be honored in one
  order without unblocking offline (no permanent manual split UX).
- Mixed-supplier requests resolve automatically into per-supplier orders.
- Every item remains auditable to exactly one allocation and one price (ADR-0055
  auditability retained at portion granularity).
- Phase 4A confirmation stays byte-compatible.

### Negative / Cost

- One logical request may produce N orders; consumers (IPC contract, orders list) must
  handle a collection of results rather than a single order.
- A confirmed same-supplier-split order contains multiple item rows for one product;
  any UI/DTO that assumed a unique product per order must remain row-based (read path
  already is).
- Draft-edit of a split order re-resolves through the single-allocation update path
  (update semantics unchanged); re-editing such an order reduces it to the
  single-portion shape — an accepted consequence of keeping update/Phase 4A unchanged.

### Compliance

- Runtime Tenet 7 (backend source of truth), P1 determinism, A2 contract ownership,
  C1/C2 change control, and the Phase 4A lock are preserved.
- No frozen area of `ARCHITECTURE_FREEZE.md` is modified by this decision (procurement
  semantics are governed by ADR, not by a freeze entry).

## Relationship to ADR-0055

ADR-0056 **partially supersedes** ADR-0055. The affected statements are superseded only
as enumerated below; all other ADR-0055 decisions remain authoritative. ADR-0055 itself
is not rewritten by this ADR.

Superseded statements:

- **ADR-0055 §3.5** — the sentence *"If `requested > effective_remaining` of the winning
  obligation → **reject the entire order** (no partial, no split)"*: the **no-split**
  restriction is superseded. The whole-request rejection on insufficient total coverage
  remains (I22). Oldest-obligation priority itself is retained (I7).
- **ADR-0055 §3.6** — the sentence *"If the resolver maps product lines to different
  suppliers, the **entire order creation is rejected** and the UNIT creates separate
  orders. **No automatic split**"*: the **reject-and-no-automatic-split** behavior is
  superseded by automatic multi-supplier splitting.

Retained from ADR-0055 (explicitly NOT superseded):

- **"One supplier per SupplierOrder"** structural rule (I1).
- Backend-authoritative pricing (`price_ttc`) and supplier/allocation selection.
- Old-obligation priority, per-allocation component ledger, WILAYA-only obligation
  release, TVA policy, `ContractCatalog` sync, authorization model.
- ADR-0055's own acceptance does not extend to the superseded clauses above.

## Implementation Boundary / Non-Goals

Approved implementation boundary for Phase 4B (not modified by this task):

- `src-tauri/src/domain/pricing/resolver.rs` — add the pure allocation-plan function;
  keep the existing single-item resolution function compatible.
- `src-tauri/src/application/services/order_service.rs` — the create path
  (plan → group → multi-header write loop); confirmation, update, delete, and release
  flows unchanged.
- `src-tauri/src/repositories/orders.rs` — `get_order_items_for_confirmation()` gains
  the explicit deterministic `ORDER BY` (allocation priority key + `SupplierOrderItem.id`
  tie-break), per §Deterministic Query Contract (I12). No other repository change.
- `src-tauri/src/commands/orders.rs` + `src/lib/tauri.ts` — the create command's result
  changes from a single order result to a collection of produced order results, because
  one request may produce N `SupplierOrder`s.
- Tests.

`CreateOrderRequest` input remains unchanged (`reference_number` + items).

Non-goals — Phase 4B does NOT:

- modify Phase 4A confirmation semantics;
- introduce multi-leg items;
- add schema;
- add a persistent request aggregate;
- redesign sync;
- change supplier-eligibility policy;
- change contract-status policy;
- add cross-unit hardening;
- activate UOM conversion;
- change FIFO semantics;
- change price authority;
- change the authorization model.

No speculative future architecture is introduced by this ADR.

## Acceptance Criteria

For this ADR to be considered accepted by the owner, all of the following must hold:

1. The cardinality rule is explicit and unambiguous — one item per allocation portion;
   a product may appear in multiple item rows of one order only via same-supplier
   multi-allocation; every item has exactly one leg.
2. Greedy-drain is an explicit invariant: the oldest eligible allocation is consumed to
   its full effective remaining before a younger allocation; only the final portion may
   be partial; a non-greedy split fails closed (I6).
3. Confirmation correctness is stated as order-independent by construction under
   greedy-drain + Phase 4A own-reservation netting, and is NOT claimed to depend on
   write/insertion order (I11).
4. The deterministic query contract is explicit: confirmation item loading is `ORDER BY`
   allocation priority key (`fiscal_year ASC, created_at ASC, id ASC`) + item id
   tie-break, with no reliance on rowid/insertion/index/join/plan order (I12).
5. The relationship to ADR-0055 is precise: §3.5 no-split sentence and §3.6
   no-automatic-split sentence superseded; "one supplier per SupplierOrder" retained.
6. The locked rules above are encoded without contradiction.
7. Same-supplier and different-supplier examples render correctly under I1–I12.
8. No schema change, no Phase 4A change, no sync change, no authorization change is
   introduced.
9. The status is `Proposed` and no implementation claim is made.

On owner acceptance, the ADR status is updated to `Accepted`, the `ADR_INDEX.md` entry
synchronized, and Phase 4B implementation is authorized subject to the repository's
governance gate.