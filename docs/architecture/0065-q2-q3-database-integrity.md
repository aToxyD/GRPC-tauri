# ADR 0065: Q2/Q3 Database Integrity — UNIT Ownership Hardening, FIFO Layer References, and Foreign-Key Enforcement

# Status

Accepted (2026-10-23)

> **Governance state.** This ADR is **Accepted** (owner acceptance on **2026-10-23** by the
> **Project Owner `1Bxm`**; acceptance record: `docs/architecture/governance_review.md` §7.5). It
> records the owner-provided Q2/Q3 Database Integrity requirements and the decisions the owner
> communicated for them.
> Status changes follow the repository convention (`docs/architecture/ADR_INDEX.md`, Policy §3:
> Draft → Proposed → Accepted → Deprecated → Superseded) and must be recorded through the
> ordinary governance process, not by this document alone.
>
> **Ratification does not authorize implementation.** Ratifying this ADR — by whichever route the
> governance process takes — records the **decision** only. It does **not** by itself authorize any
> SQL, schema, migration, source-code, or test change. Every step listed in
> §"Proposed implementation boundary" requires a **separate, explicit owner authorization**, granted
> after the applicable governance gate is satisfied (including `ARCHITECTURE_FREEZE.md` §4 Steps 2–4).
> No implementation is performed, authorized, or begun by this document.
>
> **Decision record — a single primary location.** The **primary** record of the decision for this
> ADR is **this document** (ADR-0065), cross-linked from `docs/architecture/ADR_INDEX.md` (row
> `0065`) and from the accompanying RFC
> (`docs/architecture/rfcs/2026-10-21-q2-q3-database-integrity.md`, which was **approved by the Project Owner (`1Bxm`) on 2026-10-22** (recorded in
> `docs/architecture/governance_review.md` §7.1) and is not a parallel decision record). When an actual review exists, the review record
> is `docs/architecture/governance_review.md`. The registry
> `docs/governance/frontend/GOVERNANCE_APPROVALS.md` is **frontend-layer scoped** and is **not**
> the ratification record for this backend ADR; no entry is created there by this document, and the
> repository's policy does not require one for this change set (it alters no IPC contract, no
> projection ownership, no scope owner, and no import drawing).
>
> **This is a documentation-only proposal.** It introduces **no** schema edit, **no** migration,
> **no** Rust/TypeScript/Svelte source change, **no** test change, and **no** database file
> change. Every schema decision recorded below is a **plan** to be executed in a later,
> separately authorized implementation phase.
>
> **Relationship to ADR-0064.** `docs/architecture/0064-unit-export-state-and-owned-data-deletion.md`
> recorded **Q2** (`daily_report_meal_items.fifo_layer_id` FK) and **Q3** (`NOT NULL` hardening of
> `supplier_orders.unit_id` / `supplier_order_items.unit_id` / `daily_reports.unit_id`) as
> **Option B — deferred** to a separate, dedicated schema review (§9.3.3 there). **This ADR is that
> separate schema review.** It supersedes the *deferral* outcomes Q2/Q3 and the NULL-ownership
> wording they rested on (§4.4's NULL-preservation annotations, §4.7's nullability premise and its
> `NULL` outcome rows, G25's "each nullable" clause, §4.8, §4.9, the Q2/Q3 rows of §9.3.3, and
> the related *Explicitly Out of Scope* bullets there), while **preserving** ADR-0064's
> owner-ratified decision set D11–D22 and its fail-closed deletion preflight (§4.6–§4.10). It does
> **not** reopen, narrow, or weaken any ratified decision of ADR-0064; D20 (NULL-unit stock
> movements) in particular is preserved unchanged. ADR-0064 is reconciled in place to point at
> this ADR and to stop contradicting it. That reconciliation is **prospective**: it distinguishes a
> **fresh** database — where the `NULL` sub-cases become unreachable — from an
> **already-initialized** database, where ADR-0064 §4.6–§4.10 remains the control. **No `NULL`
> outcome row, no Q6 clause, and no G25 statement is deleted**, and no ADR-0064 rule is reopened,
> narrowed, or weakened.

# Date

2026-10-21 (proposal/creation date)

> **Amendment log.** 2026-10-22 — **documentation-only** amendment recording the completed
> read-only **reachability** investigation for the **unreferenced**
> `SyncImportExecutionService::import_monthly_report` path (N9), the alignment of the
> *Import-path note* wording with those findings, the owner's decision on removal (G9), the
> removal-scope narrowing, the supersession conditions and the implementation obligations in
> §Governance-6(a)–(c), and the owner decisions on the exception-registry retirement
> (**close-and-retain**, §Governance-6(c)), the architecture-freeze applicability (**narrow
> interpretation adopted**, §Governance-6(c)), ADR-0064 (**no additional change required in this
> phase**, removal-boundary item 4), and the project-specific exception to the three mandatory
> reviewer-role perspectives for the Q2/Q3 RFC (**G10**). The **historical-occurrence** question —
> whether the path ever wrote a cross-UNIT or `NULL`-unit row — **remains unresolved**, and the §4.9
> preflight is unaffected. These are governance decisions of record for this ADR, **not** formal review
> approvals. **No approval, approving authority, signature, or approval date is recorded or implied
> by this amendment**, and the ADR's status remains `Proposed`. No source, test, schema, migration,
> restore, registry, or architecture-rule change is made or authorized by it.
>
> **Amendment log.** 2026-10-23 — **documentation-only closure-of-record amendment** (a
> documentation act only; it records, it performs nothing). It records that the **implementation
> phase** of this ADR — **separately and explicitly owner-authorized** after the acceptance
> recorded in `docs/architecture/governance_review.md` §7.5 — was **executed as one commit**:
> `6566b87b89c8b7367e7d8329a928f462917d0cc1` (branch `feat/q2-q3-database-integrity`; parent
> `0b93b023ad413517e9e101aad3f9e061c82f9217`; 13 files, +944/−89). Executed software scope:
> §"Proposed implementation boundary" **items 1–5** (the fresh-database baseline schema edit,
> repository adjustments, domain/application fail-closed validation, the reordered deletion
> sequence, and the restore `PRAGMA foreign_key_check` mechanism). Boundary item 6 is **not newly
> executed by this amendment**: its reachability investigation limb was completed pre-ratification
> and is recorded in §Context and §Governance-6(a), while its **removal limb (G9) remains open**
> and gated by §Governance-6(b)/(c). Boundary item 7 (documentation) is the act performed by this
> amendment and its accompanying records. **Not executed and still explicitly open:** G9 — the P3
> removal (`SyncImportExecutionService::import_monthly_report` /
> `ReportRepository::insert_or_replace_raw_daily_report`, preserved per the "Preservation
> requirements" there; whether the path ever wrote a cross-UNIT or `NULL`-unit row remains
> unresolved) — and the deferred `monthly_reports.unit_id` foreign key (Governance-10), which
> remains `NOT NULL`, FK-less, and requires a dedicated follow-up ADR. Test and governance gates
> **were passed during the authorized implementation phase** (recorded in `governance_review.md`
> §7.6); **no test is re-run and no new test evidence is created by this documentation
> amendment.** The proposal-time statements above are **preserved verbatim** as accurate for
> their recorded moments.
>
> **Anchor-shift note (part of this amendment).** The five insertions of this amendment add lines
> after former lines 73, 463, 711, 735, and 771 of this file, so every line anchor below each
> insertion point has shifted: +23 after the amendment log, +28 from the former line 464 onward,
> +34 from the former line 712 onward, +38 from the former line 736 onward, and +50 from the
> former line 772 onward. Line-number references into this ADR recorded before this amendment —
> e.g., in `docs/architecture/governance_review.md` (`0065:79-87`, `0065:348-367`,
> `0065:602-615`, `0065:623-626`, `0065:629-639`, `0065:652-657`, `0065:664`, `0065:690-694`,
> `0065:763-769`) — were exact when recorded (`0065:11-27` precedes this amendment log and is
> unaffected); resolve the shifted ones by their section or row names at their current locations.
> This note repairs nothing; it only explains the shift.

# Owner

Architecture / Data Governance

> **Approval convention.** The owner of record for this proposal is the Architecture / Data
> Governance function. On **2026-10-23** the **Project Owner (`1Bxm`)** accepted this ADR; the
> approving authority and the acceptance date were recorded through the repository's established
> process — ADR_INDEX row `0065` and `docs/architecture/governance_review.md` §7.5. The
> **primary** decision record is this ADR, cross-linked from `docs/architecture/ADR_INDEX.md` row
> `0065`, from the accompanying RFC (**approved on 2026-10-22** — `governance_review.md` §7.1), and from
> `docs/architecture/governance_review.md` when an actual review exists. The registry
> `docs/governance/frontend/GOVERNANCE_APPROVALS.md` is **frontend-layer scoped** and is **not**
> the ratification record for this backend ADR; no entry is created there by this document.

