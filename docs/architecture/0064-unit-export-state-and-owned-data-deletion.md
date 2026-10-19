# ADR 0064: UNIT Export State & Owned-Data Deletion

# Status
Accepted (2026-10-05)

> **Governance state.** This ADR is **Accepted** (2026-10-05) by owner ratification of the
> deletion, retention, projection-shape, and purge-scope decisions enumerated in §2 and §9.2.
> **All owner decisions U-1 through U-4 are closed; no decision is pending.** It is the
> **authoritative amendment record** for the frozen documents listed in §1 — those files are NOT
> edited by this ADR (per `AGENTS.md` §8 C1 and `docs/architecture/ARCHITECTURE_FREEZE.md` §4).
> **Implementation performed**: WILAYA UNIT owned-data deletion flow, fail-closed preflight guards,
> frontend confirmation/safeguard wiring, and integration tests implemented per this ADR.
>
> Companion to `docs/architecture/0063-unit-operator-credential-lifecycle.md`, which owns the
> credential and initialization lifecycle. This ADR owns the UNIT **lifecycle projection** and
> the **destructive purge**. The two are coupled at exactly one point, recorded in §5.3: the
> local operator row is UNIT-owned, and `audit_log` is not.

> **Revision 2 — forensic amendment (documentation only, 2026-10-15).** This revision corrects
> the repository evidence cited by Revision 1, resolves the canonical-operator ambiguity
> (D1/§5.5), moves the D14 audit-reference gate into the **initial preflight inside the
> deletion transaction** (D2/§4.6), adds an explicit ownership-source **and** deletion-predicate
> matrix for every Class-A entity (D6/§4.1), defines NULL-ownership and cascade-safety policy
> (D3/§4.8), adds aggregate-integrity fail-closed rules for supplier orders, contract
> allocations, and order-item allocations (D4/D5/§4.7), adds a fail-closed policy for
> `daily_report_meal_items.fifo_layer_id` (D8/§4.9), corrects the inaccurate NULL-unit
> `OUT`-movement claim (D9/§4.1 A11), and documents the real migration mechanism and its
> compatibility consequences (D11/§8). It introduces **no** new schema, migration, trigger,
> code, or test change, and **no** new owner-level business decision: the D13/D15/D16
> destructive-purge policy is preserved unchanged (D10), and any question that repository
> evidence cannot settle is recorded in §9.3 rather than invented. Source references were
> re-derived from working-tree HEAD `0513ae5885f644b91a2e941de3f61394893fd1db`; Revision 1's
> cited commit `3e98299` is superseded for **all** line references. Decisions not restated here
> are unchanged.
>
> **Owner ratification — Q6 (2026-10-15).** The supplier-order ownership anomaly that Revision 2
> recorded as the open question **Q6** in §9.3.3 is hereby **resolved**: **Option A — fail-closed
> rejection** is the binding decision, and **Option B (delete the target-owned item) is rejected**.
> The normative rule lives in §4.7 D4 and is restated in the resolved-decision block of §9.3.3.
> This ratification adds exactly one owner-level decision and changes no other owner-ratified
> decision (D11–D22 unchanged); it authorizes no implementation beyond the Phase-0 preflight
> rejection already specified.
>
> **Owner decisions — Q1–Q5 (recorded in this governance phase).** The remaining
> governance/owner questions that Revision 2 recorded in §9.3.3 are **owner-resolved**:
> **Q1** — Option A, amend `ARCHITECTURE_FREEZE.md` §2.4 (performed through the separate
> RFC-to-ADR process); **Q2** (`fifo_layer_id` FK) and **Q3** (`NOT NULL` hardening) — Option B,
> deferred to a separate schema review; **Q4** (UNIT contract-catalog import enforcement) and
> **Q5** (cross-UNIT repair path) — Option A, approved policy that is **not yet implemented**.
> With the previously ratified **Q6** (Option A) this closes the §9.3.3 register; none reopens an
> owner-ratified decision (D11–D22). See §9.3.3.
>
> **Owner decisions — D11 bootstrap scope and Q4 import scope (2026-10-19).**
> **D11 (identity bootstrap re-export).** The existence of **ANY** historical `identity_store`
> row for a UNIT — `ACTIVE`, `REVOKED`, `SUPERSEDED`, or `EXPIRED` — prevents **bootstrap
> re-export** (`.unit` issuance) for that UNIT, status-agnostically. This decision
> **supersedes and replaces the ACTIVE-only bootstrap guard described as `SEC-004-03`**, which
> exists only in code comments and tests (`identity_provisioning_service.rs`,
> `identity_unit_signing_guard_tests.rs`) and is **not defined in any formal document**. The
> policy aligns the implementation with the re-export rejection already documented in
> `0044-unit-trust-first-v2-bootstrap.md` §7 (S4 amendment note) and §8.3's table row *"إعادة
> التصدير"* (line 298), and in the runbooks that state *"إعادة التصدير = رفض"*
> (`docs/runbooks/key-loss-and-reprovisioning.md:80`,
> `docs/runbooks/security-production-keys.md:140`), whose ACTIVE-only mechanism wording the
> status-agnostic rule now generalizes. The decision **does not alter the identity-rotation
> path** (ADR-0044 §8.3 — the CSR/recovery/rotation path stays *"دون تغيير"*). The remaining
> D11/D19 implementation requirements — refusing **Edit** after export, enforcing the
> **status-agnostic** Export refusal, and exposing **`UnitView.is_exported`** to the frontend —
> are **ratified but NOT implemented**; no statement in this ADR (or the ADR index) claims them
> complete.
> **Q4 (contract-catalog import).** The scope wording of **Q4**, **G26**, and **V5** is corrected
> to describe the actual import path: contract catalogs are **exported from WILAYA** for import
> by a **target UNIT**; **WILAYA rejects** importing a catalog distributed to a UNIT; the **UNIT
> importer checks the local `target_node_id`**; and the approved requirement is that the **UNIT**
> catalog import **explicitly rejects** an allocation whose contract/unit relationship conflicts
> with the contract or with the local UNIT scope — with an **explicit validation error**, never a
> silent skip. This correction authorizes **no unrelated WILAYA change**.
>
> **Label disambiguation.** The `(Dn/§x.y)` markers introduced by this revision — e.g. "D1/§5.5",
> "D2/§4.6", "D11/§8" — are **Revision-2 forensic-review item numbers** identifying this
> revision's mechanical corrections. They are **distinct from** the owner-ratified decision set
> `D11–D22` in the *Owner-ratified decision set* section below. Where the two could be confused
> (only the number `D11` appears in both), the Revision-2 item is always cited together with its
> section and never denotes an owner decision. §9.3.2 restates the same mechanical choices under
> the collision-free label `R1–R10`.

# Date
2026-10-05 (Revision 2: 2026-10-15)

# Owner
Architecture / Security / Data Governance

# Reference

## 1. Documents amended or qualified by this ADR (frozen files are not edited by this ADR)

| Document | Exact locus | Relationship |
|----------|-------------|--------------|
| `docs/architecture/0055-contract-centric-procurement.md` | line 83 — *"`CANCELLED` excludes the allocation from normal obligation selection; historical fulfillment is never erased"* | **Qualified, not edited.** This sentence is sound for every non-destructive context and remains authoritative there. It is qualified for exactly one situation: explicit UNIT destruction, where `contract_allocations` rows owned by the destroyed UNIT are removed. §7 defines the boundary. No other allocation-retention reading changes. |
| `docs/architecture/0061-unit-wilaya-allocation-fulfillment-state-sync.md` | §7 "Released / inactive allocations" (line 225) | **Qualified, not edited.** §7's permanence argument rests on three verified repository facts — no `DELETE FROM contract_allocations` exists today, `deleted` is never set to `1` on an allocation, and the allocation FKs are `ON DELETE RESTRICT`. This ADR is the decision that first introduces a bounded `DELETE FROM contract_allocations`. §7 defines the boundary and restates the parts of §7 that survive unchanged (no re-keying, no re-parenting, no archival table). |
| `docs/architecture/ARCHITECTURE_FREEZE.md` | §2.4 Fiscal Semantics (line 122), §2.5 Reproducibility & Determinism (line 131), §2.3 Audit Trail (line 113) | **Re-scoped, not edited by this ADR.** §6 of this ADR re-scopes §2.4's "Immutable closed years" and "Origin immutability" bullets for UNIT-owned destructive purge only, and §7 names the one §2.5 consequence. Per the owner's Q1 decision (Option A, §9.3.3), the §2.4 "Immutable closed years" bullet was subsequently amended — **through the separate RFC-to-ADR process** (`docs/architecture/rfcs/2026-10-16-freeze-2-4-unit-owned-destructive-purge-exception.md`), **not by this ADR** — to record a narrow, permanent sanctioned exception limited to that bullet's first sentence; §2.4's second sentence and other bullets are untouched. §2.3 (dual-write, hash chain, transactional audit, additive-only schema, single repository, deterministic ordering, chain invariant) is inherited **unchanged** and is a constraint on this ADR's own audit obligation. |
| `docs/architecture/0044-unit-trust-first-v2-bootstrap.md` | §8.3 Packaged-Identity Contract (line 285) | **Referenced, unchanged.** Its verification rule (3) — *"`subject_id` == unit package's unit"* — is the load-bearing fact that makes §2's ownership join key exact. |
| `docs/architecture/0051-admin-access-package.md` | §5 Account Ownership Invariant (line 77) | **Referenced, unchanged.** §5's operator-preservation invariant holds for `admin_access`; this ADR governs the separate, owner-approved explicit deletion event. |
| `docs/architecture/0052-canonical-unit-operator.md` | §D1 (line 46), §D2 (line 48) | **Referenced, unchanged.** §D2's `UNIQUE(username, node_id)` is what makes node-scoped ownership of operator rows exact. §D1's `CreateUnitRequest` clause is superseded by `0063` §3, not by this ADR. |

# Owner-ratified decision set (normative, 2026-10-05)

| # | Ratified decision | Encoded at |
|---|-------------------|-----------|
| D11 | A WILAYA UNIT is **EXPORTED** if the WILAYA-side `identity_store` contains ANY historical identity row for that UNIT. Status does not matter (`ACTIVE`, `REVOKED`, `SUPERSEDED`, `EXPIRED`). Once EXPORTED: Edit hidden/disabled, Export hidden/disabled, Delete remains. Enforced in the backend as well as the UI. **Scope clarification (owner decision, 2026-10-19):** the status-agnostic rule also gates **bootstrap re-export** — `.unit` issuance is rejected whenever ANY identity history exists for the UNIT — **superseding the `SEC-004-03` ACTIVE-only bootstrap guard** (recorded only in code comments and tests, not in a formal document); the identity-rotation path is unchanged; the Edit-after-export refusal and `UnitView.is_exported` (D19) remain **not implemented**. | §1, §3 |
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

> **Revision 2 does not add or reopen an owner-ratified decision.** The D11–D22 set above is
> unchanged. Revision 2 refines the *design mechanics* required to implement D13 (ownership
> matrix, preflight, deletion order, NULL/cascade safety, aggregate integrity, FIFO
> references) and D14 (gate placement) so that the already-ratified policy is correct and
> fail-closed. Where a mechanical choice could not be derived from repository evidence, it was
> listed in §9.3 as an unresolved question with its consequence, **not** silently decided. (The
> Q1–Q6 items have since been owner-resolved; see §9.3.3.)

# Context

## 3. Inspected repository facts

Every fact was read from the working tree at commit
`0513ae5885f644b91a2e941de3f61394893fd1db` (Revision 2). Revision 1 cited commit
`3e982990949cc666ccbdc697ee20b8111be6c16c`; the schema file and several code files changed
after that commit (see §3.5), so **every line reference below is a current-HEAD reference**
and Revision 1's line numbers are superseded. The authoritative schema is the repository's
single migration file `src-tauri/src/db/migrations/001_initial.sql`, which defines **44
tables** (45 textual `CREATE TABLE` matches; the 45th is the phrase inside a comment at line
644).

### 3.1 The `identity_store` shape and the exact UNIT join key

