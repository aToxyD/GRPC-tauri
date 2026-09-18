# ADR 0059: Unit-Scoped Contract Catalog Export

# Decision Status

**ACCEPTED — 2026-09-17.**

This ADR establishes the protocol/domain contract foundation for a
UNIT-scoped Contract Catalog distribution, eliminating the historical
`Option<TargetUnit>` ambiguity on the WILAYA → UNIT `contract_catalog`
sync path (ADR-0055 §3.10 / SEC-087-F). In its first phase it ratifies the
explicit export mode model, the authoritative target-node identity, the
per-UNIT status eligibility rule, the `SyncPackageMetadata.target_node_id`
field with its serialization behavior, and the importer target-validation
policy. It purposely does NOT implement exporter/importer wiring.

| Item | Status |
|------|--------|
| Explicit mode model `ExportContractCatalogMode` (no `Option<TargetUnit>` ambiguity) | **ACCEPTED** |
| `UnitDistribution { target_unit_code }` requires a target code structurally | **ACCEPTED** |
| Input wrapper with `mode` field deferred to the exporter/use-case wiring phase | **ACCEPTED** |
| Target identity = `units.code` (authoritative via `resolve_unit_transport_target`) | **ACCEPTED** |
| Per-UNIT status policy: `Accepted \| Active \| Ended` included; `Proposed \| Cancelled` excluded | **ACCEPTED** |
| `Ended(B)` MUST NEVER appear in unit A's artifact (unit scope trumps Ended eligibility) | **ACCEPTED** |
| FleetRestore status policy: ALL statuses retained (separate from per-UNIT rule) | **ACCEPTED** |
| `SyncPackageMetadata.target_node_id: Option<String>` (`units.code`, `None` = fleet) — contract RATIFIED; field implemented in Phase 2 | **ACCEPTED (Phase 2 implementation)** |
| Additive serde-compatible field; `SYNC_PACKAGE_SCHEMA_VERSION` stays **V3**; no bump | **ACCEPTED** |
| Per-UNIT package may contain zero contracts (`contracts: []`) — documented, not enforced | **ACCEPTED** |
| Dataset stays 4-dimension (no `products` dimension); product lines carry identity/name denormalized | **ACCEPTED** |
| Shared `GRPC_APP_KEY` age encryption retained; `target_node_id` = authenticity, not confidentiality | **ACCEPTED** |
| Importer target matrix documented (Phase 1) — enforcement wired in importer phase | **ACCEPTED** |
| SQL parameter-binding hardening for `repositories/contracts.rs` deferred to a later phase | **ACCEPTED (deferred)** |
| No migration / no new sync envelope version / no V4 / no transport reactivation | **ACCEPTED** |
| Phase 1 does NOT wire exporter pipeline or importer validation | **ACCEPTED** |
| Phase 1 code deliverables = `ExportContractCatalogMode` + `ContractStatus::is_catalog_exportable` only; metadata field and input wrapper deferred | **ACCEPTED** |

# Date

2026-09-17 — accepted by the governance reviewer (ADR-0059).

# Relation to Existing Documents

| Document | Relation |
|----------|----------|
| ADR-0055 (Contract-Centric Procurement) | **Extended (not amended)** — ADR-0055 §3.10 ratified `contract_catalog` as an additive sync kind (WILAYA → UNIT) with a fleet-wide dataset. This ADR refines that path with an explicit distribution mode, a target identity, and a per-UNIT status policy. ADR-0055 pricing/entitlement/TVA authority is unchanged. |
| ADR-0057 (Sync Protocol V3 Schema Gate) | **Complied with** — the `[V3, V3]` closed import window and `SYNC_PACKAGE_SCHEMA_VERSION = V3` remain. This ADR adds an OPTIONAL metadata field; V0/V1/V2 reject and V4+ reject semantics are untouched. |
| ADR-0004 (Protocol Changes Are Breaking) | **Commented on** — the added `target_node_id` field is additive and optional; for `FleetRestore` packages the field is absent and canonical bytes are byte-identical (no protocol break, determinism preserved). For `UnitDistribution` packages the field is the intentional target-bearing marker. No version bump is introduced because the interaction is pre-production and the closed V3 window is the intentional cutover (ADR-0004 steps 1–4 rationale documented). |
| RFC `2026-08-04-node-identity-trust` §3.4.1 / ADR-0038 | **Complied with** — authenticity remains carried by `issuer_identity_id` + the Ed25519 `signature_version = 2` signature. `target_node_id` adds NO authenticity and NO confidentiality. |
| SEC-056D / SEC-057 + ADR-0053 | **Complied with** — `target_node_id` is NOT a transport-sequence field. Per-target transport sequencing remains permanently retired; the field is import-targeting metadata only. |
| ADR-0058 (Product Unit-Config Immutability) | **Precedent relied on** — same ADR form (Decision Status table, Governance Records, no-freeze amendment) and same fail-closed posture for a domain/semantic violation. |
| ADR-0041 (GRPC_APP_KEY Provisioning) | **Complied with** — package encryption remains the shared node key; no recipient-specific key material is introduced. |
| `ARCHITECTURE_FREEZE.md` | **No amendment required** — every Section 2 frozen clause is preserved; this ADR is additive and touches no frozen contract. |
| AGENTS.md §13 | **Supersedes conflicts** — where any temporary/design document conflicts with this ADR, this ADR governs. |

