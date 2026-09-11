# ADR 0057: Sync Protocol V3 Schema Gate — V3-Only Import Compatibility Window (SEC-087 Phase 6A)

> **ACCEPTED — 2026-09-11 (SEC-087 Phase 6A ratification).**
> The `schema_version` import window is now **V3-only**; this supersedes the
> `MIN_SUPPORTED = V2` / "V2-only" import-window clauses of ADR-0047 (see §5, §10).

# Decision Status

**ACCEPTED — 2026-09-11.**

This ADR ratifies the SEC-087 Phase 6A implementation mission:
the sync-package import compatibility window is set to **V3-only**
(`MIN_SUPPORTED = V3`, `MAX_SUPPORTED = V3`), enforced by the **single
kind-blind schema gate** in the centralized import pipeline. This supersedes the
`MIN_SUPPORTED = V2` / "V2-only" import-window clauses of ADR-0047. It is a
**metadata-level version cutover only**; every other protocol layer is
explicitly unchanged (envelope, encryption, Ed25519 identity signing, trust
distribution, replay dedup via exact `package_id` — SEC-056D/SEC-057 retired
transport sequences entirely). No payload is implemented in Phase 6A; however,
the **V3 products payload contract** (config REQUIRED) is ratified in §3.4,
with enforcement landing in Phase 6B.

| Item | Status |
|------|--------|
| `SYNC_PACKAGE_SCHEMA_VERSION` (global producer constant) | **V3** |
| Import compatibility window `MIN_SUPPORTED` / `MAX_SUPPORTED` | **V3 / V3** |
| `schema_version = V2` acceptance on any import path (incl. `stock_movements`, trust, registry, `admin_access`, `identity_access`) | **REMOVED — rejected `PACKAGE_TOO_OLD`** |
| `schema_version = V1` / `V0` or absent | **REJECTED — `PACKAGE_TOO_OLD` (unchanged class, existing behavior)** |
| `schema_version = V4+` (future) | **REJECTED — `PACKAGE_TOO_NEW` (unchanged class, existing behavior)** |
| Per-kind schema-window checks (products / daily_report / monthly_summary / contract_catalog) | **REMOVED — replaced by the single pipeline-level gate** |
| `signature_version = 2` (Ed25519), integrity hash, issuer identity, replay protection | **UNCHANGED (mandatory, fail-closed)** — replay is exact `package_id` dedup (`sync_applied_packages`); no transport sequences (SEC-056D/SEC-057) |
| Envelope / `age::x25519` / `age::scrypt` (`.adminkey`) / Ed25519 signing keys | **UNCHANGED** |
| Product payload, product importer, inventory, TVA, supplier / ContractCatalog V2 additive kind | **NOT IMPLEMENTED IN 6A — SEC-087 Phase 6B / 6C**; the V3 products payload contract (config REQUIRED) is **RATIFIED in §3.4**, enforced in Phase 6B; 6A-era config-free packages are documented non-conforming pre-release artifacts (never shipped) |
| DB migration | **NOT REQUIRED** |
| Fiscal closure package (`.fiscal-close.sync`, `FISCAL_CLOSURE_PACKAGE_VERSION`) | **UNCHANGED** |
| `.unit` bootstrap import path (`import_unit_node_package`) | **UNCHANGED — exempt from the pipeline gate by design (see §8)** |
| ADR-0004 mandatory steps (compatibility review, snapshot contract, import-behavior tests, ADR record) | **SATISFIED by this ADR + Phase 6A tests (§9)** |

# Date

2026-09-11 — accepted by the governance reviewer (ADR-0057).

# Relation to Existing Documents