# Context

ADR-0064 established the UNIT-export lifecycle and the UNIT-owned destructive purge, with a
fail-closed Phase-0 preflight as the operative control. As part of that work it identified three
schema-level integrity gaps and explicitly deferred them:

| Ref (ADR-0064) | Gap | ADR-0064 outcome |
|----------------|-----|------------------|
| **Q2** | `daily_report_meal_items.fifo_layer_id` has **no foreign key** to `fifo_stock_layers`; a surviving report row can dangle after a purge. | **Option B — defer** to a separate schema review. |
| **Q3** | `daily_reports.unit_id`, `supplier_orders.unit_id`, and `supplier_order_items.unit_id` are **nullable**, so a report or order can exist without a provable UNIT owner. | **Option B — defer** `NOT NULL` hardening to a separate schema review. |
| Monthly | `monthly_reports.unit_id` — FK to `units(id)` absent. | Not raised as a decision; the column is already `NOT NULL`. |

The owner has now communicated the Q2/Q3 design direction. This ADR records it. The requirements
below are **owner-provided requirements**; their formal acceptance is a separate governance step.

**Verified repository facts at proposal time.** Base commit
`6f5a05c56494937e81fbe33d4138f11bf11131fc`; at the recorded review point that commit was equal to
both `main` and `origin/main`, and the review was performed on branch
`feat/q2-q3-database-integrity` (whose HEAD was that same commit at the recorded review point):

* `src-tauri/src/db/migrations/001_initial.sql` is the repository's **only** migration; the runner
  applies it exactly once per database (version 1) and never re-executes it (ADR-0064 §8.1).
* `daily_report_meal_items.fifo_layer_id` is `TEXT`, **nullable**, with **no** FK.
* `daily_reports.unit_id` is **nullable at the schema level**, with **no** FK. **Schema capability
  and live-path behavior are distinct:** the audited live daily-report service
  (`application/services/daily_report_service.rs`) and the live sync package-import path
  (`application/usecases/sync/import_daily_report_package.rs`) both **require** a UNIT and write a
  **non-NULL** `unit_id`, failing closed on a missing/blank value. ADR-0064's G29 recorded a
  `NULL`-unit report as a *"real production possibility via sync import"*; at this ADR's recorded
  review point that characterization is **not demonstrable from the live paths** — the only
  audited writer that can thread an optional (`NULL`) `unit_id` through is the **unreferenced**
  `SyncImportExecutionService::import_monthly_report` method (see the import-path note below),
  which is **not** evidence of a currently reachable production write path. This ADR does not
  claim there are no other possible writers; it records what the audit established. The schema
  still **permits** `NULL` (the G29 schema fact stands); what is superseded prospectively is the
  assertion of live production reachability. Q3-A is unaffected: `daily_reports.unit_id` is to
  become `NOT NULL` and missing/invalid ownership must fail closed.
* `supplier_orders.unit_id` and `supplier_order_items.unit_id` are **nullable**, each with an
  existing FK to `units(id)` `ON DELETE RESTRICT`.
* `monthly_reports.unit_id` is already `NOT NULL` and has **no** FK.
* `stock_movements.unit_id` is **nullable**, with the asymmetric CHECK
  `movement_type != 'IN' OR unit_id IS NOT NULL` (ADR-0064 §4.1 A11 / D20).
* The `daily_reports` uniqueness index uses a `NULL`-sentinel expression —
  `COALESCE(unit_id, '__NULL__')` (ADR-0064 §4.8 fact set; see the schema section below).
* `PRAGMA foreign_keys` is a **per-connection** setting; it is not persisted in the database file.
* The restore path performs a whole-file atomic swap and does **not** currently run
  `PRAGMA foreign_key_check` anywhere.

**Import-path note — two distinct methods share the name `import_monthly_report`.** ADR-0064 §4.9
recorded a *"dead `import_monthly_report`/`insert_raw_meal_item` path."* The audit distinguishes two
same-named methods and corrects that label:

1. **Unreferenced:** `SyncImportExecutionService::import_monthly_report`
   (`application/services/sync_import_execution_service.rs`). The audit found **no callers** of this
   method. It is the method that iterates meal items and calls
   `insert_raw_meal_item`, so it is in principle able to write a meal-item row; whether it ever
   wrote a cross-UNIT or `NULL`-unit `fifo_layer_id`/report is **not established** and remains an
   open, evidence-required question. It is **not** proven safe to delete.
2. **Live:** `MonthlyReportService::import_monthly_report`
   (`application/services/monthly_report_service.rs`), reached through the registered IPC command
   `import_monthly_summary_package` (`commands/import_export.rs` → `commands/registry.rs` →
   `application/usecases/sync/import_monthly_summary_package.rs`). This path writes a **monthly
   summary** row only; it does **not** write meal items and does **not** write `fifo_layer_id`.

Consequently, this ADR **does not** characterize both methods, or every similarly named path, as
dead. The source-level **reachability** investigation of the unreferenced method has been
**completed** and found **no in-repository callers** (recorded below). Its source-level behavior and
ownership **capability** are documented there separately, while **historical execution** and any
resulting effects on existing data remain **unresolved** — the absence of in-repository callers is
**not** evidence that the method never ran. The ADR-0064 §4.9 safeguards for existing data
therefore remain **in force** (see §Governance-6 and §Contradictions-2).

**Unreferenced-path reachability investigation (read-only) — completed.** The read-only
**reachability** limb of the §Governance-6 investigation has been **completed** (2026-10-22),
against baseline branch `feat/q2-q3-database-integrity` at commit
`712d4b332702ee7b3ac3bc715c03a121bed32850` with a clean working tree. Its findings, and the
**evidential status** of each, are:

1. **Repository-search finding — reachability: no in-repository caller.** A repository-wide search
   of `src-tauri/src` and `src-tauri/tests` found **no caller** of
   `SyncImportExecutionService::import_monthly_report`. The only `.import_monthly_report(...)`
   call site in the tree is `MonthlyReportService::import_monthly_report`
   (`application/usecases/sync/import_monthly_summary_package.rs:129`) — a **different** method on
   a **different** type, writing a monthly summary row rather than daily-report rows, meals, or
   meal items. The `Service` marker trait (`src-tauri/src/architecture.rs:19`) is empty, so it
   provides no dynamic-dispatch path to this method. **Scope of this finding:** it establishes the
   absence of an in-repository caller **within the searched tree**; it is **not** a proof of
   unreachability beyond that scope.
2. **Repository-search finding — exclusive helper.** `ReportRepository::insert_or_replace_raw_daily_report`
   (`repositories/reports.rs:547`) has exactly **one** call site in the repository: P3 itself
   (`application/services/sync_import_execution_service.rs:154`).
3. **Repository-search finding — preserved helper.** `insert_raw_meal_item` is used by **14**
   integration-test call sites in `src-tauri/tests/db_integration_tests.rs` and therefore
   **remains**.
4. **Source-level finding — behavior and ownership capability.** From the source, P3 persists
   `report.report.unit_id` verbatim (nullable, with no FK) and `item.fifo_layer_id` verbatim, so it
   is **capable** of writing a `NULL`-unit report or a cross-UNIT layer reference. This is a
   statement of **capability** derived from the code; it is **not** a statement about what was
   actually written (see "Historical occurrence" below).
5. **Owner-provided fact — external consumers.** The **owner** has confirmed that there are **no
   external consumers** of these project functions. This is an **owner statement**, not a
   repository-search finding: the repository search above establishes only the **in-repository**
   absence of callers. The crate is published as `grpc_lib` with
   `crate-type = ["staticlib", "cdylib", "rlib"]` (`src-tauri/Cargo.toml`), so the absence of
   external consumers is **not** independently derivable from repository inspection.

**Historical occurrence — still unresolved.** Whether the unreferenced path ever actually wrote a
cross-UNIT or `NULL`-unit row is **not established** by the current repository and is **not**
resolved by the reachability finding. The *Import-path note* above therefore remains accurate as
written: the path is **not** proven safe to delete. The reachability finding establishes only that
the path is unreferenced **in the current tree**. It establishes **nothing** about historical data,
it does **not** retroactively change what P3 was capable of writing, and the §4.9 fail-closed
preflight and its historical rationale remain **in force** for data that may already exist.

# Owner-provided requirements (design direction)

These are the requirements the owner communicated. They are normative for the eventual
implementation, and their ADR status is **Accepted** — owner acceptance on 2026-10-23, recorded in
`docs/architecture/governance_review.md` §7.5.

## Q2-A — FIFO layer reference (`daily_report_meal_items.fifo_layer_id`)

1. **Keep `fifo_layer_id` nullable.** The sync daily-report import writes `NULL` there, and the
   nullable form is part of the sync contract; the column stays nullable.
2. **Add a foreign key** `daily_report_meal_items.fifo_layer_id → fifo_stock_layers(id)`
   with **`ON DELETE RESTRICT`**.
