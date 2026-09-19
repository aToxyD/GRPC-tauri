# ADR 0059: Unit-Scoped Contract Catalog Export

# Decision Status

**ACCEPTED — 2026-09-17.**

> **Implementation status (post C1–C4).** The exporter/importer wiring, the
> `SyncPackageMetadata.target_node_id` field, the mode-carrying input
> wrapper, the importer target-binding matrix, and the
> `repositories/contracts.rs` SQL hardening — all admitted deferred items in
> the Phase 1 ratification (§4, §8, §16) — were subsequently implemented, and
> the functional scope is complete at HEAD `f14ca73`:
>
> | Phase | Commit | Scope |
> |-------|--------|-------|
> | C1 — authenticated export-mode metadata | `26b6dd8` | `target_node_id` + `export_mode` serde contract, canonical-signature coverage, V3 retained |
> | C2 — export reproducibility persistence | `54d52bf` | export snapshot `export_mode` / `target_node_id` persistence |
> | C3 — unit-scoped contract catalog export | `917f0ad` | exporter wiring, per-UNIT dataset selection/validation, `ExportContractCatalogInput { mode }`, SQL `?1` binding hardening |
> | C4 — import target binding | `f14ca73` | importer target-matrix enforcement, `resolve_importer_unit`, empty-`UnitDistribution` allowance, `TARGET_REJECTED` classification |

This ADR established the protocol/domain contract foundation for a
UNIT-scoped Contract Catalog distribution, eliminating the historical
`Option<TargetUnit>` ambiguity on the WILAYA → UNIT `contract_catalog`
sync path (ADR-0055 §3.10 / SEC-087-F). In its first phase it ratified the
explicit export mode model, the authoritative target-node identity, the
per-UNIT status eligibility rule, the `SyncPackageMetadata.target_node_id`
field with its serialization behavior, the importer target-validation
policy, and the export-boundary invariants (producer selected-dataset
validation, `in_scope` as defense-in-depth, failure isolation, and
replay/provenance preservation, §15). The Phase 1 ratification purposely did
NOT implement exporter/importer wiring; that wiring and every §16 deferred
item were subsequently delivered by C1–C4 (implementation-status table
above). Where the body text says a control is "deferred", "future", "not yet
implemented", or "Phase 2", those statements describe the Phase 1
ratification and were superseded as mapped in the table, the annotated
sections, and §16.

| Item | Decision status | Implementation status (post C1–C4) |
|------|-----------------|-------------------------------------|
| Explicit mode model `ExportContractCatalogMode` (no `Option<TargetUnit>` ambiguity) | **ACCEPTED** | **IMPLEMENTED** (Phase 1 + C3) |
| `UnitDistribution { target_unit_code }` requires a target code structurally | **ACCEPTED** | **IMPLEMENTED** (Phase 1 + C3) |
| Input wrapper with `mode` field deferred to the exporter/use-case wiring phase | **ACCEPTED (deferred)** | **IMPLEMENTED** (C3 — `ExportContractCatalogInput { mode }`) |
| Target identity = `units.code` (authoritative via `resolve_unit_transport_target`) | **ACCEPTED** | **IMPLEMENTED** (C3 exporter / C4 importer) |
| Per-UNIT status policy: `Accepted \| Active \| Ended` included; `Proposed \| Cancelled` excluded | **ACCEPTED** | **IMPLEMENTED** (Phase 1 predicate; C3 exporter enforcement) |
| `Ended(B)` MUST NEVER appear in unit A's artifact (unit scope trumps Ended eligibility) | **ACCEPTED** | **IMPLEMENTED** (C3 — SQL unit-scope + dataset validator) |
| FleetRestore status policy: ALL statuses retained (separate from per-UNIT rule) | **ACCEPTED** | **IMPLEMENTED** (C3 — fleet path unchanged) |
| `SyncPackageMetadata.target_node_id: Option<String>` (`units.code`, `None` = fleet) — contract RATIFIED; field implemented in Phase 2 | **ACCEPTED (Phase 2 implementation)** | **IMPLEMENTED** (C1 — field + serde contract; C4 — import binding) |
| Additive serde-compatible field; `SYNC_PACKAGE_SCHEMA_VERSION` stays **V3**; no bump | **ACCEPTED** | **IMPLEMENTED** (C1 — V3 retained) |
| Per-UNIT package may contain zero contracts (`contracts: []`) | **ACCEPTED (documented)** | **IMPLEMENTED** (C3 exporter; C4 importer empty-`UnitDistribution` allowance) |
| Dataset stays 4-dimension (no `products` dimension); product lines carry identity/name denormalized | **ACCEPTED** | **IMPLEMENTED** (C3) |
| Shared `GRPC_APP_KEY` age encryption retained; `target_node_id` = authenticity, not confidentiality | **ACCEPTED** | Retained unchanged (C1–C4) |
| Importer target matrix | **ACCEPTED (documented)** | **IMPLEMENTED** (C4 — `validate_contract_catalog_target_for_import`) |
| SQL parameter-binding hardening for `repositories/contracts.rs` | **ACCEPTED (deferred)** | **IMPLEMENTED** (C3 — typed `?N` binding) |
| No migration / no new sync envelope version / no V4 / no transport reactivation | **ACCEPTED** | Retained (C1–C4) |
| Phase 1 does NOT wire exporter pipeline or importer validation | **ACCEPTED** | **SUPERSEDED** (C3 wired the exporter; C4 wired importer validation) |
| Phase 1 code deliverables = `ExportContractCatalogMode` + `ContractStatus::is_catalog_exportable` only; metadata field and input wrapper deferred | **ACCEPTED** | **SUPERSEDED** (C1–C4 delivered the deferred items) |

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