* **G1** — The `identity_store` table columns are `identity_id` (TEXT PRIMARY KEY),
  `subject_type` (`'WILAYA' | 'UNIT' | 'ADMIN'`), `subject_id` (TEXT NOT NULL),
  `issuer_identity_id`, `credential_id`, `generation`, `status`
  (`'ACTIVE' | 'REVOKED' | 'SUPERSEDED' | 'EXPIRED'`), `public_key`, `algorithm_version`,
  `signature`, `not_after`, `created_at`, `updated_at`, `deleted`.
  **There is no `unit_uuid` column and no `unit_code` column.**
* **G2** — For a UNIT subject, `subject_id` holds the **UUID string of the `units` row id**.
  Verified twice, independently:
  * `application/services/identity_provisioning_service.rs:405-418` —
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
  `identity_provisioning_service.rs::sign_unit_bootstrap_request` (declared at line 452;
  `store.upsert(&signed, now)` at line 481), which runs after a `CredentialGuard::check`
  Accept verdict and treats Replay as an idempotent no-op.
* **G5** — `identity_store.issuer_identity_id` on a UNIT subject row always holds the
  **WILAYA** identity id in production: `identity_provisioning_service.rs:256` sets
  `signed.issuer_identity_id = Some(issuer_identity.identity_id)` where `issuer_identity` is
  resolved by `NodeIdentityResolver::resolve_local_signer(self.db, node_key_store,
  SubjectType::Wilaya)` (line 420). It never holds the UNIT's own `identity_id`. (The
  `SubjectType::Unit => Some(Uuid::new_v4())` mapping at
  `identity_rotation_service.rs:288` is inside a `#[cfg(test)]` helper.)
* **G6** — No table in the schema has a foreign key to `identity_store`. The only text columns
  that hold identity identifiers are `identity_store.issuer_identity_id`,
  `applied_sync_packages.issuer_identity_id` (line 605, the **issuing** identity of an
  imported package), `registry_snapshots.wilaya_identity_id` (line 685), and
  `audit_log.actor_id` (line 571, a free-text actor attribution, indexed by
  `idx_audit_actor_timestamp`, line 842, but not a foreign key). By G5,
  `identity_store.issuer_identity_id` on a UNIT subject row can only hold the **WILAYA**
  identity, and `registry_snapshots.wilaya_identity_id` likewise names a WILAYA identity, so
  neither can hold the deleted row's own `identity_id`. The `applied_sync_packages` column is
  the exception, handled by **G6b**.
* **G6b (Revision 2)** — `applied_sync_packages.issuer_identity_id` is **not** constrained to
  WILAYA issuers. The V2 issuer gate accepts a UNIT issuer for four package kinds —
  `STOCK_MOVEMENTS`, `DAILY_REPORT`, `MONTHLY_SUMMARY`, `CONTRACT_FULFILLMENT`
  (`sync_package_identity_verification_service.rs:60-65`) — and the importer records the
  package's `issuer_identity_id` verbatim (`commands/import_export.rs:2034`). A WILAYA node can
  therefore retain an `applied_sync_packages` row that names a UNIT identity whose local
  `identity_store` row a later purge removes. Because no foreign key targets `identity_store`
  (G6), this retained value is a **non-resolving historical provenance identifier**, not a
  referential error, and it is intentionally retained under D22.
* **G7** — A partial index exists: `CREATE UNIQUE INDEX … ON identity_store(subject_type,
  subject_id) WHERE status = 'ACTIVE' AND deleted = 0;` (schema lines 911–912,
  `idx_identity_active_subject`). It is **partial on ACTIVE**, so it cannot serve D11's
  status-agnostic query. A separate status-agnostic lookup is required.

### 3.2 The current deletion path

* **G8** — `application/services/unit_service.rs:148-156` (`delete_unit`) reads the unit's
  associated `user_id` via `unit_repo.delete_unit(unit_id)?` and then calls
  `user_repo.delete_user(&user_id)?`. The `user_id` is taken from the `units` row as read at
  delete time — it is **not** re-derived from the canonical `(username, node_id)` key.
* **G8b (Revision 2)** — The current IPC command `commands/units.rs:77-94` (`delete_unit`)
  **already** executes that service body inside
  `AuditTxService::execute_with_audit(db, AuditAction::DeleteUnit, &user_ctx, |tx| …)` (line
  88). Revision 1's framing of the current path as non-transactional is therefore
  inaccurate: the transaction and the `AuditAction::DeleteUnit` audit write already exist at
  the command. What is missing is the ownership matrix, the canonical-operator resolution,
  the in-transaction preflight, and the D14 gate — all of which the service body must
  perform **inside** the closure the command already supplies. No new transaction facility
  is introduced (§4.6).
* **G9** — `src-tauri/src/repositories/units.rs:212-225` (`delete_unit`) does exactly two
  statements: `SELECT user_id FROM units WHERE id = ?1` and
  `DELETE FROM units WHERE id = ?1`. It returns `Option<String>`.
* **G10** — Because of the `ON DELETE RESTRICT` foreign keys listed in §4.1, the single
  `DELETE FROM units` fails whenever any owned child row exists. The current path therefore
  succeeds only for a UNIT that has no orders, allocations, contracts, FIFO layers, or
  consumption rows. It performs **no** child cleanup, **no** `identity_store` cleanup, and
  **no** `monthly_reports` / `stock_movements` / `daily_reports` cleanup (those three carry no
  foreign key to `units` at all, so nothing cascades and nothing blocks).
* **G11** — `src-tauri/src/repositories/users.rs:493-495` (`delete_user`) is
  `DELETE FROM users WHERE id = ?1`. Two resolution helpers exist:
  `get_user_by_username(username, node_scope)` (lines 37-65) filters `deleted = 0` and
  matches on `username = ?1 AND COALESCE(node_id, 'WILAYA') = ?2`; and
  `get_user_by_username_raw(username, node_scope)` (lines 72-99) is the same query **without**
  the `deleted = 0` filter. `users` is `UNIQUE(username, node_id)` (`001_initial.sql:49`), so
  at most one row exists per canonical key.
* **G12** — `audit_log.user_id TEXT REFERENCES users(id) ON DELETE SET NULL` (schema line
  555). Therefore G11 silently nullifies attribution for every audit row that referenced the
  operator. This is the silent FK-nullification reliance D14 forbids.
* **G13** — `application/services/audit_tx_service.rs:25-94` (`execute_with_audit` at lines
  25-35 delegates to `execute_with_audit_metadata` at lines 39-94) opens a real transaction
  (`conn.transaction()?`, line 50), runs the closure, and **on closure error returns `Err`
  with the transaction dropped — i.e. an automatic rollback — and writes no audit row**
  (lines 57-68). The success audit entry (`log_success`) is written only **after** the
  closure returns `Ok` (lines 70-83), inside the same transaction, immediately before
  `tx.commit()` (line 86). **Accurate failure semantics (D2): a failed deletion persists no
  `audit_log` row at all — not even a `status = 'Failed'` row.** Any documentation or test
  that claims otherwise is wrong. It is nevertheless the correct atomicity boundary for a
  destructive operation and already carries the `AuditAction` needed.

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
* **G19** — `fifo_stock_layers.origin_fiscal_year INTEGER NOT NULL DEFAULT 0` (schema line
  240) is the field named by `ARCHITECTURE_FREEZE.md` §2.4's "Origin immutability" bullet. It
  is a property of a *row's* value over time; the row itself is UNIT-owned via
  `fifo_stock_layers.unit_id TEXT NOT NULL REFERENCES units(id) ON DELETE RESTRICT` (schema
  lines 229–230).

### 3.4 The `units` ↔ `users` relationship

* **G20** — `units.user_id TEXT` with `FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE
  SET NULL` (schema lines 52–63). On a WILAYA node, `units.user_id` points at the
  WILAYA-side operator mirror row, which `unit_service.rs::create_unit` (lines 69-116, the
  operator upsert at lines 88-98) creates with `username = OPERATOR_USERNAME` (`"user"`,
  constant at line 18), `role = User`, `node_id = <unit code>`.
* **G21** — Ownership of that row is **node-scoped by `node_id`, not by foreign key**. G12's
  `ON DELETE SET NULL` means deleting it would also blank `units.user_id`; the unit row is
  being deleted anyway, so this is not a blocker, but it is the reason the purge MUST delete
  the `users` row explicitly rather than rely on any cascade.
* **G22** — `users` is shared: the same table also holds the WILAYA fleet `admin`, the local
  UNIT `admin`, and any other account. Only rows whose `node_id` equals the destroyed UNIT's
  code (and the canonical username `user`) are UNIT-owned. The rest are global-preserve.

### 3.5 Revision 2 — corrected schema, write-path, and runtime facts

These facts were re-derived from current HEAD (`0513ae5`). They correct or extend Revision 1
and are the evidentiary basis for the §4 predicates and the §4.6 preflight. All schema line
references are to `src-tauri/src/db/migrations/001_initial.sql`.

**G23 — Reported-absent tables.** The tables `supplier_order_receipts` and `inventory_layers`
**do not exist** anywhere in `src-tauri/src` or `src-tauri/tests` (zero matches). The actual
FIFO table is `fifo_stock_layers` (schema lines 227-252). `inventory_layer_consumptions`
**exists** (schema lines 254-266): columns `id`, `unit_id TEXT NOT NULL → units(id) ON DELETE
RESTRICT`, `movement_id TEXT NOT NULL → stock_movements(id)` (no explicit action → `NO
ACTION`), `layer_id TEXT NOT NULL → fifo_stock_layers(id)` (no explicit action → `NO ACTION`),
`quantity`, `unit_cost`, `total_cost`, `consumed_at`.

**G24 — Direct, NOT NULL `unit_id` foreign keys.** Each of the following carries a **direct,
`NOT NULL`** `unit_id` with `REFERENCES units(id) ON DELETE RESTRICT`, and is therefore a
**direct Class-A entity** (not merely indirect):
`fifo_stock_layers` (lines 229-230), `inventory_layer_consumptions` (lines 256-257),
`contracts` (lines 403, 418), `contract_allocations` (lines 456, 472). `unit_suppliers` is
direct too, via a composite primary key with `unit_id → units(id) ON DELETE RESTRICT` (lines
392, 396).

**G25 — Independently nullable supplier-order ownership.** `supplier_orders.unit_id TEXT`
(schema line 332) and `supplier_order_items.unit_id TEXT` (line 345) are **each nullable**
and **each independently foreign-keyed** to `units(id) ON DELETE RESTRICT` (lines 335, 360).
`supplier_order_items.order_id → supplier_orders(id) ON DELETE CASCADE` (line 358). At the
**schema level** a header can be target-owned while an item is `NULL`- or other-owned, and
vice versa. The ordinary production create path threads **one** settings-derived `unit_id`
through both the header and every item
(`commands/orders.rs:39,201-221` → `order_service.rs:391,434` → `repositories/orders.rs:64,93`),
so a fresh order cannot diverge. The **only** divergence path is
`OrderService::update_supplier_order` (`order_service.rs:458`), which writes items with
`order.unit_id.clone().unwrap_or_else(|| unit_id.to_string())` while the header
`UPDATE` (`repositories/orders.rs:150`) **never touches `unit_id`** — so a pre-existing
`NULL`-unit order can end up with target-unit items under a `NULL` header. No `UPDATE`
anywhere writes `unit_id` on either table, and **no sync-import or row-level restore path
exists** for either table (backup restore is a whole-file atomic swap,
`infrastructure/backup/sqlite_backup_adapter.rs:647-689`).

**G26 — Contract/allocation ownership divergence.** `contracts.unit_id` and
`contract_allocations.unit_id` are both `NOT NULL` (G24). The ordinary authoring INSERT
(`repositories/contracts.rs:472`, caller `application/services/contract_service.rs:151`) binds
`unit_id = &contract.unit_id`, so equality holds **by construction**. The contract-catalog
import INSERT (a catalog **exported from WILAYA for import by a target UNIT** — WILAYA nodes
themselves reject importing a catalog distributed to a UNIT,
`application/usecases/sync/import_contract_catalog_package.rs:117-123`;
`repositories/contracts.rs:1106`, caller
`application/services/sync_import_execution_service.rs:342,350`) binds the **payload**
allocation's `unit_id` and applies only a per-row `in_scope` filter; it performs **no**
`allocation.unit_id == contract.unit_id` check, and the import validator
(`application/sync/import_validation.rs:197-249`) never inspects allocation unit
consistency. For a UNIT importer the scope check makes equality hold transitively; the
`WILAYA`/`None` branch is **unreachable through the wired command**
(`application/usecases/sync/import_contract_catalog_package.rs:117-123` rejects a `None`
importer) but is reachable by calling `import_contract_catalog_sync` directly (tests). This
is the residual divergence risk D5 addresses.

