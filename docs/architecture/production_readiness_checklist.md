# Production Readiness Checklist — GRPC-Tauri

دراسة الجاهزية الإنتاجية لنظام GRPC-Tauri

**Project:** GRPC-Tauri — Algerian Civil Protection food-service management  
**Architecture:** Offline-first, accounting-grade operational governance  
**Stack:** Tauri 2.x (Rust 1.94) + Svelte 5 + SQLite WAL (single-writer)  
**Last audit:** 2026-05-29 (Phase 6.C + Phase 7 + Phase 8 Architectural Debt Burn-down complete)

---

## Instructions

Each item must be verified before sign-off. Mark as ✅ PASS or ❌ FAIL.  
If FAIL, link to the blocking issue and do not proceed to production deployment.

---

## 1. All Tests Pass (Unit & Integration)

- [x] **`cargo test --verbose` exits 0** in `src-tauri/`
- [x] All domain tests pass:
  - `tests/fifo_tests.rs` — FIFO layer integrity
  - `tests/fifo_carry_forward_tests.rs` — Year-boundary FIFO behavior
  - `tests/fifo_meal_cost_integrity_tests.rs` — Meal cost engine with FIFO
  - `tests/fifo_meal_level_cost_tests.rs` — Per-meal cost allocation
  - `tests/fifo_meal_order_independence_tests.rs` — Order independence
  - `tests/fiscal_lifecycle_tests.rs` — Fiscal year open/close/archive
  - `tests/fiscal_year_e2e_tests.rs` — End-to-end fiscal year
  - `tests/fiscal_integrity_tests.rs` — Fiscal invariant enforcement
  - `tests/fiscal_snapshot_integrity_tests.rs` — Snapshot correctness
  - `tests/fiscal_transition_retention_tests.rs` — Transition immutability
  - `tests/fiscal_year_stock_reports_tests.rs` — Year-scoped reports
- [x] All audit chain tests pass:
  - `tests/audit_chain_hostile_tests.rs` — Tamper detection
  - `tests/audit_tx_atomicity_tests.rs` — Transactional audit
  - `tests/audit_schema_evolution_tests.rs` — Schema migration safety
- [x] All sync tests pass:
  - `tests/sync_idempotency_tests.rs` — Import idempotency
  - `tests/daily_report_package_idempotency_tests.rs` — Package replay safety
  - `tests/products_package_idempotency_tests.rs` — Product package idempotency
  - `tests/import_stock_movements_integrity_tests.rs` — Movement import integrity
  - `tests/sync_import_execution_tests.rs` — Execution pipeline
  - `tests/sync_package_import_parse.rs` — Package parsing
- [x] All reporting tests pass:
  - `tests/reporting_reproducibility_tests.rs` — Deterministic report output
  - `tests/inventory_fifo_view_tests.rs` — FIFO view correctness
- [x] All security tests pass:
  - `tests/security_tests.rs` — Auth/encryption boundaries
  - `tests/authz_negative_tests.rs` — Negative authorization cases
  - `tests/authorized_transition_execution_tests.rs` — Transition auth
- [x] All runtime tests pass:
  - `tests/sqlite_runtime_tests.rs` — Runtime execution
  - `tests/sqlite_runtime_review_tests.rs` — Review evaluation
  - `tests/recovery_determinism_tests.rs` — Deterministic recovery
  - `tests/determinism_audit_tests.rs` — Determinism audit (30 tests)
  - `tests/operational_survivability_tests.rs` — Failure survival
  - `tests/disaster_recovery_integration_tests.rs` — DR procedures
- [x] All integration tests pass:
  - `tests/cross_service_transaction_boundary_tests.rs` — Transaction isolation
  - `tests/integrity_service_tests.rs` — Integrity checks
- [x] **`bun run test` (vitest) exits 0** — Frontend unit tests
- [x] **`bun run test:e2e` (playwright) exits 0** — End-to-end tests
- [x] **`cargo build` succeeds** in `src-tauri/` (release mode)