| Document | Relation |
|----------|----------|
| ADR-0047 (Sync Package V1/HMAC Removal — V2-Only Enforcement) | **Amended (import-window clauses)** — §3 row 4 and §4 item 1 set `MIN_SUPPORTED = V2`; the import-window clauses are superseded to `MIN_SUPPORTED = V3`. All other ADR-0047 ratifications (V2 metadata mandates, `signature_version = 2` fail-closed, no-migration decision) remain in force. |
| ADR-0004 (Protocol Changes are Breaking Changes) | **Complied with** — the `schema_version` support window is Public Contract Change #4; this ADR is the mandatory decision record (step 4) for the semantic change to the rejection policy and for the ratified V3 products payload contract (§3.4). |
| ADR-0003 (sync protocol versioning & integrity) | **Amended (window clause)** — `schema_version` acceptance now V3-only; versioning & integrity machinery otherwise unchanged. |
| ADR-0007 (signing key deprecation window) / ADR-0019 (legacy crypto isolation) | **Unchanged** — V1/HMAC removal stands as settled (ADR-0047/0048). |
| ADR-0038 (node identity & trust) | **Unchanged** — trust chain and issuer identity clauses intact. The transport-sequence mandate it describes is historical, superseded by SEC-056D/SEC-057 (sequences retired; replay is exact `package_id` dedup), and is NOT reintroduced. |
| ADR-0044 (`.unit` Trust-First V2), ADR-0045 (B8 first `identity_access` import), ADR-0046 (UNIT→WILAYA trust), ADR-0049 (trust package hardening), ADR-0051 (`admin_access`), ADR-0052 (canonical UNIT operator) | **Unchanged** — kind policies, issuer scoping, and `.unit` WILAYA gate are untouched; only the `schema_version` window that any of these kinds must satisfy moves to V3. |
| ADR-0053 (unified per-target transport sequence) | **Historical** — the producer allocator and Transport Guard it ratified were retired by SEC-056D/SEC-057; `001_initial.sql:9-13` omits `sync_issuer_sequence` / `transport_export_sequence` / `applied_sync_packages.package_sequence`. Phase 6A does not reintroduce sequences. |
| ADR-0055 / ADR-0056 (contract-centric procurement / supplier splitting) | **Unchanged** — ContractCatalog V2 additive kind and supplier semantics remain; Phase 6A only changes the `schema_version` window its package must satisfy. |
| `ARCHITECTURE_FREEZE.md` | **Unchanged** — sync-package signature machinery, single-writer SQLite, no-async, and all frozen invariants are preserved; this ADR is the sanctioned change record for the schema window (RFC-to-ADR process, §4). |

# 1. Executive Summary

SEC-087 Phase 6A moves the sync-package protocol from **V2-only** to **V3-only**.
The change is a **metadata-level version cutover**: `schema_version = 3` becomes
the sole supported value for every sync-package import path. The global producer
constant `SYNC_PACKAGE_SCHEMA_VERSION` becomes `SchemaVersion::V3`, so every
producer (data kinds, trust, registry, `admin_access`, `.unit` bootstrap export)
emits V3. A single **kind-blind** gate in the centralized import pipeline
(`run_import_pipeline_core`) validates the window before any security or
business processing, replacing the per-kind window checks that only covered
products / daily_report / monthly_summary / contract_catalog. This closes the
existing gate gap for `stock_movements` (which has no window-validating
validator) and for the security-critical kinds (trust, registry, `admin_access`,
`identity_access`).

The V3 window is **closed** (`[V3, V3]`): V0/V1/V2 are rejected as
`PACKAGE_TOO_OLD`, future versions as `PACKAGE_TOO_NEW`. No payload of any kind
is implemented in this ADR; the V3 products payload contract (config REQUIRED)
is ratified in §3.4 and enforced in Phase 6B.

# 2. Problem Statement

## 2.1 The config-free products payload contradicts the WILAYA-authoritative Product config invariant

The persisted Product catalog (and its management UI) is authoritative at the
WILAYA node and is synchronized down to UNIT nodes. The current accepted
products payload (`ProductSyncRecord`) is **config-free** — 8 fields, no
configuration dictionary — and the products importer upserts records without
any configuration. A closed V3-only window alone does NOT prevent a config-free
V3 package from being accepted: Phase 6A itself emits V3 packages with the
8-field config-free shape, and nothing ships between 6A and 6B. The operative
post-6B guarantee is therefore the **ratified V3 products payload contract**
(§3.4): every Product record in a V3 products package MUST carry valid
`purchase_unit` / `consumption_unit` / `conversion_factor` / `tva_classification`
(config REQUIRED, fail-closed), so any config-free package arriving after Phase
6B is **rejected pre-mutation**, never normalized. The schema NOT NULL columns
are the SQL backstop behind that validator. (Single-writer SQLite protects
write serialization; the contract protects payload-shape authority under
A3/A4 and A5 runtime purity.)