**G27 — `daily_report_meal_items.fifo_layer_id` has no foreign key.**
`daily_report_meal_items.fifo_layer_id TEXT` (schema line 312) has **no** FK, **no** `CHECK`,
and **no** unit column. The table's only FKs are `meal_id → daily_report_meals(id) ON DELETE
CASCADE` (line 316) and `product_id → products(id)` (`NO ACTION`, line 317). The live writer
(`repositories/reports.rs:265` via `daily_report_service.rs:248-256`) always writes a layer
that was consumed through a **unit-scoped** FIFO query
(`repositories/fifo_layers.rs:235` filters `unit_id = ?1`), and the sync daily-report import
always writes `NULL` (`import_daily_report_package.rs:137`,
`sync_import_execution_service.rs:116`). A dead method
(`SyncImportExecutionService::import_monthly_report` → `insert_raw_meal_item`) could write a
payload-supplied layer id with no validation, but it has **no caller**. The database
therefore cannot itself prove that a surviving report row does not reference a deleted
UNIT's layer; §4.9 makes this a fail-closed preflight check.

**G28 — `inventory_layer_consumptions` writer coupling.** The sole production writer is
`daily_report_service.rs:164` (`FifoRepository::create_consumption_record`, repository at
`repositories/fifo_layers.rs:283-326`). It passes `unit_id_str`, the same
`movement_id` just written with `unit_id = unit_id_str` (`daily_report_service.rs:153,156`),
and layer ids produced by `consume_fifo_typed(unit_id_str, …)`
(`daily_report_service.rs:101`, unit-filtered at `fifo_layers.rs:235`). Production therefore
keeps the consumption's `unit_id`, the movement's `unit_id`, and the layer's `unit_id`
aligned **by construction**, while the schema permits divergence and the FK actions are
`NO ACTION` in both directions. §4.10 makes a cross-UNIT reference a fail-closed preflight
error rather than a late FK failure.

**G29 — `daily_reports.unit_id` can legitimately be `NULL` in production.**
`daily_reports.unit_id TEXT` (schema line 275) has no FK. The local path
(`daily_report_service.rs:224`, `insert_daily_report_header` taking `Option<&str>`,
`reports.rs:202-219`) errors on a missing unit, but the **sync import** path
(`reports.rs:495-511 insert_raw_daily_report`) writes `report.unit_id.as_deref()` verbatim,
so a sync-delivered report may carry `NULL`. `monthly_reports.unit_id` is `NOT NULL` with no
FK (schema line 533). §4.8 states the preservation policy for the `NULL` case.

**G30 — `stock_movements.unit_id` NULL provenance (corrects Revision 1).** The schema allows
`NULL` for any movement type except `IN` (`CHECK (movement_type != 'IN' OR unit_id IS NOT
NULL)`, line 224). In **current production**, however:
* the only `OUT` producer (`daily_report_service.rs:141-156`) always sets
  `unit_id = Some(unit_id_str)` and fails closed when the unit is missing
  (`daily_report_service.rs:76-80`);
* no production code constructs an `OPENING` movement directly (no constructor call site); the
  generic sync import persists a payload-supplied `movement_type`, so it **can** write an
  `OPENING` row, but it forces a concrete `unit_id` (next bullet); and
* the stock-movement import (`sync_import_execution_service.rs:411-413`, sole production
  caller `import_stock_movements_package.rs:91`) forces `Some(unit)`.
Revision 1's statement that "in current production, `unit_id IS NULL` rows are `OUT`
movements" is therefore **inaccurate**. `NULL`-unit rows are reachable only from legacy data
or from test-only `import_stock_movements(…, None)` invocations (e.g.
`tests/import_stock_movements_integrity_tests.rs:108,181`). The conservative preservation
rule is retained for a **different, verified** reason: the repository cannot prove what such
a row represents, so the purge refuses to guess (§4.1 A11, §4.8).

**G31 — Migration runner mechanism.** `src-tauri/src/db/migrations.rs` defines exactly one
migration (`migrations_vec()`, lines 10-18: version `1`, the consolidated
`include_str!("migrations/001_initial.sql")`). `run_migrations` (lines 88-169) creates
`schema_version` if absent, reads `MAX(version)`, and applies a migration **only when
`current_version < version`** (line 116), then records the version (lines 138-145) and
commits. The initial schema uses `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT
EXISTS`. Consequences: (i) editing `001_initial.sql` has **no effect on any database already
recorded at version 1**; (ii) the initial schema is applied exactly once per database; (iii)
D18's "modify the authoritative initial schema directly" instruction is valid only for a
**fresh** or **recreated** database. This is documented as a compatibility consequence in
§8 and §D11.

**G32 — `audit_log` gate column.** `audit_log.user_id TEXT REFERENCES users(id) ON DELETE SET
NULL` (schema line 555) is the only FK column by which a surviving `audit_log` row retains an
attribution to the canonical operator. `username` is `NOT NULL` but is a text copy, not a
constraint; `entity_id`, `entity_name`, `target_id`, and `actor_id` are plain text with **no**
foreign key. The D14 gate reads `user_id` only (§5.1, D22).

**G33 — No unit-deletion tests exist today.** `grep` for `delete_unit`, `DeleteUnit`, and
`deleteUnit` across `src-tauri/tests` returns **zero** matches. The §Verification Expectations
therefore describe new tests, all of which must be added during implementation.

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
  `identity_store` row, and `identity_store.upsert` conflicts on the primary key `identity_id`
  (`ON CONFLICT(identity_id) DO UPDATE`, `repositories/identity_store.rs:202-226`). EXPORTED
  therefore cannot revert to non-EXPORTED while the unit lives — the same monotonicity property
  ADR-0063 §8 requires of the initialization latch.
* G7 forbids relying on the existing ACTIVE-partial index; a status-agnostic query is
  required.
* "EXPORTED" is a **derived backend projection**, never a stored flag. No column is added for
  it. Storing it would create a second source of truth that could drift from `identity_store`
  (`AGENTS.md` §1 P2).
* Once the UNIT is deleted (§4), its `identity_store` rows are deleted with it (§2), so a
  recreated UNIT with the same code starts non-EXPORTED — the clean-lifecycle property D12
  requires.
* **Bootstrap re-export is refused whenever any identity history exists** (owner decision,
  2026-10-19): the predicate above is also the write-time gate on `.unit` re-export — refusal
  applies to `ACTIVE`, `REVOKED`, `SUPERSEDED`, and `EXPIRED` histories alike. This
  **supersedes the ACTIVE-only bootstrap guard described as `SEC-004-03`** (recorded only in
  code comments and tests, never in a formal document) and generalizes the *"إعادة التصدير =
  رفض"* rule already documented in `0044-unit-trust-first-v2-bootstrap.md` §7 (S4 amendment
  note) and §8.3 (line 298). The **identity-rotation path is unchanged** by this decision.
  Status: **policy ratified; implementation pending** — the wired guard is still ACTIVE-only as
  of baseline `00a9d0c445a671034f69d5b1bffb9aa75913d343`.

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
  foreign key references `identity_store`, so deleting a UNIT identity row raises **no**
  referential error. `identity_store.issuer_identity_id` on the deleted row disappears with the
  row, and by G5 every other UNIT subject row's `issuer_identity_id` holds the WILAYA identity;
  `registry_snapshots.wilaya_identity_id` likewise holds a WILAYA identity. The one retained
  identifier that can **stop resolving** is `applied_sync_packages.issuer_identity_id` (G6b): an
  accepted package kind may have been signed by the UNIT, so after the purge the retained
  historical-provenance value names an `identity_store` row that no longer exists. This is a
  **non-resolving identifier, not a foreign-key violation** (no FK targets `identity_store`), and
  it is deliberately retained (D22). Deletion therefore raises no referential error and leaves no
  *referential* dangling edge; the retained provenance value may be non-resolving **by design**.
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

**Implementation status (recorded 2026-10-19 — the table above is ratified policy, not current
code).** As of baseline `00a9d0c445a671034f69d5b1bffb9aa75913d343`: the **Edit-after-export
refusal does not exist** (`unit_service::update_unit` performs no EXPORTED check); the
**Export `.unit` refusal exists only in its ACTIVE-only form** (the `SEC-004-03` guard in
`identity_provisioning_service.rs`) and does not yet reject `REVOKED`/`SUPERSEDED`/`EXPIRED`
histories; **`UnitView`/`is_exported` (D19) is not implemented** — `list_units` still returns
`Vec<Unit>`; and the Units surface does not hide/disable Edit or Export. Only the **Delete
remains** column is in effect today (§4's preflight gates Delete, never EXPORTED). These
requirements remain open, separately authorized implementation work; nothing in this ADR claims
them complete.

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

| # | Entity | Ownership source and deletion predicate |
|---|--------|-----------------------------------------|
| A1 | `supplier_order_item_allocations` | **Indirect (two-sided).** No `unit_id` column. `item_id → supplier_order_items(id)` CASCADE (line 499) and `allocation_id → contract_allocations(id)` RESTRICT (line 500). **Ownership path:** both parents must independently resolve to the target UNIT. **Predicate:** delete a row **iff** (its `item_id` parent has `unit_id = <target>`) **and** (its `allocation_id` parent has `unit_id = <target>`). If the item-parent and allocation-parent imply different UNITs, or either cannot be resolved to a proven UNIT, **fail closed before any DELETE** (§4.7). Must run before A4 (allocation RESTRICT) and before A6 (item CASCADE parent). |
| A2 | `contract_allocation_exceptions` | **Indirect.** No `unit_id`. `allocation_id → contract_allocations(id)` RESTRICT (line 486). **Ownership path:** derive through the allocation. **Predicate:** delete `WHERE allocation_id IN (SELECT id FROM contract_allocations WHERE unit_id = <target>)`. Must run before A4. |
| A3 | `inventory_layer_consumptions` | **Direct.** `unit_id TEXT NOT NULL REFERENCES units(id) ON DELETE RESTRICT` (lines 256-257); `movement_id → stock_movements(id)` and `layer_id → fifo_stock_layers(id)`, both default `NO ACTION` (lines 258-261). **Predicate:** delete `WHERE unit_id = <target>`. Must run before A10 and A11 (`NO ACTION`). Cross-UNIT safety preflight in §4.10: reject if any **non-target** consumption references a target-owned movement or layer. |
| A4 | `contract_allocations` | **Direct.** `unit_id TEXT NOT NULL REFERENCES units(id) ON DELETE RESTRICT` (lines 456, 472); `contract_id → contracts(id)` RESTRICT (470), `contract_product_id → contract_products(id)` RESTRICT (471), `product_id → products(id)` `NO ACTION` (473). **Predicate:** delete `WHERE unit_id = <target>`, after the §4.7 integrity check that `unit_id == parent contracts.unit_id`. Must run before A5 (A4→A5 RESTRICT) and A7 (A4→A7 RESTRICT), and after A1/A2 (their RESTRICT into A4). |
| A5 | `contract_products` | **Indirect.** No `unit_id`. `contract_id → contracts(id)` CASCADE (line 448). **Ownership path:** derive through the contract. **Predicate:** delete `WHERE contract_id IN (SELECT id FROM contracts WHERE unit_id = <target>)`. Must run after A4 (A4.contract_product_id RESTRICT) and before A7. |
| A6 | `supplier_order_items` | **Direct.** `unit_id TEXT` FK `REFERENCES units(id) ON DELETE RESTRICT` (lines 345, 360); `order_id → supplier_orders(id)` CASCADE (line 358); `product_id → products(id)` `NO ACTION` (line 359). **Predicate:** delete `WHERE unit_id = <target>`, after the §4.7 D4 aggregate-integrity check (a target order may not have a `NULL`/other-unit item; a target item under a `NULL`/other-unit order is the Q6 anomaly and **aborts** the deletion — Q6 resolved, §4.7 D4/§9.3.3). Must run after A1 and before A8. |
| A7 | `contracts` | **Direct.** `unit_id TEXT NOT NULL REFERENCES units(id) ON DELETE RESTRICT` (lines 403, 418); `supplier_id → suppliers(id)` RESTRICT (line 419). **Predicate:** delete `WHERE unit_id = <target>`, after A4 and A5. |
| A8 | `supplier_orders` | **Direct.** `unit_id TEXT` FK `REFERENCES units(id) ON DELETE RESTRICT` (lines 332, 335); `supplier_id → suppliers(id)` RESTRICT (line 334). **Predicate:** delete `WHERE unit_id = <target>`, after A6. `NULL`-unit orders are **preserved** (§4.8). |
| A9 | `unit_suppliers` | **Direct.** `PRIMARY KEY (unit_id, supplier_id)`; `unit_id → units(id)` RESTRICT (line 396); `supplier_id → suppliers(id)` RESTRICT (line 397). **Predicate:** delete `WHERE unit_id = <target>`. |
| A10 | `fifo_stock_layers` | **Direct.** `unit_id TEXT NOT NULL REFERENCES units(id) ON DELETE RESTRICT` (lines 229-230); `product_id → products(id)` RESTRICT (line 232). Carries `origin_fiscal_year` (G19, line 240). **Predicate:** delete `WHERE unit_id = <target>`, after A3 and after the §4.9 surviving-report-FIFO preflight. |
| A11 | `stock_movements` | **Direct, partial — see D20.** `unit_id TEXT` (line 218) with **no** foreign key to `units`; `product_id → products(id)` RESTRICT (line 223); carries `fiscal_year` (line 221). `CHECK (movement_type != 'IN' OR unit_id IS NOT NULL)` (line 224). **Predicate:** delete `WHERE unit_id = <target>`; `unit_id IS NULL` rows are **preserved** (§4.8). Must run after A3 (`NO ACTION`). |
| A12 | `daily_report_meal_items` | **Indirect.** No `unit_id`. `meal_id → daily_report_meals(id)` CASCADE (line 316); `product_id → products(id)` `NO ACTION` (line 317); `fifo_layer_id` **no FK** (line 312, §4.9). **Ownership path:** meal → report. **Predicate:** delete `WHERE meal_id IN (SELECT m.id FROM daily_report_meals m JOIN daily_reports r ON m.daily_report_id = r.id WHERE r.unit_id = <target>)`. Must run before A13. |
| A13 | `daily_report_meals` | **Indirect.** No `unit_id`. `daily_report_id → daily_reports(id)` CASCADE (line 301). **Ownership path:** report. **Predicate:** delete `WHERE daily_report_id IN (SELECT id FROM daily_reports WHERE unit_id = <target>)`. Must run after A12 and before A14. |
| A14 | `daily_reports` | **Direct.** `unit_id TEXT` nullable (line 275) with **no** foreign key; `fiscal_year` (line 283). **Predicate:** delete `WHERE unit_id = <target>`. `NULL`-unit reports are **preserved** (§4.8); this is a real production possibility via sync import (G29). |
| A15 | `monthly_reports` | **Direct.** `unit_id TEXT NOT NULL` (line 533) with **no** foreign key at all; `UNIQUE(unit_id, report_year, report_month)` (line 546); no `deleted` column. **Predicate:** delete `WHERE unit_id = <target>`. |
| A16 | `unit_monthly_snapshots` | **Direct.** `unit_id TEXT NOT NULL REFERENCES units(id) ON DELETE CASCADE` (line 526); `product_id → products(id) ON DELETE CASCADE` (line 527). **Predicate:** delete `WHERE unit_id = <target>` **explicitly**, even though the FK would cascade — so the purge is deterministic and auditable rather than dependent on SQLite cascade behaviour (`ARCHITECTURE_FREEZE.md` §2.5). |
| A17 | `identity_store` rows with `subject_type = 'UNIT'` | **Direct**, keyed `subject_id = units.id` (G2). **Predicate:** delete `WHERE subject_type = 'UNIT' AND subject_id = <target>`. No inbound FK exists (G6). See §2. |
| A18 | `users` row with `username = 'user'` and `node_id = <this unit's code>` | **Node-scoped direct.** The canonical operator, additionally required to equal `units.user_id` (D1, §5.5). **Predicate:** resolve (D1), apply the D14 gate (D2, §4.6), then `DELETE FROM users WHERE id = <resolved id>`. |
| A19 | `units` (the row itself) | The root. **Predicate:** delete `WHERE id = <target>`, last. |