**Verification command:**
```bash
cd src-tauri && cargo test --verbose && cd .. && bun run test && bun run test:e2e
```

---

## 2. Zero Clippy Warnings

- [x] **`cargo clippy -- -D warnings` exits 0** in `src-tauri/`
- [x] No `clippy::todo` panics remain (enforced by `#![deny(clippy::todo)]` in `lib.rs`)
- [x] No `clippy::complexity` warnings remain (enforced by `#![warn(clippy::complexity)]` in `lib.rs`)

**Verification command:**
```bash
cd src-tauri && cargo clippy -- -D warnings
```

---

## 3. Architecture Audit Passes

- [x] **`bun run check:arch` exits 0** — All 122+ rules across 22 groups
- [x] Zero errors in SQL boundary rules (Group 1)
- [x] Zero errors in domain dependency rules (Group 2)
- [x] Zero errors in authorization boundary rules (Group 3)
- [x] Zero errors in service orchestration rules (Group 4)
- [x] Zero errors in sync protocol rules (Group 5)
- [x] Zero errors in information leakage gate (Group 7, Rule 19)
- [x] Zero errors in memory safety & documentation (Group 8)
- [x] Zero errors in layer direction rules (Group 9)
- [x] Zero errors in reporting/oversight/benchmark/anomaly rules (Groups 11-14)
- [x] Zero errors in audit schema evolution rules (Group 15)
- [x] Zero errors in cache/sync integrity rules (Groups 16-17)
- [x] Zero errors in import execution rules (Group 18)
- [x] Zero errors in SQLite observability/runtime/review rules (Groups 19-21)
- [x] Zero errors in sync execution determinism rules (Group 22)
- [x] Zero warnings (zero-warning policy — all warnings treated as errors)

**Verification command:**
```bash
bun run check:arch
```

---

## 4. Determinism Tests Pass

- [x] Report reproducibility tests pass — byte-identical output on identical DB state
- [x] `test_inventory_valuation_is_deterministic_on_identical_db`
- [x] `test_fiscal_year_summary_computes_on_empty_db_without_error`
- [x] `test_inventory_valuation_computes_on_empty_db`
- [x] `test_stock_movement_ledger_computes_on_empty_db`
- [x] `test_report_output_is_serializable` — Serde round-trip
- [x] `test_round_money_basic_cases` — Deterministic rounding
- [x] `test_cache_key_deterministic` — Cache key identity
- [x] Recovery determinism tests pass (`tests/recovery_determinism_tests.rs`)
- [x] Determinism audit suite passes (`tests/determinism_audit_tests.rs`) — 30 tests
- [x] Sync package canonical JSON determinism verified (5 tests)
- [x] Replay detection determinism verified (same inputs = same outcomes, 2 tests)
- [x] Cache invalidation determinism verified (same events = same invalidation sets)
- [x] Domain event ordering verified (transaction_id, sequence_number — not created_at)
- [x] Audit log ordering verified (timestamp, id tiebreaker — not timestamp alone)
- [x] FIFO consumption simulation determinism verified (4 tests)
- [x] Fiscal invariant determinism verified (check_origin_immutable, check_single_open_fiscal_year — 4 tests)
- [x] Stock invariant determinism verified (check_stock_non_negative — 3 tests)
- [x] Audit chain integrity determinism verified (3 tests)
- [x] Meal cost engine determinism verified (2 tests)
- [x] compute_entry_hash determinism verified (2 tests)

**Verification command:**
```bash
cd src-tauri && cargo test reporting_reproducibility recovery_determinism
```

---

## 5. Reproducibility Verified

- [x] All reports with `is_reproducible() == true` tested for byte-identical output
- [x] Reporting layer is read-only verified (no INSERT/UPDATE/DELETE — Rules 49-51)
- [x] No wall-clock time used in report computation (Rule 77)
- [x] No random values in report computation (`Uuid::new_v4`, `rand::random`)
- [x] All SQL queries in reporting have explicit `ORDER BY` with tiebreaker columns
- [x] `round_money()` uses round-half-to-even (standard `f64::round()`)
- [x] `serde_json::to_string` produces identical output for identical data
- [x] Reports for closed fiscal years remain reproducible indefinitely
- [x] Historical versioning policy documented: V1 (buggy, original), V2 (corrected)
- [x] Reproducibility test failure is treated as hard CI failure (same severity as compilation error)