At ratification it deliberately did **not** deliver or wire:

- the `SyncPackageMetadata.target_node_id` field (§8) — deferred because
  implementing it requires touching the protected production
  serialization/export path;
- the mode into `ExportContractCatalogInput` (see §4);
- the exporter pipeline (`export_contract_catalog_dataset.rs`,
  `fleet_package_export.rs`, `identity_signed_export_service.rs`);
- the importer validation (`import_contract_catalog_package.rs`,
  `sync_import_execution_service.rs`);
- repository SQL hardening (`repositories/contracts.rs`).

Those were explicit later-phase workstreams requiring their own
implementation authorization (§8, §16, §18). Each was subsequently
authorized and implemented: the metadata field + serde contract (C1,
`26b6dd8`), export reproducibility persistence (C2, `54d52bf`), the exporter
pipeline, the mode-carrying input wrapper, and the SQL hardening (C3,
`917f0ad`), and the importer validation with the target-binding matrix (C4,
`f14ca73`). §16 records the itemized reconciliation; nothing listed here
remains deferred.

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

is the eventual consumer-facing input. It was **deferred** to the
exporter/use-case wiring phase because the existing
`ExportContractCatalogInput` was an empty marker struct whose only
construction site (`fleet_package_export.rs`) was out of the Phase 1
boundary. Introducing the field then would have forced a constructor rewrite
inside a file whose wiring was not part of the phase. Phase 1 therefore
delivered the enum only, without compatibility shims, overloaded
constructors, or speculative refactoring of the existing export APIs.

**Current state (C3).** The deferral is resolved. The mode-carrying input
wrapper is implemented —

```rust
pub struct ExportContractCatalogInput { pub mode: ExportContractCatalogMode }
```

— defined at `src-tauri/src/application/usecases/exports/types.rs` and
consumed by the export dataset builder
(`application/usecases/exports/export_contract_catalog_dataset.rs`) and the
fleet loop (`fleet_package_export.rs`), which now constructs per-target
inputs instead of a shared empty marker struct.

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
  when its status is `Ended`. The predicate is the status-level rule only;
  the exporter applies the unit-scope filter at the SQL boundary.

**Current state (C3).** The exporter applies the predicate AND the
unit-scope filter over `ContractCatalogContractRow`/allocations when building
a `UnitDistribution` dataset: `list_catalog_exportable_contracts_for_unit`
scopes at the SQL boundary (`WHERE unit_id = ?1 ... AND status IN
('accepted','active','ended')`), so an `Ended(B)` row is excluded from unit
A's artifact by construction and the `Ended(B)` MUST NEVER requirement is
enforced in code.

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