**Class-A count (Revision 2).** There are **19 Class-A entities**: the sixteen dedicated
schema tables A1-A16 plus the three row-scoped targets A17-A19 (`identity_store` UNIT rows,
the canonical operator `users` row, and the `units` root row). Revision 1's phrases "sixteen
Class-A tables" in §9.2 and the Verification Expectations understate this and are corrected
here and there. Direct-owned entities are A3, A4, A6-A11, A14-A19; indirect entities are A1,
A2, A5, A12, A13.

**Predicate discipline (normative).** Every predicate above is expressed only in terms of
foreign-key/ownership columns. No predicate may use product identity, monetary amounts,
timestamps, names, `notes`, `reference_id`, `username`, or temporal adjacency as a
substitute for ownership evidence (D20.3; §4.8).

#### A11 special handling — the NULL-unit stock movement exception (D20)

`stock_movements.unit_id` is **nullable** and the schema's CHECK is **asymmetric**:

```sql
CHECK (movement_type != 'IN' OR unit_id IS NOT NULL)
```

Only `IN` movements are *required* to carry a `unit_id`. `OUT` and `OPENING` movements may carry
`unit_id IS NULL` at the schema level. Verified repository facts (G30, correcting Revision 1):

* `StockMovementType::Opening` (`models/inventory.rs`) has **no dedicated production
  constructor**: its only typed references are the enum itself, the reporting parser
  (`application/reporting/fiscal_year_summary.rs:116`), and the balance equation
  (`application/services/stock_movement_service.rs:66`). No production path builds an
  `Opening` value. The generic `insert_raw_stock_movement`
  (`repositories/stock_movements.rs:157`) does persist a **payload-supplied** `movement_type`, so
  a sync import **can** persist an `OPENING` row — but that import forces a concrete `unit_id`
  (next bullets), so it never produces a **NULL-unit** `OPENING` movement.
* The only `OUT` producer (`daily_report_service.rs:141-156`) always writes
  `unit_id = Some(unit_id_str)` and fails closed when the unit is missing
  (`daily_report_service.rs:76-80`).
* The stock-movement sync import forces `Some(unit)`
  (`sync_import_execution_service.rs:411-413`; sole production caller
  `import_stock_movements_package.rs:91`).
* Therefore **Revision 1's claim that "in current production `unit_id IS NULL` rows are `OUT`
  movements" is inaccurate**: no production writer creates a NULL-unit `OUT` or `OPENING`
  movement. NULL-unit rows are reachable only from legacy data or from test-only
  `import_stock_movements(…, None)` invocations.

Because the schema admits a NULL attribution and such rows **do** exist in some databases, the
purge MUST scope A11 explicitly rather than delete the table's rows wholesale:

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

This is the deliberate, bounded answer to the ambiguity recorded as **A1**. The verified
rationale is now precise: the repository cannot prove what a NULL-unit row represents (the
schema permits it; production does not currently create it), so the purge refuses to guess and
preserves it. The cost is stated, not hidden: after a purge, surviving NULL-unit movements
remain while the UNIT-attributed movements they once shared a history with are gone. **No
compensating business calculation is introduced** (D21.4).

### 4.2 Class B — global-preserve (retain verbatim)

| # | Table | Why preserved |
|---|-------|---------------|
| B1 | `settings` | Node singleton (`id INTEGER PRIMARY KEY CHECK (id = 1)`). Carries `node_type`, `unit_name`, `wilaya_code`, `current_year`. Holds the local unit code that scopes ownership — deleting it would destroy the node's own configuration, not a UNIT's data. |
| B2 | `products` | Node-global catalog. `UNIQUE`-keyed by `id`; referenced with `RESTRICT`/`NO ACTION` by A4, A6, A12. Deleting the UNIT releases those references; the catalog survives. |
| B3 | `product_tax_classifications` | `product_id → products(id)` `RESTRICT` (schema line 163). Owned by B2, not by any UNIT. |
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

The **entire §4.6 preflight must complete before the first `DELETE`**. The ordered statements
below are derived from the verified foreign-key graph so that no `RESTRICT` or `NO ACTION` edge
is violated at any intermediate step. `<id>` is the target `units.id` and `<code>` is the target
`units.code`.

```text
Phase 0 — read-only preflight (NO DELETE), inside the same transaction:   §4.6
  P1. resolve the target units row (id, code); fail closed if absent
  P2. resolve the canonical operator (username='user' AND node_id=<code>); require units.user_id
      = resolved id; fail closed on missing / NULL / mismatch (canonical-key duplicate is
      defensive; unreachable under UNIQUE + NOT NULL)                             §5.5
  P3. D14 GATE: SELECT COUNT(*) FROM audit_log WHERE user_id = <resolved operator id>;
      if > 0 -> return Err (abort before any scan and before any DELETE)          §5.1
  P4. aggregate-integrity checks (supplier orders; contracts/allocations;
      supplier_order_item_allocations two-sided ownership)                        §4.7
  P5. cross-UNIT reference checks (inventory_layer_consumptions; any other
      RESTRICT/NO ACTION row owned by a different UNIT)                           §4.10
  P6. surviving-report FIFO-reference check (daily_report_meal_items.fifo_layer_id) §4.9
  P7. if any check fails -> return Err; nothing is deleted

Phase 1 — deletion (child-first), only after Phase 0 passes:
  1. supplier_order_item_allocations   WHERE item_id ∈ target items AND allocation_id ∈ target allocations   (A1)
  2. contract_allocation_exceptions    WHERE allocation_id ∈ target allocations                              (A2)
  3. inventory_layer_consumptions      WHERE unit_id = <id>                                                  (A3)
  4. contract_allocations              WHERE unit_id = <id>                                                  (A4)
  5. contract_products                 WHERE contract_id ∈ target contracts                                  (A5)
  6. supplier_order_items              WHERE unit_id = <id>                                                  (A6)
  7. contracts                         WHERE unit_id = <id>                                                  (A7)
  8. supplier_orders                   WHERE unit_id = <id>  (NULL-unit orders PRESERVED, §4.8)              (A8)
  9. unit_suppliers                    WHERE unit_id = <id>                                                  (A9)
 10. fifo_stock_layers                 WHERE unit_id = <id>                                                  (A10)
 11. stock_movements                   WHERE unit_id = <id>  (NULL-unit rows PRESERVED per D20)              (A11)
 12. daily_report_meal_items           WHERE meal_id ∈ target reports' meals                                 (A12)
 13. daily_report_meals                WHERE daily_report_id ∈ target reports                                (A13)
 14. daily_reports                     WHERE unit_id = <id>  (NULL-unit reports PRESERVED, §4.8)            (A14)
 15. monthly_reports                   WHERE unit_id = <id>                                                  (A15)
 16. unit_monthly_snapshots            WHERE unit_id = <id>  (explicit; FK would cascade)                    (A16)
 17. identity_store                    WHERE subject_type = 'UNIT' AND subject_id = <id>                    (A17)
 18. users                             WHERE id = <resolved operator id>                                      (A18)
 19. units                             WHERE id = <id>                                                        (A19)

Phase 2 — success audit: execute_with_audit writes the DeleteUnit row, then COMMIT (§5).
```

**Ordering rules that are load-bearing (not cosmetic).**

* A1 must precede A4 and A6 because its `allocation_id` FK is `RESTRICT` into A4 and its
  `item_id` FK is `CASCADE` from A6.
* A2 must precede A4 (`RESTRICT`).
* A3 must precede A10 and A11 because its `layer_id`/`movement_id` FKs are `NO ACTION` into
  those tables.