3. **Service-level validation** on application write paths: when `fifo_layer_id` is **non-NULL**,
   validate that the referenced FIFO layer's ownership (UNIT) matches the report's UNIT and that
   the layer's product matches the expected product for the meal item. This is a
   domain/application responsibility (backend is the source of truth), not a frontend one.
4. **Do not introduce composite ownership keys or extra ownership columns** in this phase. The
   `daily_report_meal_items` table gains no `unit_id` and no composite FK.

## Q2-B — UNIT deletion ordering (preserves ADR-0064 D13/D15)

1. **Preserve ADR-0064 §4.9's fail-closed deletion preflight** in full. This ADR does not relax it.
2. Where necessary, **delete dependent report items, meals, and reports before** deleting the
   target UNIT's `fifo_stock_layers`, so the new `ON DELETE RESTRICT` edge is satisfied by an
   explicitly ordered, deterministic deletion sequence rather than by a late FK failure. The
   concrete sequence is fixed in §Governance-8(a); the `ARCHITECTURE_FREEZE.md` §2.4 scope
   determination is recorded in §"ARCHITECTURE_FREEZE.md §2.4 — scope determination".
3. **Preserve transaction atomicity** (single deletion transaction, all-or-nothing) and the
   **protection for FIFO layers referenced by surviving reports from other UNITs**: a FIFO layer
   still referenced by a surviving report must **never** be deleted. This is the `ON DELETE
   RESTRICT` edge reinforcing the §4.9 preflight.

## ARCHITECTURE_FREEZE.md §2.4 — scope determination (Option B: qualify, do not amend)

**Decision recorded with this proposal.** `docs/architecture/ARCHITECTURE_FREEZE.md` is **not**
amended by this ADR. The change proposed here stays **within the existing sanctioned exception
and the existing authorities**: the ADR-0064 §4.4 / §4.6–§4.10 deletion transaction is
re-sequenced (an implementation-level re-ordering of steps inside an existing, ratified
transaction), and the schema premise recorded in ADR-0064 §4.7/G25 is qualified prospectively (a
description of the schema, not a rule).

**What this determination does *not* claim.** It does **not** claim that incorporating a clause
"by reference" immunizes a frozen clause against change. `ARCHITECTURE_FREEZE.md` §2.4 remains in
force **as written**; what follows is a specific verification of **this** ADR's interaction with
it, and it creates no general exemption. If a later change altered what §2.4 incorporates by
reference — for example by changing the **rule** of the fail-closed preflight or of the
ownership-integrity rules rather than their sequence — §2.4 would have to be re-examined under
`ARCHITECTURE_FREEZE.md` §4 Step 3.4, and the amend-the-freeze route would apply.

**The four verification points for the §4.4 re-ordering:**

| # | What must not change | Verification |
|---|-----------------------|--------------|
| 1 | The **protected data set** (`ARCHITECTURE_FREEZE.md` §6.2) | The re-ordering changes only *which* `DELETE` runs first. It adds, removes, and re-scopes **no** table, no column, and no row; the §6.2 protected set is never re-derived or re-scoped, and the preflight's cross-UNIT rejection conditions are unchanged. |
| 2 | The **ownership rules** (ADR-0064 D3–D6, Q6, §4.7) | The re-ordering is a sequence change only. Every predicate (`WHERE unit_id = <id>`, `WHERE meal_id ∈ target reports' meals`, `WHERE daily_report_id ∈ target reports`) and every ownership determination is unchanged, and no ownership is inferred or guessed. |
| 3 | **Atomicity** (ADR-0064 §4.5, D13) | The re-ordering stays inside the **same single transaction** as Phase 0 and Phase 1. It introduces no second transaction, no commit point, and no partial-delete window; any failure at any step still rolls everything back. |
| 4 | The **fail-closed controls** (ADR-0064 §4.6–§4.10, D3.3) | The Phase-0 preflight still completes **before the first `DELETE`**; the §4.9 surviving-report FIFO preflight still rejects the deletion when a surviving report references a target-owned FIFO layer; and the new `ON DELETE RESTRICT` edge is satisfied by an explicit deterministic order rather than by discovering the violation late. |

**Consequence of the verification.** With points 1–4 verified, `ARCHITECTURE_FREEZE.md` §2.4
requires **no** text amendment and §4 Step 3.4 is not triggered for it. §2.4's sanctioned exception
continues to govern the UNIT-owned destructive purge exactly as written, and §2.6's "No auto-repair"
continues to govern the `foreign_key_check` requirement. `ARCHITECTURE_FREEZE.md` remains
**unmodified** by this ADR — including by this section.

## Q3-A — Daily reports (`daily_reports.unit_id`)

1. **Every daily report must belong to one specific UNIT.**
2. Set `daily_reports.unit_id` to **`NOT NULL`**.
3. Add a **foreign key** `daily_reports.unit_id → units(id)` with **`ON DELETE RESTRICT`**.
4. **Reject missing, invalid, or mismatched UNIT ownership in all write/import paths** —
   fail-closed, with an explicit validation error, never a silent skip or a fallback.
5. **Do not infer ownership from dates or unrelated records.** Ownership must come from an
   explicit UNIT attribution provided by the caller/import, validated against the authoritative
   UNIT set.
