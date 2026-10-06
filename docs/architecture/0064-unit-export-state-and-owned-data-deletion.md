# ADR 0064: UNIT Export State & Owned-Data Deletion

# Status
Accepted (2026-10-05)

> **Governance state.** This ADR is **Accepted** (2026-10-05) by owner ratification of the
> deletion, retention, projection-shape, and purge-scope decisions enumerated in §2 and §9.2.
> **All owner decisions U-1 through U-4 are closed; no decision is pending.** It is the
> **authoritative amendment record** for the frozen documents listed in §1 — those files are NOT
> edited by this ADR (per `AGENTS.md` §8 C1 and `docs/architecture/ARCHITECTURE_FREEZE.md` §4).
> **Implementation is authorized but NOT yet performed**: governance/design only. No Rust,
> Svelte, TypeScript, schema, or test change accompanies it.
>
> Companion to `docs/architecture/0063-unit-operator-credential-lifecycle.md`, which owns the
> credential and initialization lifecycle. This ADR owns the UNIT **lifecycle projection** and
> the **destructive purge**. The two are coupled at exactly one point, recorded in §5.3: the
> local operator row is UNIT-owned, and `audit_log` is not.

# Date
2026-10-05

# Owner
Architecture / Security / Data Governance

# Reference

## 1. Documents amended or qualified by this ADR (frozen files are NOT edited)

| Document | Exact locus | Relationship |
|----------|-------------|--------------|
| `docs/architecture/0055-contract-centric-procurement.md` | line 83 — *"`CANCELLED` excludes the allocation from normal obligation selection; historical fulfillment is never erased"* | **Qualified, not edited.** This sentence is sound for every non-destructive context and remains authoritative there. It is qualified for exactly one situation: explicit UNIT destruction, where `contract_allocations` rows owned by the destroyed UNIT are removed. §7 defines the boundary. No other allocation-retention reading changes. |
| `docs/architecture/0061-unit-wilaya-allocation-fulfillment-state-sync.md` | §7 "Released / inactive allocations" (line 225) | **Qualified, not edited.** §7's permanence argument rests on three verified repository facts — no `DELETE FROM contract_allocations` exists today, `deleted` is never set to `1` on an allocation, and the allocation FKs are `ON DELETE RESTRICT`. This ADR is the decision that first introduces a bounded `DELETE FROM contract_allocations`. §7 defines the boundary and restates the parts of §7 that survive unchanged (no re-keying, no re-parenting, no archival table). |
| `docs/architecture/ARCHITECTURE_FREEZE.md` | §2.4 Fiscal Semantics (line 122), §2.5 Reproducibility & Determinism (line 131), §2.3 Audit Trail (line 113) | **Re-scoped, not edited.** §6 of this ADR re-scopes §2.4's "Immutable closed years" and "Origin immutability" bullets for UNIT-owned destructive purge only, and §7 names the one §2.5 consequence. §2.3 (dual-write, hash chain, transactional audit, additive-only schema, single repository, deterministic ordering, chain invariant) is inherited **unchanged** and is a constraint on this ADR's own audit obligation. |
| `docs/architecture/0044-unit-trust-first-v2-bootstrap.md` | §8.3 Packaged-Identity Contract (line 285) | **Referenced, unchanged.** Its verification rule (3) — *"`subject_id` == unit package's unit"* — is the load-bearing fact that makes §2's ownership join key exact. |
| `docs/architecture/0051-admin-access-package.md` | §5 Account Ownership Invariant (line 77) | **Referenced, unchanged.** §5's operator-preservation invariant holds for `admin_access`; this ADR governs the separate, owner-approved explicit deletion event. |
| `docs/architecture/0052-canonical-unit-operator.md` | §D1 (line 46), §D2 (line 48) | **Referenced, unchanged.** §D2's `UNIQUE(username, node_id)` is what makes node-scoped ownership of operator rows exact. §D1's `CreateUnitRequest` clause is superseded by `0063` §3, not by this ADR. |

# Owner-ratified decision set (normative, 2026-10-05)

| # | Ratified decision | Encoded at |
|---|-------------------|-----------|
| D11 | A WILAYA UNIT is **EXPORTED** if the WILAYA-side `identity_store` contains ANY historical identity row for that UNIT. Status does not matter (`ACTIVE`, `REVOKED`, `SUPERSEDED`, `EXPIRED`). Once EXPORTED: Edit hidden/disabled, Export hidden/disabled, Delete remains. Enforced in the backend as well as the UI. | §1, §3 |
| D12 | WILAYA-side `identity_store` rows with `subject_type = 'UNIT'` are UNIT-owned. Successful UNIT deletion removes them atomically. A later recreation of the same UNIT code starts a clean identity lifecycle; deleted UNIT identity rows are NOT preserved merely to block code reuse. | §2 |
| D13 | UNIT deletion is an explicit destructive lifecycle operation. It deletes all legitimately UNIT-owned data, preserves global/shared data, and is atomic and fail-closed, against an explicit ownership matrix that includes indirect ownership where a child table has no `unit_id`. | §4 |
| D14 | `audit_log` is globally retained and `audit_log.user_id` is nullable with `ON DELETE SET NULL`. FK nullification MUST NOT be relied on silently. The canonical local UNIT operator may be hard-deleted only when zero `audit_log` rows reference that user; if any reference exists, UNIT deletion fails closed and rolls back atomically. No tombstone model is invented. The audit-reference check is inside the same deletion transaction. | §5 |
| D15 | UNIT deletion MAY purge UNIT-owned historical/domain rows even when their `fiscal_year` is archived — an explicit destructive lifecycle exception. `ARCHITECTURE_FREEZE.md` §2.4 closed-fiscal-year immutability is explicitly re-scoped. Node-global fiscal machinery, global fiscal aggregates, `fiscal_year_status`, `opening_balance_snapshots`, `fiscal_export_snapshots`, and other node-global fiscal machinery identified by repository inspection remain protected. `FiscalHistoricalGuard` MUST NOT be extended to block the approved purge. The exception applies only to UNIT-owned destructive purge. | §6 |
| D16 | The UNIT purge explicitly qualifies the otherwise applicable historical-retention rules. The governance documentation MUST explicitly address ADR-0055 line 83, ADR-0061 §7, and `ARCHITECTURE_FREEZE.md` §2.4. The frozen documents themselves are NOT modified. The new ADR defines the precise UNIT destruction exception and its boundaries. Global audit/security records and global/cross-UNIT fiscal data remain subject to their existing retention rules. | §7 |
| D18 | The application has NOT been officially deployed or used; there is no legacy production database to preserve. Schema changes MAY directly modify the authoritative initial schema. Migrations MUST NOT be created for development-database compatibility. | §8 |
| D19 | **EXPORTED projection shape (closes U-2).** EXPORTED is exposed to the WILAYA Units surface as a backend-derived DTO `UnitView` carrying the boolean `is_exported`, derived from the authoritative status-agnostic `identity_store` predicate. No column, no mutable flag, no enum lifecycle state. `list_units` returns `Vec<UnitView>`; `create_unit` / `get_unit` / `update_unit` continue to return `Unit`. The frontend consumes `is_exported` and must not recompute it. | §1.1 |
| D20 | **NULL-unit stock movement exception (closes U-3 / A1).** `stock_movements` is purged only for rows with concrete UNIT attribution (`unit_id = <target unit>`). Rows whose `unit_id IS NULL` are **preserved**, and ownership is **never inferred** for them. | §4.1 A11, §4.4 step 11 |
| D21 | **Node-global and shared fiscal data preserved (closes U-3 / A2, A3).** `opening_balance_snapshots` is node-global and MUST be preserved. `inventory_stocks` is shared/node-global because it has no unit dimension and MUST be preserved. No adjustment, recomputation, decrement, or other mutation of `inventory_stocks` is performed during UNIT deletion. No new business calculation is introduced to compensate for preserved node-global or shared tables. | §4.2 B4/B6, §6.2 |
| D22 | **Audit gate scope (closes U-3 / A4).** `audit_log` remains globally preserved and additive. The canonical-user deletion gate applies to the foreign key `audit_log.user_id` reference only. The gate is **not** extended to the free-text audit references in `entity_id`, `target_id`, or `entity_name`. | §5.1, §5.4 |

# Context

## 3. Inspected repository facts

Every fact was read from the working tree at commit
`3e982990949cc666ccbdc697ee20b8111be6c16c`. Line references are exact. The authoritative
schema is the repository's single migration file
`src-tauri/src/db/migrations/001_initial.sql`, which defines **44 tables**.

### 3.1 The `identity_store` shape and the exact UNIT join key

* **G1** — The `identity_store` table columns are `identity_id` (TEXT PRIMARY KEY),
  `subject_type` (`'WILAYA' | 'UNIT' | 'ADMIN'`), `subject_id` (TEXT NOT NULL),
  `issuer_identity_id`, `credential_id`, `generation`, `status`
  (`'ACTIVE' | 'REVOKED' | 'SUPERSEDED' | 'EXPIRED'`), `public_key`, `algorithm_version`,
  `signature`, `not_after`, `created_at`, `updated_at`, `deleted`.
  **There is no `unit_uuid` column and no `unit_code` column.**