* A4 must precede A5 (`contract_product_id RESTRICT`) and A7 (`contract_id RESTRICT`).
* A5 must precede A7 (A5's parent is A7, `CASCADE`).
* A6 must precede A8 (A6's parent is A8, `CASCADE`).
* A12 → A13 → A14 follow the report hierarchy (`CASCADE` downward), but are performed
  explicitly so the purge is deterministic.
* A16 is explicit even though `units → unit_monthly_snapshots` is `CASCADE`.

**Cascade-safety rule (D3).** Because A6→A8 and A5→A7 and A12→A13→A14 are `CASCADE` edges, the
preflight (P4) MUST prove that deleting each target parent cannot cascade-delete a child whose
ownership is `NULL` or belongs to a different UNIT. If it can, the deletion is **rejected in
Phase 0** — the purge never relies on discovering this from an FK failure after some rows are
already deleted.

### 4.5 Atomicity and fail-closed behaviour

* The whole sequence (Phase 0 and Phase 1, plus the success audit row) runs in **one**
  transaction. `AuditTxService::execute_with_audit` (`audit_tx_service.rs:25-94`) provides
  exactly this: real transaction, closure, rollback on any error (G13). The `delete_unit` IPC
  command already invokes it (`commands/units.rs:88`, G8b). D13's atomicity requirement is
  satisfied by **reusing** it, not by introducing a second transaction facility.
* Any failure at any step — including the Phase-0 gate or any preflight check — rolls back
  **everything**. There is no partial deletion, no tombstone, no "delete what we could"
  behaviour.
* **Accurate failure semantics:** when the closure returns `Err`, the transaction is dropped and
  rolled back and **no `audit_log` row is written at all** — not even a `status = 'Failed'` row
  (G13). This ADR does not claim a failed-audit record is persisted, because no separate durable
  mechanism for it exists. A UNIT whose operator has audit history is therefore refused with the
  UNIT fully intact and the operation retryable.
* The D14 gate is evaluated in Phase 0 **before** any scan and **before** any `DELETE`, so a unit
  whose operator has audit history is never partially purged (D2, D14).
* No tombstone model is introduced (D14). If deletion fails, the UNIT row survives intact.

### 4.6 Initial deletion preflight (D2 — normative, same transaction)

The preflight in Phase 0 of §4.4 is part of the **initial** deletion operation, not a late-stage
check. It is executed inside the transaction opened by `AuditTxService::execute_with_audit`
(§4.5) and consists of the following **ordered** steps:

1. **Enter the transaction** through the existing supported mechanism —
   `AuditTxService::execute_with_audit(db, AuditAction::DeleteUnit, &user_ctx, |tx| …)`. The
   service receives `tx.executor`; it does not open a second transaction.
2. **Minimal target identity.** Load the target `units` row and keep `id` and `code`. If it does
   not exist, fail closed.
3. **Canonical operator resolution** exactly per §5.5 (D1): resolve
   `username = 'user' AND node_id = <code>`, require the resolved row's `id` to equal
   `units.user_id`, and fail closed on missing / `NULL` / mismatch — plus on a canonical-key
   duplicate, which is defensive and unreachable under the current `UNIQUE(username, node_id)` +
   `node_id NOT NULL` schema (D1.5).
4. **D14 audit-reference gate** (D2, §5.1): count `audit_log` rows with
   `user_id = <resolved operator id>`. If the count is non-zero, **abort immediately** — before
   any detailed ownership scan and before any `DELETE`. The gate is not weakened, delayed, or
   moved after the deletes, and historical attribution is not nullified to make deletion
   succeed.
5. **Ownership and integrity scans (read-only).** Only if the gate passes, evaluate:
   the supplier-order aggregate integrity (§4.7), the contract/allocation integrity (§4.7), the
   two-sided `supplier_order_item_allocations` ownership check (§4.7), the cross-UNIT
   `NO ACTION`/`RESTRICT` reference check (§4.10), and the surviving-report FIFO-reference
   check (§4.9). All are read-only.
6. **Any failure aborts.** If any check cannot prove safety, return `Err`; the transaction rolls
   back and nothing is deleted.
7. **Deletion and success audit.** Only after all checks pass, run the Phase-1 statements of
   §4.4, then return `Ok` so that `execute_with_audit` writes the successful `DeleteUnit` audit
   row and commits.

**Why the gate is first (D2).** Placing the gate in Phase 0 makes the permanent-block outcome
(U-1) observable **before** any work is done and guarantees the operation is all-or-nothing with
no late surprise. A design that deleted data and then discovered the D14 block is forbidden.

### 4.7 Aggregate-integrity and indirect-ownership fail-closed rules (D4, D5, D6)

The preflight MUST reject inconsistent parent/child relationships that could cause the target
deletion to remove another UNIT's data, or remove a row whose ownership cannot be proven. Three
aggregates are governed:

**D4 — `supplier_orders` ↔ `supplier_order_items`.** Ownership is independently nullable on both
tables (G25), and `supplier_order_items.order_id → supplier_orders(id)` is `CASCADE`. An item's
ownership is established **directly** by its own `unit_id`; an order's ownership is established by
the order's `unit_id`. The Phase-0 preflight MUST apply exactly one of the following outcomes to
every `supplier_order_items` row whose own `unit_id = <target>` **or** whose parent order's
`unit_id = <target>`:

| Item `unit_id` | Parent order `unit_id` | Outcome |
|----------------|------------------------|---------|
| `<target>` | `<target>` | Item is target-owned; deleted with A6. |
| `NULL`, or another UNIT | `<target>` | **Fail closed.** The order's `CASCADE` edge would remove a child that is not proven target-owned (D3.3). This is the case-independent invariant: for every item whose parent order's `unit_id = <target>`, the item's `unit_id` MUST equal `<target>`. |
| `<target>` | `NULL` or another UNIT | **Fail closed — Q6 resolved (Option A).** The aggregate is inconsistent: a target-owned item hangs off a `NULL`/foreign order. The Phase-0 preflight **aborts** before any `DELETE`; nothing is deleted and the UNIT remains undeletable until the record is repaired (below). |

* An item owned by neither the target nor a target order — e.g. a `NULL` item under a preserved
  `NULL`/other-unit order — is **not involved** and is untouched (D3.1, D3.2). A `NULL`-unit
  **order** is preserved (§4.8).
* **Q6 — target-owned item under a `NULL`/other-unit order (resolved: Option A, fail-closed).**
  A row whose own `unit_id = <target>` but whose parent order is `NULL`-owned or owned by another
  UNIT is the Q6 anomaly. Per the owner-ratified decision (§9.3.3), the Phase-0 preflight **MUST
  ABORT the entire deletion before executing any `DELETE` statement**. The item is **never** deleted
  with the target, and another UNIT's order is **never** deleted to reach it. The conflict MUST be
  detected in the preflight; a transaction rollback *after* a `DELETE` has been issued is **not** a
  substitute for the required preflight rejection. The preflight reports the conflicting
  `supplier_order_items.id` — together with its `order_id` and the two ownership values
  (`supplier_order_items.unit_id` and the parent `supplier_orders.unit_id`) — so an administrator
  can investigate it; the target UNIT remains undeletable until the record is repaired through an
  authorized operational procedure (Q5). This is the cross-UNIT/unprovable relationship of D3.4
  and is treated as a data-integrity anomaly, not as permission to repair or partially delete the
  aggregate.
* All of these checks run in **Phase 0, before any `DELETE`**, so the `CASCADE` edge can never
  remove a row the policy requires preserving. This is the exact cascade-safety rule of §4.4.

**D5 — `contracts` ↔ `contract_allocations`.** Both `unit_id` columns are `NOT NULL`
(G24). The required invariant: for every `contract_allocations` row whose own `unit_id =
<target>` **or** whose parent `contracts.unit_id = <target>`, the allocation's `unit_id` MUST
equal the parent contract's `unit_id`. If they disagree, deletion **fails closed** — it does
not assume either side proves safe ownership. The ordinary authoring path guarantees equality by
construction (G26); the residual risk is the contract-catalog import (a catalog exported from
WILAYA for import by a target UNIT;
`sync_import_execution_service.rs:342,350`), which binds the payload allocation's unit with no
cross-check (G26). Write-time enforcement on the **UNIT** importer — an **explicit rejection**,
never a silent skip — is **owner-approved policy (Q4,
§9.3.3)** but is **not yet implemented**; until it is, this D5 preflight is the only control.

**D6 (A1) — `supplier_order_item_allocations` two-sided ownership.** Ownership must be proven on
**both** parents. A row is safe to delete **iff** its `item_id` resolves to a
`supplier_order_items` row with `unit_id = <target>` **and** its `allocation_id` resolves to a
`contract_allocations` row with `unit_id = <target>`. If the two parents imply different UNITs,
or either relationship cannot establish safe ownership, the deletion **fails closed** rather
than deleting on the strength of one parent alone.

**Write-path validation (not implemented here).** The preflight is the authoritative control.
Two write-path controls are in view: (a) the **UNIT** contract-catalog importer **explicitly
rejecting** (never silently skipping) any accepted row whose `allocation.unit_id` conflicts with
`contract.unit_id` or with the local UNIT scope (G26) is **owner-approved policy
(Q4, §9.3.3)** and is **required**; (b) `OrderService::update_supplier_order` not being able to
produce a `NULL`-header/target-item mismatch (G25) remains a **recommendation**. This ADR
**requires** the preflight; it implements neither write-path control.

**Import and restore paths (assessed, not just ordinary construction).** There is **no
row-level sync/import or restore path** for `supplier_orders`/`supplier_order_items`; backup
restore is a whole-file swap (G25), so a restored database simply reproduces whatever
inconsistency the backup contained, and the preflight then fails closed. The contract-allocation
catalog import **is** a row-level path and is the residual risk named above (G26). The
`contract_fulfillment` import path validates the resolved allocation's unit against the importing
unit (`import_contract_fulfillment_package.rs:183-188`) and never inserts/updates an allocation
row, so it cannot create the D5 divergence.

**Defense-in-depth triggers (optional, NOT added here).** A database trigger that rejects a
`supplier_order_items` insert/update whose `unit_id` differs from its parent order, or a
`contract_allocations` insert/update whose `unit_id` differs from its parent contract, would be
a valid defense-in-depth measure. This revision deliberately **does not add triggers and does
not alter the migration** (§Explicitly Out of Scope); it records the option so a later,
owner-approved schema revision may adopt it. Triggers would only protect **new** writes, not
pre-existing inconsistent rows, so the preflight remains necessary regardless.

**Fresh-database vs upgrade.** D18 permits editing the authoritative initial schema for a
**fresh** database. Per G31, an already-initialized database at schema version 1 **never
re-executes** `001_initial.sql`, so a schema-level guarantee added later (e.g. `NOT NULL` or a
trigger) does **not** reach existing databases through this mechanism. Any such hardening must
therefore be paired with the runtime preflight, or be paired with an explicit database-recreation
step (D18's "application not deployed" posture). This distinction is normative: the ADR MUST NOT
imply that editing the initial schema upgrades an existing database.

### 4.8 Nullable ownership and cascade safety (D3)

A `NULL` `unit_id` **never proves ownership** of the target UNIT. The policy below is per column,
not per table, and `WHERE unit_id = <id>` on a single table is **not** sufficient proof that the
whole operation preserves `NULL`-owned rows.

| Column | Nullability | Policy |
|--------|-------------|--------|
| `stock_movements.unit_id` | nullable, no FK (line 218) | Delete only `unit_id = <id>`. `NULL` rows **preserved** (D20; §4.1 A11). |
| `daily_reports.unit_id` | nullable, no FK (line 275) | Delete only `unit_id = <id>`. `NULL` rows **preserved**. Real production possibility via sync import (G29). |
| `supplier_orders.unit_id` | nullable, FK RESTRICT (lines 332, 335) | Delete only `unit_id = <id>`. `NULL` orders **preserved**. |
| `supplier_order_items.unit_id` | nullable, FK RESTRICT (lines 345, 360) | Delete only `unit_id = <id>`. `NULL` items under a preserved (`NULL`/other) order are **preserved**; a `NULL`/other-unit item under a target order fails closed; a target item under a `NULL`/other-unit order is the Q6 anomaly and **aborts** the deletion (§4.7 D4; Q6 resolved §9.3.3). |

Rules:

* **D3.1** — `NULL` is preserved, never inferred. No predicate may widen a `NULL`-unit set by
  product, notes, `reference_id`, `username`, amount, or time (D20.3).
* **D3.2** — Indirect ownership of a `NULL`-unit parent's children does **not** make those
  children target-owned unless a separate, non-null ownership column proves it. Meals/items of a
  preserved (`NULL`-unit) report are preserved; a `NULL` item under a preserved order is
  preserved.
* **D3.3 — Cascade safety.** Before any `DELETE`, the preflight MUST verify that no `CASCADE`
  edge reachable from a row being deleted can remove a child whose `unit_id` is `NULL` or
  belongs to another UNIT. The reachable `CASCADE` edges are:
  `supplier_orders → supplier_order_items` (order_id),
  `contracts → contract_products` (contract_id; the child has no unit, so ownership is its
  parent's), and `daily_reports → daily_report_meals → daily_report_meal_items`. The
  `supplier_orders → supplier_order_items` edge is the only one with an independent nullable
  ownership column on the child; §4.7 D4 is its guard. `units → unit_monthly_snapshots` and
  `products → unit_monthly_snapshots` cascade only rows already scoped by the target id.
* **D3.4** — A cross-UNIT or unprovable relationship is a **preflight rejection**, never an
  implicit cascade deletion.

### 4.9 FIFO references in surviving historical reports (D8)

`daily_report_meal_items.fifo_layer_id` has **no foreign key** to `fifo_stock_layers` (G27), so
SQLite will not stop a surviving report row from pointing at a FIFO layer that the purge removes.

* **Surviving report rows.** After the purge, the report rows that survive are exactly those
  `daily_report_meals`/`daily_report_meal_items` whose report `daily_reports.unit_id` is `NULL`
  or another UNIT (A14 preserves them, §4.8).
* **Fail-closed policy.** The Phase-0 preflight MUST detect any **surviving**
  `daily_report_meal_items` row (`meal_id` not in the target reports) whose `fifo_layer_id`
  equals an `id` in the target-owned `fifo_stock_layers` set. If any such row exists, the
  deletion **fails closed**. The purge neither leaves a dangling `fifo_layer_id` nor deletes
  another UNIT's report data to remove the reference.
* **Why this is unlikely but must be checked.** The live writer always pairs a report with a
  layer consumed through the unit-scoped FIFO query, and the sync daily-report import always
  writes `NULL` (G27), so a same-unit pairing is the expected production shape. The dead
  `import_monthly_report`/`insert_raw_meal_item` path could in principle have written a
  cross-unit id, and legacy data cannot be disproven, so the check is mandatory.
* **No schema change here.** Adding a foreign key or a unit column is a schema decision that
  touches the frozen initial baseline; the owner has **deferred** it to a separate schema review
  (Q2, §9.3.3). This revision does not add it.
* If the domain's historical-report semantics ever justify retaining such a reference rather
  than failing closed, that is an owner decision, not an implementation choice; it is currently
  **not ratified**, so the default is fail-closed.

### 4.10 Cross-UNIT `RESTRICT` / `NO ACTION` reference safety

Deleting a target-owned parent can be blocked by a **non-target** child that references it, and
silently deleting such a child would remove another UNIT's data. The preflight MUST reject the
operation when any of the following holds. These are `NO ACTION`/`RESTRICT` edges, so absent a
preflight the operation would fail late (after possibly deleting earlier rows in the same
transaction, then rolling back) — the preflight converts that into a deterministic early
rejection.

| Referencing row | Referenced target row | Action | Rule |
|-----------------|-----------------------|--------|------|
| `inventory_layer_consumptions` (any `unit_id`, incl. other UNIT) | target `fifo_stock_layers` (via `layer_id`) | `NO ACTION` | Reject if a non-target consumption references a target layer. |
| `inventory_layer_consumptions` (any `unit_id`) | target `stock_movements` (via `movement_id`) | `NO ACTION` | Reject if a non-target consumption references a target movement. |
| `supplier_order_item_allocations` | target `contract_allocations` (via `allocation_id`) | `RESTRICT` | Reject unless the item parent is also target-owned (§4.7 D6). |
| `contract_allocation_exceptions` | target `contract_allocations` (via `allocation_id`) | `RESTRICT` | Deleted in A2; ownership derived through the allocation. |
| `contract_allocations` (any `unit_id`) | target `contract_products` (via `contract_product_id`) | `RESTRICT` | **Reject** if a non-target allocation (`unit_id ≠ <target>`) references a `contract_products` line owned by a target contract (deleted in A5): A4 deletes only `unit_id = <target>`, so A5 would otherwise break late. Also **reject** any allocation whose `contract_id` differs from the `contract_id` of the referenced `contract_product_id` (structural inconsistency the schema permits; detected here rather than by FK rollback). |
| `contract_allocations` | target `contracts` (via `contract_id`) | `RESTRICT` | A4 runs before A7; the non-target case is rejected by §4.7 D5. |
| `supplier_order_items` | target `supplier_orders` (via `order_id`) | `CASCADE` | Reject a `NULL`/other-unit item under a target order; a target item under a `NULL`/other-unit order is the Q6 anomaly and **aborts** the deletion (Option A; resolved §9.3.3). Guarded by §4.7 D4. |

Because production writers keep these aligned by construction (G26, G28), a violation indicates
pre-existing inconsistency or a non-production/manual write. The safe response is a fail-closed
rejection with a diagnosable error, and an owner-approved repair path if such data is ever
encountered (see §9.3).

## 5. (D14) Audit deletion safety

### 5.1 The rule

```text
The canonical local UNIT operator may be hard-deleted only when
    SELECT COUNT(*) FROM audit_log WHERE user_id = <resolved operator id>
  evaluates to zero.
If it is non-zero, UNIT deletion fails closed and the entire transaction rolls back.
```

Here `<resolved operator id>` is the id produced by the canonical resolution in §5.5 (D1) — the
row matched by `username = 'user'` **and** `node_id = <unit code>`, cross-checked against
`units.user_id`. The gate never accepts any other id, and it is not reached unless resolution
has already succeeded.

**Placement (D2).** The gate is part of the **Phase-0 preflight** (§4.5, §4.6, step P3), inside
the transaction opened by `AuditTxService::execute_with_audit` and **before any ownership scan
and before any `DELETE`**. A non-zero count aborts the transaction with the UNIT fully intact;
it is never discovered after rows have been removed (§4.5's accurate failure semantics: on abort
**no** `audit_log` row — including no `status = 'Failed'` row — is persisted, G13).

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

### 5.5 Canonical operator resolution (D1)

D14 says "the canonical local UNIT operator may be hard-deleted". D1 fixes **which `users` row
that is**, so the gate (§5.1) and the delete (A18) always name the same row. A18 in §4.1 and
preflight step P2/§4.6 step 3 both delegate here.

**D1.1 — Resolution key.** Resolve the operator by the canonical node-scoped key, never by a
bare username and never by `units.user_id` alone:

```text
username  = OPERATOR_USERNAME ("user")   -- constants: unit_service.rs:18
node scope = target units.code            -- the row is created with node_id = code (G20)
```

`users` is `UNIQUE(username, node_id)` (`001_initial.sql:49`), so this key identifies **at most
one** row per scope. Resolution MUST use the **raw** helper
`UserRepository::get_user_by_username_raw(username, node_scope)`
(`repositories/users.rs:72-99`), which omits the `deleted = 0` filter. `get_user_by_username`
(`repositories/users.rs:37-65`) is **active-only** and MUST NOT be used: a soft-deleted
(`deleted = 1`) canonical operator row is still UNIT-owned and must still be deleted with the
UNIT (otherwise the purge would leave a node-scoped operator row whose `node_id` names a unit
that no longer exists).

**D1.2 — Cross-check against `units.user_id`.** After resolution, the resolved row's `id` MUST
equal the target `units.user_id`. On a WILAYA node `UnitService::create_unit`
(`unit_service.rs:88-98` inserts the operator, `:102` stores `user_id` on the unit row) makes
this equality hold **by construction**, so the cross-check is a guard against later drift, not a
new requirement on the happy path.

**D1.3 — Fail-closed conditions (normative).** The preflight returns `Err` — aborting the whole
transaction in Phase 0, before any `DELETE` — when **any** of the following holds:

* **missing** — no `users` row matches the canonical key;
* **`NULL`** — `units.user_id IS NULL` for the target row;
* **duplicate** — the canonical key resolves to more than one row. Under the current schema this
  state is **not reachable** (`UNIQUE(username, node_id)` guarantees at most one row, and
  `node_id` is `NOT NULL`), so the branch is retained as **defensive future-proofing**; if kept
  normative it MUST be discharged by a **counting** query on the canonical key, never by the
  single-row helper (D1.5);
* **mismatch** — the resolved row's `id` differs from `units.user_id`.

There is **no fallback**: the purge does not "delete by `units.user_id` when the key misses", does
not "delete by key when `user_id` is `NULL`", and does not pick one of several candidate rows.
Each such case is a preflight rejection (§4.6 step 6), never a best-effort deletion.

**D1.4 — Why the canonical key, not `units.user_id` alone.** The current path
(`unit_service.rs:148-156`) reads `units.user_id` and deletes by that id (G8, G9, G11). Taken
alone that id proves nothing about *ownership*: `units.user_id` is a plain nullable FK
(`001_initial.sql:57-62`, `ON DELETE SET NULL`) and could be repointed or blanked independently
of the `(username, node_id)` key that actually encodes "this operator belongs to this unit code".
The canonical key is the ownership fact; `units.user_id` is the corroborating constraint. A
disagreement is exactly the inconsistency the preflight must refuse rather than guess through.

**D1.5 — Duplicate detection is defensive, not reachable under the current schema.** A
`(username = 'user', node_id = NULL)` row **cannot exist**: `users.node_id` is
`TEXT NOT NULL DEFAULT 'WILAYA'` (`001_initial.sql:40`), and `UNIQUE(username, node_id)`
(`001_initial.sql:49`) admits at most one canonical row per scope. Because there is no `NULL`
`node_id`, the earlier `COALESCE(node_id, 'WILAYA')` collision cannot arise. The duplicate branch
of D1.3 is therefore **future-proofing** for a schema that drops the `NOT NULL` constraint, not a
state reachable today. It also cannot be discharged by the current lookup: `get_user_by_username_raw`
issues `query_row_optional` (`users.rs:80`), which returns **at most one** row and so cannot
observe two matches even if they existed. If duplicate detection is retained as a normative
requirement, the preflight MUST run its own counting query on the canonical key
(`SELECT COUNT(*) FROM users WHERE username = ?1 AND node_id = ?2`) rather than a single-row fetch.
The `units.user_id` cross-check (D1.2) is unaffected and remains normative.

**D1.6 — Relationship to the D14 gate.** The gate (§5.1) counts
`audit_log WHERE user_id = <resolved operator id>`. The resolved id from D1.1–D1.3 is
therefore the *only* id the gate may use; a mismatch (D1.3) is rejected **before** the gate is
evaluated, so a failed resolution never proceeds to scan or delete.

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

Qualified — **first sentence only**: "Immutable closed years — closed fiscal years receive no new
mutations" does not bar the §6.1 UNIT-owned purge. It continues to bar every other mutation of a
closed year.

The bullet's **second sentence** — "Cache entries for closed years are never invalidated" — is
**not** re-scoped and remains in force in full. `fiscal_export_snapshots` for a closed year are
**not** invalidated, modified, or recomputed because a UNIT is deleted; historical snapshots
retain the values captured at their original issuance time, and their recorded `movement_count` /
`report_count` continue to describe the pre-deletion state (§9.1 C-c). This is consistent with
the freeze-document amendment (Q1, §9.3.3), which is scoped to the first sentence and expressly
does not authorize cache invalidation.

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

### 8.1 Migration mechanism and its compatibility consequence (D18, G31)

The rule above is only correct together with the mechanism that applies it. Verified at HEAD
(G31): `src-tauri/src/db/migrations.rs` defines **exactly one** migration — version `1`, the
consolidated `include_str!("migrations/001_initial.sql")` (`migrations_vec()`, lines 10-18) — and
`run_migrations` (lines 88-169) applies a migration **only when `current_version < version`**
(line 116), then records the version and commits. The initial schema itself uses
`CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS`.

Three consequences are normative:

1. **Editing `001_initial.sql` affects only a fresh database.** A database already recorded at
   schema version 1 never re-executes the initial schema, so a later edit (an added index, a
   tightened constraint, a trigger) does **not** reach it. D18's "modify the authoritative
   initial schema directly" instruction is valid **only** for a fresh or recreated database.
2. **The initial schema is applied exactly once per database.** There is no incremental migration
   path in this repository, and this ADR does not create one.
3. **D18's posture is load-bearing here.** D18 states the application has not been deployed and
   there is no legacy production database to preserve, which is precisely why editing the
   baseline is permissible at all. This ADR MUST NOT imply that editing the initial schema
   upgrades an existing database; where a runtime guarantee is needed for an existing database,
   it is supplied by the §4.6 preflight, not by the schema edit.

This is the same distinction §4.7 ("Fresh-database vs upgrade") applies to the aggregate-integrity
guarantees. D18 remains an owner-ratified decision; this section records its mechanism, not a new
decision.

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
* **Fail-closed refusals are a normal outcome, not an error state.** A UNIT deletion is rejected
  (whole transaction rolled back, nothing deleted, operation retryable) whenever the §4.6
  preflight cannot prove safety: the D14 audit gate (§5.1), a canonical-operator mismatch
  (§5.5), an aggregate inconsistency (§4.7), a cross-UNIT reference (§4.10), or a surviving-report
  FIFO reference (§4.9). None of these leaves partial state behind.

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
* Revision 2 adds no ratification. It refines the mechanics of the already-accepted D13 and D14
  and records every question it could not settle in §9.3 as open, rather than deciding it. In
  particular, whether `ARCHITECTURE_FREEZE.md` §2.4 itself must be amended through the
  RFC-to-ADR process was recorded as **open** (Q1). It is now **owner-resolved (Option A)** and
  that amendment has been performed through the RFC-to-ADR process; this ADR still performs no
  freeze-document edit (§9.3.3).

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
* **Schema-edit compatibility (D18/G31).** No migration is introduced. Any baseline edit this ADR
  later requires reaches **only a fresh or recreated database**; a database already at schema
  version 1 is unaffected (§8.1). The runtime guarantees that must hold on *existing* databases —
  the ownership matrix, the canonical-operator cross-check, the D14 gate, and the §4.7/§4.9/§4.10
  preflight — are therefore supplied by code, not by the schema. There is no upgrade path and
  none is claimed.

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

**Revision 2 forensic corrections (Revision 1's cited evidence vs. current HEAD).** The
following were found while re-deriving every material claim at HEAD
`0513ae5`; they are recorded here so a reader of Revision 1 is not misled. Each is a
documentation contradiction resolved in place, not an owner decision (§9.2).

* **C-e** — Revision 1 asserted the existence of tables `supplier_order_receipts` and
  `inventory_layers` and reasoned about their ownership. Neither table exists anywhere in
  `src-tauri/src` or `src-tauri/tests` (G23). The FIFO table is `fifo_stock_layers`; there is no
  receipts table. Corrected throughout §4 and removed from the matrix.
* **C-f** — Revision 1 stated that "in current production, `stock_movements` rows with
  `unit_id IS NULL` are `OUT` movements", citing the only `OUT` producer. Verified false (G30):
  that producer always writes `Some(unit_id_str)` and fails closed when the unit is missing, no
  production code creates an `OPENING` movement, and the stock-movement import forces
  `Some(unit)`. The **preservation rule is retained** (§4.1 A11, D20) but for the corrected
  reason: the repository cannot prove what such a legacy row represents.
* **C-g** — Revision 1 framed the current `delete_unit` path as non-transactional with no audit
  write. Verified false (G8b): `commands/units.rs:77-94` already wraps the service body in
  `AuditTxService::execute_with_audit(AuditAction::DeleteUnit, …)`. What is missing is the
  ownership matrix, canonical-operator resolution, in-transaction preflight, and the D14 gate —
  not the transaction or the audit action.
* **C-h** — Revision 1 counted "sixteen Class-A tables" and used that figure in §9.2 and the
  Verification Expectations. The correct count is **nineteen Class-A entities** (A1–A19): the
  sixteen dedicated tables plus the row-scoped `identity_store` UNIT rows, the canonical
  operator `users` row, and the `units` root row (§4.1 Class-A count note). The "sixteen"
  language is corrected in both places.

These four corrections change **no** owner-ratified decision (D11–D22). They correct the
evidence and mechanics only, as recorded in the Revision 2 header note.

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

  Also ratified by the same closure: all **nineteen** Class-A UNIT-owned entities (A1–A19)
  remain purgeable regardless of fiscal year, subject to §4.4's FK ordering and D20's explicit
  exception (**D13 + §6.1 + D20**); `FiscalHistoricalGuard` **MUST NOT be modified** (**D21.5**,
  §6.3); and node-global fiscal machinery remains preserved and protected (**D21.1**, §6.2).
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


## 9.3 Decision register (Revision 2)

This register separates three kinds of item so no future reader confuses a verified fact, a
resolved mechanical design choice, and an owner/governance question. **No item here reopens an
owner-ratified decision (D11–D22).** Where repository evidence could not settle a question, it was
recorded with its consequence — never silently decided; the governance/owner questions are now
resolved (§9.3.3).

### 9.3.1 Verified invariants (facts, re-derived at HEAD `0513ae5`)

| Ref | Invariant | Evidence |
|-----|-----------|----------|
| V1 | The `identity_store` UNIT join key is `units.id` (no `unit_uuid`/`unit_code` column). | G2 |
| V2 | `identity_store.deleted` is never set to `1` in production. | G3 |
| V3 | No FK targets `identity_store`; a UNIT subject's own `identity_id` is not stored as a live reference elsewhere, but `applied_sync_packages.issuer_identity_id` may retain a **non-resolving** UNIT issuer id after the purge. | G5, G6, G6b |
| V4 | `supplier_orders`/`supplier_order_items` `unit_id` are independently nullable with a divergence path only via `OrderService::update_supplier_order`. | G25 |
| V5 | `contracts`/`contract_allocations` unit equality holds by construction except the contract-catalog import (a WILAYA-exported catalog imported by a target UNIT), where no allocation/contract unit cross-check exists at write time. | G26 |
| V6 | `daily_report_meal_items.fifo_layer_id` has no FK; a surviving report could dangle after purge. | G27 |
| V7 | Production keeps consumption/movement/layer `unit_id` aligned by construction. | G28 |
| V8 | `daily_reports.unit_id` can legitimately be `NULL` (sync import); `monthly_reports.unit_id` is `NOT NULL`. | G29 |
| V9 | No production writer creates a NULL-unit `OUT` or `OPENING` movement (the sync import can persist a payload `OPENING`, but forces a concrete `unit_id`). | G30 |
| V10 | The migration runner applies `001_initial.sql` exactly once per database (version 1). | G31 |
| V11 | `AuditTxService` persists no `audit_log` row when the closure returns `Err`. | G13 |
| V12 | No unit-deletion tests exist today. | G33 |

### 9.3.2 Resolved design choices (mechanics of D11–D22, decided in this revision)

| Ref | Choice | Where |
|-----|--------|-------|
| R1 | Canonical operator = `username='user'` AND `node_id=<unit code>`, cross-checked against `units.user_id`; resolved via the raw (soft-delete-inclusive) helper; fail closed on missing/`NULL`/mismatch and on a *canonical-key* duplicate (defensive; unreachable under `UNIQUE(username, node_id)` + `node_id NOT NULL`). | §5.5 (D1) |
| R2 | The D14 audit gate is a Phase-0 preflight step, before any scan and before any `DELETE`, inside the existing transaction. | §4.5, §4.6 (D2) |
| R3 | Conservative *preserve* policy for every nullable `unit_id`; ownership never inferred from non-ownership columns. | §4.8, §4.1 A11 (D3/D20) |
| R4 | Supplier-order aggregate: a target order may not have a `NULL`/other-unit item (fail closed); a target-owned item under a `NULL`/other-unit order is the Q6 anomaly and **aborts** the deletion (Q6 resolved — Option A, fail-closed rejection; §9.3.3). | §4.7 (D4) |
| R5 | Contract/allocation: allocation `unit_id` must equal parent contract `unit_id`; violation fails closed. | §4.7 (D5) |
| R6 | `supplier_order_item_allocations` is deleted only when *both* parents are target-owned; else fail closed. | §4.7 (D6) |
| R7 | A surviving report's `fifo_layer_id` pointing at a purged layer fails closed. | §4.9 (D8) |
| R8 | Cross-UNIT `RESTRICT`/`NO ACTION` references fail closed in preflight rather than late — including a non-target `contract_allocations` row referencing a target `contract_products` line (§4.10). | §4.10 (D6/D7) |
| R9 | Class-A count is 19 entities (A1–A19). | §4.1 |
| R10 | D18 migration posture: edit the baseline for fresh DBs only; no migrations; runtime preflight covers existing DBs. | §8.1 (owner D18; Revision-2 item D11) |

### 9.3.3 Owner decisions (Q1–Q6) — resolved

All six governance/owner questions that Revision 2 recorded — Q1–Q6 — are now **owner-resolved**.
Revision 2 listed them with their consequences rather than silently deciding them; each resolved
outcome is recorded below and the normative ADR text has been reconciled to it. No item in this
set remains open, and none of the resolutions reopens an owner-ratified decision (D11–D22). The
default for every deletion sub-case remains safe rejection.

| Ref | Question (as originally recorded) | Owner-approved outcome |
|-----|-----------------------------------|------------------------|
| Q1 | Does the `ARCHITECTURE_FREEZE.md` §2.4 re-scoping require a formal freeze-document amendment (RFC-to-ADR Step 3.4), or is this ADR's qualification sufficient? | **Option A — amend.** The "Immutable closed years" bullet's **first sentence** was amended in `ARCHITECTURE_FREEZE.md` §2.4 through the separate RFC-to-ADR process (`docs/architecture/rfcs/2026-10-16-freeze-2-4-unit-owned-destructive-purge-exception.md`) to record a narrow, **permanent business-policy** sanctioned exception for the §6.1 UNIT-owned purge. This ADR remains the qualification record and performs no freeze-document edit itself. |
| Q2 | Should `daily_report_meal_items.fifo_layer_id` gain a foreign key to `fifo_stock_layers` (and/or a unit column)? | **Option B — defer.** No foreign key or unit column is added by this ADR; the question is deferred to a separate, dedicated schema review. The §4.9 preflight remains the operative protection. |
| Q3 | Should `supplier_orders.unit_id` / `supplier_order_items.unit_id` (and `daily_reports.unit_id`) be tightened to `NOT NULL` (or constrained) in the baseline? | **Option B — defer.** The columns stay nullable in the baseline for now; `NOT NULL` hardening is deferred to a separate schema review (coordinated with Q2). The §4.7/§4.8 preflight remains the guard, and `NULL` is never treated as proof of ownership. |
| Q4 | Should the WILAYA-side contract-catalog import enforce `allocation.unit_id == contract.unit_id` at write time? *(Scope corrected 2026-10-19: "WILAYA-side" named the catalog's author, not the importing node — catalogs are **exported from WILAYA** for import by a **target UNIT**; WILAYA rejects importing a catalog distributed to a UNIT, and the UNIT importer checks the local `target_node_id`. The requirement below applies to the **UNIT** catalog import.)* | **Option A — enforce (policy, not yet implemented).** The **UNIT** catalog import MUST reject a contract-catalog import when an allocation's `unit_id` differs from its parent contract's `unit_id` or conflicts with the local UNIT scope, return an explicit validation error, and fail closed with transaction atomicity (no partial commit, no automatic repair/reassignment/inference). A conflicting allocation MUST NOT be silently skipped — the current per-row `in_scope` skip does not satisfy this requirement *(clarified 2026-10-19)*. **Verified at HEAD `0513ae5` and re-verified at baseline `00a9d0c`: this enforcement does NOT exist.** It remains a separate, independently authorized implementation change and authorizes **no unrelated WILAYA change**. Until it lands, the §4.7 D5 preflight is the only control. |
| Q5 | Is there an owner-approved repair path for a cross-UNIT reference that the preflight rejects? | **Option A — controlled external operational repair (policy only).** The deletion remains rejected, no automatic/speculative repair occurs during preflight or purge; an authorized operator uses a documented, controlled repair procedure **outside** the deletion operation, using trusted evidence (never inferred from the target UNIT or one side alone), with auditability/integrity checks and preflight re-run. **Verified at HEAD `0513ae5`: no repair tool/service/UI exists.** |
| Q6 | Should a `supplier_order_items` row owned by the target under a `NULL`/other-unit parent order abort the deletion? | **Option A — fail-closed rejection** (ratified 2026-10-15). Abort before any `DELETE`; no partial deletion, no automatic repair/reassignment. The UNIT remains undeletable until the inconsistency is repaired and preflight passes. Full rule below and §4.7 D4. |

**Resolved owner decision — Q6 (supplier-order ownership conflict; ratified 2026-10-15).**

**Decision: Option A — Fail-Closed Rejection. Option B is rejected.**

If a `supplier_order_items` row has `unit_id = <target>` while its parent `supplier_orders` row
has `unit_id IS NULL` or `unit_id != <target>`, the UNIT-deletion Phase-0 preflight **MUST ABORT
the entire deletion before executing any `DELETE` statement**. The conflict MUST be detected in
the preflight; transaction rollback *after* a `DELETE` has been issued is **not** a substitute
for the required preflight rejection. Option A binds the §4.7 D4 rule above; **Option B — delete
the target-owned item anyway — is rejected.**

The preflight MUST identify and report the conflicting record sufficiently for an administrator
to investigate it. It does so through the existing delete-rejection error path — the
`DeleteForbidden { resource, reason }` business-logic error (`src-tauri/src/errors/mod.rs`,
surfaced with code `BIZ_DELETE`) — carrying the real columns of the two rows:
`supplier_order_items.id` (the conflicting item) with its `order_id`, plus the two ownership
values `supplier_order_items.unit_id` and `supplier_orders.unit_id`. No new error variant,
identifier, column, or UI element is introduced.

*Rationale.* A supplier order and its items form one business and financial aggregate. A parent
order owned by another UNIT, or with ambiguous (`NULL`) ownership, cannot be presumed to belong
exclusively to the target UNIT; deleting an item under such a parent could mutate another UNIT's
procurement, inventory, accounting, or synchronization records. The WILAYA node owns the shared
database, so deleting one UNIT must not silently alter another UNIT's records. An ownership
mismatch is a data-integrity anomaly, not permission to repair or partially delete the aggregate.

*Remediation.* The target UNIT remains undeletable until the conflicting record has been audited
and repaired through an explicit, authorized operational procedure; once repaired, deletion may
be retried. This ADR defines **no** automatic repair, reassignment, or deletion policy; remediation
is the controlled external operational-repair policy of Q5 above (§9.3.3).

**Governance note (Q1).** `docs/architecture/ARCHITECTURE_FREEZE.md` §4's RFC-to-ADR process
(Step 3.4) owns the freeze-document edit, and the §2.4 "Immutable closed years" first-sentence
amendment was performed through it per the owner's Q1 decision (Option A). This ADR itself
performs no freeze-document edit (`AGENTS.md` §8 C1). The amendment does not weaken the freeze:
it is limited to that bullet's first sentence, and §2.4's second sentence, §2.4's other bullets,
§2.3, §2.5, and the §6.2 protected set remain in force.

# Explicitly Out of Scope

* Any application, schema, migration, or test implementation.
* Credential and initialization lifecycle — `docs/architecture/0063-unit-operator-credential-lifecycle.md`.
* Modifying ADR-0040, ADR-0051, ADR-0052, ADR-0055, ADR-0061, or
  `ARCHITECTURE_FREEZE.md`. This ADR is their qualification record. (The §2.4 amendment made
  under the owner's Q1 decision is performed through the separate RFC-to-ADR process, not by this
  ADR; see §1 and §9.3.3.)
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
* **Adding database triggers, constraints, or new foreign keys** as part of this ADR. §4.7
  records defense-in-depth triggers, and the `fifo_layer_id` FK (Q2) and `NOT NULL` hardening (Q3)
  are now owner-approved **deferrals** to a separate, dedicated schema review; none is added here.
* **Creating any migration** or otherwise changing how `001_initial.sql` is applied (§8.1, D18).
* **Enforcing the allocation/contract unit relationship on the UNIT contract-catalog import
  write path** (a WILAYA-exported catalog imported by a target UNIT; explicit rejection, never a
  silent skip) *in this documentation-only ADR* (§4.7, §9.3.3 Q4). The owner has approved this
  enforcement as **required policy (Option A)**; it is **not yet implemented** and is a separate,
  independently authorized change authorizing **no unrelated WILAYA change**. This ADR requires
  only the runtime preflight today.
* **Amending `ARCHITECTURE_FREEZE.md` §2.4** *through this ADR*. The owner resolved Q1 as
  **Option A** (the amendment is required) and it has been performed through the separate
  RFC-to-ADR process; this ADR performs no freeze-document edit under any reading. See §1 and
  §9.3.3.
* **Building a repair tool, service, or UI for cross-UNIT/reference inconsistencies** that the
  preflight rejects. The owner has approved a **controlled external operational-repair policy**
  (Q5, §9.3.3 Option A), but this ADR builds none and asserts no tool exists. Fail-closed
  rejection is specified; the repair procedure is owned outside this ADR.

# Implementation Boundary

Ordered work items, each independently verifiable. Listed for traceability only; none is
performed by this ADR.

1. **Repository** — add ownership-matrix delete statements (A1–A19) to
   `src-tauri/src/repositories/units.rs` (replacing the two-statement `delete_unit` at lines
   212–225), in the §4.4 order, including the `unit_id = <units.id>` predicate on `stock_movements`
   (D20.1) and **preserving** `unit_id IS NULL` rows (D20.2) and `NULL`-unit `daily_reports`
   (§4.8); add the `identity_store` delete (A17) and the `users` delete (A18); add the following
   **read-only preflight queries** used by the service (all in the repository, no SQL elsewhere):
   * the D14 `audit_log` reference count for the resolved operator id (§5.1, `user_id` only per
     D22.1);
   * the canonical-operator lookup backing §5.5 (reuse `get_user_by_username_raw`,
     `repositories/users.rs:72-99`) plus the target `units` row read for the `units.user_id`
     cross-check;
   * the §4.7 aggregate-integrity checks (supplier-order item ownership; contract/allocation unit
     equality; two-sided `supplier_order_item_allocations` ownership);
   * the §4.10 cross-UNIT reference checks (non-target `inventory_layer_consumptions` referencing
     target `fifo_stock_layers`/`stock_movements`; a non-target `contract_allocations` row
     referencing a target `contract_products` line, including the allocation/contract-product
     `contract_id` match);
   * the §4.9 surviving-report `fifo_layer_id` check;
   * the status-agnostic `identity_store` existence query (§1.1, D19.9) and its port method in
     `domain/identity/ports.rs`.

   The repository exposes these as **facts** (counts / existence / mismatched ids); the domain
   and service layer own the pass/fail decision (`AGENTS.md` §3: SQL confined to `repositories/`,
   business rules in `domain/`/`application/`).
2. **Service** — rewrite `application/services/unit_service.rs::delete_unit` to run the **entire
   Phase-0 preflight followed by the Phase-1 deletes** inside the existing
   `AuditTxService::execute_with_audit` closure with `AuditAction::DeleteUnit`
   (already wired at `commands/units.rs:88`, G8b): resolve the canonical operator (§5.5), apply
   §5's gate before any scan and before any `DELETE`, then run the §4.7/§4.9/§4.10 checks, then
   the ordered deletes. No second transaction facility is introduced.
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
8. **Tests** — add the deletion test suite required by the Verification Expectations, including
   the cross-UNIT, aggregate-integrity, canonical-operator-mismatch, and surviving-report FIFO
   fail-closed cases. No such tests exist today (G33). Tests must not require a database
   migration (§8.1).

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
7. All **nineteen** Class-A entities (A1–A19) are purgeable **regardless of their `fiscal_year`**,
   including `archived`, `closed`, and `open` years — the §6.1 grant is not year-scoped (D13, D15,
   §6.1).
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
    returns 0 after deletion. No FK references `identity_store`, so deletion raises no referential
    error. `identity_store.issuer_identity_id` and `registry_snapshots.wilaya_identity_id` hold
    WILAYA identity ids and remain resolving. A retained
    `applied_sync_packages.issuer_identity_id` that named the destroyed UNIT may become a
    **non-resolving historical-provenance identifier** (G6b); this is expected, is not a
    foreign-key violation, and is retained under D22 (§2).
15. A UNIT code can be recreated immediately after deletion, and the recreated unit is **not**
    EXPORTED (§1, D12).
16. `audit_log` rows referencing the canonical operator cause deletion to fail closed, with the
    entire transaction rolled back: the `units` row, every Class-A row, and the `identity_store`
    rows all survive unchanged, and no tombstone is written (§5, D14).
17. The `audit_log` reference check is observed to execute inside the deletion transaction, as a
    Phase-0 step **before any scan and before any `DELETE`** (assert by inducing a failure after it
    and confirming rollback of the whole operation, §4.5/§4.6, D2).
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

**Revision 2 additions — mechanics of D13/D14 (§4.6–§4.10, §5.5).**

25. **Preflight-first.** A UNIT that the preflight rejects is left **completely untouched**: the
    `units` row, every Class-A row, and every `identity_store` row survive, and no partial delete
    occurred. Assert by forcing each rejection and diffing the database (§4.6).
26. **Canonical-operator resolution (§5.5).** Deletion fails closed, with nothing deleted, when
    any of these holds: no `users` row matches `username='user' AND node_id=<code>`; `units.user_id
    IS NULL`; the canonical key resolves to more than one row (defensive — unreachable under the
    current `UNIQUE(username, node_id)` + `node_id NOT NULL` schema; if implemented, by a counting
    query on the canonical key); or the resolved id differs from `units.user_id`. A soft-deleted
    (`deleted = 1`) canonical operator row is still resolved and deleted with the UNIT.
27. **No reliance on `ON DELETE` nullification.** An operator with audit history is refused; the
    `audit_log.user_id` column is never left nullified by a purge (§5.2, C3).
28. **Aggregate integrity fails closed (§4.7).** Deletion is rejected in Phase 0, before any
    `DELETE`, when a target `supplier_orders` row has a `NULL`/other-unit item, when a target
    `supplier_order_items` row hangs off a `NULL`/other-unit order (the Q6 anomaly — Q6 resolved:
    Option A, fail-closed rejection, §9.3.3), or when a `contract_allocations.unit_id` disagrees
    with its parent `contracts.unit_id`.
29. **Two-sided allocation ownership (§4.7 D6).** A `supplier_order_item_allocations` row whose
    item parent and allocation parent do not both resolve to the target UNIT causes fail-closed
    rejection.
30. **Cross-UNIT references fail closed (§4.10).** A non-target `inventory_layer_consumptions`
    row referencing a target-owned `fifo_stock_layers` or `stock_movements` row, or a non-target
    `contract_allocations` row referencing a target-owned `contract_products` line, causes
    fail-closed rejection before any delete.
31. **Surviving-report FIFO reference fails closed (§4.9).** A surviving
    `daily_report_meal_items` row (`meal_id` not in the target reports) whose `fifo_layer_id`
    equals a target-owned `fifo_stock_layers.id` causes fail-closed rejection; no dangling
    `fifo_layer_id` is produced.
32. **Migration compatibility (§8.1).** No migration is introduced; an already-initialized
    database at schema version 1 is not re-provisioned, and the runtime guarantees hold without a
    schema change.
