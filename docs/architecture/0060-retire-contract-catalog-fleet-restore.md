# ADR 0060: Retire the Contract Catalog FleetRestore Export Form

## Decision Status

**Status: Accepted**

**Date: 2026-09-24**

**ACCEPTED — 2026-09-24 (SEC-087-F follow-on).** This decision retires the
Contract-Catalog-specific `FleetRestore` export form established by ADR-0059
and makes `UnitDistribution` the ONLY Contract Catalog export form. It
partially supersedes ADR-0059 on the clauses enumerated in §Relationship to
ADR-0059; the generic `PackageExportMode::FleetRestore`/fleet-loop machinery
used by other package kinds is untouched.

## Owner

Architecture / Security

## Reference

- ADR-0059 (`docs/architecture/0059-unit-scoped-contract-catalog-export.md`) —
  Unit-Scoped Contract Catalog Export; **the `FleetRestore` form clauses
  enumerated in §Relationship to ADR-0059 are superseded by this decision**;
  the `UnitDistribution` model is retained.
- ADR-0053 (`docs/architecture/0053-unified-per-target-transport-sequence.md`) —
  unified per-target transport, unchanged (the generic fleet loop remains the
  products / admin access producer).
- ADR-0055 (`docs/architecture/0055-contract-centric-procurement.md`) —
  ContractCatalog V2 additive sync kind (SEC-087-F), unchanged in scope.
- AGENTS.md — Runtime Tenet 7 (backend source of truth), A2 contract ownership,
  C1/C2 change control.

## Context

ADR-0059 ratified two explicit export modes for the WILAYA → UNIT
`contract_catalog` sync path: `UnitDistribution { target_unit_code }` and
`FleetRestore`. The research, the importer target-matrix enforcement (C4), and
the defensive analysis of `Ended(B)`-in-A and unscoped-artifact application
converged on one conclusion: a fleet-wide, unscoped Contract Catalog restore
is never a legitimate operation in this system.

- A UNIT must never apply an unscoped whole-fleet catalog: `Ended`
  obligations of other units are not its scope, and atomic whole-package
  application of another scope's rows is prohibited by the per-UNIT status
  policy and by unit scoping (ADR-0059 §6, §11.1).
- A WILAYA must never re-apply a whole-fleet catalog either: the WILAYA is the
  source of truth that produced the catalog; re-importing its own projection
  adds no WILAYA-authoritative value, introduces a second mutation path to the
  same rows, and competes with the single export path for determinism.
- The `FleetRestore` form adds an extra accept-cell (WILAYA + no target →
  ACCEPT) to the target matrix without any call site: no command surfaced it
  to users, and the only exporter that ever produced it was the retired
  `export_contract_catalog_package` command.

This ADR removes the Contract Catalog `FleetRestore` code path entirely —
variant, producer, command, registry entry, and importer acceptance — so the
retired form cannot be reached by any current or future caller.

## Problem

1. `ExportContractCatalogMode::FleetRestore` represents an unscoped artifact
   that no supported operation should produce or consume.
2. The importer matrix (ADR-0059 §13) grants WILAYA an acceptance cell for the
   retired form; after C4 the matrix read "UNIT A + None → REJECT; WILAYA +
   None → ACCEPT". The ACCEPT cell is dead weight and a latent surface for a
   partial restore of another unit's obligations.
3. Keeping a producer (`export_contract_catalog_fleet`), a command
   (`export_contract_catalog_package`), and looser import semantics in place
   violates the determinism and single-source-of-truth tenets: two export
   paths produce different scopes for the same WILAYA-authoritative catalog.

## Decision

1. `ExportContractCatalogMode` loses its `FleetRestore` variant and becomes a
   single-form enum: the only construction of a Contract Catalog export is
   `UnitDistribution { target_unit_code }`.
2. The Contract Catalog fleet producer `export_contract_catalog_fleet` is
   removed. No command, service, or test calls it.
3. The `export_contract_catalog_package` command and its registry entry are
   removed; the remaining `export_contract_catalog_to_units` command is the
   SINGLE Contract Catalog export command. The `Action::ExportContractCatalogPackage`
   authorization is retained as the authz guard for the unit-distribution
   command (WILAYA Admin only).
4. The importer rejects `PackageExportMode::FleetRestore` for
   `contract_catalog` packages **unconditionally** — with or without a
   `target_node_id`, on UNIT and on WILAYA, with a domain-specific clear
   error. The WILAYA + None ACCEPT cell of ADR-0059 §13 is closed.
5. The generic `PackageExportMode::FleetRestore` (package metadata) and the
   fleet emit-loop machinery for OTHER package kinds (products, admin access)
   are unchanged. This ADR retires only the Contract Catalog use of that flag.
6. No schema change, no package-envelope version change, no migration, no
   import of legacy `FleetRestore` Contract Catalog artifacts (they are
   rejected fail-closed, not reinterpreted), and no transport reactivation.

## Relationship to ADR-0059

ADR-0060 **partially supersedes** ADR-0059. The affected statements are
superseded only as enumerated below; all other ADR-0059 decisions remain
authoritative. ADR-0059 itself is not rewritten by this ADR.