6. **Explicitly amend the older policy that permitted NULL-unit reports.** The amended loci are:
   ADR-0064 §4.8's `daily_reports.unit_id` row (the per-column nullable-ownership table), §4.8's
   Class-A row `A14` (the `NULL`-preservation clause), §4.4 step 14's `(NULL-unit reports PRESERVED,
   §4.8)` annotation, §4.9's "surviving report rows" description, and — because Q3-B applies the
   same nullability change to the supplier-order tables — §4.4 step 8's `(NULL-unit orders
   PRESERVED, §4.8)` annotation, §4.8's Class-A row `A8`, §4.7's D4 nullability premise with its
   `NULL` outcome rows and the Q6 `NULL` sub-case, §4.7's *Write-path validation* recommendation,
   and G25's "each nullable" clause. Every such amendment is **prospective**: on a **fresh**
   database the `NULL` sub-cases are unreachable, while the D4/Q6 rules and the fail-closed
   preflight are **unchanged** and remain the control for **already-initialized** databases. After
   this change, a `NULL`-unit daily report is no longer an accepted production state. The locus
   list here is the **same set** as the one recorded in §Governance-2 and in the
   "Documents amended or qualified" table.

## Q3-B — Supplier orders (`supplier_orders`, `supplier_order_items`)

1. Set `supplier_orders.unit_id` to **`NOT NULL`**.
2. Set `supplier_order_items.unit_id` to **`NOT NULL`**.
3. **Enforce that each order item's UNIT matches its parent order's UNIT at the
   application/service layer.** The existing `ON DELETE RESTRICT` FK to `units(id)` is retained.
4. **Do not rely on a caller-UNIT fallback that can create a mismatch.** A missing or ambiguous
   UNIT must fail closed rather than be filled in from an ambient caller context.
5. **Scope note (cross-reference, not a new decision).** ADR-0064 §4.7's *Write-path validation*
   bullet (b) records the item/parent-order UNIT-equality control as a **recommendation**
   ("explicitly out of scope for the D19.7 task"); Q3-B-3 states it as a **requirement**. The same
   qualification reaches ADR-0064 §4.7's D4 nullability premise and its `NULL` outcome rows, §4.7's
   Q6 `NULL` sub-case, and G25's "each nullable" clause. The exact reconciliation wording for those
   loci is scoped in §Governance-2 and drafted in
   `docs/architecture/rfcs/2026-10-21-q2-q3-database-integrity.md` §9.3–§9.5; it is applied only
   with owner delegation (RFC §3.10 items N1 and N6) and **before** this ADR is ratified, and the
   fresh-database / already-initialized-database distinction applies as recorded in §Consequences.

## Q3-C — Stock movements (`stock_movements.unit_id`) — preserved, not globally tightened

1. **Do not globally make `stock_movements.unit_id` `NOT NULL`.**
2. **Preserve D20's type-specific NULL semantics**, including the schema CHECK
   `movement_type != 'IN' OR unit_id IS NOT NULL` — i.e. `IN` movements require a UNIT, while
   `OUT`/`OPENING` may be `NULL` at the schema level. `NULL`-unit rows are preserved and ownership
   is never inferred for them (ADR-0064 D20.1–D20.3).

## Monthly reports (`monthly_reports.unit_id`)

1. **Defer** adding a foreign key to `monthly_reports.unit_id` as a **separate scope**; it is not
   included in this ADR's implementation plan.
2. **Preserve its existing `NOT NULL` rule.** This ADR does not change the column.
3. **Follow-up ownership of the deferred foreign key.** The deferred foreign key on
   `monthly_reports.unit_id` is **not** part of this ADR's implementation plan and **does not
   exist today** — the column is `NOT NULL` and carries **no** foreign key to `units(id)` in the
   current baseline. The follow-up owner is the **Project Owner**, until a specific implementer is
   designated. **A dedicated follow-up ADR must be created before that change is executed**; no
   ADR number is assigned here and none is invented, and no tracking-issue identifier is created
   for it. This deferral must not be read as implying that the foreign key exists or is scheduled
   by this ADR.

## SQLite foreign-key enforcement

1. **Verify foreign-key enforcement is enabled on every relevant write connection path** used for
   application writes.
2. **Fail safely if enforcement cannot be established** — a connection that cannot guarantee
   `PRAGMA foreign_keys = ON` must not be used for writes that depend on FK enforcement.
3. **After a restore/replacement**, run **`PRAGMA foreign_key_check`**, detect and report
   violations, and **never auto-repair** data (consistent with `ARCHITECTURE_FREEZE.md` §2.6 "No
   auto-repair").
4. **Inspect the restore atomic-swap semantics before documenting whether violations must abort
   finalization or invalidate the restored database.** The restore adapter performs a whole-file
   atomic swap; the exact fail-closed placement of `foreign_key_check` relative to finalization is
   to be determined from the implementation in the later phase and recorded then. This ADR records
   the requirement; it does not yet fix the mechanism.
5. **Do not expand this requirement to all ordinary sync imports** without a concrete, documented
   correctness justification. Ordinary imports apply rows inside a transaction; a blanket
   post-import `foreign_key_check` is **not** mandated here.

## Daily-report uniqueness (follow-on to Q3-A)

1. After `daily_reports.unit_id` becomes `NOT NULL`, **replace** the current `NULL`-sentinel
   uniqueness expression — `COALESCE(unit_id, '__NULL__')` — with a **conventional unique index on
   `(date, unit_id)`** as part of the **fresh-database baseline schema edit**, after verifying the
   existing schema and query behavior. The replacement is the **planned scope** of that schema
   edit, not a discretionary reassessment; the verification step is a **pre-implementation
   condition**, not an open option.
2. The exact intended constraint is a unique index over the pair `(date, unit_id)`, expressing
   "at most one daily report per (date, UNIT)". Because `unit_id` is `NOT NULL` after Q3-A, the
   `COALESCE` sentinel is no longer needed to distinguish multiple `NULL`-unit rows, and the plain
   pair index is the intended replacement.
3. **Equivalence argument — limited to fresh databases.** When `unit_id` is `NOT NULL` it can never
   be `NULL`, so `COALESCE(unit_id, '__NULL__')` evaluates to `unit_id` in every row: the two
   indexes coincide in meaning, and the plain `(date, unit_id)` index is the correct expression of
   the constraint.
4. **Limit of that equivalence.** This argument concerns a database created **after** the baseline
   edit. It **does not extend to upgrading an existing database**: already-initialized databases
   are not upgraded (see Governance-3 and §Consequences), and any `COALESCE` index already present
   in them stays as it is.

## Unreferenced import path (N9) — owner decision: removal, not repair

The owner has decided that the **unreferenced** `SyncImportExecutionService::import_monthly_report`
path (P3) is to be **removed** in a later, separately authorized implementation phase rather than
repaired. The reachability investigation that this decision rests on is recorded in §Context
("Unreferenced-path reachability investigation (read-only) — completed").

**Basis of the decision, and its limits.** The decision rests on the reachability findings recorded
in §Context together with the owner's confirmation that there are no external consumers. It is a
decision to stop maintaining an obsolete, unreferenced path rather than to repair it. It is **not**
a finding that no problematic historical data exists: whether the path ever wrote a cross-UNIT or
`NULL`-unit row remains **unresolved** (§Context, "Historical occurrence — still unresolved"), and
removal itself resolves **no** historical database-integrity question. The §4.9 fail-closed
preflight continues to govern data that may already exist.

**Intended removal boundary.**

1. Remove `SyncImportExecutionService::import_monthly_report`.
2. Remove `ReportRepository::insert_or_replace_raw_daily_report` **only if** its lack of other
   callers is **reconfirmed** against the implementation-phase baseline.
3. Remove **only** the live architecture annotations **specifically associated with P3** from the
   source, and **close** — rather than delete — the corresponding
   `docs/architecture/adr_exception_registry.md` rows, **retaining** their historical records for
   auditability in line with the registry's existing convention, subject to verification against
   the registry conventions in force at implementation time and to an approved implementation scope.
   Unrelated registry rows, and the pre-existing **global** drift between the registry's recorded
   row count / line numbers and the current source, are **not** part of this scope and remain a
   separate governance task.
4. Update directly relevant historical documentation where necessary, preserving an accurate
   historical record. **No additional ADR-0064 change is required in this phase (decided):**
   ADR-0064's existing statements — that the method has **no caller** (G27) and that the path could
   in principle have written a cross-UNIT layer id while legacy data cannot be disproven (§4.9) —
   **remain compatible** with the removal, because they are statements about capability and about
   unresolved historical execution, not about current reachability. **ADR-0064 §4.9's fail-closed
   preflight safeguards remain in force** and are unaffected by this decision. The recorded ADR-0064
   amendment loci (§Governance-2, §Q3-A-6, and the "Documents amended or qualified" table row) are
   **unchanged** by this decision and are **not** extended to cover G27. **This ADR does not
   authorize edits to ADR-0064.**

**Preservation requirements (unchanged by this decision).**

1. `insert_raw_meal_item` **must remain** — integration tests depend on it. It is **not** part of
   the removal scope, at any phase.
2. The live `import_daily_reports` path
   (`application/services/sync_import_execution_service.rs:74-133`, reached from
   `application/usecases/sync/import_daily_report_package.rs:150`) **must not** be modified as part
   of a P3 removal. It is **not** a drop-in replacement for P3 and is **not** to be refactored
   into one.
3. `MonthlyReportService::import_monthly_report`
   (`application/services/monthly_report_service.rs:21`) is a **distinct, live** method. It must
   **not** be confused with P3 and must **not** be removed.
4. No unrelated source code, test, schema, migration, restore behavior, architecture rule, or
   configuration may be changed under this decision. **Q2-A, Q2-B, Q3-A, Q3-B, and D20** remain
   separate workstreams; this decision does not broaden into any of them.

**Decision boundaries (normative).**

1. The owner has **decided** that P3 should be removed. That decision is **not** a ratification of
   this ADR and **not** an authorization to implement the removal.
2. The prohibition in §Governance-6 remains **in force** until this ADR has completed the
   repository's required formal review and ratification process, **and** until a **separate,
   explicit owner authorization** for the implementation phase has been granted. The approving
   authority and date are recorded through the repository's established process — **not** invented
   here. The operative conditions are stated in §Governance-6(b) and §Governance-6(c).

## Governance and documentation requirements

1. **Record these decisions through the repository's actual RFC/ADR process.** This document is
   the Proposed ADR; registration in the ADR index and any approval recording follow the
   repository convention.
2. **Reconcile the older Q2/Q3 deferrals and NULL-preservation wording without weakening D20.**
   ADR-0064 §4.4 (the deletion order and its NULL-preservation annotations at steps 8 and 14),
   §4.7 (the D4 nullability premise and its `NULL` outcome rows, the Q6 `NULL` sub-case, and the
   *Write-path validation* recommendation), G25 (its "each nullable" clause), §4.8 (both the
   Class-A table rows `A8`/`A12`/`A14` — `A12` under the Q2-A foreign-key change, `A8`/`A14`
   under the nullability change — and the per-column nullable-ownership table), §4.9,
   §9.3.3 (Q2/Q3 rows), and the related *Explicitly Out of Scope* bullets are updated to reference
   this ADR; the D20 stock-movement exception is **not** weakened. Every such reconciliation is
   **prospective**: it distinguishes a **fresh** database — where the `NULL` sub-cases become
   unreachable — from an **already-initialized** database, where the ADR-0064 §4.6–§4.10
   fail-closed preflight remains the control. **No `NULL` outcome row, no Q6 clause, and no G25
   statement is deleted or weakened.** The **decision-level** nullability clause is **§Q3-A-6**;
   the locus list recorded there, the list here, and the ADR-0064 row of the
   "Documents amended or qualified" table are **the same set**.
3. **Document that fresh-development-database schema changes do not require a compatibility
   migration for the existing development database.** Per ADR-0064 D18 and §8.1, the application
   is not deployed, there is no legacy production database to preserve, and the authoritative
   `001_initial.sql` baseline may be edited directly for a **fresh** database. **No migration is
   created for development-database compatibility.**
4. **Do not claim implementation is complete.** This ADR is a proposal; the schema, service, and
   validation work it plans has **not** been implemented.
5. **Do not modify the claim-verification matrix** to assert that unimplemented behavior is
   already verified (`docs/technical/CLAIM_VERIFICATION_MATRIX.md`). It is updated only after
   implementation, per its own guidance.
   > [2026-10-23 amendment note: the planned work was subsequently implemented in commit
   > `6566b87b89c8b7367e7d8329a928f462917d0cc1` (boundary items 1–5); this clause and the
   > previous one are preserved verbatim as proposal-time statements — see the Amendment log
   > and `docs/architecture/governance_review.md` §7.6. The matrix now carries the two Q2/Q3
   > rows describing the shipped behavior.]
6. **Do not delete or quarantine the unreferenced `SyncImportExecutionService::import_monthly_report`
   / `insert_raw_meal_item` path in this phase.** ADR-0064 §4.9 recorded a *"dead
   `import_monthly_report` / `insert_raw_meal_item` path"* that could in principle have written a
   cross-UNIT `fifo_layer_id`. The audit distinguishes that **unreferenced** method from the
   **live** same-named `MonthlyReportService::import_monthly_report` (reached via
   `import_monthly_summary_package`, which writes a monthly summary only — no meal items, no
   `fifo_layer_id`). **The need to validate the reachability and ownership behavior of the
   unreferenced method is to be investigated, with evidence, during the later code phase** — not
   acted on by deletion here, and not pre-judged as safe to delete. That investigation must cover
   **reachability** (whether any call path reaches it), **ownership** (whether it could write a
   row whose UNIT ownership is unprovable), **and behavior** (what it actually writes, including
   `fifo_layer_id` and `unit_id`). The absence of a direct caller in the current tree is **not**
   by itself evidence that the path is unreachable, unused, or safe, and no such conclusion is
   drawn here. Its signature and behavior are **not** changed, and it is **not** quarantined.

   **(a) Reachability limb completed — result recorded; scope narrowed, not weakened.** The read-only
   **reachability** limb of the investigation required by this clause has been **completed** and is
   recorded in §Context ("Unreferenced-path reachability investigation (read-only) — completed"): no
   in-repository caller was found, `ReportRepository::insert_or_replace_raw_daily_report` was found
   to have no other caller, and the owner has confirmed that there are no external consumers. The
   clause above is **narrowed in scope, not weakened**, by the following clarifications:

   * The removal scope covers **`SyncImportExecutionService::import_monthly_report`** and its
     **exclusive** helper `ReportRepository::insert_or_replace_raw_daily_report` only.
   * **`insert_raw_meal_item` is explicitly preserved** and is **not** part of the removal scope:
     integration tests depend on it. The clause's original "`import_monthly_report` /
     `insert_raw_meal_item` path" wording referred to the *write path* those two together formed,
     not to a decision to delete the repository helper.
   * The live `import_daily_reports` path and the live
     `MonthlyReportService::import_monthly_report` method are **not** part of the removal scope and
     are **not** modified.

   **(b) The prohibition stands until the governance gates are satisfied.** The **owner has
   decided** that P3 is to be removed rather than repaired, but that decision **does not** by
   itself supersede this clause. Until **(i)** this ADR has completed the repository's required
   formal review and ratification process (`docs/architecture/ARCHITECTURE_FREEZE.md` §4 Steps 2–3,
   with §4 Step 2 as modified for this ADR by **G10** — for the Q2/Q3 Database Integrity RFC the
   three mandatory reviewer-role perspectives are not review prerequisites, while the five general
   review criteria, the RFC review, and the RFC approval gate remain in force, and the general
   review requirements for other RFCs are unamended; the approving authority and date are recorded
   through the repository's established process, and are **not** asserted here), **and** **(ii)** a
   **separate, explicit owner authorization** for the implementation phase has been granted
   (§Governance-9), the prohibition above **stands unchanged**: P3 is **not** deleted, quarantined,
   or otherwise altered, and its signature and behavior are **not** changed. This amendment asserts
   **no** approval, approving authority, signature, or approval date, and ADR-0065 remains
   **`Proposed`**.

   **(c) Additional obligations that a later implementation phase must satisfy.** Two governance
   artifacts are touched by the removal. The treatment of each is **decided** below and is recorded
   here as an owner decision of record for this ADR. **Neither decision is a formal review
   approval**, no approving authority, signature, or approval date is asserted, and ADR-0065 remains
   **`Proposed`**.

   * **Exception retirement (ADR-0030) — close-and-retain (decided).** The P3 exception rows are
     governed by `docs/architecture/0030-adr-exception-governance.md` (registration, ADR reference,
     rationale, creation date, expiry/review date, owner, the 90-day expiry policy, and the quarterly
     review cadence). The **adopted treatment** for rows **9** (`[arch:allow-utc-now]`) and **11**
     (`[arch:allow-mutation-before-replay]`) is **close-and-retain**: when the separately authorized
     P3-removal implementation is performed, each row is **marked closed** and its **historical record
     is retained** for auditability, following the convention visible in the registry today
     (`docs/architecture/adr_exception_registry.md:65-77`, where a closed exception's row is retained
     with a closed marker rather than deleted). Only the **directly affected** summary counts are
     updated at that time. **Unrelated registry rows, and the pre-existing global drift between the
     registry's recorded row count / line numbers and the current source, remain out of scope** and
     are a separate governance task. This ADR performs **no** registry change and pre-authorizes
     **no** reconciliation of unrelated rows.

   * **Architecture-freeze applicability — narrow interpretation adopted (decided).** P3's annotation
     suppresses Rule 88 (`scripts/check_arch.ts:1340-1355`), which enforces the frozen §2.7 contract
     "Replay detection before mutation" (`docs/architecture/ARCHITECTURE_FREEZE.md:178`). The
     **adopted interpretation** is that removing the obsolete P3 method and its associated exception
     suppressions does **not**, by itself, require editing `ARCHITECTURE_FREEZE.md` §2 or §3,
     because **no frozen text becomes inaccurate**. Repository rationale: the freeze document
     contains **no** reference to `import_monthly_report`, `SyncImportExecutionService`,
     `adr_exception_registry`, or ADR-0030, so there is no Section 2 or Section 3 locus describing P3
     or the exception registry to update; §2.7's frozen bullet cites **Rule 83-84** and five other
     live Rule-88 suppressions remain in the same file, so the frozen text and the remaining
     exception surface are unchanged by the removal; and removing a suppression **reduces** deviations
     from the frozen contract rather than altering it — the freeze document's precedent (§2.4)
     amends the freeze text to record a *sanctioned exception*, and P3's exception was never
     recorded there. This is an **interpretation adopted by the Project Owner**, recorded as a
     governance decision of record for this ADR. It is **not** a formal `ARCHITECTURE_FREEZE.md` §4
     Step 2 review approval; the freeze document is **not** modified by this ADR.

7. **Specify the ADR-0064 reconciliation in the accompanying RFC, and apply it only with owner
   delegation.** The exact amended wording for ADR-0064 §4.4, §4.7, and G25 — including the
   widened amendment list in §Q3-A-6 above — is drafted in
   `docs/architecture/rfcs/2026-10-21-q2-q3-database-integrity.md` §9.3–§9.5. That wording is a
   **proposal**; it is applied to ADR-0064 and to this ADR's own decision text only when the owner
   delegates it (RFC §3.10 items N1 and N6), and it is applied **before** this ADR is ratified —
   never as part of an implementation phase.
8. **Close three pre-ratification conditions inside this ADR — they are implementation
   preconditions, not deferred tasks, and they are closed here in full text.** This ADR therefore
   does **not** depend on the accompanying RFC to satisfy conditions it requires itself to close
   before ratification. The RFC
   (`docs/architecture/rfcs/2026-10-21-q2-q3-database-integrity.md` §8.3) remains the drafting
   record of how these conditions arose; the two texts are reconciled.

   **(a) The concrete §4.4 deletion-step sequence.** Under ADR-0065 G1's new
   `daily_report_meal_items.fifo_layer_id → fifo_stock_layers(id) ON DELETE RESTRICT` edge, the
   target's dependent report rows must be deleted **before** the FIFO layers they reference. In
   ADR-0064 §4.4's own numbering the order is:

   | Position | Table | Predicate | Ordering reason |
   |----------|-------|-----------|-----------------|
   | 12 | `daily_report_meal_items` | `WHERE meal_id ∈ target reports' meals` | The row that **carries** `fifo_layer_id` must go first: while it exists, the referenced FIFO layer cannot be deleted under `ON DELETE RESTRICT`. |
   | 13 | `daily_report_meals` | `WHERE daily_report_id ∈ target reports` | Child-first within the report hierarchy (`CASCADE` downward). |
   | 14 | `daily_reports` | `WHERE unit_id = <id>` | The report row itself, after its meals and its items. |
   | **10** | `fifo_stock_layers` | `WHERE unit_id = <id>` | **After** steps 12/13/14 — this is the step that moves relative to ADR-0064 §4.4's current order. |

   Concretely: **steps 12/13/14 move ahead of step 10**; every other step keeps its ADR-0064 §4.4
   relative order and its predicate verbatim. The whole sequence stays inside the **same single
   transaction** as Phase 0 and Phase 1. The §4.6 Phase-0 preflight still completes **before the
   first `DELETE`**; the §4.9 surviving-report FIFO preflight still rejects the deletion when a
   surviving report — of this UNIT **or of another UNIT** — references a target-owned FIFO layer,
   and such a layer is **never** deleted; ownership is never inferred or guessed, and a row whose
   ownership cannot be proven is rejected rather than deleted. ADR-0064 §4.4 remains the
   authoritative statement of the implemented order until this ADR is ratified and implemented;
   its §4.4 now carries a prospective pointer to this order.

   **(b) The §4.4 step-14 and step-8 text.** With the §Q3-A-6 amendment applied (and the
   §Governance-2 reconciliation of §4.7/G25), the two step lines read — inside ADR-0064 §4.4's
   fenced block, with the predicates **unchanged**:

   ```text
    8. supplier_orders        WHERE unit_id = <id>  (NULL-unit orders PRESERVED, §4.8 — qualified prospectively by ADR-0065 Q3-B)   (A8)
   14. daily_reports          WHERE unit_id = <id>  (NULL-unit reports PRESERVED, §4.8 — qualified prospectively by ADR-0065 Q3-A)  (A14)
   ```

   The `(NULL-unit … PRESERVED, §4.8)` annotations are **qualified prospectively, not deleted**,
   and the predicates `WHERE unit_id = <id>` are **unchanged**. Reason: the annotated state is
   unreachable only on a **fresh** database; on an **already-initialized** database the columns
   are still nullable, §4.8's per-column policy remains the operative preserve rule, and the
   deletion transaction runs there too. Removing the annotations outright would drop a preservation
   guarantee that still binds there. The wording is the one ADR-0064 §4.8's per-column table
   already uses (`Superseded prospectively by ADR-0065 (Q3-A)` / `(Q3-B)`). Ownership rules, the
   fail-closed behaviour, and the deletion order are stated in ADR-0064 §4.4/§4.7 and are **not**
   changed by these two lines.

   **(c) The pre-implementation verification conditions for the uniqueness replacement.** The
   replacement of the `COALESCE(unit_id, '__NULL__')` expression index by a unique index on
   `(date, unit_id)` is part of the **fresh-database baseline edit only**. Before the baseline
   index is replaced, the following must be verified and recorded:
   * the **current** schema definition of `daily_reports` and of the existing
     `idx_daily_reports_date_unit_unique` index (current baseline:
     `ON daily_reports(date, COALESCE(unit_id, '__NULL__'))`), together with the exact list of
     indexes on `daily_reports`;
   * the **queries that depend on the index** — every read and every uniqueness check that
     addresses a daily report by `(date, unit_id)` — so that the replacement changes no result
     set;
   * that the replacement is applied **only** to the fresh baseline and that **no** upgrade or
     migration path is created for existing databases: an already-initialized database keeps its
     existing index unchanged, and no upgrade path for it is approved by this ADR.

   `UNIQUE(date, unit_id)` is **not** asserted to be equivalent to the expression index in a
   database that permits `NULL`: where `unit_id` may be `NULL`, SQL unique semantics allow
   multiple rows whose unique column is `NULL`, so the two indexes are **not** interchangeable
   there. The equivalence argument in §"Daily-report uniqueness" item 3 is expressly limited to a
   **fresh** database in which `unit_id` is `NOT NULL`.