# 1. Context

## 1.1 The Contract Catalog sync path

ADR-0055 §3.10 ratified `contract_catalog` as an additive sync kind
(WILAYA → UNIT). The dataset is a WILAYA-authoritative, read-only projection:

- suppliers and UNIT ↔ supplier associations (`ContractCatalogUnitSupplierLink`);
- contracts with product lines + per-UNIT allocations and their release
  exceptions (`ContractCatalogContractRow`);
- fiscal-year `tax_policies`.

The existing pipeline has exactly **one** fleet-wide export form
(`fleet_package_export.rs` → `export_contract_catalog_dataset.rs`): a WILAYA
operator issues the full catalog over the whole fleet.

## 1.2 The ambiguity being removed

The design analysis established that a UNIT-scoped distribution must be a
first-class semantic. Modeling the target as `Option<TargetUnit>` with
`None == FleetRestore` is forbidden because:

1. `None` is silent — a missing target degrades into a fleet-wide artifact
   with no structural representation of the intent error;
2. it permits a UNIT-scoped caller to accidentally produce a fleet-wide
   package;
3. it conflates "no target" with "target = entire fleet" in one type.

This ADR eliminates that ambiguity with an explicit, closed mode model
(§3).

## 1.3 Determinism and reproducibility

Offline-first determinism demands that repeated issuance from identical
persisted state produce identical outcomes. For a `UnitDistribution`
artifact, identical inputs must include the target unit code; for a
`FleetRestore` artifact the whole-fleet scope is the defining input. The
locked metadata design keeps `FleetRestore` canonical bytes unchanged and
adds the target marker only to `UnitDistribution` packages (§9).

# 2. Decision Scope (Phase 1)

This ADR is ratified in a **foundation phase** that delivers, in code:

- the `ExportContractCatalogMode` enum in the exports usecase types
  (`src-tauri/src/application/usecases/exports/types.rs`);
- `ContractStatus::is_catalog_exportable()` (per-UNIT status predicate) in
  `src-tauri/src/models/contract.rs`.

It deliberately does **not** yet deliver or wire:

- the `SyncPackageMetadata.target_node_id` field (§8) — deferred to Phase 2
  because implementing it requires touching the protected production
  serialization/export path;
- the mode into `ExportContractCatalogInput` (see §4);
- the exporter pipeline (`export_contract_catalog_dataset.rs`,
  `fleet_package_export.rs`, `identity_signed_export_service.rs`);
- the importer validation (`import_contract_catalog_package.rs`,
  `sync_import_execution_service.rs`);
- repository SQL hardening (`repositories/contracts.rs`).

Those are explicit later-phase workstreams that require their own
implementation authorization (§8, §15, §17).

# 3. Export Mode Model

The locked mode model, established in the exports usecase types:

```rust
pub enum ExportContractCatalogMode {
    FleetRestore,
    UnitDistribution { target_unit_code: String },
}
```

Invariants:

1. **No `Option<TargetUnit>`**: the mode is never represented as an optional
   target whose absence means fleet-wide.
2. **`UnitDistribution` structurally requires a target code.** The type
   cannot represent a missing target; there is no "distribution with no
   target".
3. **`FleetRestore` is the only fleet-wide form.** It carries no target code
   and is the sole producer of an unscoped artifact.
4. The mode is a closed enum; any future mode (e.g., multi-unit scope) is a
   separate accepted decision.

