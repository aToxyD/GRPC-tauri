# Governance Review — GRPC-Tauri

A comprehensive architectural governance review of the GRPC-Tauri system (Algerian Civil Protection food-service management).

---

## 1. Enforcement Mechanisms

### 1.1 `check_arch.ts` — Architectural Integrity Audit

- **What is guaranteed:** 122+ rules across 22 groups enforcing clean architecture boundaries, layer isolation, SQL confinement, authorization discipline, determinism, and production safety. Every rule is either `error` (hard block) or `warning` (zero-warning policy applies).
- **How it is enforced:** A Bun script (`scripts/check_arch.ts`) runs regex-based static analysis against all `.rs`, `.ts`, `.svelte`, `.sql`, and `.md` files. Exit code is non-zero if any error or warning fires. Runs in CI as `architectural_governance` stage, also via `bun run check:arch` locally.
- **Where to look for violations:** Run `bun run check:arch` from project root. CI pipeline logs. Specific groups:
  - **Group 1** (Rules 1-2, 7): SQL boundary — SQL strings in services/commands/application
  - **Group 2** (Rule 8): Domain dependency — domain importing infrastructure
  - **Group 3** (Rules 11-12, 20-21): Authorization — raw role checks, deprecated `get_effective_unit_id`
  - **Group 4** (Rules 5-6, 14, 16-17): Service orchestration — non-transactional audit, error swallowing, repository bypass
  - **Group 5** (Rules 15, 18, 28-29): Sync protocol — CSV refs, plaintext staging, buffered crypto
  - **Group 7** (Rule 19): Information leakage — unguarded `details` interpolation
  - **Groups 8** (Rules 31-32, 36-39): Memory safety, overclaims, terminology, crypto, session hygiene
  - **Group 9** (Rules 33-35): Layer direction — cross-layer imports
  - **Groups 11-14** (Rules 49-68): Reporting/oversight/benchmarks/anomalies — SQL, mutations, transactions, wall-clock, repository imports
  - **Group 15** (Rules 69-73): Audit schema evolution — destructive migrations, missing structured columns, direct INSERT bypass, ordering instability
  - **Groups 16-22** (Rules 74-122): Cache, sync_integrity, sync import, SQLite observability, SQLite runtime, SQLite runtime review — layered enforcement of determinism, no threads, no async, no auto-repair

### 1.2 Clippy — Rust Linting

- **What is guaranteed:** Zero clippy warnings at `-D warnings` level. The `lib.rs` header includes `#![deny(clippy::todo)]` and `#![warn(clippy::complexity)]`.
- **How it is enforced:** CI runs `cargo clippy -- -D warnings` in the `backend_governance` stage. Local pre-push hook also runs it.
- **Where to look for violations:** `cargo clippy -- -D warnings` from `src-tauri/`. CI logs.

### 1.3 `cargo test` — Unit & Integration Tests

- **What is guaranteed:** All tests pass. Tests cover domain logic (FIFO, fiscal year lifecycle, invariants), service orchestration (audit atomicity, sync import, integrity), reporting reproducibility, security, and runtime behavior.
- **How it is enforced:** CI runs `cargo test --verbose` in the `backend_governance` stage.
- **Where to look for violations:** `cargo test` from `src-tauri/`. CI logs. Key test files:
  - `tests/fifo_tests.rs`, `tests/fifo_carry_forward_tests.rs`, `tests/fifo_meal_level_cost_tests.rs`
  - `tests/fiscal_lifecycle_tests.rs`, `tests/fiscal_year_e2e_tests.rs`, `tests/fiscal_integrity_tests.rs`
  - `tests/reporting_reproducibility_tests.rs`, `tests/recovery_determinism_tests.rs`
  - `tests/audit_chain_hostile_tests.rs`, `tests/audit_tx_atomicity_tests.rs`
  - `tests/sync_idempotency_tests.rs`, `tests/daily_report_package_idempotency_tests.rs`
  - `tests/operational_survivability_tests.rs`, `tests/disaster_recovery_integration_tests.rs`

### 1.4 `scripts/run_ci.ts` — Local CI Gate (subset)