9. **Separate ratification from implementation.** Ratifying this ADR records the **decision
   only**. It does **not** authorize any SQL, schema, migration, source-code, or test change.
   Every step in §"Proposed implementation boundary" requires a **separate, explicit owner
   authorization**, granted after the applicable governance gate is satisfied. The owner accepted
   this ADR on **2026-10-23** (**Project Owner `1Bxm`**); the acceptance record is
   `docs/architecture/governance_review.md` §7.5.
10. **Name the follow-up owner for the deferred `monthly_reports.unit_id` foreign key.** The
    follow-up owner is the **Project Owner**, until a specific implementer is designated. A
    **dedicated follow-up ADR must be created before that change is executed**; no ADR number and
    no tracking-issue identifier are invented here, and the foreign key does **not** exist today.
11. **Settle the restore-path decision before modifying the restore path.** The decision on how
    `PRAGMA foreign_key_check` violations are handled must be settled **before** the restore path
    is modified — including **when** the check runs relative to the atomic replacement, and **what
    is done** when violations are detected (abort finalization, or invalidate the restored
    database). Nothing in this ADR settles that mechanism, and no restore-path change is made
    here. The standing constraints remain: detect, report, never auto-repair
    (`ARCHITECTURE_FREEZE.md` §2.6).

# Decision (proposed)