## 2.2 The window is currently enforced per-kind — and not every kind has a window check

The window is enforced today inside `import_validation.rs` for exactly four data
kinds (products, daily_report, monthly_summary, contract_catalog). Three gaps:

1. `stock_movements` has **no** window-validating validator — its import path
   (`import_stock_movements_package.rs`) never calls a compatibility check.
2. The security-critical kinds (trust, registry, `admin_access`, `identity_access`)
   are not window-checked by a validator.
3. Four near-identical copies of the window check + error mapping exist, each
   in validator code — duplicate business logic (violates P2 single source of
   truth; drifts easily).

A single pipeline-level, kind-blind gate at the top of the centralized import
function is the single source of truth for the schema window, is uniform across
all kinds, and cannot be bypassed by a kind that lacks its own validator.

## 2.3 Environment allows a clean cutover

The software has never been deployed to production (pre-production environment).
Per the ADR-0048 precedent — a ratified cutover that closed a compatibility
window without a dual-window migration period — a closed V3-only window is the
deterministic (P1) and clean choice. No backward-compatibility window is
required, kept, or honored.

# 3. Decision

## 3.1 Version cutover

- `schema_version.rs`: add `SchemaVersion::V3 = Self(3)`; keep `V1 = Self(1)` /
  `V2 = Self(2)` declarations (V1/V2 remain representable so that rejection
  tests and `PACKAGE_TOO_OLD` semantics keep exact versions).
- `constants.rs`: `SYNC_PACKAGE_SCHEMA_VERSION = SchemaVersion::V3`.
- `compatibility/mod.rs`: `MIN_SUPPORTED = SchemaVersion::V3` and
  `MAX_SUPPORTED = SYNC_PACKAGE_SCHEMA_VERSION` (identical by construction —
  closed `[V3, V3]` window). Error codes `PACKAGE_TOO_OLD` / `PACKAGE_TOO_NEW`
  and their semantics are unchanged.

## 3.2 Single kind-blind gate

- One window check executes in `run_import_pipeline_core` (the centralized
  import function), **immediately after the package loader / deserializer and
  before the import security requirements validation**. Exact site:
  `commands/import_export.rs`, between package load and
  `validate_import_security_requirements`.
- The gate is **kind-blind**: it reads only
  `metadata.schema_version` and calls the compatibility window
  (`SupportedSchemaWindow::can_import`) — no kind parameter, no per-kind
  branch, no Product special-case.
- The gate is reached by **every** pipeline import, including the bootstrap
  path (`run_import_pipeline_bootstrap`), because both entry points delegate to
  `run_import_pipeline_core`.
- The per-kind window checks in `import_validation.rs` (products, daily_report,
  monthly_summary, contract_catalog) are **removed**; the validators keep all
  payload/semantic validation. One window, one owner.

## 3.3 Explicit non-changes (binding)

This ADR changes **nothing** else at runtime in Phase 6A:

1. Envelope shape, field names, types, canonicalization contract (ADR-0009) —
   unchanged (a V3 package is byte-identical in structure to a V2 package; only
   the `schema_version` value differs).
2. Encryption (`age::x25519` node secrets, `age::scrypt` `.adminkey`) — unchanged.
3. Signing (Ed25519, `signature_version = 2`, issuer identity, integrity hash,
   replay protection via exact `package_id` dedup) — unchanged and still
   fail-closed (ADR-0047 §4 items 2–8 remain in force; transport sequences are
   retired per SEC-056D/SEC-057 and NOT reintroduced).
4. Product payload, product importer, inventory, TVA, supplier, ContractCatalog
   additive payloads — **not implemented in Phase 6A** (SEC-087 Phase 6B / 6C).
   The Product data shape is byte-identical to the current V2 shape; only the
   version bucket moves. No payload field is added, removed, or renamed in Phase
   6A. However, the **V3 products payload contract** (config REQUIRED) is
   ratified by this ADR in §3.4 and governs all 6B-era write paths; 6A-era
   config-free packages are documented non-conforming pre-release artifacts.