- **What is guaranteed:** Local sequential gate: `check:obs` → `check:arch` → `svelte-check` → `vitest` → `cargo build` → `playwright test:e2e`. Any failure aborts the run.
- **Scope:** A developer-facing convenience subset. The authoritative full gate is the GitHub Actions pipeline (`.github/workflows/ci.yml`), which additionally enforces `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, secrets scanning, dependency audits, documentation governance, and release integrity. See `docs/technical/ci_governance_audit.md`.
- **How it is enforced:** Run via `bun run ci` locally. The GitHub Actions pipeline runs the full canonical gate set.
- **Where to look for violations:** Output of `bun run ci`. CI pipeline logs.

### 1.5 Compile-Time Guards (`architecture.rs`)

- **What is guaranteed:** Marker traits `Repository` and `Service` ensure that repositories and services are structurally typed. Zero-cost guards `RepoGuard<R>` and `ServiceGuard<S>` enforce at compile time that a type implements the correct trait.
- **How it is enforced:** Rust compiler. If a repository struct does not `impl Repository`, compilation fails.
- **Where to look for violations:** `src-tauri/src/architecture.rs:15`, `src-tauri/src/architecture.rs:19`.

### 1.6 Domain Invariants

- **What is guaranteed:** Four invariant categories — `StockNonNegative`, `FifoOrderStable`, `OriginImmutable`, `SingleOpenFiscalYear`, `AuditChainIntegrity` — run as pure checks that never mutate state.
- **How it is enforced:** Via the `Invariant` trait (`src-tauri/src/domain/invariants/mod.rs`). Each invariant is checked independently; all violations are collected.
- **Where to look for violations:** `src-tauri/src/domain/invariants/stock.rs`, `fifo.rs`, `fiscal.rs`, `audit.rs`.

---

## 2. Determinism Guarantees

### 2.1 Reporting

- **What is guaranteed:** Any report where `is_reproducible() == true` produces byte-identical `data` JSON on identical database state. `computed_at` metadata is excluded from the assertion.
- **How it is enforced:** Integration tests (`tests/reporting_reproducibility_tests.rs`) create two independent empty databases and assert byte-identical output. Rules 49-52 in `check_arch.ts` enforce that reporting is read-only (no SQL mutations, no transaction ownership, no repository mutations). Rules 77-80 ban `Utc::now`, TTL, filesystem, and rusqlite in the cache layer.
- **Where to look for violations:** `tests/reporting_reproducibility_tests.rs`. Run `cargo test reporting_reproducibility`. Any `ORDER BY` without an `id` tiebreaker column. Any `serde_json::to_string` call that might produce non-deterministic field ordering.

### 2.2 Sync Packages

- **What is guaranteed:** Canonical JSON serialization (ADR-0009, ADR-0005) ensures that the same input data and the same encryption key produce bit-identical encrypted packages. SHA-256 package hashes are deterministic.
- **How it is enforced:** `canonical_json.rs` in the sync infrastructure enforces key ordering. Rules 15, 18 ban CSV legacy. Rules 28-29 ban plaintext staging and buffered crypto. Rules 96, 82-87 ban wall-clock and enforce ordering in sync paths.
- **Where to look for violations:** `src-tauri/src/infrastructure/sync/packages/canonical_json.rs`. Any serde round-trip in sync that does not use deterministic serialization.

### 2.3 Cache Invalidation

- **What is guaranteed:** Invalidation is deterministic — given the same `DomainEvent` and same `is_year_closed` state, the set of invalidated cache keys is identical. No TTL, no "clear all", no wall-clock dependency.
- **How it is enforced:** Rule 78 bans TTL/expiry logic. Rule 77 bans `Utc::now` in reporting. Tests in `reporting_reproducibility_tests.rs` assert cache key determinism.
- **Where to look for violations:** `src-tauri/src/application/reporting/cache/invalidation.rs`. Any event-to-slug mapping that is not purely semantic.

### 2.4 Domain Event Ordering

- **What is guaranteed:** Domain event ordering is deterministic — authoritative order is `(transaction_id ASC, sequence_number ASC)`, not `created_at`.
- **How it is enforced:** Rule 47 bans `ORDER BY created_at` on the `domain_events` table without an `[arch:allow-created-at]` tag.
- **Where to look for violations:** Any SQL query ordering domain events by `created_at` alone.

### 2.5 Audit Log Ordering

- **What is guaranteed:** Paginated audit queries use `ORDER BY timestamp ASC, id ASC` — the `id` tiebreaker ensures deterministic ordering even for sub-second operations.
- **How it is enforced:** Rule 72 checks that `ORDER BY timestamp` in audit queries includes an `id` tiebreaker. Legacy OFFSET-based methods (`fetch_entries`, `fetch_entries_iter`) are grandfathered.
- **Where to look for violations:** `src-tauri/src/repositories/audit.rs` — any query with `ORDER BY timestamp` without `, id ASC`.

### 2.6 SQLite Runtime & Observability

- **What is guaranteed:** All evaluations in `sqlite_runtime/`, `sqlite_runtime_review/`, and `sqlite_observability/` are pure functions — no connections opened, no state mutated, no wall-clock, no threads, no async.
- **How it is enforced:** Rules 97-122 ban business logic, repository imports, mutations, threads, async, loops, checkpoints, second connections, wall-clock, filesystem operations, and auto-repair in these layers.
- **Where to look for violations:** Any `Utc::now`, `std::thread`, `async fn`, `loop`, `Connection::open`, `VACUUM`, `repair`, `-wal`, or business domain terminology inside these directories.

---

## 3. Reproducibility Contracts

### 3.1 Report Reproducibility

- **Contract:** A report is reproducible if:
  1. Reads only from the database via `DbExecutor` — no network, no filesystem, no external APIs
  2. Uses no wall-clock time in computation (`computed_at` in metadata is excluded from assertion)
  3. Uses no random values — no `Uuid::new_v4()`, no random sampling
  4. Uses deterministic ordering — all SQL queries have explicit `ORDER BY` with tiebreaker columns
  5. Uses round-half-to-even for monetary rounding via `round_money()`
  6. Serializes identically — same serde representation for identical data
- **Enforcement:** `check_arch.ts` Rules 49-52 (reporting layer). `round_money()` tests. Serde round-trip tests (`test_report_output_is_serializable`). Integration test suite in `tests/reporting_reproducibility_tests.rs`.
- **Violation sites:** Any report computation that calls `Utc::now()`, `SystemTime::now()`, `rand::random()`, `Uuid::new_v4()`, or uses `ORDER BY` without tiebreaker.

### 3.2 Sync Package Reproducibility

- **Contract:** The same data inputs + same encryption key = identical encrypted output bytes. Canonical JSON serialization enforces stable key ordering.
- **Enforcement:** Rules 15, 18 (CSV ban). Rule 28 (plaintext SQLite suffix ban). `canonical_json.rs` implementation. ADR-0009.
- **Violation sites:** Any use of `serde_json::to_string` without canonical key ordering in sync paths. Any non-deterministic encryption (e.g., randomized nonce in age without deterministic seed).

### 3.3 Replay Detection Reproducibility

- **Contract:** Same `AppliedPackages`, `AppliedTransitions`, `SeenTransactions` sets + same incoming `package_id` = same `ConflictDetectionOutcome` every time.
- **Enforcement:** Rules 81-87 (sync_integrity). Rule 82 bans `Utc::now`. Use of `BTreeSet` for deterministic iteration. SHA-256 for conflict IDs (no randomness).
- **Violation sites:** Any use of `HashSet` (non-deterministic iteration), any randomness in conflict ID computation, any wall-clock dependency.

### 3.4 Benchmark & Anomaly Reproducibility

- **Contract:** All KPIs in oversight/ compute from report outputs only. Benchmarks and anomaly detectors are pure functions over distributions.
- **Enforcement:** Rules 53-57 (oversight), 58-62 (benchmarks), 63-68 (anomalies). No SQL, no mutations, no transactions, no repository imports, no rusqlite, no wall-clock.
- **Violation sites:** Any SQL string, `Utc::now`, or repository import in `oversight/`.

---

## 4. Audit Trail Integrity

### 4.1 Dual-Write Model

- **What is guaranteed:** Every audit write populates three representations simultaneously: legacy flat columns, structured columns (`event_type`, `actor_id`, `target_type`, `target_id`, `before_snapshot`, `after_snapshot`, `node_id`), and a `details` JSON object.
- **How it is enforced:** Rule 70 checks that every `INSERT INTO audit_log` includes `event_type`. Rule 71 bans direct `audit_log` INSERT outside `AuditRepository`. Rule 73 bans UPDATE/DELETE on `audit_log` outside the explicit cleanup path.
- **Where to look for violations:** `src-tauri/src/repositories/audit.rs` — any INSERT missing `event_type`. Any file outside `audit.rs` that directly touches `audit_log`.

### 4.2 Hash Chain Integrity

- **What is guaranteed:** Audit entries are linked cryptographically. `compute_entry_hash()` hashes flat legacy fields (backward-compatible). `verify_audit_chain_streaming()` verifies chain integrity via `ORDER BY rowid ASC`.
- **How it is enforced:** Integration tests (`tests/audit_chain_hostile_tests.rs`, `tests/audit_tx_atomicity_tests.rs`). Domain invariant `AuditChainIntegrity`.
- **Where to look for violations:** `src-tauri/src/domain/invariants/audit.rs`. Any modification to `compute_entry_hash()` that changes the hash domain. Any disconnection in the hash chain detected by the integrity service.

### 4.3 Schema Evolution

- **What is guaranteed:** Audit schema migrations are additive-only. No destructive DDL (ALTER TABLE DROP COLUMN, DROP TABLE audit_log).
- **How it is enforced:** Rule 69 scans migration SQL files for destructive patterns.
- **Where to look for violations:** Migration files in `src-tauri/src/db/migrations/` with DROP or destructive ALTER.

### 4.4 Atomicity

- **What is guaranteed:** Audit writes within a fiscal operation are transactional. A closure either completes fully (update year status + snapshot + FIFO reclassification + audit log) or rolls back entirely.
- **How it is enforced:** Rule 5 ("Non-transactional audit in services") checks that services do not call `AuditLogger::log_success/log_failure` directly. Only `FiscalClosingService` and `SyncImportExecutionService` own transactions. The `AuditService` wraps audit writes in the caller's transaction context.
- **Where to look for violations:** Any `AuditLogger::log_success()` call in a service without an owning transaction. Any service calling `with_transaction()` without being `FiscalClosingService` or `SyncImportExecutionService`.

---

## 5. Fiscal Consistency

### 5.1 Year Semantics

- **What is guaranteed:** Fiscal year is an explicit domain concept, not derived from dates. `stock_movements.fiscal_year`, `daily_reports.fiscal_year` are populated at insert time from `input.date.year()`. `settings.current_year` tracks the open year.
- **How it is enforced:** Rule 6 (domain rules over implicit behavior) in AGENTS.md. The `FiscalYearStatusRepository`, `fiscal_year_status` table, and `settings.current_year` are the authoritative sources. The system rejects any fiscal decision derived purely from `Utc::now()`.
- **Where to look for violations:** Any code computing fiscal year from `Utc::now().year()` instead of reading `settings.current_year`. Any code that would allow operating on closed years.

### 5.2 Year Closure

- **What is guaranteed:** Closing a fiscal year atomically: (1) updates `fiscal_year_status`, (2) creates a fiscal snapshot, (3) reclassifies remaining FIFO `ORDER` layers to `OPENING`, (4) records audit trail. All-or-nothing via `with_event_persistence()`.
- **How it is enforced:** `src-tauri/src/application/services/fiscal_closing_service.rs`. Integration tests `tests/fiscal_lifecycle_tests.rs`, `tests/fiscal_year_e2e_tests.rs`.
- **Where to look for violations:** Any partial closure path. Any closure that does not reclassify FIFO layers. Any closure missing audit entries.

### 5.3 FIFO Integrity

- **What is guaranteed:** FIFO layers for closed years are immutable after year closure. `source_type` reclassification is the only mutation at year boundary. No deletion, no copy, no data movement.
- **How it is enforced:** `src-tauri/src/domain/invariants/fifo.rs`, `src-tauri/src/domain/invariants/fiscal.rs`. Rules in `check_arch.ts` against FIFO mutation outside the service layer. Integration tests `tests/fifo_carry_forward_tests.rs`, `tests/fifo_meal_cost_integrity_tests.rs`.
- **Where to look for violations:** Any code that deletes, moves, or duplicates FIFO layers across year boundaries. Any code that modifies `origin_fiscal_year` after creation.

### 5.4 Historical Immutability

- **What is guaranteed:** Historical reports (those referencing a closed fiscal year) remain reproducible indefinitely. Closed fiscal years receive no new mutations. Cache entries for closed fiscal years are never invalidated.
- **How it is enforced:** Cache invalidation logic preserves closed-year entries unconditionally. Rules 49-52 enforce read-only reporting. Integration tests verify that closed-year data produce consistent reports across upgrades.
- **Where to look for violations:** Any invalidation path that touches closed-year cache entries. Any mutation path that writes to closed-year data tables.

### 5.5 Fiscal Transition Retention

- **What is guaranteed:** Fiscal transitions are all-or-nothing. A transition ID is recorded in `AppliedTransitions` once applied. Any subsequent import referencing the same transition ID is rejected as replay.
- **How it is enforced:** Replay detection in `validation.rs` (Rule 83-84). Tests `tests/fiscal_transition_retention_tests.rs`.
- **Where to look for violations:** Any partial fiscal transition application. Any transition that can be applied twice.

---

## 6. Architecture Boundary Rules

### 6.1 Layer Isolation

| Direction | Rule | Enforcement |
|-----------|------|-------------|
| `commands/` → `repositories/` | Forbidden | Rule 14, 16 — direct repository use/import in commands |
| `application/services/` → SQL | Forbidden | Rule 1 — SQL strings in services |
| `domain/` → `infrastructure/` | Forbidden | Rule 8 — domain imports of infrastructure |
| `application/` → `app/` | Forbidden | Rule 33 — application imports of app/state |
| `domain/` → `application/` | Forbidden | Rule 34 — domain imports of application |
| `infrastructure/` → `commands/` | Forbidden | Rule 35 — infrastructure imports of commands |
| `application/reporting/` → mutations | Forbidden | Rules 49-52 — read-only reporting |
| `application/oversight/` → SQL/rusqlite | Forbidden | Rules 53-57 |
| `application/oversight/benchmarks/` → SQL/rusqlite | Forbidden | Rules 58-62 |
| `application/oversight/anomalies/` → SQL/rusqlite | Forbidden | Rules 63-68 |
| `application/reporting/cache/` → SQL/repos | Forbidden | Rules 74-76, 80 |
| `application/sync_integrity/` → SQL/Utc::now | Forbidden | Rules 81-82 |
| `infrastructure/sqlite_observability/` → business logic | Forbidden | Rules 97-105 |
| `infrastructure/sqlite_runtime/` → threads/repair/Utc::now | Forbidden | Rules 106-115 |
| `infrastructure/sqlite_runtime_review/` → threads/async/mutations | Forbidden | Rules 116-122 |

### 6.2 Frontend Boundaries

| Rule | What it enforces | check_arch.ts Rule |
|------|-----------------|--------------------|
| No `invoke` outside `lib/tauri.ts` | All Tauri API calls through single adapter | Rule 27 |
| No `@tauri-apps/` imports outside `lib/tauri.ts` | No direct plugin access | Rule 27b |
| No `any` types in frontend | Type-safety enforcement | Rule 26 |
| No SQL in Svelte components | No frontend data access | Rule 25 |
| No SQL in frontend adapter files | No frontend DB access | Rule 23 |
| No raw `alert`/`confirm` | Use Tauri dialog wrappers | Rule 43 |
| No manual loading/submitting state | Use `createOperation`/`createOperationGuard` | Rule 40 |
| No untracked timers | Use `createRuntimeScope` | Rule 41, 42, 44 |
| No silent catch blocks | Error handling discipline | Rule 45 |

### 6.3 Authorization Boundaries

- **Fail-closed model:** If authorization fails, session expired, or security resource unresponsive, access is denied (ADR-0018).
- **Frontend never owns authz truth:** Frontend checks are UX-only; backend is sole source of truth.
- **Enforcement:** Rules 11-12 ban direct role/node_type comparisons in commands. Rule 20 restricts `require_authenticated` to guards.rs only. Rule 21 bans deprecated `get_effective_unit_id`. Password verification is isolated to `auth.rs` (Rule 13).

### 6.4 Production Safety

- **Information leakage gate (ADR-0012):** Rule 19 bans unguarded `details` interpolation in error strings outside `#[cfg(debug_assertions)]` blocks.
- **Fake security claims:** Rule 36 bans "military grade", "unbreakable", etc. Rule 32 bans fake architectural claims ("O(1)", "zero-copy", "true streaming"). [arch:allow-overclaim]
- **Deprecated terminology:** Rule 37 bans legacy extensions (.bss, .bssync, BSS, grpcsync, etc.). [arch:allow-history]
- **Crypto discipline:** Rule 38 bans age::scrypt (must use age::x25519). Rule 29 bans `read_to_end` in domain/security.rs.

