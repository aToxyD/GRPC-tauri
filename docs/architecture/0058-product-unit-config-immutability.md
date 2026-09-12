# ADR 0058: Product Unit-Config Immutability After First Stock Movement

# Decision Status

**ACCEPTED — 2026-09-12.**

This ADR ratifies the domain invariant that a Product's unit/factor
configuration — `purchase_unit`, `consumption_unit`, `conversion_factor` — is
frozen once at least one row exists in `stock_movements` for that product.
After the freeze point, no Product write path may change any member of the
tuple on an existing Product. A V3 sync package that attempts such a change
rejects the **entire package**, fail-closed and before mutation, inside the
existing Product V3 validation/import boundary. This is a domain invariant
enforced under the existing V3 contract/import architecture; no schema,
protocol, envelope, or payload change is introduced.

| Item | Status |
|------|--------|
| `purchase_unit` immutable after first stock movement | **ACCEPTED** |
| `consumption_unit` immutable after first stock movement | **ACCEPTED** |
| `conversion_factor` immutable after first stock movement | **ACCEPTED** |
| Freeze trigger = `EXISTS(SELECT 1 FROM stock_movements WHERE product_id = ?)` | **ACCEPTED** |
| Forbidden tuple change ⇒ reject the entire sync package, fail-closed, pre-mutation | **ACCEPTED** |
| Invariant applies to every Product write path; currently reachable production mutation path = V3 sync upsert | **ACCEPTED** |
| `tva_classification` / `base_price` / `name` NOT frozen by this ADR | **OUT OF SCOPE** |
| No migration / no new sync envelope version / no V4 / no inventory re-keying or merging | **ACCEPTED** |

# Date

2026-09-12 — accepted by the governance reviewer (ADR-0058).

# Relation to Existing Documents

| Document | Relation |
|----------|----------|
| ADR-0057 (Sync Protocol V3 Schema Gate) | **Extended (not amended)** — ADR-0057 governs the V3 Product payload shape and the existing import gate (`validate_products_package_for_import`, §3.4); ADR-0058 governs the mutability of the unit/factor tuple. Envelope/version/window remain unchanged. |
| ADR-0055 (Contract-Centric Procurement) | **Precedent relied on** — immutability-after-activity (`agreed_price` and contracted quantity) and fiscal-year TVA governance (§3.9: one immutable rate per fiscal year). This ADR does not replace or duplicate ADR-0055 TVA rules. |
| ADR-0056 (Multi-Supplier Request Splitting) | **Precedent relied on** — whole-request atomicity within a single transaction; supports whole-package rejection. Supplier-splitting semantics are unchanged. |
| ADR-0023 (Reproducible Reporting) / `ARCHITECTURE_FREEZE.md` §2.4/§2.5 | **Complied with** — reproducibility and frozen fiscal/reporting behavior are preserved. |

# 1. Executive Summary

The Product master row (`products`) carries the unit/factor configuration that
denominates all stock quantity state for the product:

- `consumption_unit` is the unit in which the stock movement ledger and the
  keyed inventory rows are expressed;
- `purchase_unit` defines the procurement unit captured by order/FIFO
  snapshots;
- `conversion_factor` (> 0) maps the purchase unit to the consumption unit.

`stock_movements` does not store a `consumption_unit` column; its quantities
are meaningful only through the product's `consumption_unit` at the time each
movement was written. `inventory_stocks` is keyed on
`(product_id, consumption_unit)`. Changing the product tuple after movement
history exists therefore reinterprets the unit-less movement ledger
retroactively and orphans previously keyed inventory rows.

This ADR freezes the tuple once historical stock activity exists and makes a
forbidden V3 change an atomic, fail-closed, pre-mutation rejection of the
entire sync package at the single authoritative Product import gate. The
application is pre-production; no migration or protocol change is required.

# 2. The Invariant

## 2.1 Normative decision

The unit/factor configuration of a Product,

```
(
    purchase_unit,
    consumption_unit,
    conversion_factor
)
```

is frozen once at least one row exists in `stock_movements` for that
`product_id`.

Formal trigger:

```
Frozen(product_id) ⇔ EXISTS(
    SELECT 1
    FROM stock_movements
    WHERE product_id = :product_id
)
```

- When `Frozen(product_id)` is false, the tuple may be changed.
- When `Frozen(product_id)` is true:
  - `purchase_unit` cannot change;
  - `consumption_unit` cannot change;
  - `conversion_factor` cannot change.

The invariant applies to **every Product write path that can mutate the
tuple**, local and synchronized.

## 2.2 Current implementation surface