# 8. Metadata: `target_node_id` (contract ratified; field implemented in C1)

This ADR RATIFIED the metadata contract. The actual
`SyncPackageMetadata.target_node_id` field was NOT introduced in Phase 1; it
was introduced in C1 (`26b6dd8`) together with the C3 exporter/serializer
and C4 importer wiring as part of target-binding implementation, because
adding the field required modifying the protected production
serialization/export path. Phase 1 introduced no placeholder, no
`None`-valued construction, no duplicate metadata type, and no speculative
serialization change; the current state is the C1 field definition.

The ratified contract:

- **`target_node_id` is MANDATORY for `UnitDistribution`** — the field is
  present and carries the target's `units.code`.
- **`target_node_id` is ABSENT for `FleetRestore`** — serde
  `default` + `skip_serializing_if = Option::is_none`, so fleet package
  canonical bytes stay unchanged.
- **Cryptographic coverage (implemented C1/C3):** the value is covered by the
  package Ed25519 signature (`signature_version = 2`) exactly like every
  other metadata field, because the signature is computed over the whole
  canonical package — the canonical-coverage test
  `authenticated_fields_change_canonical_integrity_and_signature_bytes` in
  `package_metadata.rs` shows that adding `export_mode` / `target_node_id`
  changes both the canonical integrity bytes and the canonical signature
  bytes, i.e. the authenticated fields participate in the
  integrity/signature material. It therefore cannot be altered in transit or
  at rest without invalidating the package.

Serialization contract (implemented in C1):

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

At ratification this rationale was contract-level (no field bytes existed).
Field bytes now exist: C1 introduced the serialization together with the
canonical-signature coverage test, and C3 wired it through the exporter
serializer.

# 10. Zero-Contract Semantics

A per-UNIT `UnitDistribution` artifact MAY legitimately contain
`contracts: []` (no exportable contracts for that unit in the scoped fiscal
state).

- Phase 1 documented this; the allowance is now enforced (C3 exporter / C4
  importer).
- The importer (C4) accepts an empty contracts array for a
  `UnitDistribution` package that is otherwise valid, because a target unit
  may legitimately have no exportable contracts: `UnitDistribution` is
  exempted from the non-empty `contracts` rule in
  `validate_contract_catalog_package_for_import`.
- The whole-package rule that rejects empty `contracts` for the fleet
  artifact (`FleetRestore` import validation) is a separate policy for the
  fleet form; the per-UNIT empty allowance differentiates the modes.

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

Documented in Phase 1; enforcement is wired and implemented (C4, `f14ca73`).
The matrix is keyed on the receiving node's own `units.code` and the
package's `target_node_id`. Enforcement lives in
`validate_contract_catalog_target_for_import` inside the importer usecase,
after structural validation and the provenance/source check and before
replay dedup and any application (§15.2).

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

# 15. Export Boundary Invariants

This section records four mandatory architectural invariants governing the
unit-scoped Contract Catalog export/import boundaries. They are contract
requirements. "Most take effect with the Phase 2 exporter/importer wiring"
described the Phase 1 ratification; the wiring was delivered by C3/C4, so
these invariants are now in force. They did not broaden the Phase 1 code
deliverables.

## 15.1 Producer Selected-Dataset Validation

Producer-side validation MUST be performed against the EXACT dataset selected
for the export target/mode, before canonical serialization, hashing, signing,
and encryption. For `UnitDistribution` the producer pipeline is strictly:

```
select target-unit dataset
-> validate that selected dataset
-> canonicalize/serialize
-> hash
-> sign
-> encrypt
```

- The producer MUST NOT validate a broader WILAYA-wide dataset and only
  later filter it into the unit artifact. Validation and emission operate on
  one dataset: the exact rows the target unit will receive.
- This preserves artifact correctness: unrelated-unit data defects never
  become a dependency of the target-unit artifact, because those rows are
  never part of the validated, serialized, hashed, signed payload.