---

## 7. RFC Review Records

### 7.1 2026-10-22 — Q2/Q3 Database Integrity RFC — Freeze §4 Steps 2–3 review and RFC approval

* **RFC reviewed:** `docs/architecture/rfcs/2026-10-21-q2-q3-database-integrity.md`
  (working-tree state SHA-256 `b23c3cf993b480fc1faacfc7a2261e5c94624d72dff30e555521a8ab13d4ae08` — the reviewed
  modifications are uncommitted; uncommitted work remains owner-controlled on
  `feat/q2-q3-database-integrity`).
* **Companion ADR:** `docs/architecture/0065-q2-q3-database-integrity.md` — status **`Proposed` (unchanged by this record)**.
* **Review performed:** the Freeze §4 Step-2 assessment and an RFC approval-readiness review were completed through
  the project's review process; no material contradiction to the Step-2 determination was found. The three
  reviewer-role perspectives are waived for this RFC only by recorded decision G10
  (`0065`, decision table `G10`); the five general Step-2 criteria remained in force. Conversation-derived
  assessments summarized above are context, not independently verifiable repository artifacts; the repository
  facts cited here are the file paths and decisions of record in this entry.
* **Freeze §4 Step 2 — owner determination of record:** **positive at design/plan level.** All pre-implementation
  conditions and independent gates remain open.
