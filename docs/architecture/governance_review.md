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

### 1.4 `scripts/run_ci.ts` — Comprehensive Governance CI Gate

- **What is guaranteed:** Full sequential gate: `check:arch` → `svelte-check` → `vitest` → `cargo build` → `playwright test:e2e`. Any failure aborts the pipeline.
- **How it is enforced:** Run via `bun run ci` locally. CI mirrors the same stages.
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