5. Fiscal closure package — unchanged (`FISCAL_CLOSURE_PACKAGE_VERSION` remains
   an independent, unrelated versioning domain).
6. `.unit` bootstrap **import** path (`import_unit_node_package`) — unchanged.
   It does not pass through the centralized pipeline; its `.unit` packages
   derive `schema_version` from the same `SYNC_PACKAGE_SCHEMA_VERSION` constant,
   so they become V3 automatically on the producer side. The `.unit` deserializer
   path must NOT gain a window ceiling in Phase 6A.
7. The package deserializer — unchanged. It is deliberately schema-agnostic
   (canonical branch `schema >= V2` remains valid for V3). No ceiling is added;
   the dead V1 branch remains as the historical fail-closed guard.
8. No DB migration — existing columns (`sync_applied_packages` fields) remain
   unchanged; Phase 6A touches no schema.

## 3.4 V3 products payload contract (RATIFIED — enforced in Phase 6B)

This ADR ratifies the V3 products payload contract that Phase 6B must implement:

- **Required configuration:** every `Product` record in a V3 products package
  MUST carry valid `purchase_unit`, `consumption_unit`, `conversion_factor`
  (integer > 0), and `tva_classification` (closed set), mutually consistent per
  SEC-087 rule G (identical purchase/consumption units ⇒ `conversion_factor == 1`).
- **Single authoritative enforcement:** `validate_product_units`
  (`domain/validation.rs:113-147`) is the sole unit/factor/TVA gate; its doc
  contract already obligates "any future write path" to source enforcement there.
  Absence of any code is rejected fail-closed (same semantics as
  `CreateProductRequest`, `models/product.rs:38-50`).
- **Wire shape vs. semantics (precedent-respecting):** config fields may be
  `Option<i32>` + `#[serde(default)]` for deserializer shape-compatibility
  (precedent: `models/user.rs:71-74`, `models/product.rs:32`), which is a SYNTAX-
  only shim and NEVER satisfies the business requirement. The semantic requirement
  is enforced by the validator, pre-mutation, in the 6B sync write path.
- **Pre-mutation rejection point (6B):** extended
  `validate_products_package_for_import` requires per-record config, invoked at
  the top of `import_products_package::execute` BEFORE the registry
  `has_imported` check and before any Product/inventory row write.
- **SQL backstop:** the products config columns become NOT NULL in the single
  baseline edit (`001_initial.sql`), fulfilling the deferral originally noted in
  `001_initial.sql:133-138` ("deferred until sync-import product writes carry the
  codes"; that comment mis-cited ADR-0056, which covers supplier splitting — this
  ADR is the authoritative tracking record).
- **Distribution unchanged:** products packages remain WILAYA-authoritative and
  UNIT-imported only from the importer's wilaya (`products_source_allowed_for_unit`,
  `import_provenance.rs:43-47`). UNIT can never repair config locally.
- **Behavior after 6B:** a 6A-era config-free V3 products package arriving after
  Phase 6B is **REJECTED** (not normalized). Remediation is operator-driven
  WILAYA re-export/import — a procedure, not a protocol invariant.
- **No V4:** this kind-contract change is ratified under the unchanged V3 envelope
  (precedents: ADR-0051 kind retired, ADR-0055 §3.10/§5 kind added — no envelope
  bump). A future V4 is reserved solely for changes to ENVELOPE semantics
  (ADR-0003). No timestamp / node-build / version heuristic determines the boundary.

# 4. Mandatory V3 Contract (All Layers, Fail-Closed)

An accepted sync package MUST satisfy, on every pipeline import path:

1. `schema_version == 3` — enforced by the pipeline gate before any security or
   business processing (`PACKAGE_TOO_OLD` for `< 3`, `PACKAGE_TOO_NEW` for `> 3`);
2. `integrity_hash` present and matching SHA-256 over canonical bytes
   (ADR-0009) — engine unchanged;