Superseded statements:

- **ADR-0059 §3** — the mode-model enum listing showing `FleetRestore` and
  §3 invariant 3 *"`FleetRestore` is the only fleet-wide form. It carries no
  target code and is the sole producer of an unscoped artifact"*: the
  variant is removed; the mode model is single-form. §3 invariant 4 (closed
  enum; any future mode is a separate accepted decision) is retained.
- **ADR-0059 §7 (FleetRestore Status Policy)** — the whole clause: there is
  no FleetRestore artifact, so no all-status-retention policy applies to any
  Contract Catalog export or import.
- **ADR-0059 §13 importer target matrix** — the cell "WILAYA | `None` (fleet)
  | **ACCEPT** (restore scope)" is superseded and becomes a rejection. The
  `UnitDistribution` cells are retained.
- **ADR-0059 §19 testing-contract item 2** — *"`ExportContractCatalogMode::FleetRestore`
  has no target"*: the variant no longer exists; the corresponding unit test
  was removed.
- **ADR-0059 §10** — any reference to a fleet empty-catalog import policy:
  Moot for Contract Catalog; only `UnitDistribution` empty-contract allowance
  applies.

Retained from ADR-0059 (explicitly NOT superseded):

- `UnitDistribution { target_unit_code }` structural require-a-target model.
- Target identity = `units.code` (authoritative via `resolve_unit_transport_target`),
  never `units.id` / filename / renderer label.
- Per-UNIT status policy (`Accepted | Active | Ended`; `Proposed | Cancelled`
  excluded) and `Ended(B)` never in unit A's artifact.
- `SyncPackageMetadata.target_node_id` additive serde contract and
  `SYNC_PACKAGE_SCHEMA_VERSION` **V3** (no bump).
- Zero-contract UNIT artifact allowance.
- Encryption & confidentiality boundary and SQL `?1` binding hardening.
- Determinism, target binding, replay/provenance preservation, and failure
  isolation for the `UnitDistribution` path.
- ADR-0059's own acceptance does not extend to the superseded clauses above.

## Implementation Boundary / Non-Goals

Approved implementation boundary (this change):

- `src-tauri/src/application/usecases/exports/types.rs` — remove the
  `FleetRestore` variant from `ExportContractCatalogMode` and the
  `fleet_restore_has_no_target` unit test; update doc comments.
- `src-tauri/src/application/usecases/exports/export_contract_catalog_dataset.rs` —
  remove `execute_fleet` and the fleet branch of `execute()`; fold the
  producer validation into a single `UnitDistribution`-scoped check.
- `src-tauri/src/application/services/fleet_package_export.rs` — remove
  `export_contract_catalog_fleet`; `services/mod.rs` re-export updated.
- `src-tauri/src/commands/import_export.rs` — remove
  `export_contract_catalog_package` + `_impl`; `commands/registry.rs` updated.
- `src-tauri/src/application/usecases/sync/import_contract_catalog_package.rs` —
  unconditional rejection of Contract Catalog `FleetRestore` (any target, any
  node type).
- Tests: export/import/dataset tests updated; the WILAYA full-restore import
  test flips to a rejection test; the replay test moves to a
  `UnitDistribution` package on a UNIT node.
- `docs/architecture/0060-retire-contract-catalog-fleet-restore.md` +
  `ADR_INDEX.md` row + partial-supersession annotation on the ADR-0059 row.

Non-goals — this ADR does NOT:

- modify `PackageExportMode` (metadata) or the generic fleet loop;
- change the products / admin access export paths;
- change `resolve_unit_transport_target` or any `UnitDistribution` semantics;
- change the authorization model (`Action::ExportContractCatalogPackage`);
- add or remove schema / envelope versions / migrations;
- reinterpret or accept legacy `FleetRestore` Contract Catalog artifacts under
  another mode;
- reactivate any transport.

No speculative future architecture is introduced by this ADR.

## Acceptance Criteria

For this ADR to be considered accepted by the owner, all of the following must
hold:

1. `ExportContractCatalogMode` is single-form; no `FleetRestore` variant,
   producer, command, registry entry, or `execute_fleet` exists anywhere in
   Rust or TypeScript.
2. Contract Catalog `FleetRestore` import is rejected unconditionally on every
   node type, with or without a target, fail-closed and pre-mutation.
3. `UnitDistribution` export/import behavior is unchanged and all retained
   ADR-0059 invariants still pass their tests.
4. The generic `PackageExportMode::FleetRestore` and the products / admin
   access fleet exports are untouched.
5. A global reference audit finds no stale Contract Catalog `FleetRestore`
   reference outside ADR-0059's historical record and this ADR.
6. `bun run check:arch`, `cargo check`, and the Contract Catalog export/import
   test suites pass; no clippy warnings.
7. The status is recorded as `Accepted` in `ADR_INDEX.md` and the ADR-0059 row
   is annotated as partially superseded by ADR-0060.

On owner acceptance (2026-09-24), the ADR status was recorded as `Accepted` in
`ADR_INDEX.md`, the index entry was synchronized, and implementation was
authorized subject to the repository's governance gate.