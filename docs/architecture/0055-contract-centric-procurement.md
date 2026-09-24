# ADR 0055: Contract-Centric Procurement

# Decision Status

**Status: Accepted**
**Ratification Date: 2026-09-01**

**ACCEPTED — 2026-09-01 (SEC-087-F implementation ratification).**

This ADR ratifies the contract-centric procurement model (SEC-087-A → SEC-087-F design and implementation): Supplier as a first-class entity with a UNIT M:N relationship, Contract as the pricing + entitlement authority, per-UNIT obligations, WILAYA-authorized obligation release, and fiscal-year-immutable TVA policy. It supersedes the SEC-087-E design draft on any point where SEC-087-F (this contract) is more specific.

| Item | Status |
|------|--------|
| Supplier first-class; UNIT ↔ Supplier M:N | **ACCEPTED** |
| ONE supplier per SupplierOrder; single-supplier resolution | **ACCEPTED** |
| Old-obligation priority over current entitlement | **ACCEPTED** |
| WILAYA-only obligation release/reassignment | **ACCEPTED** |
| One ACTIVE entitlement per (UNIT, Product, FiscalYear) | **ACCEPTED** |
| Materialized `entitlement_state` + SQLite-valid partial unique index | **ACCEPTED** |
| Full-order rejection on over-request / mixed-supplier | **ACCEPTED** |
| Backend-authoritative pricing; no `products.base_price` fallback | **ACCEPTED** |
| TVA — one immutable rate per FiscalYear (WILAYA-controlled) | **ACCEPTED** |
| `ContractCatalog` V2 additive sync package kind | **ACCEPTED** |
| No trust / identity / SEC-009 protocol changes | **ACCEPTED** (unchanged) |
| No migrations — single baseline `001_initial.sql` edit | **ACCEPTED** |

---

## 1. Problem

Procurement is unit-era free-form: a SupplierOrder carries an arbitrary free-text supplier name, per-item unit prices are caller-supplied, and order price falls back to product-level `base_price`/`tva` metadata. There is no contractual relationship between a supplier and a UNIT, no per-UNIT obligation ledger, no authoritative agreed price, and no fiscal-year-owned tax policy.

This produces:

1. No supplier identity — order provenance is a string, so a supplier rename is impossible to express and historical orders carry no stable contract reference.
2. No obligation tracking — a UNIT cannot prove that an old supplier still owes a leftover quantity from a prior fiscal year.
3. Untrusted pricing — the caller decides the order price and the system stores it as FIFO cost, violating backend-source-of-truth (AGENTS.md Runtime Tenet 7).
4. Product-owned TVA — TVA is a per-product number, so the same fiscal year can yield inconsistent tax rates; a fiscal-year policy is impossible.
5. No one-supplier-per-order enforcement — an order can silently mix obligations that belong to different suppliers.

## 2. Scope

This ADR governs the procurement domain only:

- New tables: `suppliers`, `unit_suppliers`, `contracts`, `contract_products`, `contract_allocations`, `contract_allocation_exceptions`, `fiscal_year_tax_policy`, `supplier_order_item_allocations`.
- Modified tables: `products` (remove `tva`, `supplier_name`), `supplier_orders` (add `supplier_id`; keep `supplier_name` as immutable snapshot), `supplier_order_items` (add `fiscal_year`, `unit_id`).
- New domain modules: supplier, contract, fiscal tax policy, pricing resolver.
- New application services: `SupplierService`, `ContractService`, `FiscalTaxPolicyService`; order service integration.
- New authorization actions (WILAYA-scoped manage/release/revoke; unit-scoped read).
- New sync package kind: `ContractCatalog` (V2, additive).
- New IPC commands + Svelte pages.

Out of scope (unchanged): identity architecture, trust package handling, SEC-009 hardening, transport/sequencing protocol, backup/restore, observability.

## 3. Decision

### 3.1 Supplier is first-class