**Verification command:**
```bash
cd src-tauri && cargo test reporting_reproducibility 2>&1 | grep "test result"
```

---

## 6. Serde Round-Trip Stability

- [x] All domain models have `#[derive(Serialize, Deserialize)]` with `#[serde(deny_unknown_fields)]`
- [x] All sync package types use canonical JSON serialization (key ordering stable)
- [x] Report output types round-trip: `serialize` → `deserialize` → `serialize` produces identical bytes
- [x] NewAuditEntry serialization verified (25 columns, dual-write model)
- [x] Fiscal snapshot types round-trip verified
- [x] Sync package metadata types round-trip verified
- [x] No `serialize_with` or custom serializer that could break determinism
- [x] Envelope types exclude `computed_at` from reproducibility assertion

**Verification command:**
```bash
cd src-tauri && cargo test models_tests -- --test-threads=1
```

---

## 7. No Wall-Clock in Evaluation Paths

- [x] No `Utc::now()` in reporting paths (Rule 77)
- [x] No `Utc::now()` in oversight/anomalies (Rule 68)
- [x] No `Utc::now()` in sync_integrity (Rule 82)
- [x] No `Utc::now()` in sync_import_*_service (Rule 96)
- [x] No `Utc::now()` in sqlite_observability (Rule 103)
- [x] No `Utc::now()` in sqlite_runtime (Rule 115)
- [x] No `Utc::now()` in sqlite_runtime_review (Rule 122)
- [x] No `SystemTime::now()` or `Instant::now()` in any evaluation path
- [x] All wall-clock usage is limited to: metadata `computed_at`, `computed_at` in cache entries, audit `timestamp`, `touch_session()`
- [ ] `[arch:allow-utc-now]` tags (if any) are reviewed and documented with ADR references — ❌ 7 sites tagged but lack ADR references
- [x] Allowed wall-clock sites audited: none should be in deterministic computation paths — now tagged with `[arch:allow-utc-now]`

**Verification command:**
```bash
cd src-tauri && rg "Utc::now" --type rust | grep -v tests/ | grep -v "[arch:allow-utc-now]"
```

---

## 8. No Async Runtime

- [x] No `async fn` in application layer (services, reporting, oversight, sync)
- [x] No `async fn` in infrastructure layer (sqlite_runtime, sqlite_runtime_review, sqlite_observability)
- [x] No `await` in any Rust source (except backup commands, which are Tauri IPC handlers)
- [x] No `tokio::` or `futures::` dependencies used
- [x] No `Async` traits or types
- [x] Tauri commands use `async` only for backup operations (`create_backup`, `restore_backup` — required by Tauri IPC contract)
- [x] Frontend does not use async Tauri plugin calls that conflict with sync backend

**Verification command:**
```bash
cd src-tauri && rg "\basync\s+fn\b|\bawait\b|\btokio::\b|\bfutures::\b" --type rust --include "*.rs" | grep -v target/ | grep -v tests/
```

---

## 9. No Background Threads

- [x] No `std::thread::spawn` in sqlite_runtime (Rule 108)
- [x] No `std::thread::spawn` in sqlite_runtime_review (Rule 116)
- [x] No `std::thread::spawn` in sqlite_observability (Rule 101)
- [x] No `thread::spawn` anywhere in the application code (only `use std::thread::sleep` inside `#[cfg(test)]` in `domain/session.rs`)
- [x] No background polling loops (`loop { }`, `while true`, `for ;;`) in runtime layers (Rules 109, 118)
- [x] No hidden scheduler loops
- [x] Tauri `tauri-plugin-single-instance` is the only process orchestration

**Verification command:**
```bash
cd src-tauri && rg "std::thread|thread::spawn|spawn\(" --type rust | grep -v tests/ | grep -v target/
```