* **Freeze §4 Step 3 entry condition — owner decision of record:** the Project Owner **approves the Q2/Q3
  database-integrity RFC**.
* **Approving authority:** Project Owner (`1Bxm`).
* **Scope limitations of this record:**
  * Approval records the **decision only**. It does **not** authorize implementation. Every implementation
    condition in RFC §5.2/§5.3 and Freeze §4 Steps 4–5 requires its own separately granted authorization.
  * ADR-0065 remains **`Proposed`**. Ratification — including the approving-authority/date record in the ADR of
    record per its own approval convention (`0065:79-87`), any status transition, and the corresponding
    `ADR_INDEX.md` row update — remains a **separate, explicitly authorized** governance act. This entry is a
    review record, **not** the primary decision record (`0065:22-27`).
  * ADR-0066 remains an **independent** `Proposed` proposal; this approval does not affect it.
  * **P3 removal** remains separately gated (`0065` G9/G10; `adr_exception_registry.md` rows 9 & 11).
  * **Database, migration, backup, restore, and destructive operations** remain unperformed and unauthorized;
    each requires operation-specific owner authorization.
* **Wording-reconciliation note:** ADR-0065's pre-approval description of the RFC as an "**unapproved draft**"
  (lines 25 and 84) predates this record and is reconciled only under the separately authorized ADR-0065
  documentation/ratification act.

### 7.3 2026-10-22 — Owner decision of record — legacy development-database backup invalidated; new test cycle uses fresh data

* **Recorded decision (Project Owner `1Bxm`, authorized and recorded 2026-10-22):**
  1. The backup created before deletion of the development-stage database is **invalid for the new
     test cycle**: it must not be used as a data source and is **not** evidence of successful restore
     testing.
  2. The database for the new test cycle will be **initialized afresh** and populated with **new
     test data**.
  3. The obsolete backup must **not** be restored into, or used to seed, the new test database.
  4. This decision **does not waive** integrity checks for newly created backups or future restore
     operations.
  5. No production-data migration, legacy-data recovery, database operation, or code change is
     authorized by this decision.
