# SEC-087-F Implementation Report — Contract-Centric Procurement (ADR-0055)

Status: Completed (all 14 stages)
Scope: GRPC-Tauri `model-c-b8-work` branch
Reference: `docs/architecture/0055-contract-centric-procurement.md`

## 1. Summary

SEC-087-F replaced per-unit supplier/price assumptions with a contract-centric
procurement model:

* `Supplier` is a first-class entity (M:N with `units` via `unit_suppliers`).
* `Contract` is the pricing + entitlement authority (one UNIT + one Supplier +
  one fiscal year).
* `ContractAllocation` rows are the per-UNIT obligations; one ACTIVE per
  (unit, product, fiscal year) via partial unique index.
* `effective_remaining` is a derived, domain-owned projection
  (`ContractAllocationView`) — never stored, never recomputed by the frontend.
* Exactly one immutable TVA rate per fiscal year, WILAYA-controlled.
* ContractCatalog is an additive V2 sync package kind (WILAYA → UNIT).

## 2. Stage-by-stage delivery

| Stage | Scope | Commit |
|-------|-------|--------|
| 1–10 | ContractCatalog V2 sync package kind, UNIT scoping, replay/sequencing, WILAYA/UNIT import paths, sync fixtures | `07ffa5a` |
| 11 | Procurement IPC commands (25), domain-owned TVA arithmetic (`domain/pricing/price.rs`), audit actions + entities, lifecycle + sync import tests | `426fb2a` |
| 12 | Frontend procurement contract layer (25 wrappers), `SuppliersPage`/`ContractsPage`, `ContractAllocationView` (domain-computed `effective_remaining`), TVA wrapper consolidation, governance baselines recertified | `167e5a6` |
| 13 | XLSX exports (suppliers / contracts / allocations), WILAYA-admin authz actions, frontend export buttons, governance baselines recertified (25→28 exports/commands) | `b7cf1bf` |
| 14 | Full verification + this report | current |

## 3. Delivery details

### 3.1 Backend (stages 11–13)

* `domain/pricing/price.rs` is the single owner of `price_with_tva(base, tva_rate)`
  (P2 / A5). Duplicate in `DailyReportService::calculate_product_price_with_tva`
  removed.
* `FiscalTaxPolicyService` enforces exactly one immutable rate per fiscal year;
  `tva_rate` validated 0–100 (sync fixtures bypass validation and use 19.0).
* Contract lifecycle transitions guarded by `ContractStatus::can_accept/
  can_activate/can_end/can_cancel`; CANCELLED excluded from fulfillment;
  ENDED-with-remaining remains fulfillable.
* Obligation release (`release_contract_allocation`) is WILAYA-only and recorded
  in `contract_allocation_exceptions` (SUPPLIER_NON_PERFORMANCE,
  SUPPLIER_DELAY, SERVICE_CONTINUITY, OTHER_AUTHORIZED); a release may be
  revoked only while safe.
* Authz: suppliers/contracts/price approval/closing/TVA policy/release/revoke
  and the three XLSX exports are WILAYA Admin-only; `ReadContractProjection`
  is WILAYA Admin-only; ContractCatalog import is operational on UNIT nodes.
* Excel exports: `export_suppliers_xlsx`, `export_contracts_xlsx`,
  `export_contract_allocations_xlsx` via `ExcelPort` +
  `XlsxAdapter` (RTL worksheets, Protection-Civile styling), registered in the
  invoke handler.

### 3.2 Frontend (stage 12–13)

* Procurement communication confined to `src/lib/contracts/procurement.contract.ts`
  (28 wrappers); single `invoke` seam in `src/lib/tauri.ts` (Frontend isolation).
* `ContractAllocationView` serializes `effective_remaining` from the backend —
  pages never re-derive business arithmetic (A5 / P2).
* Duplicate TVA helper removed from `src/lib/tauri.ts`; sole owner remains
  `inventory.contract.ts` (`calculateProductPriceWithTva`).
* New pages `SuppliersPage` (`/wilaya/suppliers`) and
  `ContractsPage` (`/wilaya/contracts`) with create/edit, lifecycle
  transitions, agreed-price approval, allocation release/revoke, and Excel
  export actions.

## 4. Governance

* `procurement` domain registered in `DOMAIN_REGISTRY`
  (`scripts/governance/scanner.ts`) with cross-domain exceptions
  `listProducts`, `listUnits`; `getSettings` is universally allowed.
* Baselines recertified: `contracts.snapshot.json` (procurement 28 exports /
  28 commands / 204 LOC), `domain-ownership.snapshot.json` (2 new pages),
  `projection-ownership.snapshot.json` (104 → 123 types).

## 5. Verification results (Stage 14)

| Gate | Command | Result |
|------|---------|--------|
| Architecture | `bun run check:arch` | PASS — zero warnings |
| TypeScript / Svelte | `bun run check` | 0 errors, 0 warnings |
| Rust fmt | `cargo fmt --check` | PASS |
| Rust clippy | `cargo clippy --all-targets` | PASS — zero warnings |
| Rust unit/integration | `cargo test -p grpc` | lib 845 passed; all suites passed except 2 pre-existing baseline failures (see §6); procurement lifecycle 2/2, contract sync suites all green |
| Frontend tests | `bun run test` | 25 files / 159 tests passed |
| Secrets | `node scripts/check_secrets.ts` | PASS |
| Documentation governance | `node scripts/check_docs_governance.ts` | PASS |
| Release integrity | `bun scripts/check_release_integrity.ts` | PASS |

## 6. Pre-existing baseline failures (NOT caused by SEC-087-F)

Both reproduce at the pre-SEC-087-F baseline HEAD
(`e45459d85f73772be916590d704913386a2cd01d`) and remain open:

1. `f1_multi_unit_producer_tests::real_producer_delivery_continuation_and_idempotent_reimport`
   — panics at `tests/f1_multi_unit_producer_tests.rs:788`; a stale re-import
   now resolves to `DuplicateSyncPackage` (sync behavior hardened before this
   feature).
2. `sec038_trust_rotation_fleet_tests::trust_rotation_then_products_export_emit_readable_packages`
   — panics at `tests/sec038_trust_rotation_fleet_tests.rs:275`;
   `{items:[]}` product package lacks `product_rows` (fixture expectation).

Recommended: file these as separate defects and quarantine them from the
certification gate before release.

## 7. References

* ADR-0055: `docs/architecture/0055-contract-centric-procurement.md`
* Agents contract: `AGENTS.md`