* **G2** — For a UNIT subject, `subject_id` holds the **UUID string of the `units` row id**.
  Verified twice, independently:
  * `application/services/identity_provisioning_service.rs:405-419` —
    `let unit_id = request.subject_id.to_string();` then
    `self.db.executor().units().get_unit(&unit_id)?` and, on miss, the error
    *"CSR subject_id {unit_id} does not correspond to a known local unit"*. The code
    therefore treats `subject_id` as `units.id`.
  * `docs/architecture/0044-unit-trust-first-v2-bootstrap.md` §8.3 verification rule (3)
    requires *"`subject_id` == unit package's unit"*.
* **G3** — `identity_store.deleted` is **never set to 1** anywhere in
  `src-tauri/src` or `src-tauri/tests`. Every write to the table outside
  `repositories/identity_store.rs` is a test-only `UPDATE ... SET status` or
  `SET not_after`. Consequently a `deleted = 0` filter on `identity_store` is a no-op in
  production, and "any historical row" is exactly "any row".
* **G4** — A UNIT identity row is written to the WILAYA store by
  `identity_provisioning_service.rs:472-500` (`sign_unit_bootstrap_request`), which calls
  `store.upsert(&signed, now)` after a `CredentialGuard::check` Accept verdict and treats
  Replay as an idempotent no-op.
* **G5** — `identity_store.issuer_identity_id` on a UNIT subject row always holds the
  **WILAYA** identity id in production: `identity_provisioning_service.rs:256` sets
  `signed.issuer_identity_id = Some(issuer_identity.identity_id)` where `issuer_identity` is
  resolved by `NodeIdentityResolver::resolve_local_signer(self.db, node_key_store,
  SubjectType::Wilaya)` (line 424). It never holds the UNIT's own `identity_id`. (The
  `SubjectType::Unit => Some(Uuid::new_v4())` mapping at
  `identity_rotation_service.rs:288` is inside a `#[cfg(test)]` helper.)
* **G6** — No table in the schema has a foreign key to `identity_store`. The only text columns
  that hold identity identifiers are `identity_store.issuer_identity_id`,
  `applied_sync_packages.issuer_identity_id` (line 598, the **issuing** identity of an
  imported package), `registry_snapshots.wilaya_identity_id` (line 678), and
  `audit_log.actor_id` (line 564, a free-text actor attribution, indexed but not a foreign
  key). By G5, none of these can hold a UNIT subject's own `identity_id` on a WILAYA node.
* **G7** — A partial index exists: `CREATE INDEX … ON identity_store(subject_type, subject_id)
  WHERE status = 'ACTIVE' AND deleted = 0;` (line 905). It is **partial on ACTIVE**, so it
  cannot serve D11's status-agnostic query. A separate status-agnostic lookup is required.

### 3.2 The current deletion path

* **G8** — `application/services/unit_service.rs:120-129` (`delete_unit`) reads the unit's
  `user_id` via `unit_repo.delete_unit(unit_id)?` and then calls
  `user_repo.delete_user(&user_id)?`.
* **G9** — `src-tauri/src/repositories/units.rs:212-225` (`delete_unit`) does exactly two
  statements: `SELECT user_id FROM units WHERE id = ?1` and
  `DELETE FROM units WHERE id = ?1`. It returns `Option<String>`.
* **G10** — Because of the `ON DELETE RESTRICT` foreign keys listed in §4.1, the single
  `DELETE FROM units` fails whenever any owned child row exists. The current path therefore
  succeeds only for a UNIT that has no orders, allocations, contracts, FIFO layers, or
  consumption rows. It performs **no** child cleanup, **no** `identity_store` cleanup, and
  **no** `monthly_reports` / `stock_movements` / `daily_reports` cleanup (those three carry no
  foreign key to `units` at all, so nothing cascades and nothing blocks).
* **G11** — `src-tauri/src/repositories/users.rs:345-350` (`delete_user`) is
  `DELETE FROM users WHERE id = ?1`.
* **G12** — `audit_log.user_id TEXT REFERENCES users(id) ON DELETE SET NULL` (schema line 549).
  Therefore G11 silently nullifies attribution for every audit row that referenced the
  operator. This is the silent FK-nullification reliance D14 forbids.
* **G13** — `application/services/audit_tx_service.rs:25-66`
  (`execute_with_audit`) opens a real transaction (`conn.transaction()?`), runs the closure,
  and **rolls back on any error**. It is the correct atomicity boundary for a destructive
  operation and already carries the `AuditAction` needed.

### 3.3 Fiscal immutability enforcement, and which tables it actually covers

* **G14** — `application/services/fiscal_historical_guard.rs` exposes
  `assert_year_not_archived`, `assert_import_year_allowed`,
  `assert_opening_snapshot_mutable`, `assert_fiscal_status_mutation_allowed`,
  `assert_export_snapshot_mutable`, and
  `assert_restore_would_not_regress_archived_state`.
* **G15** — Every table the guard touches is **node-global and has no `unit_id`**:
  `fiscal_year_status`, `opening_balance_snapshots`, `fiscal_export_snapshots`, plus
  year-scoped sync imports and backup restore. Consequently the guard imposes **no**
  code-level obstacle to a UNIT purge.
* **G16** — `src-tauri/tests/immutable_historical_mutation_tests.rs:1` states *"archived years
  must remain immutable (fail-closed)"* and pins this with
  `case_a_rejects_mutation_of_archived_fiscal_year_status`,
  `case_b_rejects_import_records_for_archived_year`,
  `case_c_rejects_restore_older_than_live_archived_state`,
  `case_d_rejects_deleting_opening_snapshots_for_archived_year`, and
  `case_e_rejects_overwrite_of_fiscal_export_snapshot_for_archived_year`. These tests remain
  valid and unchanged: they cover global tables, not UNIT-owned tables.
* **G17** — `fiscal_export_snapshots` has **no** `unit_id`. Its columns are `id`,
  `export_hash`, `generated_at`, `generated_by`, `fiscal_year`, `movement_count`,
  `report_count`, `inventory_total_value`, `integrity_state`, `archived_years_count`,
  `active_anomalies_count`, `signing_key_id`, `export_reason`, `export_mode`,
  `target_node_id`. Its `movement_count` and `report_count` are **node-global aggregates over
  data that includes UNIT-owned rows.** See §9 for the consequence this creates.
* **G18** — `opening_balance_snapshots` has **no** `unit_id`; it is keyed by product and fiscal
  year. It is therefore a global-preserve table and is untouched by any UNIT purge.
* **G19** — `fifo_stock_layers.origin_fiscal_year INTEGER NOT NULL DEFAULT 0` is the field
  named by `ARCHITECTURE_FREEZE.md` §2.4's "Origin immutability" bullet. It is a property of
  a *row's* value over time; the row itself is UNIT-owned via
  `fifo_stock_layers.unit_id NOT NULL REFERENCES units(id) ON DELETE RESTRICT`.

### 3.4 The `units` ↔ `users` relationship

* **G20** — `units.user_id TEXT` with `FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE
  SET NULL` (schema lines 45–60). On a WILAYA node, `units.user_id` points at the
  WILAYA-side operator mirror row, which `unit_service.rs:41-79` creates with
  `username = "user"`, `role = User`, `node_id = <unit code>`.
* **G21** — Ownership of that row is **node-scoped by `node_id`, not by foreign key**. G12's
  `ON DELETE SET NULL` means deleting it would also blank `units.user_id`; the unit row is
  being deleted anyway, so this is not a blocker, but it is the reason the purge MUST delete
  the `users` row explicitly rather than rely on any cascade.
* **G22** — `users` is shared: the same table also holds the WILAYA fleet `admin`, the local
  UNIT `admin`, and any other account. Only rows whose `node_id` equals the destroyed UNIT's
  code (and the canonical username `user`) are UNIT-owned. The rest are global-preserve.

# Decision

## 1. (D11) EXPORTED semantics

For a WILAYA node, a UNIT is **EXPORTED** — for the entire lifetime of that `units` row — if
and only if the local `identity_store` contains at least one row whose subject is that UNIT:

```sql
EXISTS (
    SELECT 1
      FROM identity_store
     WHERE subject_type = 'UNIT'
       AND subject_id    = units.id
)
```

Normative properties of this predicate:

* It is **status-agnostic**. No `status` filter: `ACTIVE`, `REVOKED`, `SUPERSEDED`, and
  `EXPIRED` all satisfy it, exactly as D11 requires.
* It is **`deleted`-agnostic in effect**, because G3 proves `deleted` is never set to 1. A
  `deleted = 0` filter MAY be written for clarity but MUST NOT be relied upon to mean
  "historical".
* It joins on `subject_id = units.id`, which G2 establishes twice over.
* It is **monotone while the `units` row exists**: no operation removes or reassigns an
  `identity_store` row, and `identity_store.upsert` is an upsert on
  `(subject_type, subject_id, credential_id, generation)`
  (`repositories/identity_store.rs:202-226`). EXPORTED therefore cannot revert to
  non-EXPORTED while the unit lives — the same monotonicity property ADR-0063 §8 requires of
  the initialization latch.
* G7 forbids relying on the existing ACTIVE-partial index; a status-agnostic query is
  required.
* "EXPORTED" is a **derived backend projection**, never a stored flag. No column is added for
  it. Storing it would create a second source of truth that could drift from `identity_store`
  (`AGENTS.md` §1 P2).
* Once the UNIT is deleted (§4), its `identity_store` rows are deleted with it (§2), so a
  recreated UNIT with the same code starts non-EXPORTED — the clean-lifecycle property D12
  requires.

### 1.1 The `UnitView` projection shape (D19 — U-2 closed)

The predicate above is **authoritative** and unchanged. D19 decides only how it is *exposed*.