The design direction above is recorded as the proposed decision set for Q2/Q3 database integrity.
The corresponding **mechanical choices** are:

| Ref | Proposed choice | Requirement |
|-----|-----------------|-------------|
| G1 | `daily_report_meal_items.fifo_layer_id` stays **nullable**; FK to `fifo_stock_layers(id)` added with `ON DELETE RESTRICT`; non-NULL ownership/product validated in the service layer; no composite key, no new ownership column. | Q2-A |
| G2 | `daily_reports.unit_id` becomes **`NOT NULL`** with FK to `units(id)` `ON DELETE RESTRICT`; write/import paths reject missing/invalid/mismatched UNIT; ownership never inferred. | Q3-A |
| G3 | `supplier_orders.unit_id` and `supplier_order_items.unit_id` become **`NOT NULL`**; item-vs-parent-order UNIT equality enforced at the application/service layer; existing `units(id)` RESTRICT FK retained; no caller-UNIT fallback. | Q3-B |
| G4 | `stock_movements.unit_id` remains **nullable** with its type-specific CHECK; D20 preserved; no global `NOT NULL`. | Q3-C |
| G5 | `monthly_reports.unit_id` unchanged (**deferred** FK — the FK does **not** exist today); its `NOT NULL` rule preserved. Follow-up owner: **Project Owner**, until a specific implementer is designated; a **dedicated follow-up ADR** is required before that change is executed. | Monthly |
| G6 | Foreign-key enforcement verified on relevant write connections, fail-safe otherwise; `PRAGMA foreign_key_check` after restore reports but never repairs; scope not expanded to ordinary sync imports without justification. | SQLite enforcement |
| G7 | Replace the `COALESCE(unit_id, '__NULL__')` uniqueness expression with a conventional unique index on `(date, unit_id)` in the fresh-database baseline schema edit, after verifying the existing schema and query behavior as a pre-implementation condition. | Uniqueness |
| G8 | Baseline-only schema edits for a fresh database; **no compatibility migration** for the development database. | Governance |
| G9 | The **unreferenced** `SyncImportExecutionService::import_monthly_report` path (P3) is **removed** in a later, separately authorized implementation phase rather than repaired; the removal scope is limited to that method and its **exclusive** helper `ReportRepository::insert_or_replace_raw_daily_report`; `insert_raw_meal_item`, the live `import_daily_reports` path, and the live `MonthlyReportService::import_monthly_report` method are **preserved**. Ratification of this ADR records the decision only — the removal implementation requires a separate, explicit owner authorization (§Governance-6, §Governance-9). | Unreferenced path (N9) |
| G10 | **Q2/Q3 RFC review-angle exception.** **Decision:** the three mandatory reviewer-role perspectives specified in `ARCHITECTURE_FREEZE.md` §4, Step 2 are **not** mandatory prerequisites for the review of the Q2/Q3 Database Integrity RFC, within the scope of this project-specific exception. **Scope:** the exception applies **only** to that RFC; it does **not** amend the general review requirements for other RFCs, and the **five general review criteria remain in force**. **Review status:** this decision does **not** assert that a formal RFC review has occurred, that the RFC has been approved, or that ADR-0065 has been ratified. **Unchanged requirements:** the RFC approval gate, the ADR-0065 ratification requirements, the applicable governance records, and the separation between ratification and implementation authorization remain in force. | RFC review (Q2/Q3) |

## Documents amended or qualified by this ADR