The invariant is an architectural rule stated independently of the current
implementation. The currently reachable production path capable of mutating
the tuple on an existing Product is the **V3 sync upsert**
(`upsert_product_sync`, `repositories/products.rs`), reached only through the
atomic Product V3 import pipeline (`with_event_persistence`,
`commands/import_export.rs`). The local Product update path
(`update_product`) touches `name` and `base_price` only and cannot mutate the
tuple today. Future local configuration write paths must enforce the same
invariant.

## 2.3 Minimal and sufficient trigger — first stock movement

The correct freeze trigger is **historical stock activity** (a `stock_movements`
row), not the mere existence of related state:

- **Product creation** creates an empty keyed `inventory_stocks` row
  (`create_initial_stock_for_product`, called at
  `product_service.rs` and on sync import). An empty stock row does **not**
  represent historical inventory activity; it is identity without history.
- A **contract** may snapshot unit/factor data without writing a stock
  movement; contracts carry their own snapshot and never re-read the product
  tuple after creation.
- A **supplier order** may snapshot unit/factor data without writing a stock
  movement; open orders can exist with no receipt.
- The first **receipt/order flow that creates inventory activity** writes the
  first `stock_movements` row. This is the freeze point.
- **FIFO layer creation** is strictly downstream of stock activity (a receipt
  writes an IN movement) and therefore does not need to be a separate trigger.

Therefore the trigger is `EXISTS(SELECT 1 FROM stock_movements ...)`, and the
following are **not** independent freeze triggers: existence of
`inventory_stocks`, existence of a contract, existence of an order, existence
of a FIFO layer.

# 3. Why the Three Fields Are One Immutable Tuple

The three fields form a single conversion relationship, not three independent
labels:

- `consumption_unit` denominates the stock movement ledger (`stock_movements`
  stores no unit) and is the key of `inventory_stocks`
  (`UNIQUE(product_id, consumption_unit)`);
- `purchase_unit` defines the procurement unit captured by order-item/FIFO
  snapshots;
- `conversion_factor` defines the mapping from purchase unit to consumption
  unit, with the database already enforcing the same-unit relationship:
  `CHECK (purchase_unit <> consumption_unit OR conversion_factor = 1)`
  (`001_initial.sql`).

Changing any member after stock activity changes or destabilizes historical
interpretation or reconciliation semantics:

- a changed `consumption_unit` reinterprets every earlier unit-less movement
  and re-keys the inventory identity;
- a changed `purchase_unit` decouples the product's canonical procurement
  identity from its own snapshotted procurement history;
- a changed `conversion_factor` splits the purchase-to-consumption mapping into
  distinct eras with no per-row marker in the ledger.

The tuple is therefore a single unit of immutability. This distinguishes the
tuple from cosmetic or catalog metadata (see §4).

# 4. Explicit Non-Goals

This ADR does **not** freeze:

1. **`name`** — it has no quantity-denomination semantics.
2. **`base_price`** — it is a reference/catalog value and is not the
   authoritative transactional/FIFO pricing source (contract/order/FIFO
   pricing is backend-authoritative per ADR-0055).
3. **`tva_classification`** — it remains governed by the existing
   **fiscal-year** governance: the per-`(product, fiscal_year)` immutable
   TVA ledger and ADR-0055 §3.9 (one immutable rate per fiscal year). This
   ADR does not create a second TVA immutability rule and does not modify
   ADR-0055 TVA governance.

# 5. Sync Failure Semantics

If a V3 Product record attempts to change any member of the frozen tuple for a
Product that already has stock movement history:

**THE ENTIRE SYNC PACKAGE MUST BE REJECTED.**

The rejection is:

- fail-closed;
- before Product mutation;
- before inventory mutation;
- inside the existing Product V3 validation/import boundary
  (`validate_products_package_for_import`, `import_validation.rs`);
- consistent with the existing single-transaction import boundary.

The following are **not** permitted:

- silently ignoring the frozen field and applying the remaining fields —
  this would create WILAYA/UNIT divergence and leave the UNIT with product
  state that no longer matches the authoritative catalog;
- rejecting only the offending Product record while applying other records —
  this violates the established atomic package semantics
  (single transaction per import; whole-request atomicity precedent);
- re-keying inventory;
- merging inventory;
- automatic inventory repair;
- interactive conflict resolution;
- legacy compatibility behavior;
- partial package application.

The UNIT cannot override WILAYA authority (`products_source_allowed_for_unit`,
`import_provenance.rs`); product data is WILAYA-authoritative and the UNIT
cannot repair it locally.

# 6. Offline-First Remediation

The operational meaning of a rejection:

- The package may be **cryptographically valid and structurally valid** — the
  bytes, signature, and envelope are sound.