3. `signature_version == 2` (Ed25519) — engine unchanged, still fail-closed;
4. `signature` present and non-empty — engine unchanged;
5. `issuer_identity_id` present — engine unchanged;
6. Replay protection — exact `package_id` dedup via `sync_applied_packages`;
   no transport sequences (SEC-056D/SEC-057) — engine unchanged;
7. Ed25519 signature verified against the issuer certificate public key
   (ACTIVE, valid `not_after`) — engine unchanged (ADR-0038/0046/0049);
8. `.unit` bootstrap additionally requires the WILAYA-only dedicated path
   (ADR-0044) with anchor-first ordering and source pin — engine unchanged (no
   fixed sequence; the envelope carries no `package_sequence`).

Enforcement points (all reject, none bypass):

- `commands/import_export.rs` — **pipeline schema gate (new, kind-blind)**;
- `application/sync/compatibility` — window policy (`[V3, V3]`, sole owner);
- `package_deserializer.rs` — metadata contract (2–4) as today;
- `import_validation.rs` — payload/semantic checks per kind (window checks removed);
- `sync_package_identity_verification_service.rs` — (7), including the
  kind-scoped UNIT issuer policy and `.unit` WILAYA gate — engine unchanged.

# 5. No-Migration Decision

- **No DB migration:** no column is added, removed, or constrained.
- **No package-format migration:** existing V2 files are FAIL-CLOSED REJECTED at
  the pipeline gate (`PACKAGE_TOO_OLD`), not converted. Operators re-export
  through the V3 pipeline (deterministic, ADR-0004 step "compatibility review").
- **No dual-window period:** pre-production environment; the ADR-0048 precedent
  of clean cutover applies.

# 6. Snapshot Contract

Per ADR-0004 step 2, standard package snapshot models are reviewed. Phase 6A
changes no structural field of any snapshot; the only change is the
`schema_version` value emitted by fixtures and goldens, which is moved from 2 to 3.
No snapshot file changes shape. The V3 products payload contract (§3.4) is
RATIFIED by this ADR; its structural manifestation (the config-carrying Product
export/import surface) lands with the Phase 6B implementation, which carries its
own snapshot-contract update and contractual rejection test.

# 7. Testing Contract

- **Pipeline gate rejection tests (new):**
  - a syntactically valid, correctly verified **V2** data package (products) is
    rejected `PACKAGE_TOO_OLD` by the gate — and the rejection occurs **before
    the products importer runs**, provable by asserting no Product/inventory
    rows were created or modified;
  - a **V2 stock_movements** package cannot bypass the gate (closes the
    no-validator gap);
  - a **V1** package remains rejected (existing behavior);
  - a **V4 / future** package is rejected `PACKAGE_TOO_NEW` (existing behavior
    class).
- **Pipeline gate acceptance tests (new):** a valid **V3** package (products
  and stock_movements) passes the gate and reaches the security verification +
  import path unchanged.
- **Version-policy unit tests** (`compatibility/mod.rs`): accept V3; reject
  V0/new(0), V1, V2 as `PACKAGE_TOO_OLD`; reject V4/future as `PACKAGE_TOO_NEW`.
- **Phase 6B contract test (registered here, executed with 6B):** a config-free
  V3 products package is rejected by `validate_products_package_for_import`
  (§3.4) with zero Product/inventory rows written.
- **Fixture updates:** current-valid test builders move to `SchemaVersion::V3`;
  rejection tests keep explicit V1/V2 literals; fixtures that construct via the
  `SYNC_PACKAGE_SCHEMA_VERSION` constant auto-adopt V3.
- **Full regression:** `cargo test`, `vitest`, Playwright; governance gates
  (`check:arch` zero warnings, `check_docs_governance`, `check_secrets`,
  `check_release_integrity`).

# 8. Implementation Boundary (Binding)

Allowed to change (Phase 6A):

- `src-tauri/src/application/sync/schema_version.rs` — add `V3`;
- `src-tauri/src/application/sync/constants.rs` — constant = V3;
- `src-tauri/src/application/sync/compatibility/mod.rs` — `MIN_SUPPORTED = V3`,
  test updates;