```text
Authoritative source : WILAYA-side identity_store, subject_type = 'UNIT', subject_id = units.id
Projection shape     : backend-derived DTO `UnitView` carrying `is_exported: bool`
Derivation           : `is_exported == true`  iff  any historical identity_store row exists
Storage              : none — no column, no mutable flag, no cached copy
```

* **D19.1** — A new backend-derived DTO named **`UnitView`** carries the state. It follows the
  repository's existing derived-projection convention (`models/contract.rs::ContractAllocationView`,
  `models/fifo.rs::InventoryLayerView`, `models/unit.rs::UnitInventoryView`): a `*View` type that
  mirrors the row model and adds backend-derived fields, so that the frontend never re-derives
  business state (`AGENTS.md` §2 A5, §4 F3).
* **D19.2** — The field is the boolean **`is_exported: bool`**, following the repository's
  established derived-boolean prefixes `is_*` / `has_*` (`models/unit.rs:82,84,87`,
  `models/user.rs:80-81`, `models/dto.rs:340`).
* **D19.3** — `is_exported` is `true` **iff** at least one historical `identity_store` row exists
  for that UNIT subject. There is no third source and no fallback reading.
* **D19.4** — The predicate is **deliberately status-agnostic**. `ACTIVE`, `REVOKED`,
  `SUPERSEDED`, and `EXPIRED` **all** satisfy EXPORTED semantics. `identity_store.status` is
  **not** exposed and **not** reused as the UNIT lifecycle state; it is a credential-validity
  fact, not an export fact.
* **D19.5** — `identity_store.deleted` does **not** change this historical-existence meaning.
  G3 already proves `deleted` is never set to `1`, so the meaning is unaffected either way; the
  decision is recorded so no implementation may introduce a `deleted` filter and thereby weaken
  the definition.
* **D19.6** — **Scope is `list_units` only at this stage.** The command contract becomes:

  ```text
  list_units   -> Vec<UnitView>      (new projection)
  create_unit  -> Unit               (unchanged)
  get_unit     -> Unit               (unchanged)
  update_unit  -> Unit               (unchanged)
  ```

  The row model `Unit` (`models/unit.rs:12-20`) and the frontend type `Unit`
  (`src/lib/types.ts:82-89`) are **unchanged**. Widening `create_unit` / `get_unit` /
  `update_unit` to `UnitView` is not decided here and is not required by this ADR.
* **D19.7** — The projection is **backend/domain-owned**. The frontend **consumes**
  `is_exported` and **must not recompute it**. A frontend-side derivation would duplicate the
  authoritative predicate and breach `AGENTS.md` §1 P2 and the FE-141…FE-148 projection-purity
  rules; hiding the two actions in the UI is presentation only, and §3's backend refusal remains
  the authoritative control.
* **D19.8** — **SQL stays in the repository** (`ARCHITECTURE_FREEZE.md` §2.1). The existence
  query is SQL-only in `repositories/identity_store.rs`; the service supplies the raw existence
  fact and the domain derives the field, mirroring how `ContractAllocationView`'s `From`
  implementation computes `effective_remaining` from a row rather than letting SQL or the
  frontend own the derivation.
* **D19.9** — A **status-agnostic existence query is therefore required**.
  `IdentityStorePort` (`domain/identity/ports.rs:13-57`) currently exposes
  `get_by_identity_id`, `get_active_by_subject`, `get_active_by_subject_type`,
  `get_active_by_credential_id`, `max_generation_for_credential`, `list_all`, and `upsert` — and
  **none** of them answers "does any row exist for this UNIT subject irrespective of status".
  `get_active_by_subject` is `ACTIVE`-only and therefore wrong for D19.4. A new port method plus
  its repository implementation is required. This is forced by the existing port surface, not a
  design preference.
* **D19.10** — **No enum lifecycle state is introduced.** D19 fixes a boolean. No
  `export_state` / `UnitLifecycleState` string, no closed value set, and no transition machine
  are ratified, because no repository convention or evidence required one.

## 2. (D12) `identity_store` ownership

```text
identity_store row with subject_type = 'UNIT'  →  UNIT-owned  →  deleted with the UNIT
identity_store row with subject_type = 'WILAYA' →  global-preserve
identity_store row with subject_type = 'ADMIN'  →  global-preserve
```

* UNIT identity rows are deleted **atomically inside the deletion transaction** (§4.3), not
  before or after it.
* Deleting them is the mechanism that makes code reuse clean: `units.code` is `TEXT UNIQUE`,
  so without this step a recreated code would immediately satisfy D11's EXPORTED predicate
  through its predecessor's identity history. D12 explicitly rejects preserving the rows to
  block reuse.
* **Exact dependency behaviour, stated rather than assumed** (D13 class C, §4.5): by G6 no
  foreign key references `identity_store`; by G5 no `issuer_identity_id` value inside this
  database can hold the deleted row's own `identity_id`; by G6 the `applied_sync_packages` and
  `registry_snapshots` columns store the *issuer's* identity id, which for a UNIT subject is
  the WILAYA identity and is therefore retained. Deletion therefore raises **no** referential
  error and leaves **no** dangling identifier inside this database.
* Cross-node trust material that other nodes hold about the destroyed identity is outside this
  database and outside this ADR's scope. This ADR makes no claim about other nodes.

## 3. (D11) WILAYA Units lifecycle and backend enforcement

| UNIT state | Create | Edit | Export `.unit` | Delete |
|------------|--------|------|----------------|--------|
| not EXPORTED | allowed | allowed | allowed | allowed |
| EXPORTED | n/a | **refused** | **refused** | **allowed** |