# 4. Input Wrapper Deferral

A mode-carrying input wrapper

```rust
pub struct ExportContractCatalogInput { pub mode: ExportContractCatalogMode }
```

is the eventual consumer-facing input. It is **deferred** to the
exporter/use-case wiring phase because the existing
`ExportContractCatalogInput` is an empty marker struct whose only
construction site (`fleet_package_export.rs`) is out of the Phase 1
boundary. Introducing the field now would force a constructor rewrite
inside a file whose wiring is not part of this phase. Phase 1 therefore
delivers the enum only, without compatibility shims, overloaded
constructors, or speculative refactoring of the existing export APIs.

# 5. Target Node Identity

The target of a `UnitDistribution` package is identified by **`units.code`**
— the authoritative transport target identity — never by `units.id`, a
filename, a display label, or a renderer label.

- Resolution MUST go through the existing fail-closed resolver
  `transport_target::resolve_unit_transport_target`
  (`src-tauri/src/application/services/transport_target.rs`, `units.code`).
- An unknown / non-existent code MUST be a hard error; there is no
  auto-create and no silent fallback.
- `target_node_id` in `SyncPackageMetadata` therefore carries `units.code`.
- Scope: a UNIT node is only ever expected to receive packages whose
  `target_node_id` equals its own `units.code` (or fleet scopes it is
  entitled to, §13).

# 6. Per-UNIT Status Policy

For a **UNIT-scoped** Contract Catalog artifact, the status eligibility rule
is:

```
is_catalog_exportable(status) =
    status ∈ { Accepted, Active, Ended }
```

- `Accepted | Active | Ended` are INCLUDED.
- `Proposed | Cancelled` are EXCLUDED.
- The predicate `ContractStatus::is_catalog_exportable()` is purely
  classificatory: it observes the status and never mutates it.

Additional MUST:

- **`Ended(B)` belonging to a different UNIT MUST NEVER appear in unit A's
  artifact.** UNIT scope trumps Ended eligibility. A contract whose
  authoritative owner unit differs from the target unit is excluded even
  when its status is `Ended`. Enforcement lives in the exporter's unit
  scoping (later phase); the predicate is the status-level rule only.

The exporter phase will apply the predicate AND the unit-scope filter over
`ContractCatalogContractRow`/allocations when building a `UnitDistribution`
dataset.

# 7. FleetRestore Status Policy

`FleetRestore` is a **separate policy**, not the per-UNIT rule applied
fleet-wide:

- Fleet/restore artifacts RETAIN **all** statuses — `Proposed`, `Accepted`,
  `Active`, `Ended`, `Cancelled` — for WILAYA restore/reissue fidelity.
- `is_catalog_exportable()` MUST NOT be applied to `FleetRestore`.
- This matches the stated restore requirement: a WILAYA node re-issuing the
  authoritative catalog must be able to reproduce the full authoritative
  state, including contracts that would be excluded from a UNIT distribution.

The two policies are distinct decision points and are documented as such;
no code path may implicitly conflate them.

# 8. Metadata: `target_node_id` (Phase 2 contract)

This ADR RATIFIES the metadata contract; the actual
`SyncPackageMetadata.target_node_id` field is NOT introduced in Phase 1.
Phase 2 introduces it together with the exporter/serializer/importer wiring
as part of target-binding implementation, because adding the field requires
modifying the protected production serialization/export path. Phase 1
introduces no placeholder, no `None`-valued construction, no duplicate
metadata type, and no speculative serialization change.

The ratified contract:

- **`target_node_id` is MANDATORY for `UnitDistribution`** — the field is
  present and carries the target's `units.code`.
- **`target_node_id` is ABSENT for `FleetRestore`** — serde
  `default` + `skip_serializing_if = Option::is_none`, so fleet package
  canonical bytes stay unchanged.
- **Cryptographic coverage:** once the exporter/serialization path is
  implemented, the value is covered by the package Ed25519 signature
  (`signature_version = 2`) exactly like every other metadata field, because
  the signature is computed over the whole canonical package. It therefore
  cannot be altered in transit or at rest without invalidating the package.