- `src-tauri/src/commands/import_export.rs` — pipeline schema gate + test
  fixture `schema_version` updates (metadata builders, V1 rejection test kept);
- `src-tauri/src/application/sync/import_validation.rs` — remove the four
  per-kind window checks (keep payload/semantic checks);
- current-valid test fixtures/tests moving to V3: `tests/admin_access_sync_tests.rs`,
  `tests/sync_trust_registry_import_tests.rs`, `tests/identity_access_sync_tests.rs`,
  `tests/sync_unit_to_wilaya_import_tests.rs`,
  `src-tauri/src/application/sync/import_provenance.rs` (test fixture),
  `src-tauri/src/application/services/sync_package_identity_verification_service.rs`
  (test helpers);
- `tests/sync_package_import_parse.rs` — re-home schema-rejection tests onto the
  compatibility policy (validators no longer enforce the window);
- new Phase 6A regression tests (gate acceptance/rejection, stock_movements
  cannot bypass);
- docs: this ADR (upon acceptance), and `ADR_INDEX.md` (number/status stamped by
  the maintainer on acceptance).

Forbidden (unless a separate decision explicitly authorizes):

- any Product payload / importer / inventory / TVA / supplier / ContractCatalog
  payload or importer change (SEC-087 Phase 6B / 6C);
- any envelope, canonicalization, encryption, signing, trust, or replay-dedup
  change (transport sequences retired by SEC-056D/SEC-057 are NOT reintroduced);
- any DB migration;
- any fiscal closure package change;
- adding a window ceiling to the package deserializer;
- modifying the `.unit` bootstrap import path or its trust semantics;
- creating any Phase 6A code path that establishes config-free products as a
  lawful permanent state;
- any timestamp / build / node-version heuristic as the 6A↔6B payload boundary
  signal (the boundary is the §3.4 contract, applied pre-mutation);
- keeping any V2 acceptance path anywhere (a V2 window returned in any layer is
  a violation of this ADR).

# 9. ADR-0004 Compliance Mapping

| ADR-0004 mandatory step | Evidence |
|-------------------------|----------|
| 1. Compatibility review (producer/consumer/migration impact) | §2, §3, §3.4, §5 — single constant producers, closed window, no migration; V3 products payload contract ratified; pre-production cutover |
| 2. Snapshot Contract update | §6 — no structural change; fixture `schema_version` values bump to 3; V3 products payload contract ratified (§3.4) |
| 3. Import-behavior tests (success + explicit rejection) | §7 — gate rejection/acceptance tests, version-policy unit tests, no-bypass test, registered 6B contract test |
| 4. ADR record for semantic behavior change | This ADR (ADR-0057, status: Accepted, 2026-09-11), including the V3 products payload contract (§3.4) |

# 10. Governance Records

- SEC-087 Phase 6 design freeze (engineering decision authorization) precedes
  this ADR; this ADR is the required governance record ratifying the semantic
  change to the import rejection policy (ADR-0004 step 4).
- Acceptance record: assigned number **0057**, stamped 2026-09-11; the
  `ADR_INDEX.md` row is added per `ADR_INDEX.md` policy (statuses, numbering,
  superseded references).
- Numbering note: `ADR_INDEX.md` lists 0044–0099 as available; number **0057**
  was assigned by the governance reviewer.
- The V2-only import-window clauses in ADR-0047 (§3 row 4, §4 item 1) and any
  earlier statements of `MIN_SUPPORTED = V2` are **historical** from acceptance
  of this ADR; where they conflict with §3–§4, this ADR takes precedence
  (per AGENTS.md §13). This ADR does NOT reopen the V1/HMAC removal or any other
  ADR-0047 ratification.
- Acceptance of this Phase 6A ADR does NOT authorize SEC-087 Phase 6B/6C payload
  or importer work; those require their own decision records.
- This ADR is the tracking record for the Product-config NOT NULL deferral that
  `001_initial.sql:138` mis-attributed to ADR-0056 (which covers supplier
  splitting, not product config). The stale schema-comment reference is noted
  here for a separate sanctioned correction; §3.4 governs the deferral until then.