- Precision: this does NOT claim that validation discovers arbitrary database
  corruption. The architectural point is that the dataset being validated
  MUST be the dataset actually emitted and signed — the validation domain is
  identical to the artifact domain.

## 15.2 `in_scope` as Defense-in-Depth

Importer `in_scope` filtering remains a **defense-in-depth** control, not the
primary isolation mechanism:

- **Authentication is the primary package-to-UNIT boundary** — Ed25519
  `signature_version = 2` verification via `verify_v2_package_for_import`.
  **Target binding** (§13 importer target matrix) is implemented (C4) as the
  explicit package-to-UNIT semantic boundary.
- **Dataset selection at export is the producer-side boundary** (§15.1): the
  artifact physically contains only the target unit's rows.
- **`in_scope` filtering at import remains an additional defensive layer**,
  applied late in the pipeline.
- `in_scope` MUST NOT be treated as the primary isolation mechanism.
- A package targeting UNIT A MUST never become valid for UNIT B merely
  because `in_scope` happens to filter rows. Row filtering never substitutes
  for target binding.

### Current implemented import pipeline (verified, post-C4)

The current V3 import pipeline, verified against the executable order of
`commands/import_export.rs::run_import_pipeline_core` and
`application/usecases/sync/import_contract_catalog_package.rs::execute` at
HEAD `f14ca73`:

```
schema (kind-blind envelope gate, SupportedSchemaWindow)
-> kind-aware import security requirements
-> cryptographic authentication (verify_v2_package_for_import,
   Ed25519 signature_version=2 + membership/issuer policy)
-> structural/business validation (validate_contract_catalog_package_for_import)
-> provenance / source check (products_source_allowed_for_unit)
-> importer-unit scope resolution (resolve_importer_unit, returns units.code)
-> target binding validation (validate_contract_catalog_target_for_import)
-> replay protection (exact package_id dedup, registry.has_imported)
-> in_scope + target-scoped application (import_contract_catalog_sync)
-> atomic apply (single transaction) + package_id marked imported
```

Target binding therefore runs AFTER cryptographic authentication and
structural validation and BEFORE replay dedup and any business mutation; a
target-rejected package consumes no replay slot and writes zero catalog rows.

Step resolution across both layers:

**Command layer (`commands/import_export.rs`).** Authorization and session
touch precede the DB transaction; after encrypted package load/decrypt the
kind-blind schema/envelope gate (`SupportedSchemaWindow::can_import`) runs at
envelope entry, followed by the kind-aware import security requirements
(`validate_import_security_requirements`, rejecting unsigned / V1-downgraded
critical kinds). Inside the single import transaction
(`with_event_persistence`), the transactional settings are re-read (F-04 /
SYNC-007) and `verify_v2_package_for_import` performs the cryptographic
authentication — Ed25519 node identity, `signature_version = 2`, with the
kind-scoped issuer/membership policy — before the per-kind importer runs.
No transport sequencing exists (SEC-056D/SEC-057).

**Importer usecase (`import_contract_catalog_package.rs::execute`),** in
executable order:

1. `validate_contract_catalog_package_for_import(&input.package)` —
   structural/business validation (mode-aware; empty `contracts` allowed for
   `UnitDistribution`, §10);
2. importer-wilaya code presence requirement (`Required { wilaya_code }`, the
   F-04 transactional setting re-read);
3. `products_source_allowed_for_unit(...)` — provenance/source check on
   `source_node_id` against the importer's wilaya code;
4. `resolve_importer_unit(...)` — resolves the local UNIT's authoritative
   `units.code`; the resolver returns `ImporterUnit { id, code }`, the exact
   string used by the target matrix (§13);
5. `validate_contract_catalog_target_for_import(...)` — target binding (§13
   matrix: `export_mode` × `target_node_id` against the local unit's exact
   `units.code`); rejects legacy targetless and cross-unit packages before
   any replay slot is consumed and any row is written;
6. `registry.has_imported(&package_id)` — exact `package_id` replay dedup;
7. `import_contract_catalog_sync(...)` — the target-scoped application path,
   including `in_scope` enforcement during dimension application, inside the
   atomic transaction;