* **Scope boundaries (of record):** this decision **does not ratify** ADR-0065 (which remains
  `Proposed`; no approval metadata, ratification record, or status change is created or implied by
  this record) and **does not ratify or affect** ADR-0066 (an independent `Proposed` proposal). It
  does **not** authorize Q2/Q3 implementation and **does not** authorize P3 removal. It does
  **not** authorize database creation, deletion, migration, backup, restore, seeding, or any other
  database operation.
* **Explicitly not resolved by this decision:** future restore-failure semantics remain unresolved
  and require a separate owner decision. The `PRAGMA foreign_key_check` policy (Governance-11,
  `0065:631-637`), future failure handling, and the atomic-replacement policy remain open with
  **no prescribed or implied outcome** from this record.

### 7.4 2026-10-23 — Q2/Q3 implementation-precondition decisions of record — TA2–TA7 design, G7 static inventory (Governance-8(c)), and restore-failure policy

* **Recorded decisions (Project Owner `1Bxm`, authorized and recorded 2026-10-23).** This record settles
  the Q2/Q3 pre-implementation design decisions listed below and records the completed G7 static
  inventory. It is **one documentation edit to this file only** (new subsection only; §7.1–§7.3 and all
  other content remain unchanged). It implements nothing, authorizes nothing to be implemented, is
  **not** ADR-0065 ratification (ADR-0065 remains `Proposed`, untouched by this edit), performs **no**
  database operation, and authorizes **no P3 work**.
* **Distinct governance events (of record).** RFC approval (§7.1, 2026-10-22), ADR-0065 ratification
  (a future gate), owner design decisions (this record), this documentation authorization, and
  implementation authorization are **separate** events. None of the former grants implementation
  authorization (`2026-10-21-q2-q3-database-integrity.md` §8.5, `:497-508`; ADR-0065 §Governance-9).

**TA2–TA7 owner decisions (design of record).** Only existing typed variants and their existing mapping
arms are reused — no new error system, variant, or code is introduced: `OperationNotPermitted{message}`
→ `OPERATION_NOT_PERMITTED` (`src-tauri/src/errors/mod.rs:284-286`, `:562-567`); `ResourceNotFound{resource,id}`
→ `BIZ_NOT_FOUND` (`:245-246`, `:519-525`); `DeleteForbidden{resource,reason}` → `BIZ_DELETE` (`:260`, `:538-544`).

* **TA2 — missing `unit_id`:** (1) reject a missing `unit_id` **explicitly in the service path before any
  write**, rather than relying solely on the future database `NOT NULL` constraint; (2) reuse the existing
  `OperationNotPermitted` variant and its established mapping (the UI create path already exhibits exactly
  this rejection — `daily_report_service.rs:76-80`); (3) the future `NOT NULL` constraint (G2) remains a
  defense-in-depth safeguard; (4) required regression evidence: a missing-unit request returns the typed
  error and inserts **no** daily-report row. *Recorded current fact:* the service-level
  `import_daily_reports` `None` branch (`sync_import_execution_service.rs:95-102`) currently writes
  NULL-unit rows; this decision targets it without authorizing the edit.
* **TA3 — unknown unit:** add **explicit service-layer unit-existence validation** using the established
  `ResourceNotFound` pattern (precedent: `import_daily_report_package.rs:56-63`); retain the database FK
  as an **additional** integrity safeguard. Regression evidence: an unknown-unit request fails with the
  intended typed error and creates no relevant report or meal-item rows.