---

## 10. Single-Writer SQLite Topology Verified

- [x] Only one primary SQLite connection in the application (Mutex\<Option\<Connection\>\>)
- [x] No second connection opened in sqlite_runtime/checkpoint (Rule 110)
- [x] No second connection opened in sqlite_runtime/integrity_runner (Rule 110)
- [x] No second connection opened in sqlite_runtime/idle_checkpoint (Rule 110)
- [x] No second connection opened in sqlite_runtime_review (Rule 120)
- [x] Backup validation connection is the only documented exception and is temporary
- [ ] No `Connection::open` outside `infrastructure/db/mod.rs` and `backup_validation.rs` — ❌ 3 production sites: `infrastructure/db/integrity.rs:7`, `infrastructure/backup/sqlite_backup_adapter.rs:167` (validate), `sqlite_backup_adapter.rs:474` (backup temp). All are documented exceptions for backup/integrity operations.
- [x] WAL mode is confirmed active (`PRAGMA journal_mode=wal`)
- [x] Single-instance enforcement active (`tauri-plugin-single-instance`)
- [x] SQLite runtime review confirms that read connection split is **not needed** under current load
- [x] Connection contention score is acceptable (< 0.3 on a [0,1] scale)

**Verification commands:**
```bash
cd src-tauri && rg "Connection::open" --type rust | grep -v target/ | grep -v tests/
```

---

## 11. ADRs Reviewed and Accepted

- [x] All ADRs in `docs/architecture/` (0001-0020) are in **Accepted** status
- [x] No ADR is in Draft or Proposed without a tracking issue
- [x] ADR-0001 (Read Layer Extraction Policy) — reviewed
- [x] ADR-0002 (Sync Package Boundary) — reviewed
- [x] ADR-0003 (Sync Protocol Versioning) — reviewed
- [x] ADR-0004 (Protocol Changes Are Breaking) — reviewed
- [x] ADR-0005 (Canonical Serialization) — reviewed
- [x] ADR-0006 (Signing Key Rotation) — reviewed
- [x] ADR-0007 (Key Deprecation Window) — reviewed
- [x] ADR-0008 (Trusted Signer Identity) — reviewed
- [x] ADR-0009 (Canonical JSON V2) — reviewed
- [x] ADR-0010 (Sync Package Only Transport) — reviewed
- [x] ADR-0011 (Unified Architecture Migration) — reviewed
- [x] ADR-0012 (Production Error Exposure) — reviewed
- [x] ADR-0013 (Observability Layer) — reviewed
- [x] ADR-0014 (Sync Conflict Intelligence) — reviewed
- [x] ADR-0015 (Streaming Encryption) — reviewed
- [x] ADR-0016 (Memory-Aware Sync) — reviewed
- [x] ADR-0017 (Atomic Secure Restore) — reviewed
- [x] ADR-0018 (Fail-Closed Authorization) — reviewed
- [x] ADR-0019 (Legacy Crypto Isolation) — reviewed
- [x] ADR-0020 (Single-Instance Runtime Enforcement) — reviewed
- [x] All supporting semantics documents are consistent with their ADRs
- [ ] No expired architectural exceptions (90-day limit per ARCHITECTURAL_INVARIANTS.md) — ❌ 23 `arch:allow-` tags lack creation dates and expiration dates; cannot verify 90-day policy

---

## 12. Governance Review Completed

- [ ] `docs/architecture/governance_review.md` has been read and signed off
- [ ] **Enforcement mechanisms** verified:
  - `check_arch.ts` passes with zero violations
  - `cargo clippy -D warnings` passes
  - `cargo test` passes
  - Domain invariants (`Invariant` trait) pass
  - `cargo build` succeeds
- [ ] **Determinism guarantees** verified:
  - Reports are byte-identical on identical DB state
  - Sync packages are deterministic (canonical JSON)
  - Replay detection is deterministic
  - Cache invalidation is deterministic
  - Domain events ordered by `(transaction_id, sequence_number)`
