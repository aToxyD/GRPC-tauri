# ADR 0066: Development Lifecycle Posture — Owner-Controlled Lifecycle Stage, Development-Resource Classification, and Destructive-Operation Authorization

# Status

Accepted (2026-10-23)

> **Governance state.** This ADR is **Accepted** (owner acceptance on **2026-10-23** by the
> **Project Owner (`1Bxm`)**; acceptance record: `docs/architecture/governance_review.md` §7.7;
> registered as `ADR_INDEX.md` row `0066`). It records the Project Owner's development lifecycle
> policy directions as a standalone governance policy; its acceptance is an independent
> governance event that neither follows from nor implies ADR-0065's acceptance, and it changes no
> other document's status. Status changes follow the repository convention
> (`docs/architecture/ADR_INDEX.md`, Policy §3: Draft → Proposed → Accepted → Deprecated →
> Superseded) and must be recorded through the ordinary governance process, not by this document
> alone. **This acceptance records a governance decision only: it authorizes no implementation,
> no destructive operation, no lifecycle-stage transition, and no operational activity.**

# Date

2026-10-22 (draft/creation date; proposed 2026-10-22)

> **Status transition record.** 2026-10-22 — documentation-only status transition `Draft` →
> `Proposed`, recorded per the repository's status convention (`docs/architecture/ADR_INDEX.md`,
> Policy §3) on the Project Owner's submission decision. The submission asserts **no** approval,
> approving authority, signature, review outcome, or ratification date, and changes no file other
> than this document. The policy content of this ADR is unchanged by the transition.

> **Clarification revision record.** 2026-10-23 — on the Project Owner's instruction, a
> documentation-only clarification revision was applied to this document; no other file was
> modified. It made three reconciliations: (1) ADR-0065 references, repository-state provenance,
> and line anchors were corrected to the working tree at HEAD
> `987229a2e69eeb14b4f36d06adb70602899be52b` (see the References footnote); (2) ADR-0065's status
> reference was updated to **Accepted (2026-10-23)** as recorded in
> `docs/architecture/governance_review.md` §7.5 — this document's own status is unchanged and
> remains **Proposed**, records no approval, and awaits the Project Owner's separate ratification
> decision; ADR-0065's acceptance neither ratifies this document nor follows from it; and (3)
> clauses 4.6, 4.7, and 5.5 were added to record the explicit resource-class distinctions of the
> destructive-operation policy and to acknowledge standing application-managed backup rotation.
> This record documents an edit; it is not a ratification, approval, or status transition, and it
> authorizes no implementation and no operational activity.

> **Status transition record — `Proposed` → `Accepted`.** 2026-10-23 — documentation-only status
> transition recorded per the repository's status convention (`docs/architecture/ADR_INDEX.md`,
> Policy §3) on the Project Owner's acceptance decision; acceptance record:
> `docs/architecture/governance_review.md` §7.7; index registration: `ADR_INDEX.md` row `0066`.
> No policy clause of this ADR is changed by the transition; no other file's status is affected;
> and the transition authorizes no implementation, no destructive operation, no lifecycle-stage
> transition, and no operational activity.

# Owner

Architecture / Data Governance

> **Approval convention.** The policy directions recorded here were provided and accepted by the
> **Project Owner (`1Bxm`)**. The recorded acceptance date (2026-10-23) and approving authority are
> preserved in this document's `# Status` governance state and in the repository's acceptance
> record (`docs/architecture/governance_review.md` §7.7); nothing was invented here. Registration
> in `docs/architecture/ADR_INDEX.md` (row `0066`) is performed by the ratification step this
> block anticipated. No signature or tracking identifier exists beyond the recorded decision, per
> repository convention.

# Context

The governance corpus currently contains **no standing project-lifecycle policy**. Every existing
"lifecycle" provision is a *domain* lifecycle: the UNIT operator credential and initialization
lifecycle (`docs/architecture/0063-unit-operator-credential-lifecycle.md`, referenced by
`docs/architecture/0064-unit-export-state-and-owned-data-deletion.md:14-15`) and the fiscal-year
lifecycle (`docs/architecture/governance_review.md` §5.1–§5.5). The repository's development
posture is instead established by two adjacent, narrower provisions:

| Provision | Location | What it actually governs |
|-----------|----------|--------------------------|
| Production Root-key prerequisite (A44-06) | `ARCHITECTURE_FREEZE.md` §2.8, line 189 | A **deployment gate**: acceptance of ADR-0044/ADR-0045 is not a production deployment decision; production remains blocked until the production Root key is approved (ceremony, installation, possession, distribution, verification — ADR-0044 §9.4) |
| D18 schema-change posture and §8.1 mechanism | ADR-0064 `## 8. (D18) Schema-change posture` (line 1416) and `### 8.1 Migration mechanism and its compatibility consequence (D18, G31)` (line 1431) | The **fresh-baseline-only** rule: repository changes are made directly in `src-tauri/src/db/migrations/001_initial.sql`; "No migration is created for development-database compatibility" (ADR-0064:1429); an edited baseline reaches only a fresh database (:1442-1445); the schema is applied exactly once per database (:1446-1447); "D18 remains an owner-ratified decision; this section records its mechanism, not a new decision" (:1455-1456) |

Both provisions rest on a deployment-state fact — "the application has not been deployed and there
is no legacy production database to preserve" (ADR-0064:1448-1450) — that is asserted inside
individual ADRs rather than declared as a standing, owner-controlled lifecycle rule. The gap this
draft addresses: **what the project's lifecycle stage is, who may change it, how artifact
classification works, and what standing rule governs destructive operations** — as a single,
cross-referenced governance policy, without duplicating or amending D18/§8.1 or any frozen clause.

# Decision (draft)

The following numbered clauses record the Project Owner's policy directions as a draft decision
set. They are **declarative governance rules**: they change no runtime behavior, no schema, no
migration, no source, and no test.

## 1. Purpose and scope

This policy governs **lifecycle declarations** and **resource classification** for the project. It
establishes a general project-governance rule only. It does **not** approve ADR-0065 (Q2/Q3
Database Integrity), the handling of `SyncImportExecutionService::import_monthly_report` (P3), any
RFC, or any implementation, and it performs none of them. Nothing in this ADR modifies another
document.

## 2. Owner-controlled lifecycle stage

1. The project remains in its **current development lifecycle stage** until the Project Owner
   **explicitly declares and records a transition** through the applicable governance process. A
   transition declaration is a recorded owner decision; the passage of time, absence of a counter-
   declaration, or continuation of current activity is not a declaration.
2. The following events do **not independently constitute a lifecycle transition**: application
   execution, successful builds, successful tests, application launches, package creation, database
   initialization, and backup creation. Each remains subject to its own applicable authorizations;
   none of them, alone or in combination, changes the declared lifecycle stage.
3. No transition to any other stage is claimed by this document, and no transition is recorded by
   its creation.

## 3. Development-resource classification

1. Unless the Project Owner declares otherwise, the following are classified as **development
   resources**: application instances, development databases, generated application data,
   development packages and build artifacts, and development backup artifacts.
2. Classification describes a resource's **lifecycle stage and context**. It is a status
   statement, not a permission statement, and it does not — by itself — decide, project, narrow,
   change, or exempt any compatibility requirement. The project's existing compatibility,
   testing, and architecture-review policies are not modified by this clause and remain governed
   by their own authoritative documents.
3. **Development-resource classification does not mean that a resource is disposable, expendable,
   or authorized for deletion.**
4. A development resource may contain data or artifacts subject to **retention, ownership, audit,
   recovery, identity, or security requirements**. Examples of standing requirements that are
   unaffected by classification: audit-trail integrity (`governance_review.md` §4), the ADR-0064
   preserved node-global data set (§6.2) and D20's preserved `NULL`-unit stock movements,
   ownership rules of ADR-0064 §4, and identity/trust material held outside the application
   database (ADR-0038; ADR-0062 identity data directory). Clause 3.4 identifies *examples*, not an
   exhaustive list, and adds no new obligation and removes none.

## 4. Destructive-operation authorization

1. Development status does **not** authorize deleting, resetting, overwriting, replacing,
   recreating, or otherwise destructively modifying any resource. Development-resource
   classification does not make a resource destructible and confers no deletion, recreation,
   replacement, or destructive-modification permission on it.
2. Every actual destructive operation on a resource covered by this policy — **not databases
   alone** — requires a **separate, explicit authorization from the Project Owner for that
   operation**, obtained before execution. Such authorization is never inferred from this policy,
   from development status, from an RFC, from an ADR, or from a general development mandate, and
   it covers only the operation it explicitly names and the resources that authorization itself
   identifies: it does **not** extend to other operations or to other resources.
3. For destructive **database** operations, the authorization in 4.2 is additionally subject to,
   at minimum: a **verified target** (the operation must positively identify what it acts on), a
   **defined scope**, and an **appropriate operational safety procedure**.
4. Where relevant, rules 4.1–4.3 cover explicitly — non-exhaustively — application **database
   files**, SQLite **`-wal` and `-shm` sidecar files**, **backup artifacts**, **identity/trust
   material**, **encryption keys**, **recovery artifacts**, **journals** (for example the
   restore-journal file used by the backup/restore path), and **audit records**.