8. `registry.mark_imported(&package_id)` — applied-package record.

The executable order therefore places structural/business validation BEFORE
the provenance/source check, the provenance/source check and target binding
BEFORE exact `package_id` replay dedup, and replay dedup BEFORE any
application. `in_scope` is enforced at apply time inside the atomic
transaction rather than as an early isolation gate.

### Target binding (implemented by C4)

- The target-binding control for `UnitDistribution` packages is
  **implemented** (C4, `f14ca73` — `validate_contract_catalog_target_for_import`
  + `resolve_importer_unit`), settling the earlier "Phase 2 design decision"
  on placement: after structural validation and the provenance/source check,
  before replay dedup and before any target-specific application or
  acceptance.
- Target binding occurs inside the single import transaction and before any
  business mutation; a target-rejected package consumes no replay slot and
  writes zero catalog rows.
- The diagram above is the current implemented order and remains the
  reference for present behavior. Legacy targetless V3 `contract_catalog`
  packages (absent `export_mode` / `target_node_id`) are rejected per §13.

## 15.3 Failure Isolation: B Must Not Invalidate A

**Invariant:** for two distinct units A and B, data belonging exclusively to
B MUST NOT cause unit A's `UnitDistribution` artifact to be rejected,
malformed, semantically invalid, or otherwise fail. Examples of B-only data
that must be incapable of invalidating A's artifact include:

- malformed B-only contract data;
- invalid B-only draft/contract state;
- B-only allocation/data inconsistency;
- B-only dataset validation failure.

Conversely, A-only defects MAY invalidate A's artifact without invalidating
B's artifact.

The unit-scoped artifact is an **independent validation/signing failure
domain**. This is a direct failure-isolation invariant, not merely the
statement that "the payload is unit-scoped." It is underpinned by §15.1:
because producer validation runs over the exact selected dataset, B-only rows
are excluded before validation, serialization, hashing, and signing, so a
B-only defect cannot surface in A's artifact; and by §13: target binding
rejects at import any artifact that somehow reaches the wrong unit.

## 15.4 Replay and Provenance Preservation

The unit-scoped export redesign does NOT remove, bypass, or weaken existing
replay protection or provenance mechanisms:

- **Replay protection via the existing applied-package / `package_id`
  mechanism** is preserved. Each emitted package is uniquely identified;
  import dedup on the exact `package_id` remains the replay boundary.
- **Provenance via `source_node_id`** is preserved.
- **Authenticity / signature verification** (Ed25519,
  `signature_version = 2`, whole-package canonical coverage) is preserved
  unchanged.
- **Target binding is an ADDITIONAL semantic boundary** (§15.2, §13); it does
  NOT replace provenance or replay protection.
- `target_node_id` is signed/authenticated as part of the package contract
  (implemented C1): it is covered by the same package signature as every
  other metadata field (§8), so it cannot be altered without invalidating the
  package.

No new replay or provenance mechanism is introduced by this ADR.

# 16. Deferred Work (historical — resolved by C1–C4)

All items below were listed at ratification as deferred work requiring
separate authorization. Each has since been authorized and implemented; the
list is preserved for traceability.