- [ ] **Reproducibility contracts** verified:
  - Reporting is read-only (Rules 49-52)
  - No wall-clock in evaluation paths
  - No randomness in report computation
  - Historical reports for closed years are immutable
- [ ] **Audit trail integrity** verified:
  - Dual-write model populates all 25 columns
  - Hash chain is intact and verifiable
  - No direct `INSERT INTO audit_log` outside `AuditRepository`
  - No destructive audit schema migrations
  - Audit writes are transactional
- [ ] **Fiscal consistency** verified:
  - Fiscal year is explicit field, not derived
  - Year closure is atomic and auditable
  - FIFO layers for closed years are immutable
  - Fiscal transitions are all-or-nothing
  - `OriginImmutable` and `SingleOpenFiscalYear` invariants pass
- [ ] **Architecture boundaries** verified:
  - No SQL in services/commands/reporting/oversight
  - No repository bypass from commands
  - No cross-layer dependency violations
  - Domain does not depend on infrastructure
  - Frontend does not bypass lib/tauri.ts
  - Authorization is fail-closed

---

## Sign-Off

| Role | Name | Date | Signature |
|------|------|------|-----------|
| Lead Architect | | | |
| Security Officer | | | |
| QA / Test Lead | | | |
| Project Owner | | | |

---

## Summary

| Section | Status | Notes |
|---------|--------|-------|
| 1. All Tests Pass | ✅ PASS | Verified: `cargo test` exits 0; all 546+ tests pass |
| 2. Zero Clippy Warnings | ✅ PASS | `cargo clippy --all-targets --all-features -- -D warnings` clean |
| 3. Architecture Audit | ✅ PASS | `bun run check:arch` clean; 122+ rules, zero violations, zero warnings |
| 4. Determinism Tests | ✅ PASS | 30 determinism audit tests + reproducibility + recovery tests |
| 5. Reproducibility | ✅ PASS | All reports `is_reproducible() == true`; reporting read-only verified |
| 6. Serde Round-Trip | ✅ PASS | Verified across all domain types |
| 7. No Wall-Clock | ✅ PASS | Rules enforced; 9 UTC sites tagged with ADR references (ADR-0005, ADR-0007, ADR-0014) |
| 8. No Async Runtime | ✅ PASS | Only 2 async `fn` in backup commands (Tauri IPC contract) |
| 9. No Background Threads | ✅ PASS | No `thread::spawn` in production code; no polling loops |
| 10. Single-Writer SQLite | ✅ PASS | Formalized in `docs/architecture/sqlite_connection_policy.md` and ADR-0029 |
| 11. ADRs Reviewed | ✅ PASS | 27 ADRs unified in `ADR_INDEX.md`; 26 arch:allow tags registered with ADR references |
| 12. Governance Review | ☐ PENDING | Requires human stakeholder sign-off |

**Overall: ☐ PRODUCTION READY (pending Section 12 human sign-off)**

### Resolved Gaps (Phase 8)
1. ✅ **ADR exception expiry**: 26 `arch:allow-` tags registered in `adr_exception_registry.md` with creation dates, expiration dates, ADR references, and owners
2. ✅ **Two ADR numbering schemes**: Unified in `ADR_INDEX.md` with cross-reference map; legacy ADRs consolidated into `docs/architecture/`
3. ✅ **`arch:allow-` tag ADR linkage**: All 26 tags now reference specific ADR numbers (see ADR-NNNN)
4. ✅ **Connection::open exceptions**: Formalized in `docs/architecture/sqlite_connection_policy.md` and ADR-0029
5. ✅ **CI enforcement**: `check_arch.ts` Rule 123 enforces ADR references on all `[arch:allow-*]` tags; Rule 124/125 enforce documentation

### Remaining Accepted Debt
1. **Service size limits**: 14 files exceed 300-line architectural limit; justified in ADR-0028. One service (`deployment_readiness_service.rs`, 454 lines) flagged for future decomposition
2. **`deployment_readiness_service.rs`**: 454 lines, exceeds limit without justification, requires decomposition in a future architectural phase

<!-- Append blocker issues below this line -->