| Document | Exact locus | Relationship |
|----------|-------------|--------------|
| `docs/architecture/0064-unit-export-state-and-owned-data-deletion.md` | §4.4 (deletion order — steps 12/13/14 moved before step 10 — and the NULL-preservation annotations at steps 8 and 14), §4.7 (the D4 nullability premise, its `NULL` outcome rows, the Q6 `NULL` sub-case, and the *Write-path validation* recommendation), G25 (the "each nullable" clause), §4.8 (nullable-ownership table — `daily_reports.unit_id` / supplier-order rows — and the Class-A rows `A8`/`A12`/`A14`), §4.9 (surviving-report FIFO reference + "No schema change here" deferral), §9.3.3 Q2/Q3 rows, and the *Explicitly Out of Scope* bullets on Q2/Q3 | **Partially superseded / reconciled, not wholesale.** ADR-0064's Q2/Q3 **deferrals** and the NULL-ownership statements they relied on are superseded by this ADR's proposed hardening. That supersession is **prospective**: for a **fresh** database the `NULL` sub-cases become unreachable, while for an **already-initialized** database ADR-0064's §4.6–§4.10 preflight remains the control and its §4.7 rules and G25 remain accurate as written. ADR-0064's ratified decision set D11–D22, its fail-closed deletion preflight (§4.6–§4.10), and its D18 schema posture are **preserved**. ADR-0064 is edited only to point at this ADR, to qualify the §4.7/G25 schema premise, and to stop contradicting it; **no `NULL` outcome row, no Q6 clause, and no G25 statement is deleted**; the full decision record lives here. **Applied under the owner delegation of this round (RFC §3.10 N1 and N6):** the listed loci of ADR-0064 now carry the prospective qualification — §4.4's two annotations and two new ordering-rule bullets, §4.7's D4 premise, outcome-table note, complementary bullet and Q6 bullet, §4.7's *Write-path validation* item (b), G25's "each nullable" clause, and §4.8's Class-A rows `A8`/`A12`/`A14`. The §4.8 per-column nullable-ownership table and §4.9's "surviving report rows" bullet were already qualified beforehand and were **not** re-edited. The locus list in this row is **the same set** as §Q3-A-6 and §Governance-2. |
| `docs/architecture/ARCHITECTURE_FREEZE.md` | §2.6 SQLite Topology & Runtime ("No auto-repair"), §2.4 fiscal semantics and its sanctioned ADR-0064 purge exception | **Referenced, unchanged.** §2.6's "No auto-repair" governs the `foreign_key_check` requirement (detect/report, never repair). §2.4's closed-year semantics are unaffected: this ADR adds no destructive path and does not alter the ADR-0064 purge exception. **Option B applies** (§"ARCHITECTURE_FREEZE.md §2.4 — scope determination"): the §4.4 re-ordering was verified against the four points recorded there — the §6.2 protected data set, the ownership rules, the transaction atomicity, and the fail-closed controls — and none of them changes, so §2.4 needs no amendment and `ARCHITECTURE_FREEZE.md` is **not** modified by this ADR. This determination does **not** assert that by-reference incorporation immunizes a frozen clause. |
| `docs/architecture/rfcs/2026-10-21-q2-q3-database-integrity.md` | §3.1–§3.9 (the schema decisions restated), §3.10 (the owner-decision list N1–N7), §4.5–§4.6 (the §2.4 scope determination and the approval-record location), §8.1–§8.5 (the deferred questions and their dispositions), §9.1–§9.7 (the documentation-scope reconciliation and the code-locus analysis), Appendix B (the §4 Step 2 review checklist) | **Referenced, reconciled, not amended by this ADR.** The RFC is the drafting vehicle for these decisions: the §4.4/§4.7/G25 reconciliation wording is specified there and has now been applied to ADR-0064 under the owner delegation of this round (RFC §3.10 N1 and N6). The RFC is **approved (2026-10-22; `governance_review.md` §7.1)**; that approval does not constitute ratification of this ADR, and this ADR's ratification does not depend on the RFC's own approval state. The RFC's §8.3 conditions are now **closed inside this ADR** (§Governance-8), so this ADR no longer depends on the RFC to satisfy them. |

# Proposed implementation boundary (later phase; not performed here)

Listed for traceability only. **None of the following is performed by this ADR, and ratifying this
ADR does not authorize any of it** — each step requires a separate, explicit owner authorization
(§Governance-9).

1. **Schema (fresh-database baseline only)** — edit `src-tauri/src/db/migrations/001_initial.sql`:
   * `daily_report_meal_items.fifo_layer_id` → add `REFERENCES fifo_stock_layers(id) ON DELETE RESTRICT` (column stays nullable);
   * `daily_reports.unit_id` → `NOT NULL` + `REFERENCES units(id) ON DELETE RESTRICT`;
   * `supplier_orders.unit_id` → `NOT NULL` (existing `units(id)` RESTRICT FK retained);
   * `supplier_order_items.unit_id` → `NOT NULL` (existing `units(id)` RESTRICT FK retained);
   * replace the `COALESCE(unit_id, '__NULL__')` uniqueness expression with a unique index on `(date, unit_id)`;
   * leave `stock_movements.unit_id` and `monthly_reports.unit_id` unchanged.
   * **No** migration file is created; the baseline is edited for a fresh database only.
2. **Repositories** — adjust SQL (still confined to `src-tauri/src/repositories/`) for the new
   NOT NULL assumptions and any index needed by the `(date, unit_id)` uniqueness change.
3. **Domain / application** — add service-layer validation for non-NULL `fifo_layer_id`
   ownership/product, `daily_reports.unit_id`, and supplier-order item/header UNIT equality.
   Reject with explicit fail-closed errors; never infer ownership.
4. **Deletion ordering** — update the ADR-0064 §4.4 delete order exactly as fixed in
   §Governance-8(a): steps 12/13/14 (`daily_report_meal_items` → `daily_report_meals` →
   `daily_reports`) move **ahead of** step 10 (`fifo_stock_layers`), so the new `ON DELETE
   RESTRICT` edge is satisfied by an explicit deterministic order while atomicity, the Phase-0
   preflight, the surviving-report FIFO protection, and the other-UNIT data protection are
   preserved.
5. **Restore path** — add `PRAGMA foreign_key_check` after restore/replacement, report violations,
   never auto-repair, and determine (from the implementation) whether violations abort finalization
   or invalidate the restored database. **The decision on that handling — including the timing of
   the check relative to the atomic replacement — must be settled before this path is modified
   (§Governance-11); nothing here settles it.**