| # | Historical deferred item | Implementation status | Implemented by | Current state |
|---|--------------------------|----------------------|----------------|---------------|
| 1 | Exporter wiring — per-UNIT dataset selection in `export_contract_catalog_dataset.rs`; fleet loop + per-target dataset precompute in `fleet_package_export.rs`; metadata population in `identity_signed_export_service.rs` | **IMPLEMENTED** | C3 (`917f0ad`) | Unit-scoped dataset selection + validation; fleet loop with per-target precompute; `export_mode`/`target_node_id` populated in the exporter |
| 2 | Importer wiring — target matrix enforcement + `units.code` resolution in `import_contract_catalog_package.rs` / `sync_import_execution_service.rs`; `resolve_importer_unit_id` extended to return `unit.code`; empty-contracts allowance | **IMPLEMENTED** | C4 (`f14ca73`) | `validate_contract_catalog_target_for_import` enforces §13; `resolve_importer_unit` returns `ImporterUnit { id, code }`; empty `UnitDistribution` accepted |
| 3 | SQL hardening — replacement of the manual `unit_id = '...'` escaping at `repositories/contracts.rs` with typed parameter binding (`?1`), repository-layer confined, zero business logic change | **IMPLEMENTED** | C3 (`917f0ad`) | `unit_id`/`supplier_id`/`fiscal_year` filters use typed `?N` binding (`list_contracts`, `list_catalog_exportable_contracts_for_unit`, `count_live_contracts_for_unit_year`, ...) |
| 4 | Input wrapper — `ExportContractCatalogInput { mode }` introduction (§4) | **IMPLEMENTED** | C3 (`917f0ad`) | `ExportContractCatalogInput { mode }` in `exports/types.rs`, consumed by the exporter |
| 5 | Metadata field — introduce `SyncPackageMetadata.target_node_id` with its serde contract and tests, together with the exporter/serializer wiring that the value flows through (§8) | **IMPLEMENTED** | C1 (`26b6dd8`); exporter wiring C3 | Field + serde contract + canonical-signature coverage; V3 retained |

**No deferred implementation remains from this ADR as of C4.**

# 17. Non-Goals

This ADR does **not**:

1. implement exporter or importer wiring at ratification time (§2, §16) —
   subsequently delivered by C1–C4;
2. introduce recipient-specific encryption or any key split (§12);
3. add a `products` dimension or change the four-dimension dataset shape;
4. alter lifecycle rules of `ContractStatus` or any transition predicate;
5. make the Excel (`.entitlements`) export consume per-unit mode, apply
   target filtering, or change renderer authority (file title is not a
   security boundary);
6. use a filename, rendering label, or `units.id` as the security identity;
7. do a broad SQL refactor (only the targeted hardening above — delivered by
   C3);
8. migrate the schema, bump the envelope version, add tables/columns, or
   reopen the V3 window;
9. add a legacy compatibility path or V4;
10. apply `is_catalog_exportable` to `FleetRestore` (§7);
11. define the frontend's mode UI or add any frontend business logic;
12. change transport sequencing (still permanently retired).

# 18. Implementation Boundary

Phase 1 (this ratification) delivered: the mode enum (§3) and the status
predicate (§6), plus their unit tests and this ADR. The metadata field (§8),
the input wrapper (§4), and everything in §16 stayed out of scope at
ratification and were subsequently authorized and implemented by C1–C4; the
functional scope of this ADR is complete at HEAD `f14ca73`.

# 19. Testing Contract

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

**Expansion status.** The later-phase test contract was delivered with
C1–C4: the §8 serde and canonical-signature coverage tests (C1,
`package_metadata.rs`), the export reproducibility snapshot tests (C2,
`export_reproducibility_persistence_tests.rs`), the exporter per-UNIT
isolation, shared-supplier, all-status-retention, empty-artifact, and
`Ended(B)`-exclusion tests (C3, `contract_catalog_unit_distribution_export_tests.rs`,
including `unit_distribution_scopes_contracts_allocations_and_exceptions`),
the importer target-matrix, cross-unit-import isolation
(`unit_import_is_scoped_to_local_unit`), and empty-`UnitDistribution`
acceptance tests (C4, `contract_catalog_package_import_tests.rs`), and the
C4 target-tamper test
`tampered_target_node_id_fails_signature_verification_before_semantic_binding`
in `contract_catalog_unit_distribution_export_tests.rs`.

# 20. Governance Records

- Follow-on SEC-087-F decision record; ADR-0055 §3.10 reserved this path.
- Accepted as number **0059**; the `ADR_INDEX.md` row is added per
  `ADR_INDEX.md` policy (sequential 4-digit numbering, statuses, owner).
- `ARCHITECTURE_FREEZE.md` requires **no amendment**: Section 2 frozen
  clauses are preserved; this ADR is additive.
- Pre-production environment (ADR-0057 §2.3 clean-cutover precedent); no
  deployed fleet to migrate and no legacy packages to reinterpret.
- AGENTS.md §13 precedence: where a temporary/design document conflicts,
  this ADR governs.