5. The list in 4.4 does not replace, widen, or narrow any existing retention, ownership, audit,
   security, or recovery requirement; it enumerates artifact classes to which rules 4.1–4.3 apply.

6. **Scope distinctions.** Rules 4.1–4.3 apply across distinct resource classes, distinguished
   explicitly below; no class's rule is implied by another's:

   a. **Class 1 — active development database and its backups.** The project's active development
      database — the operating database of the current development stage, by default the database
      the application resolves and opens through its normal startup path — together with its
      SQLite `-wal`/`-shm` sidecar files and its standing backup artifacts, is an owner-reserved
      resource. Its destructive lifecycle operations (deleting, recreating, resetting,
      replacing, reinitializing) and the direct destructive deletion of its backups may be
      performed, if at all, only under a separate, explicit Project Owner authorization for the
      named operation (4.2–4.3); no contributor, agent, or automation may perform such an
      operation without that authorization. The project's development stage, the experimental
      character of development data, and this ADR's proposed (or any later) status confer no
      permission, individually or in combination, and create no exception to 4.1–4.3.

   b. **Class 2 — isolated test databases and test artifacts.** Databases and artifacts dedicated
      exclusively to a testing effort may be created, modified, deleted, and recreated only
      inside a testing scope the Project Owner has explicitly authorized, as a narrow testing
      carve-out — not a blanket destructive-operation permission and not permission for any work
      outside that authorized scope. Before any destructive operation on such a target, the
      target must be verified as genuinely test-only and isolated from the active development
      database, any production database, and any non-test resource; that verification must not
      rest on file naming, on an apparently temporary path, or on a caller's stated intention
      alone, and the operating procedure must prevent accidental fallback to — or operation
      against — the active development or production database. If the target cannot be reliably
      verified as test-only, the operation must not proceed and the Project Owner must be asked.
      Outside the authorized testing scope, rules 4.1–4.3 apply in full. This class describes a
      procedural safeguard — classification, target verification, scope discipline — and asserts
      no technical isolation guarantee: the current architecture provides procedural controls,
      and no clause of this ADR claims otherwise. Consistent with 4.2, source-level changes to
      schema-definition files (including the consolidated fresh-install baseline
      `src-tauri/src/db/migrations/001_initial.sql`) are ordinary repository edits that do not,
      in themselves, authorize any destructive operation on an existing database.

   c. **Class 3 — production databases and valuable backups.** When the project enters production
      and holds valuable data, the standing default is preservation of production databases and
      valuable backups. Creating a new migration and applying a migration to an existing
      database are distinct authorization decisions, each requiring its own explicit Project
      Owner authorization appropriate to the operation (4.2–4.3); neither an ADR's acceptance, a
      passing test suite, nor the existence of a migration mechanism authorizes applying a
      migration to an existing database. Nothing in this ADR authorizes recreating, resetting,
      or replacing a production database, introducing a migration under the guise of ordinary
      development cleanup, or reinterpreting any of its clauses as permission to recreate a
      production database. Under the fresh-baseline mechanism recorded in ADR-0064 §8.1, editing
      `src-tauri/src/db/migrations/001_initial.sql` changes only a fresh database: a database
      whose version-gated migration mechanism has already applied schema version 1 does not
      re-execute the baseline and is not affected by later baseline edits.

   d. Together with the runtime-operation distinction in clause 5.5, these classes are mutually
      exclusive: a permission stated for one class does not extend to another, and no combination
      of development-stage facts, resource classifications, or document statuses transfers
      authority between classes.

7. **Application-managed backup rotation.** The application's runtime backup path applies its
   own retention policy to backup artifacts inside the backup directory adjacent to the active
   database file — automatic rotation beyond a fixed maximum count (`MAX_BACKUP_COUNT = 7`,
   `src-tauri/src/infrastructure/backup/sqlite_backup_adapter.rs`). The reservations in 4.1 and
   4.6 govern direct destructive actions performed outside the application by contributors,
   agents, or automation; they do not modify, suspend, or imply a change to that
   application-managed rotation behavior, and no provision of this ADR asserts that the
   reservation has already changed the application's behavior. Any change to the application's
   retention or rotation behavior is a separate source change requiring its own authorization.

## 5. Relationship with ADR-0064 (D18 and §8.1)