6. **Unreferenced-path investigation** — with evidence, validate the reachability and ownership
   behavior of the **unreferenced** `SyncImportExecutionService::import_monthly_report` /
   `insert_raw_meal_item` path; **do not delete or quarantine it** until that investigation is
   complete, and do not pre-judge it as safe to delete. **Reachability investigation status:
   complete** (read-only, 2026-10-22 — §Context "Unreferenced-path reachability investigation
   (read-only) — completed"); the **historical-occurrence** question — whether the path ever
   actually wrote a cross-UNIT or `NULL`-unit row — **remains unresolved**, and the §4.9
   fail-closed preflight continues to govern data that may already exist. **Owner decision:
   removal, not repair** (G9). The removal itself is a **later, separately authorized
   implementation step**, gated by §Governance-6(b) and §Governance-6(c): this ADR's ratification
   **and** a separate, explicit owner authorization. It is **not performed by this ADR**, which
   remains `Proposed` and performs no source, test, schema, migration, restore, registry, or
   architecture-rule change.
7. **Documentation** — after implementation, update `docs/technical/CLAIM_VERIFICATION_MATRIX.md`
   and any operational-limitations documents to describe the shipped behavior.
   > [2026-10-23 amendment note: performed by the 2026-10-23 closure amendment and its
   > accompanying records (`governance_review.md` §7.6; `CLAIM_VERIFICATION_MATRIX.md` Q2/Q3 rows;
   > `CHANGELOG.md` unreleased entry) for the implementation commit
   > `6566b87b89c8b7367e7d8329a928f462917d0cc1`. `OPERATIONAL_LIMITATIONS.md` required no
   > correction: its existing restore limitation statements remain accurate; no fresh-database
   > upgrade path exists.]

# Verification expectations (later phase; not executed here)

1. A fresh database created from the edited `001_initial.sql` has `daily_reports.unit_id`,
   `supplier_orders.unit_id`, and `supplier_order_items.unit_id` as `NOT NULL`, each with the
   stated FK, and `daily_report_meal_items.fifo_layer_id` nullable with the `ON DELETE RESTRICT`
   FK.
2. Inserting a daily report, supplier order, or order item without a UNIT fails closed at the
   schema and/or service layer; no `NULL`-unit row is accepted.
3. A supplier order item whose UNIT differs from its parent order's UNIT is rejected at the
   application/service layer with an explicit error.
4. An order item is never assigned a UNIT from a caller-UNIT fallback; an ambiguous UNIT fails
   closed.
5. `stock_movements.unit_id` remains nullable; `IN` movements still require a UNIT via the CHECK;
   `OUT`/`OPENING` `NULL`-unit rows remain representable and are preserved by the ADR-0064 purge
   (D20).
6. Deleting a UNIT whose FIFO layers are referenced by no surviving report succeeds via the
   reordered deletion sequence; deleting a UNIT whose FIFO layer is referenced by a surviving
   report from another UNIT fails closed and deletes nothing.
7. Restoring a database with a foreign-key violation reports the violation via
   `PRAGMA foreign_key_check` and does not auto-repair or silently accept it.
8. `bun run check:arch` reports zero warnings; `cargo clippy -D warnings`, `cargo test`, `vitest`,
   `svelte-check`, and the release-integrity and documentation-governance scripts pass — **run in
   the implementation phase, not in this documentation phase.**
   > [2026-10-23 amendment note: items 1–7 above were implemented and verified in the authorized
   > implementation phase (commit `6566b87b89c8b7367e7d8329a928f462917d0cc1`; recorded in
   > `governance_review.md` §7.6), and item 8's gates were passed there. "not executed here"
   > remains accurate — this ADR itself executed none of them; see the Amendment log.]

# Consequences (proposed)

* **Integrity.** UNIT ownership of daily reports and supplier orders becomes enforceable at the
  schema level, and FIFO layer references become referentially constrained — closing the Q2/Q3
  gaps ADR-0064 deferred.
* **Fresh-database-only reach.** Because the initial schema is applied once per database
  (ADR-0064 §8.1), these guarantees reach **fresh** databases only. The runtime preflight of
  ADR-0064 §4.6–§4.10 remains the control for already-initialized databases. This ADR MUST NOT
  imply that editing the baseline upgrades an existing database.
* **No change to D20 or the ADR-0064 retained set.** `NULL`-unit stock movements and the
  node-global/shared preserved tables are unaffected.
* **Compatibility.** No compatibility migration is introduced; the development database is
  recreated as needed (D18 posture).
* **`ARCHITECTURE_FREEZE.md` is unchanged (Option B).** The §4.4 re-ordering was verified against
  the four points in §"ARCHITECTURE_FREEZE.md §2.4 — scope determination" — protected data set,
  ownership rules, atomicity, fail-closed controls — and none of them changes. No frozen clause
  is amended, and no immunity from by-reference incorporation is claimed.
* **Ratification alone changes nothing at runtime.** This ADR records a decision. Any SQL, schema,
  source, or test change needs a separate, explicit owner authorization after the governance gate
  is satisfied (§Governance-9).
* **Read paths are retained, not rewritten.** The `IS NULL` read branches and the `None` read path
  of the WILAYA consumption flow are **kept** in this phase. Retention preserves the current code
  paths and the related fail-closed semantics; it does **not** make the outputs of a fresh
  `NOT NULL` database equal to those of an already-initialized database that may still hold `NULL`
  rows. No proven security defect is claimed for that difference.

# Contradictions and open questions (recorded, not decided)

1. **Restore finalization semantics (open).** Whether a `foreign_key_check` violation must **abort
   restore finalization** or **invalidate the restored database** depends on the atomic-swap
   implementation and is to be determined — with evidence — in the implementation phase. This ADR
   records the requirement (detect, report, never auto-repair) but deliberately does not fix the
   mechanism. **The decision must be settled before the restore path is modified**, including when
   the check runs relative to the atomic replacement and what is done on detection
   (§Governance-11).

   **Resolution (recorded 2026-10-23 — amendment).** The decision was settled by the owner
   (`governance_review.md` §7.4 restore-failure policy) and implemented in commit
   `6566b87b89c8b7367e7d8329a928f462917d0cc1` as a pre-swap fail-closed gate
   (`validate_restored_foreign_keys` — `sqlite_backup_adapter.rs:200`, invoked as Step 2b at
   `:739`). The phase's disaster-recovery test `test_restore_aborts_on_foreign_key_violation_before_swap`
   (`disaster_recovery_integration_tests.rs:113-206`) demonstrates the four record points of
   §7.4 (`:456-458`): pre-swap validation (restore artifacts never created, `:190-191`), abort
   on detection (`:177-183`), live-database preservation (length and marker assertions,
   `:190-205`), and diagnostics carrying table/parent/fkid (`:184-186`); the violating candidate
   is rejected, never repaired. This closes **only** the open-question status above; the
   question paragraph is preserved verbatim.
2. **Unreferenced import path (reachability closed; governance gate open).** The reachability and
   ownership behavior of the **unreferenced** `SyncImportExecutionService::import_monthly_report` /
   `insert_raw_meal_item` path (ADR-0064 §4.9) required evidence-based investigation before any
   change; this ADR neither judged nor acted on it, and did **not** treat the label "dead" as
   established. **The reachability investigation is now complete** (§Context "Unreferenced-path
   reachability investigation (read-only) — completed"; §Governance-6(a)): no in-repository caller
   was found, `ReportRepository::insert_or_replace_raw_daily_report` has no other caller, and the
   owner has confirmed that there are no external consumers. **The historical-occurrence question
   remains unresolved** and continues to be governed by ADR-0064 §4.9. **The owner's decision is
   removal, not repair** (G9). **What remains open** is the governance gate itself: the prohibition
   in §Governance-6 stands until this ADR is ratified and a separate implementation authorization is
   granted (§Governance-6(b)), subject also to §Governance-6(c). The **live** same-named
   `MonthlyReportService::import_monthly_report` (reached via `import_monthly_summary_package`)
   writes a monthly summary only, is **not** implicated, and is **not** part of the removal scope.
3. **Cross-UNIT FIFO reference remediation (inherited).** ADR-0064 Q5's controlled external
   operational-repair policy remains the remediation path for data the preflight rejects; this ADR
   adds no automatic repair.

# Explicitly out of scope

* Any application, schema, migration, test, or database implementation (this is a proposal).
* Editing `src-tauri/src/db/migrations/001_initial.sql`, Rust source, frontend source, tests,
  fixtures, database files, or generated baselines **in this phase**.
* Creating a compatibility migration for the development database.
* Globally making `stock_movements.unit_id` `NOT NULL`, or otherwise weakening D20.
* Adding a foreign key to `monthly_reports.unit_id` (deferred, separate scope).
* Expanding `PRAGMA foreign_key_check` to ordinary sync imports.
* Deleting or quarantining the unreferenced `SyncImportExecutionService::import_monthly_report` /
  `insert_raw_meal_item` path — **in this phase**. The read-only **reachability** investigation is
  complete and the owner has decided on removal (G9), but the deletion itself is deferred to a
  later implementation phase gated by §Governance-6(b) and §Governance-6(c). `insert_raw_meal_item`
  is **preserved** in every phase.
* Updating `docs/technical/CLAIM_VERIFICATION_MATRIX.md` to assert unimplemented behavior.
* Reopening, narrowing, or weakening any owner-ratified decision (D11–D22) of ADR-0064, its
  fail-closed deletion preflight (§4.6–§4.10), or its D18 schema posture. The **prospective**
  qualification of ADR-0064 §4.7/G25 (see Governance-2) describes the schema premise for a
  **fresh** database and does **not** reopen, narrow, or weaken the preflight rule, the D4/Q6
  rules, the D20 exception, or the D18 posture.
* Amending `ARCHITECTURE_FREEZE.md` through this ADR.
* **Recording an approval.** No individual signature is created by this ADR. The owner accepted
  this ADR on **2026-10-23** (approving authority: **Project Owner `1Bxm`**), recorded through the
  repository's ordinary governance process in `docs/architecture/governance_review.md` §7.5. The
  previously specified three-role review requirement is **not** a mandatory prerequisite for the
  Q2/Q3 Database Integrity RFC under the project-specific exception recorded in this ADR (G10).
  **No formal review is recorded by that exception itself.** RFC approval was separately recorded
  on 2026-10-22 in `governance_review.md` §7.1; ADR-0065 acceptance is separately recorded on
  2026-10-23 in `governance_review.md` §7.5.
* **Creating a tracking issue.** No tracking-issue identifier exists for ADR-0065, and none is
  invented here.
* **Adding an entry to the frontend governance approvals registry.** That registry is
  frontend-layer scoped and is not the ratification record for this ADR.

# References

| Document | Location |
|----------|----------|
| ADR-0064 (UNIT export state & owned-data deletion) | `docs/architecture/0064-unit-export-state-and-owned-data-deletion.md` |
| Accompanying RFC (approved by the Project Owner (`1Bxm`) on 2026-10-22 — record: `docs/architecture/governance_review.md` §7.1; RFC approval only, not ratification of this ADR) | `docs/architecture/rfcs/2026-10-21-q2-q3-database-integrity.md` |
| ADR index (registration of this ADR, row `0065`) | `docs/architecture/ADR_INDEX.md` |
| Architecture freeze declaration (**unmodified** by this ADR) | `docs/architecture/ARCHITECTURE_FREEZE.md` |
| Architectural invariants charter | `docs/architecture/ARCHITECTURAL_INVARIANTS.md` |
| Claim verification matrix | `docs/technical/CLAIM_VERIFICATION_MATRIX.md` |
| Architecture governance review record (when an actual review exists) | `docs/architecture/governance_review.md` |
| Governance approvals registry — **frontend layer only**; **not** the ratification record for this ADR | `docs/governance/frontend/GOVERNANCE_APPROVALS.md` |