- `suppliers` entity with a stable `id`; `name` mutable (rename does not rewrite history).
- UNIT ↔ Supplier is **M:N** via `unit_suppliers`. `units.supplier_id` is forbidden.
- Supplier deletion restricted (`RESTRICT`) when referenced by contracts or orders; soft-delete via `deleted`.

### 3.2 Contract is the pricing + entitlement authority

- `contracts` scoped to one UNIT + one Supplier + one fiscal year.
- `contract_products`: authorized products with `agreed_price`. After acceptance, `agreed_price` and contracted quantity are immutable (in-place amendment requires a new contract).
- `contract_allocations`: per-UNIT entitlement rows carrying `contracted_quantity`, `fulfilled_quantity`, `released_quantity`, `reserved_quantity`, `entitlement_state`, `version`.
- Lifecycle: `proposed → accepted → active → ended`; any of the first three may transition to `cancelled`. Domain guards reject invalid transitions.

### 3.3 Entitlement state (SQLite-valid enforcement)

- `entitlement_state` ∈ `ACTIVE | ENDED | CANCELLED` represents entitlement **lifecycle only**. It is independent of `fulfilled_quantity` / `released_quantity` / `reserved_quantity` / `effective_remaining`.
- One-ACTIVE invariant enforcement via a **SQLite-valid partial unique index** (no subquery):

```sql
CREATE UNIQUE INDEX IF NOT EXISTS idx_contract_allocations_single_active
    ON contract_allocations(unit_id, product_id, fiscal_year)
    WHERE entitlement_state = 'ACTIVE' AND deleted = 0;
```

- The DB constraint is the hard backstop; application/service guards provide domain-level validation and human-readable errors.
- `ENDED` is validity-end only: an `ENDED` allocation with `effective_remaining > 0` remains a fulfillable obligation.
- `CANCELLED` excludes the allocation from normal obligation selection; historical fulfillment is never erased.
- No allocation is reactivated; a new contract creates a new allocation.

### 3.4 Effective remaining (derived, not authoritative)

```text
effective_remaining =
    contracted_quantity
    - fulfilled_quantity
    - released_quantity
    - reserved_quantity
```

Component quantities are authoritative. `effective_remaining` is not stored as authoritative mutable state.

### 3.5 Old-obligation priority (X → Y)

Resolution for a requested `(unit, product, quantity, current_fiscal_year)`:

- **Phase 1:** oldest outstanding historical obligation (`contract fiscal_year < current FY`) with `effective_remaining > 0`, ordered by `fiscal_year ASC, created_at ASC, id ASC`.
- **Phase 2:** current-FY `ACTIVE` entitlement when no outstanding obligation remains.
- No `products.base_price` fallback; no implicit supplier transition.
- If `requested > effective_remaining` of the winning obligation → **reject the entire order** (no partial, no split). Once X is exhausted, the next order resolves to Y.

### 3.6 Single-supplier SupplierOrder

A `SupplierOrder` binds to exactly one supplier. If the resolver maps product lines to different suppliers, the **entire order creation is rejected** and the UNIT creates separate orders. No automatic split; no silent supplier switch; confirmation never redirects X → Y.

### 3.7 Creation vs confirmation

- **Creation:** system resolves supplier from authoritative state; stores `supplier_id`, immutable `supplier_name` snapshot, `unit_id`, `fiscal_year`, and reserves quantities (caller-supplied price/supplier ignored).
- **Confirmation** (`confirm_order_atomic`, single-writer transaction): authoritative re-resolution; verifies the stored supplier still wins, full quantity is still covered, prices still authoritative; then a conditional storage-level UPDATE converts reservation → fulfillment; on zero affected rows the transaction aborts with full rollback (no FIFO/stock-movement residue).

### 3.8 WILAYA obligation release

The only transition path for an old supplier's leftover obligation is a **WILAYA-authorized, auditable release**:

```text
WILAYA decision → auditable release (reason_code) → old allocation no longer blocks → new supplier entitlement usable
```