1. The authoritative existing mechanism for the development-database / fresh-baseline posture is:
   * ADR-0064, `## 8. (D18) Schema-change posture` — `docs/architecture/0064-unit-export-state-and-owned-data-deletion.md`, line 1416, including "No migration is created for development-database compatibility" (line 1429);
   * ADR-0064, `### 8.1 Migration mechanism and its compatibility consequence (D18, G31)` — same file, line 1431, including its three normative consequences (an edited baseline reaches **only a fresh database**, lines 1442-1445; the initial schema is applied **exactly once per database**, lines 1446-1447; the no-deployment fact is load-bearing, lines 1448-1452).
2. Those provisions are treated as the authoritative mechanism. This ADR **does not duplicate,
   amend, reinterpret, or broaden** their mechanism, and D18 remains an owner-ratified decision as
   recorded there (ADR-0064:1455-1456).
3. Accepting development-database **recreation as an architectural option** (as ADR-0064 §8.1 and
   the Q2/Q3 RFC §5.1/§6 discuss) does **not authorize any individual recreation operation**. Each
   such operation is governed by clause 4 of this policy: separate, explicit Project Owner
   authorization for that operation, a verified target, a defined scope, and an appropriate
   operational safety procedure.
4. Conversely, clause 4 imposes **no new approval pathway on non-destructive work**: ordinary
   repository changes continue to flow through `ARCHITECTURE_FREEZE.md` §4, which remains
   untouched by this ADR.
5. **Runtime destructive operations remain governed by ADR-0064.** Row-level destructive runtime
   operations — including Unit deletion through the application's authorized runtime workflow by
   an `admin` on a `WILAYA` node — remain governed by ADR-0064 and its existing operational
   authorization controls. Clauses 4.1–4.3 add no separate Project Owner approval layer for such
   governed runtime operations: a row-level operation inside the database is not the
   file/artifact-level destructive operation that 4.2 reserves. Conversely, permission to perform
   a governed runtime operation under ADR-0064 is not permission to delete, recreate, or
   otherwise destructively modify database files, `-wal`/`-shm` sidecar files, backups, or any
   other artifact listed in 4.4. Row-level runtime operations and database-file lifecycle
   operations remain distinct classes under this policy.

## 6. Preservation of existing governance

This policy does **not** waive or weaken any of the following, each of which remains in full force:

* `docs/architecture/ARCHITECTURE_FREEZE.md` requirements — frozen obligations (§2) and the RFC-to-
  ADR process (§4), including Step 2 review criteria ("Is the migration plan complete and
  testable?", "Is the rollback plan safe?" — §4 Step 2) and the Step 4 implementation gate.
* Applicable RFC review criteria, including migration-plan completeness and testability and
  rollback safety. **This draft records no review outcome**: it does not state or imply that any
  review criterion has been satisfied, passed, or closed for any proposed change, and in particular
  it makes no finding about the Q2/Q3 RFC's B4 (migration plan) or B5 (rollback plan) rows.
* ADR approval and ratification requirements (`docs/architecture/ADR_INDEX.md` §Policy).
* Separate implementation authorization — ratification of any ADR records a decision and does not,
  by itself, authorize SQL, schema, migration, source, test, or operational change (the boundary
  recorded in ADR-0065 Status and §Governance-9, preserved unchanged).
* Required testing and backward-compatibility verification obligations.
* Accounting correctness, data ownership, integrity, audit-trail, and applicable retention
  constraints.

## 7. Status and approval boundary

This ADR is **Accepted** (2026-10-23) by the Project Owner (`1Bxm`); the acceptance record is
`docs/architecture/governance_review.md` §7.7. Acceptance records the governance decision only:
it authorizes no implementation, no destructive operation, and no lifecycle-stage transition,
changes no other document's status, and leaves clause 4's requirement of a separate, explicit
Project Owner authorization fully in force for every future destructive operation.

# Consequences (draft)

* **Declarative only.** This draft changes no code, no schema, no migration, no test, and no
  runtime behavior, and performs no database operation. Its creation is itself a documentation
  change: it adds this single draft governance/architecture document and modifies no other file.
  Its subject matter is governance policy.
* **No other file is created or modified by this ADR.** Registration in `ADR_INDEX.md` was
  performed by the acceptance act (`governance_review.md` §7.7). A cross-reference line in
  `ARCHITECTURE_FREEZE.md` §2.8, and any clarifying clause in ADR-0065 or the Q2/Q3 RFC, are
  **separate, future, owner-authorized steps**, not part of this draft.
* **Future transitions.** Any lifecycle transition or any exception to the classification in
  clause 3.1 will be declared and recorded by the Project Owner through the applicable governance
  process — not inferred from runtime events.
* **Effect on planning language.** Where planning documents describe development-database
  recreation as a planned step, clause 5.3 supplies the standing boundary that the description of a
  step is not an authorization for it. No such planning document is modified by this draft.

# Explicitly out of scope

* Approval, ratification, or execution of Q2/Q3 (ADR-0065 — **Accepted (2026-10-23)**, as
  recorded in `governance_review.md` §7.5; that acceptance is an independent governance event
  recorded elsewhere and does not change this document's status); any decision on P3 or its
  removal path (ADR-0065 §Governance-6(b) remains in force and is not touched here); any RFC's
  approval; any implementation phase.
* Authorization of **any individual destructive operation**. Clause 4 states how such authorization
  must be sought and what it must satisfy; it grants none, and it authorizes no deletion,
  recreation, reset, or cleanup of any artifact.
* Amendment of ADR-0064 (D18/§8.1), ADR-0065, the Q2/Q3 RFC, `governance_review.md`,
  `ARCHITECTURE_FREEZE.md`, or `ADR_INDEX.md` by this document.
* The detailed content of any future rollback or recreation procedure — including the restore-path
  handling decision that ADR-0065 §Governance-11 reserves — which remains owned by the applicable
  documents and their processes.
* Any claim about a lifecycle transition, a production readiness state, or a deployment decision.

# References

| Document | Location |
|----------|----------|
| ADR-0064 (D18 schema-change posture; §8.1 migration mechanism) | `docs/architecture/0064-unit-export-state-and-owned-data-deletion.md` — `## 8. (D18) Schema-change posture` (line 1416), `### 8.1 Migration mechanism and its compatibility consequence (D18, G31)` (line 1431), `## Data loss (explicit and bounded)` (line 1460) |
| Architecture freeze declaration (**unmodified** by this ADR) | `docs/architecture/ARCHITECTURE_FREEZE.md` — §2.8 Production Safety (line 184, A44-06 at line 189), §4 RFC-to-ADR Process (line 240), Step 2 Review (line 254), Step 4 Implement (line 274) |
| ADR index (registration performed by the acceptance act, `governance_review.md` §7.7) | `docs/architecture/ADR_INDEX.md` — §Policy (line 114; statuses at line 118 — post-acceptance positions) |
| ADR-0065 (Q2/Q3 — **Accepted (2026-10-23)**, as recorded in `governance_review.md` §7.5; touched by this ADR only by reference) | `docs/architecture/0065-q2-q3-database-integrity.md` — `# Status` (line 3), ratification-boundary note (lines 15-20), §Governance-9 separate-ratification-from-implementation clause (lines 662-667), §Governance-6(b) P3-removal gate (line 535), `# Proposed implementation boundary` (line 706) — verified at HEAD `987229a2e69eeb14b4f36d06adb70602899be52b` |
| Q2/Q3 RFC (approved 2026-10-22 by the Project Owner, recorded in `governance_review.md` §7.1 — referenced, not modified by this ADR) | `docs/architecture/rfcs/2026-10-21-q2-q3-database-integrity.md` — §5 Migration Plan (line 348), §6 Rollback Plan (line 423) |
| Architecture governance review records | `docs/architecture/governance_review.md` — §4 Audit Trail Integrity (line 139); §7.5 ADR-0065 acceptance of record (2026-10-23); §7.7 ADR-0066 acceptance of record (2026-10-23) |
| Domain lifecycle ADRs (distinct scope) | `docs/architecture/0063-unit-operator-credential-lifecycle.md`; `docs/architecture/0064-unit-export-state-and-owned-data-deletion.md` (UNIT lifecycle projection and destructive purge) |

All line references were re-verified against the working tree at HEAD
`987229a2e69eeb14b4f36d06adb70602899be52b` (branch `feat/q2-q3-database-integrity`,
2026-10-23). Provenance note: this ADR was first composed on 2026-10-22 against the working
tree at HEAD `712d4b332702ee7b3ac3bc715c03a121bed32850`, when ADR-0065 was still **`Proposed`**
and its `# Proposed implementation boundary` heading stood at line 467. On 2026-10-23, ADR-0065
was amended and accepted, shifting its line layout — that heading now stands at line 706 — and
the ADR-0065 line references in the table above were corrected to the current revision. A
cumulative-diff check over the revision range `712d4b33..987229a` (read-only) shows ADR-0064,
`ARCHITECTURE_FREEZE.md`, and the Q2/Q3 RFC unchanged, so their line references are the same as
at drafting; `ADR_INDEX.md` and `governance_review.md` were modified in that range, and their
cited line references were re-verified against current content. ADR-0066's own acceptance act
(2026-10-23) added row `0066` to `ADR_INDEX.md` above §Policy, shifting that section from lines
113/117 to 114/118; the index row above cites the post-acceptance positions.