* **Backend enforcement is mandatory and authoritative** (D11's second clause). The refusal
  MUST be produced by the backend for both operations, and MUST be a fail-closed error, so a
  UI-only disable is never the control.
* The owning checks belong to the unit domain's validation layer and its service, per
  `AGENTS.md` §3 (layer map: SQL confined to `repositories/`, business rules in
  `domain/`/`application/`). The command layer remains a thin auth-guard-and-dispatch shim
  (`AGENTS.md` §3).
* The UI hides or disables the two actions for the same reason D2 requires backend-only
  forced-state enforcement: a UI control is presentation, not authorization.
* **Delete is never gated on EXPORTED.** D11 retains it in every state. It is gated instead on
  §4's ownership matrix and §5's audit-reference gate.
* The WILAYA Units surface reads the state from the `list_units` projection's `is_exported`
  field (D19), which carries the same authoritative semantics as §1. The UI hides or disables the
  two actions accordingly; the backend refusal above remains the control that actually enforces
  them.

## 4. (D13) UNIT deletion ownership matrix

Classification rules used:

* **Class A — UNIT-owned.** The row exists only because this UNIT exists. Deleting the UNIT
  without it would leave an orphan or would preserve state the owner declared destroyed.
  Includes indirect ownership: a child table with no `unit_id` whose only path to the UNIT is
  a foreign-key chain rooted at `units` or at a Class-A row.
* **Class B — global-preserve.** The row is shared, node-global, or belongs to a different
  lifecycle. Retained verbatim.
* **Class C — external / reference-owned.** Not deleted blindly; the exact dependency
  behaviour is stated instead.

### 4.1 Class A — UNIT-owned (delete)

Verified against `src-tauri/src/db/migrations/001_initial.sql`.

| # | Table | Ownership evidence |
|---|-------|--------------------|
| A1 | `supplier_order_item_allocations` | **Indirect.** No `unit_id`. Reached via `item_id → supplier_order_items(id)` (line 482, CASCADE) and via `allocation_id → contract_allocations(id)` (line 483, RESTRICT). Both parents are A-row sets. |
| A2 | `contract_allocation_exceptions` | **Indirect.** No `unit_id`. `allocation_id → contract_allocations(id)` RESTRICT (line 471). |
| A3 | `inventory_layer_consumptions` | **Direct.** `unit_id TEXT NOT NULL REFERENCES units(id) ON DELETE RESTRICT` (lines 249–250). Also `movement_id → stock_movements(id)` and `layer_id → fifo_stock_layers(id)` with no explicit `ON DELETE` (default `NO ACTION`). |
| A4 | `contract_allocations` | **Direct.** `unit_id … REFERENCES units(id) ON DELETE RESTRICT` (line 465). Also `contract_id → contracts(id)` RESTRICT, `contract_product_id → contract_products(id)` RESTRICT, `product_id → products(id)` `NO ACTION`. |
| A5 | `contract_products` | **Indirect.** No `unit_id`. `contract_id → contracts(id)` CASCADE (line 417). Retained as Class A because A4's `contract_product_id` RESTRICT requires it to be deleted *after* A4. |
| A6 | `supplier_order_items` | **Direct.** `unit_id … REFERENCES units(id) ON DELETE RESTRICT` (line 353); `order_id → supplier_orders(id)` CASCADE (line 333); `product_id → products(id)` `NO ACTION`. |
| A7 | `contracts` | **Direct.** `unit_id … REFERENCES units(id) ON DELETE RESTRICT` (line 411); `supplier_id → suppliers(id)` RESTRICT. |
| A8 | `supplier_orders` | **Direct.** `unit_id … REFERENCES units(id) ON DELETE RESTRICT` (line 328); `supplier_id → suppliers(id)` RESTRICT. |
| A9 | `unit_suppliers` | **Direct.** `PRIMARY KEY (unit_id, supplier_id)` with `unit_id → units(id)` RESTRICT (line 389) and `supplier_id → suppliers(id)` RESTRICT. |
| A10 | `fifo_stock_layers` | **Direct.** `unit_id TEXT NOT NULL REFERENCES units(id) ON DELETE RESTRICT` (lines 222–223); `product_id → products(id)` RESTRICT. Carries `origin_fiscal_year` (G19) and `fiscal_year`-bearing inventory valuation. |
| A11 | `stock_movements` | **Direct, partial — see D20.** `unit_id TEXT` (line 211) with **no** foreign key to `units`; `product_id → products(id)` RESTRICT (line 215); carries `fiscal_year` (line 216). `CHECK (movement_type != 'IN' OR unit_id IS NOT NULL)`. Only rows with concrete UNIT attribution are purged; see D20 below the table. |
| A12 | `daily_report_meal_items` | **Indirect.** No `unit_id`. `meal_id → daily_report_meals(id)` CASCADE (line 299); `product_id → products(id)` `NO ACTION`. |
| A13 | `daily_report_meals` | **Indirect.** No `unit_id`. `daily_report_id → daily_reports(id)` CASCADE (line 280). |
| A14 | `daily_reports` | **Direct.** `unit_id TEXT` nullable (line 268) with **no** foreign key; `fiscal_year` (line 276). |
| A15 | `monthly_reports` | **Direct.** `unit_id TEXT NOT NULL` (line 526) with **no** foreign key at all; `UNIQUE(unit_id, report_year, report_month)`; no `deleted` column, so no soft-delete exists here. |
| A16 | `unit_monthly_snapshots` | **Direct.** `unit_id TEXT NOT NULL REFERENCES units(id) ON DELETE CASCADE` (line 519); `product_id → products(id)` CASCADE. Listed as Class A even though the FK would cascade, so the purge is explicit and auditable rather than implicit. |
| A17 | `identity_store` rows with `subject_type = 'UNIT'` | **Direct**, keyed `subject_id = units.id` (G2). See §2. |
| A18 | `users` row with `username = 'user'` and `node_id = <this unit's code>` | **Node-scoped direct.** The canonical operator. See §5 for its hard-delete gate. |
| A19 | `units` (the row itself) | The root. Last. |

#### A11 special handling — the NULL-unit stock movement exception (D20)

`stock_movements.unit_id` is **nullable** and the schema's CHECK is **asymmetric**:

```sql
CHECK (movement_type != 'IN' OR unit_id IS NOT NULL)
```

Only `IN` movements are *required* to carry a `unit_id`. `OUT` and `OPENING` movements may carry
`unit_id IS NULL`. Verified repository facts:

* `StockMovementType::Opening` (`models/inventory.rs:21,29,37,44`) has **no writer**: the only
  references are the enum itself, a parser (`application/reporting/fiscal_year_summary.rs:116`),
  and the balance equation (`application/services/stock_movement_service.rs:66`). No code path
  inserts an `OPENING` movement.
* Therefore, in current production, `unit_id IS NULL` rows are `OUT` movements.

Because the schema admits a NULL attribution, the purge MUST scope A11 explicitly rather than
delete the table's rows wholesale:

```text
DELETE FROM stock_movements WHERE unit_id = <target unit id>
-- rows with unit_id IS NULL are PRESERVED
```

Three rules are normative:

* **D20.1** — Delete **only** rows with a concrete UNIT attribution (`unit_id = <target unit>`).
* **D20.2** — Rows with `unit_id IS NULL` are **preserved**, exactly like the Class-B tables.
* **D20.3** — Ownership is **never inferred** for NULL-unit rows. No implementation may widen
  the predicate — for example by `reference_id`, `notes`, `username`, or temporal adjacency — to
  claim that a NULL-unit row belongs to the deleted UNIT. Such widening would be a business
  inference this ADR does not authorize.

This is the deliberate, bounded answer to the ambiguity recorded as **A1**: the repository cannot
prove what a NULL-unit row represents, so the purge refuses to guess and preserves it. The cost
is stated, not hidden: after a purge, surviving NULL-unit `OUT` movements remain while the
UNIT-attributed movements they once shared a history with are gone. **No compensating business
calculation is introduced** (D21.4).

### 4.2 Class B — global-preserve (retain verbatim)

| # | Table | Why preserved |
|---|-------|---------------|
| B1 | `settings` | Node singleton (`id INTEGER PRIMARY KEY CHECK (id = 1)`). Carries `node_type`, `unit_name`, `wilaya_code`, `current_year`. Holds the local unit code that scopes ownership — deleting it would destroy the node's own configuration, not a UNIT's data. |
| B2 | `products` | Node-global catalog. `UNIQUE`-keyed by `id`; referenced with `RESTRICT`/`NO ACTION` by A4, A6, A12. Deleting the UNIT releases those references; the catalog survives. |
| B3 | `product_tax_classifications` | `product_id → products(id)` CASCADE. Owned by B2, not by any UNIT. |
| B4 | `inventory_stocks` | **Shared / node-global — ratified by D21.3.** **Has no `unit_id`** (G-from-schema: columns are `product_id`, `quantity`, `unit`, `consumption_unit`, `last_updated`, `updated_at`, `node_id`, `deleted`; `UNIQUE(product_id, consumption_unit)`). On a WILAYA node there is exactly **one shared stock row per product across all units**, so the row is not attributable to any single UNIT. Preserved verbatim; see D21.3–D21.4. |
| B5 | `suppliers` | Node-global catalog; `RESTRICT`-referenced by A7, A8, A9. |
| B6 | `opening_balance_snapshots` | **Node-global — ratified by D21.2.** **No `unit_id`** (G18); `UNIQUE(product_id, fiscal_year)`, so there is one opening row per product per year and **no unit dimension at all**. Explicitly protected by D15 and by `FiscalHistoricalGuard::assert_opening_snapshot_mutable` (G14), pinned by `immutable_historical_mutation_tests.rs` `case_d` (G16). Preserved verbatim; see D21.2–D21.4. |
| B7 | `fiscal_year_status` | Node-global; D15-protected; guarded by `assert_fiscal_status_mutation_allowed`; pinned by `case_a`. |
| B8 | `fiscal_export_snapshots` | **No `unit_id`** (G17). D15-protected; guarded by `assert_export_snapshot_mutable`; pinned by `case_e`. See §9 for the aggregate consequence. |
| B9 | `fiscal_operational_snapshots` | Node-global per `fiscal_year`; no `unit_id`. |
| B10 | `applied_fiscal_transitions`, `fiscal_closure_package_registry`, `fiscal_year_tax_policy` | Node-global fiscal machinery; no `unit_id`. |
| B11 | `audit_log` | **Globally retained by D16.** No `unit_id`. Its `user_id` is the only path to a UNIT-owned row and it is governed by §5. |
| B12 | `audit_summary` | Node-global daily aggregate; no `unit_id`, no FK. |
| B13 | `integrity_verification_attempts`, `operational_findings_log`, `operational_sessions`, `telemetry_events`, `rate_limiter_attempts` | Security/observability records. No `unit_id`. `operational_sessions.user_id TEXT NOT NULL` and `telemetry_events.user_id TEXT` are **plain text, no FK** (verified) — so deleting A18 leaves them as intentional historical attributions rather than dangling constraints. |
| B14 | `applied_sync_packages`, `import_audit_events`, `sync_conflicts`, `import_reproducibility_metadata`, `registry_snapshots` | Sync provenance and security-critical replay/audit records. No `unit_id`; no FK to any Class-A row. |
| B15 | `domain_events` | The `ARCHITECTURE_FREEZE.md` §2.5 semantic-cache-invalidation source. Node-global; no `unit_id`. |
| B16 | `users` rows that are **not** the canonical operator of this UNIT | Including the WILAYA fleet `admin` and the local UNIT `admin` on a UNIT node. Ownership is `node_id`-scoped (G22). The local UNIT `admin` is the ADR-0063 §8.1 initialization latch and is destroyed only when its whole node is destroyed, which is outside this ADR's scope — this ADR governs deletion of a *remote* UNIT row on a WILAYA node. |

#### B4 / B6 — node-global and shared preservation (D21, closing U-3 ambiguities A2 and A3)

Both tables lack a `unit_id`, so neither is reachable from a UNIT purge by ownership. D21 ratifies
their preservation explicitly, because repository inspection surfaced a real tension that is
**stated rather than silently resolved**.

**B6 `opening_balance_snapshots` — node-global (D21.2).** The table is keyed
`UNIQUE(product_id, fiscal_year)`: one opening row per product per year, node-wide, with no unit
dimension. It is read by `repositories/inventory.rs:465` joined against `inventory_stocks` (also
unitless), it is guarded by `FiscalHistoricalGuard`, and it is pinned by `case_d`. Meanwhile the
UNIT-owned `fifo_stock_layers` (A10) carries `source_type = 'OPENING'` with a `source_id`.
**Resolution:** the opening-balance row is **node-global and MUST be preserved**; it is not
re-derived, re-scoped, or re-attributed to the destroyed UNIT.

**B4 `inventory_stocks` — shared / node-global (D21.3).** The table is keyed
`UNIQUE(product_id, consumption_unit)` and has **no unit dimension**, so on a WILAYA node a
single quantity row is shared by every unit. Purging one UNIT's `stock_movements` (A11, D20),
`fifo_stock_layers` (A10), and `inventory_layer_consumptions` (A3) removes the movement history
that partly explains that shared quantity, while the quantity row itself survives.

Three rules are normative:

* **D21.1** — `opening_balance_snapshots` (B6) and `inventory_stocks` (B4) are preserved
  verbatim. Both are byte-identical before and after a successful deletion.
* **D21.2** — `opening_balance_snapshots` is classified **node-global**. It is not a UNIT-owned
  aggregate and is never purged, even for an archived fiscal year (D15's §6.1 grant applies to
  Class-A rows only).
* **D21.3** — `inventory_stocks` is classified **shared / node-global** *because it has no unit
  dimension*. It MUST be preserved.
* **D21.4** — **No adjustment, recomputation, decrement, or any other mutation of
  `inventory_stocks` is performed during UNIT deletion.** The unreconciled shared quantity after a
  purge is accepted as a declared consequence, exactly like the stale aggregate counters recorded
  in C-c. **No new business calculation is introduced to compensate** for any preserved
  node-global or shared table. A compensation calculation would be new inventory-valuation logic,
  which is outside this ADR and outside the ratified decision set.


### 4.3 Class C — external / reference-owned (never blindly deleted)

| # | Referenced entity | Exact dependency behaviour |
|---|--------------------|------------------------------|
| C1 | `products` (from A4, A6, A12) | `RESTRICT` / `NO ACTION` in the direction product ← UNIT-row. Deleting UNIT rows satisfies them. The catalog is never a deletion target. |
| C2 | `suppliers` (from A7, A8, A9) | Identical shape to C1. `unit_suppliers` (A9) is a pure join row and is deleted as Class A; the `suppliers` row survives. |
| C3 | `audit_log.user_id` → `users.id` (`ON DELETE SET NULL`) | **This is the one Class-C dependency that changes state as a side effect.** Left unguarded it would nullify attribution (G12). D14 forbids relying on it: §5 converts it into a hard gate instead. No `ON DELETE` behaviour is altered by this ADR. |
| C4 | `units.user_id` → `users.id` (`ON DELETE SET NULL`) | Deleting A18 blanks the `units.user_id` of the row being deleted in A19. No integrity consequence. Recorded so the purge's statement order is deliberate rather than accidental. |
| C5 | `identity_store` (from A17) | No FK anywhere (G6); no value inside this database can reference the deleted row's own `identity_id` (G5, G6). Deletion is referentially inert in-database. Cross-node trust material is out of scope (§2). |
| C6 | `fiscal_year_status` / `opening_balance_snapshots` / `fiscal_export_snapshots` / `inventory_stocks` aggregate inputs | Class B rows whose *recorded values* were computed over data that includes Class-A rows. The rows are preserved (D15, D21); their recorded values describe the pre-deletion state. §9.1 C-c states the consequence, and D21.4 forbids introducing any compensating calculation. |
| C7 | `audit_log` free-text columns `entity_id`, `target_id`, `entity_name` | Plain text, **no foreign key** (verified). They may name a destroyed UNIT and survive as historical attribution. **D22.2** states that the deletion gate is deliberately not extended to them. |

### 4.4 Deletion order (child-first, deterministic)

Derived from the verified foreign-key graph so that no `RESTRICT` or `NO ACTION` edge is
violated at any intermediate step:

```text
 1. supplier_order_item_allocations      (A1; before A6 and before A4 — RESTRICT both ways)
 2. contract_allocation_exceptions       (A2; before A4 — RESTRICT)
 3. inventory_layer_consumptions         (A3; before A10 and A11 — NO ACTION)
 4. contract_allocations                 (A4; before A5 and A7 — RESTRICT)
 5. contract_products                    (A5; after A4 — RESTRICT from A4)
 6. supplier_order_items                 (A6; after A1 — CASCADE parent)
 7. contracts                            (A7; after A4, A5)
 8. supplier_orders                      (A8; after A6 — CASCADE parent)
 9. unit_suppliers                       (A9)
10. fifo_stock_layers                    (A10; after A3)
11. stock_movements  WHERE unit_id = <units.id>                        (A11; after A3; NULL-unit rows PRESERVED per D20)
12. daily_report_meal_items              (A12)
13. daily_report_meals                   (A13; after A12)
14. daily_reports                        (A14; after A13)
15. monthly_reports                      (A15; no FK at all)
16. unit_monthly_snapshots               (A16)
17. identity_store  WHERE subject_type = 'UNIT' AND subject_id = <units.id>   (A17)
18. FAIL-CLOSED GATE: audit_log reference count for the canonical operator   (§5)
19. users  WHERE username = 'user' AND node_id = <units.code>               (A18)
20. units  WHERE id = <units.id>                                            (A19)
```

Steps 12–16 would partially cascade today (`daily_reports` → meals → items;
`units` → `unit_monthly_snapshots`). They are listed explicitly so that the purge is
deterministic and its effect is auditable rather than dependent on SQLite cascade
behaviour, satisfying `ARCHITECTURE_FREEZE.md` §2.5's determinism requirement.

### 4.5 Atomicity and fail-closed behaviour

* The whole sequence runs in **one** transaction. `AuditTxService::execute_with_audit`
  (`audit_tx_service.rs:25-66`) already provides exactly this: real transaction, closure,
  rollback on any error (G13). D13's atomicity requirement is satisfied by reusing it rather
  than by introducing a second transaction facility.
* Any failure at any step rolls back **everything**, including steps already executed. There
  is no partial deletion, no tombstone, no "delete what we could" behaviour.
* The gate at step 18 is evaluated **inside** that transaction, so a unit whose operator has
  audit history is never partially purged (D14).
* No tombstone model is introduced (D14). If deletion fails, the UNIT row survives intact and
  the operation is retryable.

## 5. (D14) Audit deletion safety

### 5.1 The rule

```text
The canonical local UNIT operator may be hard-deleted only when
    SELECT COUNT(*) FROM audit_log WHERE user_id = <operator id>
  evaluates to zero.
If it is non-zero, UNIT deletion fails closed and the entire transaction rolls back.
```

**Scope of the gate (D22.1).** The gate reads exactly one thing: the **foreign key** column
`audit_log.user_id`. It is a reference-integrity precondition on the canonical operator row
(A18), evaluated inside the deletion transaction.

### 5.2 Why the gate replaces FK nullification

* `audit_log.user_id TEXT REFERENCES users(id) ON DELETE SET NULL` (G12) means the existing
  `delete_user` statement (G11) silently destroys attribution for every audit row that
  referenced the operator. D14 forbids relying on that.
* `audit_log` is globally retained (B11, D16). Its rows must survive with their original
  attribution intact, which is only possible if the referencing operator is **not** deleted.
* The two rules are therefore reconciled by making the reference count a **precondition**,
  not by altering the foreign key. **No schema change to `audit_log` is made by this ADR**;
  `ON DELETE SET NULL` remains, as a structural backstop that the gate now prevents from
  firing on the canonical operator.
* The gate runs inside the deletion transaction (D14's final clause), not before it.

### 5.3 The coupling with ADR-0063

ADR-0063 §8.1 makes the local UNIT `admin` row the durable initialization latch, and §8.2
makes explicit UNIT deletion the terminal lifecycle event. Concretely, this ADR must delete
the local operator row (A18) in the same transaction that destroys the latch's UNIT, and the
gate above can legitimately refuse that deletion for a UNIT whose operator acted in the
audit trail. **A UNIT whose operator has audit history is therefore not deletable.** That is the
ratified outcome (U-1, §9.2): no recovery, re-attribution, scoping, or retention escape exists,
and no owner-level decision is pending on it.

### 5.4 What the gate does **not** cover (D22 — closing U-3 ambiguity A4)

`audit_log` carries the destroyed UNIT's identity in two structurally different ways, and D22
draws the line between them explicitly.

| Kind | Columns | Referential status | Gate applies? |
|------|---------|--------------------|---------------|
| **Foreign-key reference** | `user_id` → `users(id) ON DELETE SET NULL` | A real constraint. Deleting the operator **fires** it and destroys attribution. | **Yes — this is the gate (D22.1).** |
| **Free-text reference** | `entity_id`, `target_id`, `entity_name` | Plain text, **no foreign key** (verified). Nothing is nullified, no constraint fires, and the stored value survives the deletion verbatim. | **No — D22.2 forbids extending the gate to these.** |

**D22.2** — The deletion gate is **not** extended to `entity_id`, `target_id`, or
`entity_name`. Rationale, stated rather than assumed: those columns carry no referential
constraint, so deleting a UNIT cannot corrupt them; they are precisely the *historical attribution*
that D16 requires `audit_log` to preserve. Scoping a hard-delete gate to free text would convert
ordinary audit history into a deletion blocker on the strength of a substring match, which is a
different rule from D14's referential-integrity precondition and was not ratified.

**D22.3** — The consequence is stated, not resolved away: after a UNIT purge, surviving
`audit_log` rows may name a UNIT that no longer exists, and there is no join that distinguishes
them from rows naming preserved units. This is the intended, accepted behaviour — an audit record
describing something that was deliberately destroyed is the historical truth, not a defect.

**D22.4** — `audit_log` remains **globally preserved and additive** (`ARCHITECTURE_FREEZE.md`
§2.3). This ADR makes no schema change to it and alters no `ON DELETE` clause (§5.2). No audit
row is deleted, rewritten, or re-attributed by a UNIT purge, other than the one `DeleteUnit` row
the deletion itself writes under §2.3.

## 6. (D15) Archived fiscal-year exception and its exact boundary

### 6.1 The grant

A UNIT-owned historical or domain row **may be purged even when its `fiscal_year` is
archived**. This applies to every Class-A row that carries a fiscal year:

| Row | Fiscal-year column |
|-----|--------------------|
| `stock_movements` (A11) | `fiscal_year INTEGER` |
| `fifo_stock_layers` (A10) | `origin_fiscal_year INTEGER NOT NULL DEFAULT 0` (G19) |
| `daily_reports` (A14) | `fiscal_year INTEGER` |
| `monthly_reports` (A15) | `report_year INTEGER NOT NULL` |
| `unit_monthly_snapshots` (A16) | `report_year INTEGER NOT NULL` |
| `contracts` (A7) | `fiscal_year INTEGER` |
| `contract_allocations` (A4) | `fiscal_year INTEGER` |
| `supplier_orders` (A8) | `fiscal_year INTEGER` |
| `supplier_order_items` (A6) | `fiscal_year INTEGER` |

This is an explicit destructive lifecycle exception to
`ARCHITECTURE_FREEZE.md` §2.4's "Immutable closed years" bullet. The exception is scoped to
**UNIT-owned destructive purge and nothing else**: it does not license any other mutation of
an archived year, and it does not change what "closed" means for any other write path.

### 6.2 What remains protected

**Closed by D21.** D15's protected set is enumerated below and is now **final**; no wider set is
implied. Each entry is verified node-global by G15/G17/G18 and is therefore structurally
unreachable from a UNIT purge:

* `fiscal_year_status` (B7) — the `archived` flag itself.
* `opening_balance_snapshots` (B6) — **node-global by D21.2**; keyed by product and year, with no
  unit dimension. Preserved verbatim, never re-derived or re-scoped.
* `fiscal_export_snapshots` (B8) — global, no `unit_id`.
* `fiscal_operational_snapshots` (B9), `applied_fiscal_transitions` (B10),
  `fiscal_closure_package_registry` (B10), `fiscal_year_tax_policy` (B10).
* `inventory_stocks` (B4) — **shared / node-global by D21.3**, global by
  `(product_id, consumption_unit)` with no unit dimension. Preserved verbatim, and **not
  adjusted, recomputed, or decremented** (D21.4).
* All of `products`, `suppliers`, and their dependent classification rows (B2, B3, B5).
* `audit_log` and `audit_summary` (B11, B12) — globally preserved and additive (D22.3, D22.4).
* All remaining B-series observability, integrity, sync-provenance, and security records
  (B13–B15).
* Backup restore's archived-state regression guard.

Three boundaries keep this list from being misread, and each is deliberate:

1. **The §6.1 grant applies to Class-A rows only.** A `fiscal_year` or `origin_fiscal_year`
   column does **not** make a table globally protected. Conversely, a table is not purgeable
   merely because it carries a fiscal year — it must be UNIT-owned under §4.1.
2. **The archive-state immutability enforced by `FiscalHistoricalGuard` never covered UNIT-owned
   rows.** The five hostile tests (§6.3) exercise global tables only; none seeds a `units` row.
   D15's destructive exception therefore removes no existing protection from any protected table.
3. **Preserved aggregates keep describing the pre-deletion state.** This is stated as a
   consequence (C-c, C6, D21.4), never repaired by a new calculation.

### 6.3 `FiscalHistoricalGuard` is not extended

* **D21.5** — `FiscalHistoricalGuard` (G14) **MUST NOT be modified** for this lifecycle. It is
  neither extended to block the purge, nor adjusted, nor given a new assertion method.
* `FiscalHistoricalGuard` (G14) touches **only** the global tables of §6.2 (G15). A UNIT purge
  therefore requires **no change** to it, and D15 explicitly forbids extending it to block the
  purge. Complete call-site census, verified: `assert_opening_snapshot_mutable` and
  `assert_year_not_archived` (both in `repositories/opening_balances.rs:34,37`),
  `assert_fiscal_status_mutation_allowed` (`repositories/fiscal_year_status.rs:44`),
  `assert_export_snapshot_mutable`
  (`application/services/fiscal_export_snapshot_service.rs:43`),
  `assert_import_year_allowed` (`application/services/sync_import_validation_service.rs:36`), and
  `assert_restore_would_not_regress_archived_state` (`commands/backup.rs:423`). **None** of them
  is on the UNIT purge path.
* The five hostile tests in `immutable_historical_mutation_tests.rs` (G16) remain **valid and
  unchanged**, because every one of them exercises a global table (`case_a`
  `fiscal_year_status`, `case_b` import records, `case_c` restore, `case_d`
  `opening_balance_snapshots`, `case_e` `fiscal_export_snapshots`). None seeds a `units` row.
* `fifo_stock_layers.origin_fiscal_year` immutability (G19) is a rule about a **row's value
  over time**. Destroying the row is outside that rule's scope. A surviving
  `ARCHITECTURE_FREEZE.md` §2.4 "Origin immutability" test that assumed
  `fifo_stock_layers` rows are never deleted would have to be re-scoped by the implementation;
  no such test exists in the repository today.

## 7. (D16) Historical-retention qualification

The retention rules this ADR qualifies, with the surviving text stated rather than left to
inference:

### 7.1 ADR-0055 line 83

Surviving, unchanged: `CANCELLED` excludes the allocation from normal obligation selection;
`historical fulfillment is never erased` **in every non-destructive context**; no allocation is
reactivated; a new contract creates a new allocation.

Qualified: when a UNIT is explicitly destroyed under D13, `contract_allocations` rows (A4)
owned by that UNIT are removed, together with A2 and A5. The `fulfilled_quantity` history they
carried is destroyed with them. This is the owner's explicit decision (D16) and is bounded to
the destroyed UNIT's own allocations.

### 7.2 ADR-0061 §7

Surviving, unchanged: fulfillment resolves against the original `allocation_id`; resolution
filters `deleted = 0` only and never filters on `entitlement_state`; no re-keying; no
re-parenting; no archival table and no archival mechanism; a package that cannot be resolved
fails closed.

Qualified: the three repository facts §7 of ADR-0061 relied on — no `DELETE FROM
contract_allocations` exists today, `deleted` is never set to `1` on an allocation, and the
allocation foreign keys are `ON DELETE RESTRICT` — are true at commit `3e98299` and cease to
be a permanence argument after this ADR. Allocation identity remains **stable for the lifetime
of its UNIT**. It is not stable across the UNIT's destruction, which is a deliberate, bounded
change of meaning and not a re-keying mechanism.

### 7.3 `ARCHITECTURE_FREEZE.md` §2.4

Surviving, unchanged: explicit fiscal year as a stored column; atomic year closure; FIFO
reclassification rather than deletion; single open year; all-or-nothing transitions. The
reclassification bullet is untouched: year close still moves nothing and deletes nothing, and
the §6.1 exception operates on a later, separate, explicitly owner-approved event.

Qualified: "Immutable closed years — closed fiscal years receive no new mutations" and "Cache
entries for closed years are never invalidated" do not bar the §6.1 UNIT-owned purge. They
continue to bar every other mutation of a closed year.

Surviving and untouched by this ADR: §2.3 in its entirety, and §2.5's determinism
requirements. §4.4's explicit ordering exists partly to satisfy §2.5.

### 7.4 What this ADR does not qualify

Global audit and security records (`audit_log` B11, `operational_sessions` B13,
`integrity_verification_attempts` B13, `rate_limiter_attempts` B13) and global / cross-UNIT
fiscal data (B2–B10) remain fully subject to their existing retention rules. Nothing in this
ADR shortens their retention.

## 8. (D18) Schema-change posture

* This ADR introduces **no** new column, table, index, or foreign key.
* The `EXPORTED` predicate (§1) is derived, not stored (D11 + `AGENTS.md` §1 P2). D19's
  `UnitView.is_exported` is likewise **not** a column and **not** a mutable flag: it is computed
  per read from `identity_store`, so it cannot diverge from the authoritative source.
* The audit-reference gate (§5.1) is an existing-table query.
* The status-agnostic `identity_store` existence query required by D19.9 is a **new query on an
  existing column set**, not a schema change. G7 shows the existing ACTIVE-partial indexes
  (`idx_identity_active_subject`, `idx_identity_subject_type_status`) cannot serve it.
* Consistent with D18, any repository change that this ADR later requires — including the
  ownership-matrix queries and any index needed for the status-agnostic `identity_store`
  lookup — is made directly in `src-tauri/src/db/migrations/001_initial.sql`, the repository's
  only migration file. No migration is created for development-database compatibility.

# Consequences

## Data loss (explicit and bounded)

* Destroying a UNIT destroys its orders, allocations, contracts, entitlements, FIFO layers,
  stock movements, daily/monthly reports, inventory snapshots, identity history, and operator
  credential. This is the owner's declared intent (D13) and is not recoverable except from a
  backup.
* Deleting a UNIT whose operator has audit history is **refused**, not degraded (D14, §5).
* `stock_movements` rows whose `unit_id IS NULL` **survive** the purge (D20.2). The surviving
  history is therefore partial, and the repository deliberately does not repair it (D20.3).
* `inventory_stocks` quantities and `opening_balance_snapshots` survive unchanged while the
  UNIT-attributed movements that partly explain them are gone (D21). The resulting
  unreconciled state is **accepted, not repaired** — D21.4 forbids any compensating calculation.

## Trust and identity

* UNIT identity history is no longer available after destruction (D12). A recreated code
  starts a clean identity lifecycle, which means the trust history that would otherwise warn
  an operator about code reuse is gone. That is D12's explicit trade.
* The `EXPORTED` predicate is monotone per `units` row and is not weakened by the deletion of
  `ACTIVE`-status rows: a `REVOKED` or `SUPERSEDED` identity still marks the unit EXPORTED
  (§1, D11). The same monotonicity is what the projected `is_exported` field exposes (D19.4),
  so the WILAYA surface shows a unit as EXPORTED even after its credential has been fully
  revoked.
* After a purge, surviving `audit_log` rows continue to name the destroyed UNIT in their
  free-text columns (D22.3). This is historical truth, not a dangling reference: those columns
  carry no foreign key, and the deletion gate is not extended to them (D22.2).

## Governance

* `ARCHITECTURE_FREEZE.md` §2.4's "immutable closed years" now has a single, named, narrowly
  scoped exception. Any future attempt to widen it must cite §6 and fail the §6.2 protected
  list.
* ADR-0061 §7's permanence argument and ADR-0055 line 83 both retain force everywhere except
  inside a destroyed UNIT. §7 states the surviving text explicitly so no reader has to infer
  it.

## Compatibility

* The `unit` contract surface is otherwise unchanged by this ADR — `deleteUnit` keeps its
  signature (`src/lib/contracts/inventory.contract.ts:47`). Only the backend semantics behind it
  change.
* **One IPC return type changes: `list_units` returns `Vec<UnitView>`** (D19.6). This is an
  additive projection change: `UnitView` carries the same fields as the current `Unit` plus
  `is_exported`, so `UnitsPage` continues to read `id`, `code`, `name`, `wilaya_code`,
  `user_id`, and `created_at` unchanged. `create_unit`, `get_unit`, and `update_unit` keep
  returning `Unit` at this stage.
* The `Unit` row model and the frontend `Unit` type are **unchanged** (`models/unit.rs:12-20`,
  `src/lib/types.ts:82-89`). A new `UnitView` type is added alongside them.
* Because `list_units`' return shape changes, the FE-160 baseline
  `docs/governance/frontend/baselines/projection-ownership.snapshot.json` **requires
  regeneration** during implementation. That baseline is generated and is **not** edited by this
  ADR.
* `docs/governance/frontend/PROJECTION_OWNERSHIP_MAP.md` currently records this projection as
  *`EXPORTED` state on `Unit` (decision U-2 open)* and states that no field is added until U-2 is
  decided. D19 has now decided it. This ADR **does not edit** that map — it is a
  `PROJECTION_OWNERSHIP_MAP.md` governance update, and recording the required follow-up here is
  the correct place for it (Implementation Boundary item 7). Until that update lands, the map
  understates the ratified shape; this ADR, and specifically §1.1 and D19, are authoritative.

# Contradictions discovered, and owner decisions (all closed)

Recorded rather than silently resolved, per the standing instruction.

## 9.1 Contradictions found in the repository (facts, not decisions)

* **C-a** — `application/services/admin_access_first_import_predicates_service.rs:17-19`
  claims the `admin_access` first-import gate is "self-terminating" because a canonical Admin
  then exists. Verified false under soft-delete disable (G12, ADR-0063 F15). Resolved by
  `docs/architecture/0063-unit-operator-credential-lifecycle.md` §8, not by this ADR.
* **C-b** — ADR-0061 §7 justifies allocation permanence partly with "the repo persists no
  durable historical fulfillment ledger". This ADR does not contradict that finding; it
  qualifies only the permanence conclusion (§7.2).
* **C-c** — `fiscal_export_snapshots` is a node-global, closed-year, non-invalidatable record
  whose `movement_count` and `report_count` (G17) are computed over data that includes
  Class-A rows. After a UNIT purge those recorded counts describe the pre-deletion state
  forever, because `ARCHITECTURE_FREEZE.md` §2.4 forbids invalidating closed-year cache
  entries and D15 protects the row. **This is a direct, unavoidable consequence of combining
  D15 with D16.** It is stated, not resolved.
* **C-d** — The current `delete_unit` path (G8–G10) is already broken for any UNIT with
  owned data, because of `ON DELETE RESTRICT`. §4 replaces it; nothing here depends on the old
  behaviour surviving.

## 9.2 Owner decisions: all closed (U-1, U-2, U-3, U-4)

**No open owner decision remains in this ADR.** Every item below is closed by an explicit owner
ratification. U-2 and U-3 were raised by repository inspection; the inspection evidence and the
ratified answers are both recorded so no future reader has to re-derive them.

* **U-1 — The audit-reference gate can permanently block deletion.** *(Status: closed —
  ratified as intended. This is a consequence, not an open question.)* D14 makes deletion fail
  closed when `audit_log` references the canonical operator. Because `audit_log` is globally
  retained (D16), any UNIT whose operator ever performed an audited action is undeletable.
  **This is the ratified behaviour**: §5.1's gate applies unconditionally, there is no
  re-attribution step, no action-type scoping, no credential-row retention escape, and no
  operator-facing recovery for the reference count. The owner selected this fail-closed outcome
  explicitly, so this ADR does **not** present it as a pending choice; it is recorded here only
  so that the permanent-block consequence is visible to implementers and operators.
* **U-2 — Surface `EXPORTED` to the frontend, and in what shape.** *(Status: closed by D19.)*
  D11 requires backend enforcement and UI hiding, and the owner has now ratified the projection
  shape: a backend-derived DTO **`UnitView`** carrying the boolean **`is_exported`**, true iff any
  historical `identity_store` row exists for the UNIT subject, **status-agnostic** across `ACTIVE`
  / `REVOKED` / `SUPERSEDED` / `EXPIRED`, unaffected by `identity_store.deleted`, with
  `identity_store.status` neither exposed nor reused as a lifecycle state. Scope is
  **`list_units` only** (`Vec<UnitView>`); `create_unit`, `get_unit`, and `update_unit` keep
  returning `Unit`. The projection is backend/domain-owned and the frontend must consume rather
  than recompute `is_exported`. SQL stays in the repository. A status-agnostic existence query is
  required because `IdentityStorePort` has no such method (D19.9). **No enum lifecycle state is
  introduced** (D19.10). Encoded at §1.1, §3, and §Compatibility; §4.3's Class-C rows are
  unaffected.
* **U-3 — Exact archived-fiscal-year protection beyond §6.2.** *(Status: closed by D20, D21,
  D22.)* D15 protected "other node-global fiscal machinery identified by repository inspection",
  and §6.2 enumerated what inspection found. The owner has now ratified that enumeration as final
  and has ruled on the four ambiguities inspection surfaced:

  * **A1 — NULL-unit stock movements.** Closed by **D20**. `stock_movements` is purged only for
    rows with concrete UNIT attribution (`unit_id = <target unit>`); `unit_id IS NULL` rows are
    **preserved**; ownership is **never inferred** for them. Encoded at §4.1 (A11 special
    handling) and §4.4 step 11.
  * **A2 — `opening_balance_snapshots` has no unit dimension but feeds UNIT-owned FIFO layers.**
    Closed by **D21.2**: the table is **node-global and MUST be preserved**, never re-derived or
    re-scoped to the destroyed UNIT. Encoded at §4.2 (B6) and §6.2.
  * **A3 — `inventory_stocks` is preserved but becomes unreconcilable after a purge.** Closed by
    **D21.3 / D21.4**: `inventory_stocks` is **shared / node-global because it has no unit
    dimension** and MUST be preserved; it is **not** adjusted, recomputed, or decremented; the
    unreconciled quantity is an accepted declared consequence; and **no new business calculation
    is introduced** to compensate for any preserved node-global or shared table. Encoded at §4.2
    (B4) and §6.2.
  * **A4 — free-text audit references survive with no referential anchor.** Closed by **D22**:
    `audit_log` remains globally preserved and additive; the canonical-user deletion gate applies
    to the foreign key `audit_log.user_id` reference (D22.1) and is **not** extended to the
    free-text `entity_id`, `target_id`, or `entity_name` columns (D22.2). Encoded at §5.1,
    §5.4, and §4.3 C7.

  Also ratified by the same closure: all sixteen Class-A UNIT-owned tables remain purgeable
  regardless of fiscal year, subject to §4.4's FK ordering and D20's explicit exception (**D13 +
  §6.1 + D20**); `FiscalHistoricalGuard` **MUST NOT be modified** (**D21.5**, §6.3); and
  node-global fiscal machinery remains preserved and protected (**D21.1**, §6.2).
* **U-4 — `OPERATIONAL_LIMITATIONS` / `CLAIM_VERIFICATION_MATRIX` reconciliation.** *(Status:
  closed — resolved in this ADR's governance phase, not deferred.)* Both documents are governed by
  `scripts/check_docs_governance.ts`, and `CLAIM_VERIFICATION_MATRIX.md:31` explicitly requires
  the matrix to be updated "when introducing any security modification or new business logic".
  This phase therefore qualified the two affected claims in place:
  * `docs/technical/CLAIM_VERIFICATION_MATRIX.md` — the **Fail-Closed** row's limitations cell
    now records ADR-0063's forced-change allowlist at the single authorization point; the
    **Immutable** row's limitations cell now records that this guard covers `audit_log` and the
    node-global closed-year fiscal machinery absolutely, while UNIT-owned rows were never inside
    that guard and are purgable per §6.1; and a new row records the credential-lifecycle /
    Unit-deletion authority as **not yet implemented**.
  * `docs/technical/OPERATIONAL_LIMITATIONS.md` — §3's One-Way Account Sync claim is now
    explicitly scoped to **sync-delivered accounts only**, with a new adjacent bullet recording
    ADR-0063's local UNIT-admin authority (local admin as sole post-initialization recovery
    authority, self-change, and admin-initiated operator reset) and the absence of any anonymous
    recovery path.
  * No frozen architecture document was touched by this reconciliation, and the generated FE-160
    baselines were not modified.


# Explicitly Out of Scope

* Any application, schema, migration, or test implementation.
* Credential and initialization lifecycle — `docs/architecture/0063-unit-operator-credential-lifecycle.md`.
* Modifying ADR-0040, ADR-0051, ADR-0052, ADR-0055, ADR-0061, or
  `ARCHITECTURE_FREEZE.md`. This ADR is their qualification record.
* Extending `FiscalHistoricalGuard`, or changing any of the five
  `immutable_historical_mutation_tests.rs` cases (§6.3).
* Altering `audit_log`'s schema or its `ON DELETE SET NULL` clause (§5.2).
* Backup, restore, disaster recovery, and the archived-state regression guard — unchanged.
* Cross-node trust material about a destroyed UNIT identity (§2).
* Any *expansion* of the §6.2 protected set beyond what §6.2 enumerates. That enumeration is
  final (D21.1).
* Any *narrowing* of the §5.1 audit-reference gate to action types, credential rows, or
  re-attribution. The gate is unconditional (U-1).
* Extending the §5.1 gate to the free-text audit columns (D22.2), and any future proposal to do
  so requires a new ADR.
* Re-deriving, re-scoping, or adjusting `opening_balance_snapshots` or `inventory_stocks`
  (D21.2–D21.4), and any compensating calculation for preserved node-global or shared rows
  (D21.4).
* Inferring ownership of `unit_id IS NULL` stock movements for a deleted UNIT (D20.3).
* Widening `UnitView` beyond `list_units`, renaming `is_exported`, or introducing an enum
  lifecycle state (D19.6, D19.10).
* Any frontend-side re-derivation of `is_exported` (D19.7).

# Implementation Boundary

Ordered work items, each independently verifiable. Listed for traceability only; none is
performed by this ADR.

1. **Repository** — add ownership-matrix delete statements to
   `src-tauri/src/repositories/units.rs` (replacing the two-statement `delete_unit` at lines
   212–225), in the §4.4 order, including the `unit_id = <units.id>` predicate on `stock_movements`
   (D20.1) and **preserving** `unit_id IS NULL` rows (D20.2); add the `identity_store` delete
   (A17) and the `users` delete (A18); add the `audit_log` reference-count predicate (§5.1,
   `user_id` only per D22.1); add the status-agnostic `identity_store` existence query (§1.1,
   D19.9) and its port method in `domain/identity/ports.rs`.
2. **Service** — rewrite `application/services/unit_service.rs::delete_unit` to run the
   sequence inside `AuditTxService::execute_with_audit` with `AuditAction::DeleteUnit`,
   applying §5's gate in-transaction.
3. **Domain** — add the EXPORTED predicate (§1) and the "Edit/Export refused while EXPORTED"
   validation (§3) to the unit domain's validation layer; add the `UnitView` DTO with
   `is_exported: bool` and its derivation from the domain-owned predicate (§1.1, D19.1–D19.3,
   D19.7). `FiscalHistoricalGuard` is **not** touched (D21.5).
4. **Commands** — keep `delete_unit`, `update_unit`, `export_unit_node_package` as thin
   auth-guard-and-dispatch shims; surface the backend refusals. `list_units` returns
   `Vec<UnitView>`; `create_unit`, `get_unit`, and `update_unit` keep returning `Unit`
   (D19.6).
5. **Frontend** — hide/disable Edit and Export based on the consumed `is_exported` field, and
   surface the fail-closed deletion error (§1.1 D19.7, §3). The frontend MUST NOT recompute
   `is_exported`.
6. **Governance snapshots** — regenerate `docs/governance/frontend/baselines/*.snapshot.json`
   because `list_units` adds the `UnitView` projection field (D19.6, §Compatibility).
7. **Documentation** — `docs/technical/OPERATIONAL_LIMITATIONS.md` and
   `docs/technical/CLAIM_VERIFICATION_MATRIX.md` are **already reconciled in this governance
   phase** (§9.2 U-4). `docs/governance/frontend/PROJECTION_OWNERSHIP_MAP.md` must be updated to
   replace its *"decision U-2 open"* wording with D19's ratified shape (`UnitView.is_exported` on
   `list_units` only); that update is deliberately **not** performed by this ADR (§Compatibility).
   The remaining documentation work is to re-verify the two qualified claims still describe the
   shipped behaviour once implementation lands.

# Verification Expectations

Normative checklist for the implementation phase.

1. A fresh UNIT is not EXPORTED; Edit and Export succeed.
2. After the WILAYA-side `identity_store` holds any UNIT row in any of `ACTIVE`, `REVOKED`,
   `SUPERSEDED`, `EXPIRED`, the unit is EXPORTED and **both** Edit and Export are refused by
   the backend with a fail-closed error — independently of the UI.
3. `list_units` returns each unit's `is_exported` as `true` for every status in the set above and
   `false` for a unit with no `identity_store` row, using one status-agnostic query that does not
   filter on `status` and does not filter on `deleted` (D19.3–D19.5, D19.9). `create_unit`,
   `get_unit`, and `update_unit` still return `Unit`, and the `Unit` model and frontend `Unit`
   type are unchanged (D19.6). A unit whose only `identity_store` rows are non-`ACTIVE` still
   reports `is_exported = true` — proving the predicate is not the ACTIVE-only port method.
4. The Units surface hides or disables Edit and Export using only the consumed `is_exported` field,
   and performs no re-derivation of it; the backend refusal still fires when the UI is bypassed
   (D19.7).
5. Delete remains available in the EXPORTED state.
6. Deleting a UNIT with rows in every Class-A table succeeds and leaves zero Class-A rows
   behind, with no foreign-key violation at any intermediate step of §4.4.
7. All sixteen Class-A tables are purgeable **regardless of their `fiscal_year`**, including
   `archived`, `closed`, and `open` years — the §6.1 grant is not year-scoped (D13, D15, §6.1).
8. `stock_movements` rows with `unit_id = <target unit>` are deleted while `unit_id IS NULL` rows
   for the same products are **preserved** and byte-identical afterwards (D20.1, D20.2).
9. A NULL-unit `OUT` movement is **not** deleted even when its `product_id`, `reference_id`,
   `notes`, `username`, or timing would suggest the destroyed UNIT owned it; no implementation
   infers that ownership (D20.3).
10. Every Class-B table is byte-identical before and after a successful deletion, with the sole
    documented exception of `audit_log`'s attribution being preserved rather than nullified.
11. `opening_balance_snapshots` is preserved verbatim and is **not** re-derived, re-scoped, or
    re-attributed to the destroyed UNIT (D21.2). `inventory_stocks` is preserved verbatim with no
    decrement, adjustment, or recomputation of any `quantity` (D21.3, D21.4).
12. The deletion gate is observed to read `audit_log.user_id` only: an `audit_log` row whose
    `entity_id`, `target_id`, or `entity_name` names the destroyed UNIT does **not** block
    deletion, while a row referencing the canonical operator's `user_id` does (D22.1, D22.2).
13. After a successful deletion, surviving `audit_log` rows still name the destroyed UNIT in their
    free-text columns, and no row was deleted, rewritten, or re-attributed by the purge (D22.3,
    D22.4).
14. `SELECT COUNT(*) FROM identity_store WHERE subject_type = 'UNIT' AND subject_id = <id>`
    returns 0 after deletion, and the deletion leaves no dangling identifier in
    `identity_store.issuer_identity_id`, `applied_sync_packages.issuer_identity_id`, or
    `registry_snapshots.wilaya_identity_id` (§2).
15. A UNIT code can be recreated immediately after deletion, and the recreated unit is **not**
    EXPORTED (§1, D12).
16. `audit_log` rows referencing the canonical operator cause deletion to fail closed, with the
    entire transaction rolled back: the `units` row, every Class-A row, and the `identity_store`
    rows all survive unchanged, and no tombstone is written (§5, D14).
17. The `audit_log` reference check is observed to execute inside the deletion transaction
    (assert by inducing a failure after it and confirming rollback).
18. A UNIT whose owned rows carry an **archived** `fiscal_year` is deletable, and no
     `FiscalHistoricalGuard` assertion fires during the purge (§6).
19. `fiscal_year_status`, `opening_balance_snapshots`, `fiscal_export_snapshots`,
     `fiscal_operational_snapshots`, `applied_fiscal_transitions`,
     `fiscal_closure_package_registry`, `fiscal_year_tax_policy`, `inventory_stocks`,
     `products`, and `suppliers` are all unchanged by a successful deletion (§6.2).
20. `FiscalHistoricalGuard` is unmodified: no new assertion method, no changed signature, and the
    diff for this ADR touches no guard file (D21.5, §6.3).
21. All five `immutable_historical_mutation_tests.rs` cases still pass unmodified (§6.3).
22. The audit chain still verifies after a deletion, and the deletion itself produced one
    `DeleteUnit` audit row carrying the acting operator's identity
    (`ARCHITECTURE_FREEZE.md` §2.3, inherited unchanged).
23. No password, hash, or pre-hash appears in any `audit_log` column or `details` payload.
24. `bun run check:arch` reports zero warnings; `cargo clippy -D warnings`, `cargo test`,
    `vitest`, and the release-integrity and documentation-governance scripts pass.