Serialization contract when implemented:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub target_node_id: Option<String>,
```

- `Some(units.code)` — present in the serialized envelope (UnitDistribution);
- `None` — field ABSENT (FleetRestore);
- deserialization of a missing field yields `None` (backward compatible);
- a present field deserializes to its exact string (no normalization, no
  trimming, no case folding).

Semantics:

- Meaningful ONLY for the `contract_catalog` package kind; other kinds are
  unaffected and never inspect it in an authorization-relevant way.
- Provides **authenticity of intent** (which node the package is for); it
  does NOT provide confidentiality (§12).

# 9. Schema Version: No Bump (V3 retained)

`SYNC_PACKAGE_SCHEMA_VERSION` stays **V3**. Rationale:

1. **Closed V3 window.** ADR-0057 fixed a closed `[V3, V3]` import window as
   the intentional pre-production cutover. A version bump would force a
   window change this ADR does not authorize.
2. **Additive serde-compatible field.** `target_node_id` is optional, is
   serialized only when present, and deserializes as `None` when absent.
   Existing V3 parsers that ignore unknown/absent fields remain conformant.
3. **Byte stability for FleetRestore.** Because `None` omits the field,
   FleetRestore payload bytes are unchanged — deterministic canonical bytes
   preserved (ADR-0009 canonical sorted-key JSON; signature covers the whole
   package via `serde_json::to_value`).
4. **Intentional target-bearing bytes for UnitDistribution.** A
   `UnitDistribution` package's canonical bytes carry the target marker by
   construction, making the artifact self-identifying for verification.

No V4, no timestamp/build/version/heuristic boundaries, no legacy
compatibility path.

This rationale is contract-level: no field bytes exist until Phase 2
introduces the serialization together with the exporter/serializer wiring.

# 10. Zero-Contract Semantics

A per-UNIT `UnitDistribution` artifact MAY legitimately contain
`contracts: []` (no exportable contracts for that unit in the scoped fiscal
state).

- Phase 1 documents this; it is NOT yet enforced.
- The later-phase importer must accept an empty contracts array for a
  UNIT-scoped package that is otherwise valid, because a target unit may
  legitimately have no exportable contracts.
- The existing whole-package rule that rejects empty `contracts` for the
  fleet artifact (`import_validation.rs`) is a separate policy for the
  current fleet form; the per-UNIT empty allowance differentiates the modes.

# 11. Dataset Scope

## 11.1 Unit-scoped dimensions

For a `UnitDistribution`, the produced dataset is restricted to the target
unit's scope:

- contracts + product lines + allocations + release exceptions scoped to the
  target unit (subject to §6 status filter);
- `unit_supplier_links` for the target unit;
- the suppliers REFERENCED by the scoped links/contracts.

## 11.2 Global dimension

- `tax_policies` remain GLOBAL: every tax policy is carried regardless of
  the target unit. The importer's existing global tax-policy application is
  preserved.

## 11.3 No products dimension

The dataset has NO `products` dimension. Product identity and name are
denormalized into the product lines. The unit-consumable rows
(`UnitEntitlementRow`) are derived by the application layer; the dataset
shipping boundary is the four current dimensions
(`suppliers`, `unit_supplier_links`, `contracts`, `tax_policies`).

# 12. Encryption & Confidentiality Boundary

- Package encryption remains the shared node key
  (ADR-0041 `GRPC_APP_KEY` age encryption). No recipient-specific encryption
  is introduced.
- `target_node_id` provides **authenticity of intended recipient**, not
  confidentiality.
- **Data isolation is payload selection**: a `UnitDistribution` package only
  contains the target unit's rows. A recipient that decrypts a package it
  can receive sees only the rows selected for it.
- A UNIT never receives another unit's rows because those rows are not in
  the payload; this is the isolation boundary, not any key split.

# 13. Importer Target Validation Matrix

Documented for Phase 1; enforcement is wired in the importer phase. The
matrix is keyed on the receiving node's own `units.code` and the package's
`target_node_id`.

| Importer node | `target_node_id` | Outcome |
|---------------|------------------|---------|
| UNIT A | `A.code` | **ACCEPT** (scope match) |
| UNIT A | `B.code` (≠ A) | **REJECT** (wrong target, fail-closed) |
| UNIT A | `None` (fleet) | **REJECT** (UNIT is not a fleet scope) |
| WILAYA | `None` (fleet) | **ACCEPT** (restore scope) |
| WILAYA | `A.code` | **REJECT** (WILAYA is not the target) |

Rationale: a UNIT never applies fleet-scoped artifacts (it has no fleets
below it), and a WILAYA never applies a unit-scoped artifact. Every denial
is a hard, fail-closed, before-mutation rejection of the whole package,
consistent with the single-transaction import boundary and whole-package
atomicity precedents (ADR-0056, ADR-0058 §5).

# 14. Fail-Closed Denial Semantics

Any importer-side violation — status rule breach, `Ended(B)` in A's
artifact, target mismatch (§13), dataset scope breach — MUST:

- reject the ENTIRE package;
- be fail-closed;
- occur before Product/inventory/mutation;
- be recorded as an audit-visible deterministic rejection;
- produce zero partial application.

No silent ignore, no partial application, no interactive conflict
resolution, no legacy fallback (ADR-0058 §5 precedent).

# 15. Deferred Work (later phases, separate authorization)

1. **Exporter wiring** — per-UNIT dataset selection in
   `export_contract_catalog_dataset.rs`; fleet loop + per-target dataset
   precompute in `fleet_package_export.rs`; metadata population in
   `identity_signed_export_service.rs`.
2. **Importer wiring** — target matrix enforcement + `units.code`
   resolution in `import_contract_catalog_package.rs` /
   `sync_import_execution_service.rs`; `resolve_importer_unit_id` extended to
   return `unit.code`; empty-contracts allowance.
3. **SQL hardening** — replacement of the manual
   `unit_id = '...'` escaping at `repositories/contracts.rs` with typed
   parameter binding (`?1`), repository-layer confined, zero business logic
   change.
4. **Input wrapper** — `ExportContractCatalogInput { mode }` introduction
   (§4).
5. **Metadata field** — introduce `SyncPackageMetadata.target_node_id` with
   its serde contract and tests, together with the exporter/serializer wiring
   that the value flows through (§8).

# 16. Non-Goals

This ADR does **not**:

1. implement exporter or importer wiring (§2, §15);
2. introduce recipient-specific encryption or any key split (§12);
3. add a `products` dimension or change the four-dimension dataset shape;
4. alter lifecycle rules of `ContractStatus` or any transition predicate;
5. make the Excel (`.entitlements`) export consume per-unit mode, apply
   target filtering, or change renderer authority (file title is not a
   security boundary);
6. use a filename, rendering label, or `units.id` as the security identity;
7. do a broad SQL refactor (only the targeted hardening above, later);
8. migrate the schema, bump the envelope version, add tables/columns, or
   reopen the V3 window;
9. add a legacy compatibility path or V4;
10. apply `is_catalog_exportable` to `FleetRestore` (§7);
11. define the frontend's mode UI or add any frontend business logic;
12. change transport sequencing (still permanently retired).

# 17. Implementation Boundary

Phase 1 (this ratification) delivers: the mode enum (§3) and the status
predicate (§6), plus their unit tests and this ADR. The metadata field (§8),
the input wrapper (§4), and everything in §15 remain out of scope until
separately authorized.

# 18. Testing Contract

Phase 1 tests (included in this change):

1. `ExportContractCatalogMode::UnitDistribution` requires a target code
   (structural; no missing-target representation).
2. `ExportContractCatalogMode::FleetRestore` has no target.
3. `is_catalog_exportable`: `Accepted`/`Active`/`Ended` → true;
   `Proposed`/`Cancelled` → false.
4. Predicate is a pure observation: calling it never mutates the status.

Later-phase test contract (expansion): metadata serde tests for §8
(`None` omits the field; `Some(code)` emits the exact code; missing field
deserializes to `None`; present field preserved; round-trip through
`serde_json::Value`); importer target matrix acceptance/rejection;
`Ended(B)` exclusion; empty `contracts: []` acceptance for UNIT-scoped;
fleet all-status retention.

# 19. Governance Records

- Follow-on SEC-087-F decision record; ADR-0055 §3.10 reserved this path.
- Accepted as number **0059**; the `ADR_INDEX.md` row is added per
  `ADR_INDEX.md` policy (sequential 4-digit numbering, statuses, owner).
- `ARCHITECTURE_FREEZE.md` requires **no amendment**: Section 2 frozen
  clauses are preserved; this ADR is additive.
- Pre-production environment (ADR-0057 §2.3 clean-cutover precedent); no
  deployed fleet to migrate and no legacy packages to reinterpret.
- AGENTS.md §13 precedence: where a temporary/design document conflicts,
  this ADR governs.