Recorded in `contract_allocation_exceptions` with a reason code (`SUPPLIER_NON_PERFORMANCE | SUPPLIER_DELAY | SERVICE_CONTINUITY | OTHER_AUTHORIZED`). The pricing resolver never performs this transition; it only reads authoritative state. Release never rewrites `fulfilled_quantity` or historical confirmation records.

### 3.9 TVA — one immutable rate per fiscal year

- Remove `products.tva`.
- `fiscal_year_tax_policy` = the single TVA source per fiscal year (`fiscal_year` PRIMARY KEY, `tva_rate`, WILAYA-set, `frozen` at fiscal close).
- Once established for a fiscal year, the rate may not be changed to a different value; closed fiscal years are immutable.
- All report/TVA consumers read from the fiscal-year policy of the report's fiscal year. The pure math `base × (1 + tva_rate / 100)` remains; the rate input comes from policy, never from Product.
- This supersedes any SEC-087-D drafting that allowed intra-year TVA edits.

### 3.10 ContractCatalog sync (additive V2 package kind)

- New additive kind under the existing V2 signed-package infrastructure: `SyncPackageKind::ContractCatalog`.
- WILAYA publishes the authoritative contract/supplier entitlement projection; UNIT is read-only.
- Uses existing `SyncPackage`, `SyncPackageMetadata`, canonical JSON V2, Ed25519 verification, `package_id` replay protection, source-node validation, `SupportedSchemaWindow`.
- `SYNC_PACKAGE_SCHEMA_VERSION` is **not** bumped unless implementation evidence proves a protocol requirement.
- `ContractCatalog` is not a trust/identity/registry package and is never routed through trust-package handling; it cannot mutate trust/identity/supplier-authority/WILAYA ledger on the consumer side.

### 3.11 Authorization

New actions: `ManageSuppliers`, `ManageContracts`, `ApproveContractPrice`, `CloseContract`, `ManageTaxPolicy`, `ReadContractProjection`, plus release/revoke operations (`ReleaseContractAllocation`, `RevokeContractAllocationRelease`) where required. WILAYA-only operations require `ResourceContext::WilayaNode` + Admin. UNIT cannot create/modify suppliers, create/approve contracts, change agreed price/entitlement/TVA, release obligations, or forge projections, order price, or supplier choice. No parallel authorization framework.

## 4. Consequences

### 4.1 Positive

- Order pricing and supplier are backend-authoritative and auditable.
- Obligations survive fiscal-year boundaries deterministically.
- TVA is consistent per fiscal year and frozen at close.
- One supplier per order is structural (schema + resolver).
- UNIT becomes a pure consumer/operator of authoritative procurement state.

### 4.2 Negative / Cost

- A mixed-supplier order is rejected: UNIT must create separate orders (explicit UX requirement).
- Confirmation can fail if coverage was drained between creation and confirmation (transactional safety over convenience).
- Product import/export loses `tva`/`supplier_name` columns; consumers must migrate (XLSX, report services, imports, IPC, frontend).
- New tables/indexes increase schema surface (`schema_version` stays 1; single baseline).

### 4.3 Compliance

- Runtime Tenet 7 (backend source of truth), A5 (no frontend business rules), §17 (untrusted caller price), §20 (conditional storage guard), C1 (no frozen-invariant violation) preserved.
- No migrations added; `001_initial.sql` remains the single baseline.
- Trust/identity/SEC-009 untouched.

## 5. Migration / Rollout

- Single baseline `001_initial.sql` edit; `schema_version` stays `1`.
- No staged migration history; existing databases are recreated from baseline in dev/test (pre-release).
- Frontend + backend + governance baselines updated as one atomic change-set.

## 6. References

- `docs/architecture/ADRs_INDEX.md` entry added (0055).
- AGENTS.md Layer Map (suppliers/contracts in repositories/services/domain), Critical Isolation Rules (SQL confinement, Frontend isolation, Domain purity).
- ADR-0021 (FIFO determinism), ADR-0025 (SQLite single-writer), ADR-0047 (V2 Ed25519), ADR-0049 (trust-package hardening — unchanged), ADR-0053 (transport sequence — unchanged).