- The failure is a **semantic/domain rejection against the receiving database
  state**: a valid package that violates a domain invariant of that node's
  persisted state.
- **No Product mutation occurs.**
- **No inventory mutation occurs.**
- The package is **not recorded as successfully applied**, so re-import
  deterministically produces the same semantic rejection until the
  authoritative source is corrected.
- **Remediation occurs at WILAYA**: the operator corrects the Product tuple
  (or retires the Product) and produces a corrected conforming package, which
  the UNIT then imports normally.
- The UNIT performs **no self-repair and no conflict resolution**.

This is an operator procedure, not a new conflict-resolution subsystem.

# 7. Security / Accounting Rationale

This rule protects:

- **Inventory identity** — `inventory_stocks` identity is
  `(product_id, consumption_unit)`; freezing the tuple prevents the identity
  drifting under existing keyed rows.
- **Historical meaning of the unit-less stock movement ledger** —
  `stock_movements` quantities remain interpretable because the configuration
  that denominated them cannot change after the history exists.
- **FIFO / accounting reproducibility** — order items, FIFO layers, and
  contract products snapshot their own unit/factor; freezing the product tuple
  removes the last mutable reference over unsnapshotted quantity state.
- **Report reproducibility** — reports and oversight recompute from persisted
  state; a changed tuple under an old ledger would change the interpretation of
  prior quantities.
- **Auditability** — a forbidden change surfaces as a deterministic,
  fail-closed import/rejection audit event.
- **WILAYA-authoritative synchronization** — UNIT product state can never
  silently diverge from the authoritative catalog.

**Precision:** this ADR does **not** add `consumption_unit` to
`stock_movements`. It prevents the Product configuration that currently
denominates the unit-less movement ledger from changing after historical stock
activity exists.

# 8. No Schema / Protocol Change

This ADR introduces **no**:

- migration;
- schema redesign;
- new table;
- new column;
- new payload field;
- sync envelope version change;
- V4;
- legacy compatibility path;
- inventory re-keying or merging.

This is a domain invariant enforced under the existing V3 contract/import
architecture (ADR-0057 §3.4 gate, unchanged).

# 9. Implementation Boundary

A future implementation MAY:

- add a repository/service-level **stock-activity existence predicate** (the
  `EXISTS(stock_movements WHERE product_id = ?)` test, SQL confined to the
  repositories layer);
- extend the existing Product V3 validation gate
  (`validate_products_package_for_import`) to compare the incoming tuple
  against the persisted tuple when movement history exists;
- reject the entire package before mutation on a forbidden tuple change;
- apply the same invariant to any future local Product configuration write
  path.

A future implementation MUST NOT:

- migrate the schema;
- introduce a new sync version;
- add legacy fallback;
- re-key inventory;
- merge inventory;
- partially apply the package;
- silently ignore frozen fields;
- introduce interactive conflict resolution;
- alter TVA fiscal-year governance;
- alter `base_price` / `name` handling.

Function names other than the verified existing
`validate_products_package_for_import` are not prescribed here.

# 10. Testing Contract

The following minimum behavioral tests are required for the implementation
phase (not part of this ratification):

1. No movement + unchanged tuple → accepted.
2. No movement + changed tuple → accepted.
3. Movement exists + unchanged tuple → accepted.
4. Movement exists + changed `purchase_unit` → whole package rejected.
5. Movement exists + changed `consumption_unit` → whole package rejected.
6. Movement exists + changed `conversion_factor` → whole package rejected.
7. Movement exists + changed `name`/`base_price` only → accepted.
8. `tva_classification` behavior remains governed by the existing
   fiscal-year rule.
9. Forbidden tuple change → zero Product mutation.
10. Forbidden tuple change → zero inventory mutation.
11. Multi-product package with one violating Product → entire package
    rejected, with no Product changes applied.
12. Corrected / re-exported conforming package → subsequently accepted.

# 11. Governance Records

- This ADR is the SEC-087 Phase 6C follow-on decision record; ADR-0057 §10
  explicitly reserved SEC-087 Phase 6B/6C work for separate decision records.
- Accepted as number **0058**; the `ADR_INDEX.md` row is added per
  `ADR_INDEX.md` policy (sequential 4-digit numbering, statuses, owner; range
  0044–0099 available for governance ADRs).
- `ARCHITECTURE_FREEZE.md` requires **no amendment**: every Section 2 frozen
  clause is preserved or strengthened by this additive, fail-closed invariant.
- Pre-production environment; no deployed databases and no legacy inventory
  records (clean-cutover precedent, ADR-0057 §2.3).
- AGENTS.md §13 precedence: where any temporary or design document conflicts
  with this ADR, this ADR governs.