* **TA4 — order-item unit consistency:** for the current API and implementation contract, item-unit
  equality with the parent order is **structurally guaranteed** — all items receive the same single
  `unit_id` supplied for the order (`order_service.rs:250-282`; the item column is written from that same
  parameter, `repositories/orders.rs:93-94`). Do **not** introduce a new item-level unit field or expand
  the API/schema within the approved scope. Tests must verify the persisted invariant (every created item's
  `unit_id` equals its parent order's `unit_id`) and must state explicitly that a negative test with
  independently supplied, mismatching item and parent units is **not constructible** under the current
  API contract. The existing `units(id) RESTRICT` FKs on orders/items (`001_initial.sql:334-336, :358-360`)
  remain.
* **TA5 — explicit unit provenance:** no additional production change is required where existing paths
  already resolve provenance explicitly and reject missing/invalid context (reports: caller argument
  discarded, unit resolved from settings — `commands/reports.rs:149, :163-166` — and `None` fail-closed —
  `daily_report_service.rs:76-80`; orders: `resolve_order_unit` with typed failures for undefined current
  unit and non-UNIT nodes — `commands/orders.rs:38-40, :201-221`). The approved test plan shall include
  negative assertions guarding against silent ambient-context fallback.
* **TA6/TA7 — FIFO layer ownership and product consistency:** the existing **structural protection is the
  approved design** — consumption selects layers by the report unit **and** product
  (`fifo_layers.rs:223-246` `WHERE unit_id = ?1 AND product_id = ?2`), and callers on the covered creation
  path (`daily_report_service.rs:100-102`) never supply arbitrary layer IDs; portions pair-write
  `(product_id, Some(layer_id))` from the same engine portions (`daily_report_service.rs:163-185`). Do
  **not** add duplicate production-level ownership/product assertions under this decision. Tests must
  detect violations of the resulting invariants **without implying** the normal API can necessarily
  construct every invalid state. The distinction between structural application behavior and the Q2-A
  FK's **existence-only** guarantee (RFC `:390`) is preserved. The excluded P3 path
  (`SyncImportExecutionService::import_monthly_report` — `sync_import_execution_service.rs:135-171`,
  raw item writer `repositories/reports.rs:534`; RFC `:419`) remains out of scope.
* **D4/D5 — test design (of record):** use the existing integration-test `Harness`/helper conventions
  where suitable (`src-tauri/tests/common/mod.rs::create_test_state` — per-test temp-file DB over a real
  `Mutex<Option<Connection>>`, preserving single-writer/WAL; `unit_deletion_tests.rs:232-298` seed helpers
  as reusable patterns); use an **independent temporary-file database per test**; rely on Cargo's automatic
  discovery of integration test files — **no `[[test]]` registration change is required** (verified: none
  present in `src-tauri/Cargo.toml`). Any new test or fixture remains subject to the **separate
  implementation authorization** and applicable governance gates.

**G7 static inventory — recorded per Governance-8(c) (`0065:602-615`), static-source evidence only
(anchors re-verified 2026-10-23 against working tree):**

* **Item 1 — current schema and exact index list.** `daily_reports`
  (`src-tauri/src/db/migrations/001_initial.sql:272-284`): `id TEXT PRIMARY KEY`; `date TEXT NOT NULL`;
  **`unit_id TEXT` — nullable, no FK, no NOT NULL, no CHECK on it**; numeric CHECKs; `node_id TEXT`;
  `deleted`; `fiscal_year INTEGER`. Indexes on `daily_reports`: `:773` `idx_daily_reports_date(date)`;
  `:774` `idx_daily_reports_unit_id(unit_id)`; `:775` **`idx_daily_reports_date_unit_unique` —
  `CREATE UNIQUE INDEX … ON daily_reports(date, COALESCE(unit_id, '__NULL__'))`** (expression unique index,
  NULL-guarded via sentinel, no WHERE predicate); `:778` `idx_daily_reports_fiscal_year(fiscal_year)`;
  `:888` `idx_daily_reports_sync ON daily_reports(updated_at, node_id, deleted) WHERE deleted = 0`
  (partial, sync); trigger `trg_daily_reports_updated_at` `:939` (does not touch indexed columns).
  Related child-table constraints structurally outside the change: `daily_report_meals` FK→`daily_reports`
  CASCADE `:301` + `UNIQUE(daily_report_id, meal_type)` `:302`; `daily_report_meal_items` FK `:316`.
  Observation (no change ordered): `idx_daily_report_meal_items_meal_id` is declared twice (`:777`, `:795`
  — the second is a no-op under `IF NOT EXISTS`).
* **Item 2 — consumers of `(date, unit_id)` uniqueness (verified couplings):**
  1. `daily_report_exists_for_date_unit` — `WHERE date = ?1 AND unit_id = ?2` (`repositories/reports.rs:179-190`).
  2. `daily_report_exists_for_date_global` — `WHERE date = ?1 AND unit_id IS NULL` (`:192-199`) — the only
     direct NULL-uniqueness consumer.
  3. `create_daily_report_impl` duplicate gate → fail-closed `ReportAlreadyExists`
     (`daily_report_service.rs:59-80`), then header insert (`:224-233`).
  4. `import_daily_reports` existence branch then plain insert
     (`sync_import_execution_service.rs:95-102`).
  5. `insert_or_replace_raw_daily_report` — `INSERT OR REPLACE` (`repositories/reports.rs:547-568`), sole
     caller the P3-excluded `import_monthly_report` (`sync_import_execution_service.rs:135-171`).
     *Verified absences:* no `ON CONFLICT` clause targets `daily_reports` anywhere in the repository
     (28 hits, all on other tables); no `INSERT OR IGNORE` on `daily_reports`; no `INDEXED BY`/index hints;
     no existing test directly asserts the current unique-index behavior.
     Reads serving the pair (`?3 IS NULL OR unit_id = ?3` monthly list/count and the SEC-057 monthly
     completeness gate — `infrastructure/db/read/reports.rs:146-201`; other list/counts
     `repositories/reports.rs:12, :339-434, :437-450`) filter on `unit_id` equality/NULL only and depend on
     the index solely through that predicate; unit-scoped joins (`inventory.rs:259-441`; deletion preflight
     `units.rs:327, :461-476` with A14 NULL-preservation `:474-476`) and aggregate checks
     (`anomaly.rs:122`; `integrity.rs:104-125`) are not uniqueness-dependent.
* **Item 3 — fresh-baseline-only scope (recorded by citation; no new evidence required).** G7
  (`0065:652`) and G8 (`:653`); ADR-0065 §"Daily-report uniqueness" `:348-367` (replacement is planned
  scope of the **fresh-database baseline schema edit**; equivalence limited to fresh databases; no
  upgrade of initialized databases); Governance-8(c) `:602-615` (fresh-baseline-only; **no** upgrade or
  migration path approved); `:617-621` (`UNIQUE(date, unit_id)` **not** asserted equivalent to the
  expression index where NULL is permitted); Consequences `:676-680`; RFC S9 `:62`, §3.8 `:165-168`,
  §5.1 M1–M3 `:354-360`, §7 `:440-442`, B13 `:842`; single fresh-install migration baseline
  (`src-tauri/src/db/migrations.rs:14-23`); owner fresh-cycle decision of 2026-10-22 (§7.3 of this
  document).
* **NULL semantics supporting items 1–2 (static):** the current schema permits `unit_id` NULL and the
  sentinel index permits **at most one** NULL-unit row per date; per SQLite-documented semantics
  (`lang_createtable` — NULLs considered distinct from all other values, including other NULLs, under both
  PRIMARY KEY and UNIQUE), a plain `UNIQUE(date, unit_id)` would permit unlimited NULL-unit rows per date.
  Therefore the replacement is safe **only** when Q2-A/Q3-A `NOT NULL` and the G7 swap are applied together
  in the same fresh-baseline edit — matching ADR-0065 §"Daily-report uniqueness" item 1 ordering
  (`0065:350-355`).
* **Approved architecture constraints (of record):** G2 and G7 are applied **together as one consistent
  fresh-baseline change**; `daily_reports.unit_id` is **not** made `NOT NULL` independently of the
  intended index replacement; the target conventional unique index on `(date, unit_id)` is implemented
  **only** within the authorized fresh-baseline change; the migration mechanism is not changed and no
  upgrade/compatibility migration is created; existing ordinary indexes are preserved unless a separately
  authorized design decision says otherwise; exact DDL placement and naming remain implementation-owned
  within the approved architecture.
* **Evidence status (explicit non-claims).** Governance-8(c) items 1–2 have completed **static** evidence
  available and are recorded here; item 3 is supported by the cited fresh-baseline/no-upgrade constraints.
  **TC1–TC5 remain unexecuted** — no runtime, test, or build evidence is created by this documentation
  edit; in particular, no claim is made that any TC or the D1 policy has been tested. The G2/G7 baseline
  edit is **not authorized and not performed** by this record. ADR-0065 remains `Proposed` — this edit is
  **not** ratification.

**Restore-failure policy (owner-established 2026-10-23; superseding the §7.3 "explicitly not resolved"
clause by a separate owner decision; §7.3 remains an unchanged historical record).** This is a policy
decision only — **not yet implemented and not runtime-tested**:

1. Run `PRAGMA foreign_key_check` against the **temporary restored database** before atomic replacement.
2. If validation fails or violations are found, **abort replacement**.
3. **Preserve the live database unchanged.**
4. Provide sufficient diagnostics through the existing error path.
5. Do **not** silently repair the restored database.

*Recorded current fact:* the restore adapter validates via `PRAGMA quick_check` only
(`src-tauri/src/infrastructure/backup/sqlite_backup_adapter.rs:167, :177, :668-669`; whole-file atomic swap
`:641, :685+`); `PRAGMA foreign_key_check` is invoked **nowhere** in `src-tauri` (verified). *Governance
linkage (citation only; ADR-0065 unchanged by this record):* Governance-11 (`0065:631-637`), restore-path
roadmap step 5 (`:690-694`), open question 1 (`:763-769`), RFC §8.1 (`:455-463`). The fail-closed
requirement (detect, report, never auto-repair) is fixed; this record supplies the owner's mechanism
decision (abort-before-replacement). TC3 runtime evidence, still outstanding, must demonstrate: pre-swap
validation occurring, abort behavior on violations, preservation of the live database, and useful
diagnostics. **None of these behaviors is tested today.**

**TC5 sequence (recorded constraint).** (1) The accepted Governance-8(c) inventory is recorded **before**
the index edit (this record). (2) The baseline edit combines G2 and G7 consistently. (3) TC5 runtime
verification occurs **only after** that edit, against a **newly initialized** test database. (4) Evidence
must verify the actual index definition and the intended uniqueness invariant. (5) Initialized databases
are **not** upgraded (G8/RFC §5.1/§7).

### 7.5 2026-10-23 — ADR-0065 acceptance of record — status `Proposed` → `Accepted` (owner decision; documentation act only)

* **Decision of record (Project Owner `1Bxm`).** The owner **accepted ADR-0065 on 2026-10-23**;
  approving authority: **Project Owner `1Bxm`** (consistent with the owner's confirmation that this
  matches the actual approval record). The formal status value is **`Accepted`** (repository status
  vocabulary — the Policy §3 chain cited in ADR-0065's governance-state block `0065:11-13`:
  `Draft → Proposed → Accepted → Deprecated → Superseded`); **no `Ratified` status value is
  introduced anywhere**.
* **Scope of the acceptance (documentation act only).** One minimal documentation patch, applied
  under the owner's explicit authorization for this round:
  * `docs/architecture/0065-q2-q3-database-integrity.md` — status line (`Accepted (2026-10-23)`);
    governance-state block (acceptance facts + record pointer; the status-transition convention
    sentence retained verbatim); approval-convention block (the recorded approving authority and
    date); owner-provided-requirements preamble (status fact); Governance-9 item-9 tail
    (acceptance-metadata fact only); "Recording an approval" restricted item (metadata facts; G10
    and §7.1 sentences preserved).
  * `docs/architecture/ADR_INDEX.md` row `0065` — status column `Accepted`, date column
    `2026-10-23`, inline status phrase updated; description otherwise unchanged.
  * `docs/architecture/governance_review.md` — this §7.5.
  Nothing else in any document was modified by this act; no unrelated section of ADR-0065 was
  rewritten.
* **Acceptance does not authorize implementation.** Acceptance records the **governance decision
  only**. ADR-0065's "**Ratification does not authorize implementation.** … No implementation is
  performed, authorized, or begun by this document." block (`0065:15-20`) is preserved verbatim, and
  Governance-9's separate-authorization clauses (`0065:623-626`) are preserved verbatim. At the
  moment of acceptance the owner confirms the standing answer recorded in RFC §8.5/N4
  (`2026-10-21-q2-q3-database-integrity.md` §8.5, `:497-508`, preserved unchanged): **acceptance is
  not implementation authorization**. Every step of §"Proposed implementation boundary" requires a
  **separate, explicit owner authorization**, granted after the applicable governance gates are
  satisfied.
* **Separate governance events (of record).** RFC approval (**§7.1, 2026-10-22**), owner design
  decisions of record (**§7.4, 2026-10-23**), ADR-0065 acceptance (**this record, 2026-10-23**), and
  implementation authorization (**not granted by this act**) remain **separate** events. §7.1 is
  neither restated nor reopened by this record; §7.4 is neither amended nor reopened by this
  record.
* **Freeze §4, Step 3, item 4 — owner-confirmed no-op.** The owner confirmed that
  `ARCHITECTURE_FREEZE.md` §4, Step 3, item 4 ("Update this freeze document (Section 2 or 3)") does
  **not require modification** in this case. This is consistent with ADR-0065's own reconciliation
  table (`0065:664`): Freeze §2.6/§2.4 are **"Referenced, unchanged"** and the ADR states
  "`ARCHITECTURE_FREEZE.md` is **not** modified by this ADR". `ARCHITECTURE_FREEZE.md` was not
  modified by this act and remains in its pre-act state.
* **G10 exception (preserved, not expanded).** Per the project-specific exception recorded in
  ADR-0065 (G10, `0065:657`), the three mandatory reviewer-role perspectives of
  `ARCHITECTURE_FREEZE.md` §4, Step 2 are **not** mandatory prerequisites for the Q2/Q3 Database
  Integrity RFC. **No formal three-role review occurred, and this record does not imply that one
  occurred.** ADR-0065's sentence "**No formal review is recorded by that exception itself.**" is
  preserved verbatim in the ADR. The RFC approval recorded in §7.1 (2026-10-22) remains distinct
  from this acceptance; this record neither repeats nor re-litigates it.
* **Standing constraints preserved by this act (post-patch anchors).** ADR-0065: "Ratification does
  not authorize implementation" (`:15-20`); Governance-8(c) fresh-baseline-only verification
  conditions and the absence of any upgrade or migration path for initialized databases
  (`:603-622`); Governance-9 separate-authorization clauses (`:623-626`); Governance-10 deferred
  `monthly_reports.unit_id` foreign key — dedicated follow-up ADR required, the FK does **not**
  exist today (`:629-632`); Governance-11 restore-mechanism decision must be settled **before** any
  restore-path modification (`:633-639`; the §7.4 restore-failure policy remains a **recorded
  policy** — not implemented and not runtime-tested). §7.4 decisions (TA2–TA7, D4/D5, D1) are
  unchanged by this act. P3 — `SyncImportExecutionService::import_monthly_report` and its exclusive
  helper `ReportRepository::insert_or_replace_raw_daily_report` — is **untouched**. **No
  compatibility migration or existing-database upgrade path is created or authorized.**
* **Act boundaries (explicit non-claims).** No source, SQL schema, migration, test, configuration,
  or database file was modified; no automated validation (`check:arch`, `check:docs-governance`,
  release-integrity), build, test, or database operation was executed (automated validation was not
  included in this round's authorization); no staging, commit, reset, or clean was performed.
  **TC1–TC5 remain unexecuted.** Implementation remains **not authorized**. P3 removal and the
  `monthly_reports.unit_id` FK follow-up ADR each remain subject to **separate** owner
  authorizations.

### 7.6 2026-10-23 — Q2/Q3 implementation record of record — separately authorized implementation executed in commit `6566b87` (owner decisions; documentation act only)

* **Recorded record (Project Owner `1Bxm`).** After the acceptance recorded in §7.5, the owner
  granted the **separate, explicit implementation authorization** required by ADR-0065
  §Governance-9 (`0065:651-654`) and §"Proposed implementation boundary" (`0065:695-699`). This
  event **supersedes the momentary** "Implementation remains **not authorized**" state of §7.5
  (`:529-530`) **by a separate owner decision; §7.5 remains an unchanged historical record**
  (precedent: §7.4's supersession of §7.3's "explicitly not resolved" clause).
* **Scope executed (single commit; verified inventory):** **`6566b87b89c8b7367e7d8329a928f462917d0cc1`**
  — branch `feat/q2-q3-database-integrity`, parent `0b93b023ad413517e9e101aad3f9e061c82f9217`,
  one commit, 13 files, +944/−89: baseline schema G1–G8 (`001_initial.sql:275-277, :314-317,
  :337-341, :351-352, :782-785`), service fail-closed unit gates (TA2/TA3/TA5 per §7.4),
  deletion re-ordering A12→A14→A10 (`repositories/units.rs:451, :469, :474`), restore
  `PRAGMA foreign_key_check` gate (§7.4 mechanism — `sqlite_backup_adapter.rs:200, :739`), and
  test additions.
* **No claim beyond this scope:** the P3 path (`SyncImportExecutionService::import_monthly_report`
  / `repositories/reports.rs:534/:547`) was **not touched** and G9 remains gated by
  §Governance-6(b)/(c); **no `monthly_reports.unit_id` foreign key was added** (Governance-10; a
  dedicated follow-up ADR is required before that change); **no existing-database upgrade or
  migration path exists** (§7.4 Item 3 / ADR-0065 G8).
* **Implementation-phase gate outputs (recorded in the phase reports; NOT executed or re-run for
  this record):** full `cargo test` — recorded 109 test suites with 0 failures; `cargo clippy
  -- -D warnings` — zero warnings; `bun run check:arch` — zero warnings; docs-governance,
  secrets, and release-integrity checks — passing, as recorded.
* **Test files in the commit (verifiable from the commit inventory):** five files — four modified
  and one new (`daily_reports_schema_integrity_tests.rs`).
* **Test-case counts per committed file (counted from current source; source counts, not test
  execution totals):** `unit_deletion_tests.rs` 27 — covers ADR-0065 §Verification expectation 6;
  `daily_reports_schema_integrity_tests.rs` 10 — expectations 1–5;
  `disaster_recovery_integration_tests.rs` 5 — expectation 7 and the §7.4 restore-failure policy
  (test `test_restore_aborts_on_foreign_key_violation_before_swap`,
  `disaster_recovery_integration_tests.rs:113-206`); `sync_unit_to_wilaya_import_tests.rs` 24;
  `phase4_integer_boundary_tests.rs` 9. Phase-report case-count notes that do not match a file's
  test-function count are **not** recorded as run numbers here; per-file execution totals are not
  re-derivable without re-running.
* **Documentation effect of this record:** the ADR-0065 amendment (2026-10-23), the
  `CLAIM_VERIFICATION_MATRIX.md` Q2/Q3 rows (per `0065:738-739`), and the `CHANGELOG.md`
  unreleased entry jointly satisfy `ARCHITECTURE_FREEZE.md` §4 Step 5 (`:279-281`).
* **Act boundaries (explicit non-claims):** documentation only — no source, test, schema,
  migration, database, or protected-file change; no test re-run; no staging, commit, reset, or
  clean by this record; no claim that any gate was newly executed by the author of this record.

## Review Summary

| Domain | Guaranteed | Enforced By | Violation Indicators |
|--------|-----------|-------------|---------------------|
| Architecture boundaries | Layer isolation | check_arch.ts (122 rules), compile-time guards | Rule violations, cross-layer imports |
| Determinism | Reports, sync, cache, ordering | Tests, check_arch.ts rules | Utc::now, HashSets, missing ORDER BY |
| Reproducibility | Byte-identical output | Integration tests | Non-deterministic serialization, randomness |
| Audit integrity | Dual-write, hash chain, atomicity | check_arch.ts Rules 69-73, domain invariants | Missing event_type, direct audit INSERT |
| Fiscal consistency | Year semantics, closure, FIFO | Service tests, domain invariants | Partial closure, FIFO mutation |
| Production safety | Error exposure, crypto, threads | check_arch.ts Rules 19, 36-39, 101+ | Leaky error details, banned crypto |

---

## Recommended Review Cadence

- **Pre-merge:** Full `check_arch.ts` run, `cargo clippy -D warnings`, `cargo test`, `bun run ci`
- **Weekly:** Governance spot-check of recent changes against check_arch.ts rules
- **Per release:** Complete governance review as documented in this file, signed off by lead architect
