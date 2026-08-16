# ARCHITECTURE FREEZE DECLARATION

**تاريخ الإعلان:** 2026-05-29  
**Project:** GRPC-Tauri — Gestion des Restaurants de la Protection Civile  
**Scope:** Algerian Civil Protection food-service management platform  
**Status:** 🧊 **FROZEN** — All items in Section 2 are locked. Changes require RFC-to-ADR process.

---

## 1. Current Architecture Scope

```
┌─────────────────────────────────────────────────────┐
│                     Frontend (Svelte 5)              │
│  src/pages/  src/components/  src/lib/  src/tests/  │
└──────────────────────┬──────────────────────────────┘
                       │ Tauri IPC (sync commands)
                       ▼
┌─────────────────────────────────────────────────────┐
│                  Commands Layer                       │
│  src-tauri/src/commands/  (thin — auth + dispatch)   │
│  Guards  │  Registry  │  Type definitions            │
└──────────────────────┬──────────────────────────────┘
                       │ delegation
                       ▼
┌─────────────────────────────────────────────────────┐
│              Application Layer                        │
│  src-tauri/src/application/                          │
│  ├── services/          (orchestration)              │
│  ├── usecases/          (exports, reports)           │
│  ├── reporting/         (cache, valuation, ledger)   │
│  ├── oversight/         (KPIs, benchmarks, anomaly)  │
│  ├── sync/              (package, import, compat)    │
│  ├── sync_integrity/    (replay, sequencer, audit)   │
│  ├── authz/             (policies, resource context) │
│  └── ports/             (export/output ports)        │
└──────────────────────┬──────────────────────────────┘
                       │ repository injection
                       ▼
┌─────────────────────────────────────────────────────┐
│              Domain Layer                             │
│  src-tauri/src/domain/                               │
│  ├── accounting/     (fifo, fiscal, movement, cost)  │
│  ├── invariants/     (audit, fifo, fiscal, stock)    │
│  ├── audit.rs        (audit chain, hash)             │
│  ├── events/         (domain event definitions)      │
│  ├── security.rs     (rate limiter, session)         │
│  └── ports/          (backup, export)                │
└──────────────────────┬──────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────┐
│              Repository Layer                         │
│  src-tauri/src/repositories/  (SQL ONLY)             │
│  Executor | FIFO | Fiscal | Audit | Sync | ...      │
└──────────────────────┬──────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────┐
│            Infrastructure Layer                       │
│  src-tauri/src/infrastructure/                       │
│  ├── db/               (read, sync, transaction)     │
│  ├── backup/           (sqlite_backup_adapter)       │
│  ├── export/           (xlsx, file)                  │
│  ├── security/         (encryption, identity, hash)  │
│  ├── sync/             (packages, signing, source)   │
│  ├── sqlite_observability/  (metrics, diagnostics)   │
│  ├── sqlite_runtime/   (checkpoint, integrity)       │
│  └── sqlite_runtime_review/  (connection, topology)  │
└──────────────────────┬──────────────────────────────┘
                       │ single connection
                       ▼
               ┌───────────────┐
               │   SQLite DB    │
               │   (WAL mode)   │
               │  Mutex<Conn>   │
               └───────────────┘
```

**Tenets:**
1. Offline-first — zero network dependencies at runtime
2. Single-writer SQLite topology — `Mutex<Option<Connection>>` serializes all access
3. No async runtime — all Rust code is synchronous
4. No background threads — no daemons, no polling loops
5. Accounting-grade correctness — FIFO, dual-write audit, fiscal year semantics
6. Reproducible reporting — byte-identical output on same inputs
7. Deterministic sync — canonical JSON, SHA-256 packages, replay detection
8. Fail-closed authorization — access denied unless explicitly granted
9. Backend is source-of-truth — frontend never owns authz or business logic

---

## 2. What Is Frozen (🔒 Locked)

The following contracts, boundaries, and guarantees are **frozen** and may not be modified without an approved ADR:

### 2.1 Architecture Boundaries
- **🔒 Layer isolation** — commands → services → repositories → infrastructure → SQLite. No layer skipping. No circular dependencies.
- **🔒 SQL confinement** — SQL strings only in `src-tauri/src/repositories/`. Zero SQL in commands, services, reporting, oversight, or infrastructure evaluation layers.
- **🔒 Thin commands** — commands are auth + dispatch only. No business logic in commands.
- **🔒 Domain purity** — `domain/` must not import `infrastructure/`, `application/`, or `commands/`.
- **🔒 Application isolation** — `application/` must not import `app/` (Tauri state).
- **🔒 Infrastructure isolation** — `infrastructure/` must not import `commands/`.
- **🔒 Frontend isolation** — no `invoke` or `@tauri-apps/` imports outside `src/lib/tauri.ts`. No SQL in Svelte components. No `any` types. No raw `alert/confirm`.

### 2.2 Authorization Model
- **🔒 Fail-closed** — authorization defaults to deny. Session expiry, resource failure, or missing role = rejection.
- **🔒 Backend-only authz** — frontend role checks are UX-only; backend is sole authority.
- **🔒 Structured authorization** — via `application/authz/` with policies, principal, resource context. No bare role comparisons in commands.
- **🔒 Password operations** — confined to `commands/auth.rs`. No `verify_password_argon2` elsewhere.
- **🔒 B8 first-`identity_access` bootstrap exemption (ADR-0045 — Accepted 2026-08-14)** — استثناء ضيّق معتمد من قاعدة الرفض الافتراضي: استيراد أول حزمة `identity_access` على عقدة UNIT جديدة **بدون جلسة Admin** (مجهول على طبقة التخويل — لا يوجد حساب Admin بعد؛ التخويل من سلسلة المصادقة الكاملة للحزمة). Package-authenticated، ذاتي الإنهاء، V2-only، بضوابط SEC-003-01/02 إلزامية، تركيب مرساة WILAYA صالحة سابقًا مع ربط المُصدِّر والمرساة و`payload.unit_code` بالوحدة المحلية، سجل B8 فارغ، أول تسلسل = 1، تطبيق ذري مع حماية إعادة اللعب، ثم تعود التخويلات إلى AdminOnly. لا حساب Admin مؤقت؛ لا `.unit role=Admin` بديل؛ لا صنف/قطعة إقلاع جديدة. لا يُنشئ الإعفاء هوية ADMIN ويبقى SEC-002 منفصلًا. **تحديث 2026-08-15 (F-1 Option A، ADR-0045 §26.9):** مُنتِج `identity_access` يخصّص التسلسل من تدفق **`(issuer_identity_id, target_unit_code)`** (الهجرة 009) بدل سجل المُنتِج العام، فتحصل كل وحدة UNIT جديدة على حزمة أولى `sequence = 1` من نفس مُصدِّر WILAYA؛ ضابط "أول تسلسل = 1" يُطبَّق **لكل تدفق وحدة مستهدفة** دون إضعاف أي ضابط مستهلك؛ الأنواع الأخرى تحتفظ بسجل المُنتِج العام لكل مُصدِّر. بقية القرارات: ADR-0045 §26.6.

### 2.3 Audit Trail
- **🔒 Dual-write model** — every audit write populates flat columns + structured columns + details JSON.
- **🔒 Hash chain** — `compute_entry_hash()` covers flat legacy fields. Chain verified via `verify_audit_chain_streaming()`.
- **🔒 Transactional audit** — audit writes within fiscal operations are atomic. No bare `AuditLogger::log_success` in services.
- **🔒 Additive-only schema** — no destructive audit migrations. No DROP COLUMN, no DROP TABLE on `audit_log`.
- **🔒 Single repository** — all `INSERT INTO audit_log` goes through `AuditRepository`. Zero exceptions.
- **🔒 Deterministic ordering** — `ORDER BY timestamp ASC, id ASC` with `id` tiebreaker.
- **🔒 Chain invariant** — `AuditChainIntegrity` invariant detects tampering.

### 2.4 Fiscal Semantics
- **🔒 Explicit fiscal year** — `stock_movements.fiscal_year`, `daily_reports.fiscal_year`, `settings.current_year`. Not derived from dates.
- **🔒 Atomic year closure** — `FiscalClosingService` closes year atomically: status update + snapshot + FIFO reclassification + audit. All-or-nothing.
- **🔒 FIFO reclassification, not deletion** — year close reclassifies `source_type` from `ORDER` to `OPENING`. No data moved, copied, or deleted.
- **🔒 Immutable closed years** — closed fiscal years receive no new mutations. Cache entries for closed years are never invalidated.
- **🔒 Single open year** — exactly one fiscal year may be open at any time. `SingleOpenFiscalYear` invariant.
- **🔒 Origin immutability** — `fifo_stock_layers.origin_fiscal_year` is immutable after creation. `OriginImmutable` invariant.
- **🔒 All-or-nothing transitions** — fiscal transition IDs are recorded in `AppliedTransitions`. Replayed transitions are rejected.

### 2.5 Reproducibility & Determinism
- **🔒 No wall-clock in evaluation paths** — `Utc::now`, `SystemTime::now`, `Instant::now` banned in: reporting, oversight, anomalies, benchmarks, sync_integrity, sync_import, sqlite_observability, sqlite_runtime, sqlite_runtime_review.
- **🔒 No randomness in reporting** — no `Uuid::new_v4()`, no `rand::random`, no sampling in report computation.
- **🔒 Deterministic ordering** — all SQL queries in reporting have explicit `ORDER BY` with tiebreaker columns.
- **🔒 Round-half-to-even** — `round_money()` uses `f64::round()`.
- **🔒 Read-only reporting** — no INSERT/UPDATE/DELETE in reporting layer. No transaction ownership.
- **🔒 Byte-identical reproducibility** — `is_reproducible() == true` reports produce identical `data` JSON on identical DB state.
- **🔒 Semantic cache invalidation** — no TTL, no "clear all", no wall-clock. Only `DomainEvent`-driven.
- **🔒 Deterministic sync packages** — canonical JSON, key ordering stable. Same input + same key = identical encrypted bytes.
- **🔒 Deterministic replay detection** — same state + same input = same outcome. `BTreeSet` for ordered iteration. SHA-256 for conflict IDs.

### 2.6 SQLite Topology & Runtime
- **🔒 Single-writer connection** — `Mutex<Option<Connection>>`. No second write connection.
- **🔒 No background threads** — `std::thread::spawn` banned in sqlite_runtime, sqlite_runtime_review, sqlite_observability.
- **🔒 No async** — no `async fn`, `await`, `tokio`, `futures` in any Rust layer.
  **Sanctioned exception (ADR-0043):** `tauri::async_runtime::spawn_blocking` for blocking
  backup/restore file I/O in `src-tauri/src/commands/backup.rs` ONLY — `create_backup` and
  `restore_backup` are the sole async entry points; no async beyond these two command handlers,
  and none in Domain/Application/Ports.
- **🔒 No auto-repair** — integrity monitoring detects but does not repair.
- **🔒 No auto-VACUUM** — VACUUM must not execute automatically.
- **🔒 No automatic checkpoints** — checkpoint evaluation is caller-driven, not automatic.
- **🔒 WAL mode** — SQLite WAL journal mode is mandatory.
- **🔒 Single-instance enforcement** — via `tauri-plugin-single-instance`.

### 2.7 Sync Protocol
- **🔒 Package-only transport** — no CSV, no legacy sync (ADR-0010).
- **🔒 Canonical JSON V2** — deterministic serialization for signing and verification (ADR-0009).
- **🔒 Encrypt: two-tier secret protection (ADR-0039, amended by ADR-0041)** — `age::x25519` is mandatory for node-managed secrets; `age::scrypt` is permitted in **exactly two** sites — portable operator key material (`.adminkey`) in `src-tauri/src/infrastructure/identity/adminkey_provider.rs` and the passphrase-protected app-key-at-rest store in `src-tauri/src/infrastructure/security/appkey_store.rs` (Rule 38, amended by ADR-0039 and ADR-0041).
- **🔒 Sign: Ed25519 node identity** — package signatures use Ed25519 (RFC 8032) bound to a node identity via `signature_version = 2` (RFC `2026-08-04-node-identity-trust`, ADR-0003). Symmetric HMAC remains only for reading legacy V1 packages during the deprecation window.
- **🔒 Streaming encryption** — no whole-buffer `read_to_end` in security/sync paths (Rules 29, 31).
- **🔒 Replay detection before mutation** — `ValidationGate` checks are all-or-nothing before any DB write (Rule 83-84).
- **🔒 No nested transactions in sync** — single transaction boundary per import (Rule 89).
- **🔒 Trust/Registry packages** — new `trust` (certificates + revocations) and `registry` (fleet state) kinds flow through `run_import_pipeline` with two independent fail-closed guards, no wall-clock (RFC `2026-08-04-node-identity-trust`): Transport Guard `(issuer_identity_id, package_sequence)` and Credential Guard `(credential_id, generation)`.
- **🔒 UNIT-issued data packages — kind-scoped (ADR-0046 — Accepted 2026-08-16)** — تعديل نطاق SEC-003-01 حصريًا لحزم البيانات الثلاث (`stock_movements`/`daily_report`/`monthly_summary`): تُقبل حزم UNIT الموقَّعة V2/Ed25519 على المستورد **WILAYA فقط** مع اشتراطات إلزامية — شهادة ACTIVE غير منتهية، توقيع Ed25519 سليم، ثم (بعد المصادقة) عضوية `cert.subject_id → units` مع `wilaya_code == settings.wilaya_code` المحلي، وربط `unit_id` المستهدف == `cert.subject_id`، وربط حمولة حركات المخزون الموقَّعة. `products`/`identity_access`/`trust`/`registry`/`.unit` تبقى WILAYA-only؛ لا سلطة توقيع عامة لـ UNIT؛ لا UNIT↔UNIT؛ لا إصدار هويات من UNIT؛ لا ترحيل/تنسيق/تشفير/B8/AppKey/تخويلات. بقية القرارات: ADR-0046 §3.
- **🔒 `.unit` Trust-First V2/Ed25519 (ADR-0044 — Accepted 2026-08-14)** — `.unit` يُوقَّع V2/Ed25519 بهوية WILAYA (سلطة الإصدار) ويُتحقق عبر مرساة WILAYA **ACTIVE** مثبَّتة **قبل القبول** وموثوقة بسلسلة الجذر الإنتاجي (Root → WILAYA → Ed25519). دور `.unit` = **User فقط** (`role=Admin` غير صالح)؛ أول حزمة V2 بـ `package_sequence = 1`؛ لا سر توقيع أسطوري مشترك (HMAC) على UNIT ولا مفتاح WILAYA خاص؛ لا استبدال صامت للمرساة؛ عدم تطابق عابر يفشل مغلقًا؛ التراجع = إعادة توفير منضبطة. **HMAC-V1 (`.unit` بلا `signature_version`) قراءة إرثية فقط** خلال نافذة إغلاق **مبنية على الأدلة** (A44-07) بانتقال **fleet-sync** (A44-09)؛ لا إصدار حزم V1 جديدة. بقية القرارات: ADR-0044 §28.6.

### 2.8 Production Safety
- **🔒 Error exposure gate (ADR-0012)** — `details` field in error strings gated behind `#[cfg(debug_assertions)]`.
- **🔒 No overclaims** — "O(1)", "zero-copy", "military grade", "unbreakable", etc. banned (Rules 32, 36). [arch:allow-overclaim]
- **🔒 No deprecated terminology** — BSS, .bss, .bssync, grpcsync, grpcunit banned (Rule 37). [arch:allow-history]
- **🔒 Zero-warning policy** — all `check_arch.ts` warnings are treated as errors. CI fails if any warning exists.
- **🔒 Production Root-key prerequisite (A44-06 — unchanged 2026-08-14)** — قبول ADR-0044 وADR-0045 (2026-08-14) **لا يساوي اعتمادًا إنتاجيًا**: الإنتاج يبقى **محجوبًا** حتى اعتماد مفتاح Root الإنتاجي (مراسم + تثبيت + حيازة + توزيع + تحقق — ADR-0044 §9.4). `root_public_key.rs` يحمل ناقل TEST-2 ولا يُغيَّر في هذه العملية. يُحظر ادعاء الجاهزية الإنتاجية قبل اكتمال هذا العائق.

---

## 3. What Is Still Open for Change (🔓 Unlocked)

The following areas are **not frozen** and may be modified with standard code review:

### 3.1 Frontend Presentation
- UI layout, component styling, page structure
- Addition or removal of Svelte components and pages
- Frontend tests (vitest, Playwright)
- Localization strings

### 3.2 Reporting Metrics & KPIs
- New KPI implementations in `application/oversight/metrics/`
- New benchmarks in `application/oversight/benchmarks/`
- New anomaly detectors in `application/oversight/anomalies/`
- Must remain within frozen boundaries (no SQL, no wall-clock, no mutations)

### 3.3 Export Formats
- Additional XLSX report templates
- CSV export for non-audited data (but not sync)
- Must go through `application/ports/`

### 3.4 Domain Events
- New `DomainEvent` variants
- New event handlers
- Must extend existing patterns, not modify frozen event ordering

### 3.5 Repository Extensions
- New query methods in existing repositories
- New repository implementations
- Must not break frozen boundary (SQL confined, no business logic)

### 3.6 Observability & Diagnostics
- Additional metrics in `sqlite_observability/`
- Additional query plan diagnostics
- Must remain read-only and deterministic

### 3.7 Tests
- New unit, integration, or E2E tests
- Test infrastructure improvements
- Must not change frozen behavior of tested code

### 3.8 Infrastructure Adaptations
- New backup strategies (as long as atomic restore is preserved)
- New export adapters (as long as reproducibility is preserved)

---

## 4. How to Propose Architecture Changes (RFC-to-ADR Process)

Any change to a **frozen** item (Section 2) requires the following process:

### Step 1: Write an RFC
Create a document under `docs/architecture/rfcs/` with filename `YYYY-MM-DD-title.md` containing:
- **Problem statement** — what needs to change and why
- **Current behavior** — what the frozen contract currently guarantees
- **Proposed change** — what would change and how
- **Impact analysis** — which frozen contracts are affected (Section 2 items)
- **Migration plan** — how existing data/code transitions to the new model
- **Rollback plan** — how to undo the change if it fails
- **Backward compatibility** — what existing behavior is preserved

### Step 2: Review
The RFC is reviewed by:
- Lead Architect (architectural soundness)
- Security Officer (security impact)
- Domain experts (fiscal/accounting correctness)

Review criteria:
- Does the change preserve accounting correctness?
- Does the change preserve auditability?
- Does the change preserve backward compatibility for historical data?
- Is the migration plan complete and testable?
- Is the rollback plan safe?

### Step 3: Approve & Create ADR
Once the RFC is approved:
1. Create an ADR in `docs/architecture/XXXX-title.md` (increment highest number)
2. ADR status: **Accepted**
3. Link to the RFC in the ADR
4. Update this freeze document (Section 2 or 3) to reflect the change

### Step 4: Implement
- All implementation code must pass the full governance gate
- Tests must cover the new behavior AND verify backward compatibility
- Documentation must be updated

### Step 5: Audit Trail
- The RFC, ADR, and implementation commit must be cross-referenced
- Changelog must note the architecture change

### Exception Path (Emergency)
In case of a production-critical bug that violates a frozen contract:
1. File a tracking issue immediately
2. Implement a temporary fix with `[arch:allow-*]` tags documented
3. Within 7 days, follow the full RFC-to-ADR process for a permanent solution
4. Temporary exceptions expire after 90 days (per ARCHITECTURAL_INVARIANTS.md)

---

## 5. Zero-Warning Policy

### Policy Statement
**There are zero tolerated warnings in the architecture audit.**

All `check_arch.ts` rules, whether classified as `error` or `warning`, must be resolved before merge. A `warning` classification means "requires review" — it does **not** mean "optional". The CI pipeline (`bun run check:arch`) exits non-zero if any warning exists.

### Enforcement
| Layer | Tool | Policy |
|-------|------|--------|
| Architecture | `check_arch.ts` | Zero warnings. Exit code 1 if any. |
| Rust | `cargo clippy -D warnings` | Zero warnings. Compilation fails. |
| TypeScript/Svelte | `svelte-check` | Zero errors. |
| Tests | `cargo test`, `vitest` | Zero failures. |
| Secrets | `check_secrets.ts` | Zero hardcoded secrets. |
| Documentation | `check_docs_governance.ts` | Zero broken links/missing refs. |
| Release | `check_release_integrity.ts` | Zero integrity violations. |

### Escalation
If a warning cannot be immediately resolved:
1. It must be documented with an `[arch:allow-*]` tag AND an inline comment referencing a tracking issue
2. A follow-up ADR must be filed within 7 days
3. The issue must be resolved before the next release

---

## 6. Governance Review Cadence

| Frequency | Review Type | Participants | Scope |
|-----------|-------------|-------------|-------|
| **Pre-merge** | Gate check | CI pipeline | `check_arch.ts`, clippy, tests, svelte-check |
| **Weekly** | Spot check | Lead Architect | Recent changes against freeze document |
| **Per release** | Full governance review | Lead Architect, Security Officer, QA | Production readiness checklist (all 12 sections) |
| **Quarterly** | Architecture freeze audit | All stakeholders | Review freeze document for needed updates, expired exceptions, ADR compliance |
| **Ad-hoc** | Incident-driven | Relevant team | Any production incident or security finding |

### Sign-off Requirements
- **Weekly spot check:** Lead Architect sign-off in commit message or MR description
- **Per-release full review:** Signed production readiness checklist (`docs/architecture/production_readiness_checklist.md`)
- **Quarterly freeze audit:** Signed governance review (`docs/architecture/governance_review.md`) with updated freeze document

---

## 7. References

| Document | Location |
|----------|----------|
| Governance Review | `docs/architecture/governance_review.md` |
| Production Readiness Checklist | `docs/architecture/production_readiness_checklist.md` |
| Architectural Invariants Charter | `docs/architecture/ARCHITECTURAL_INVARIANTS.md` |
| ADR Index | `docs/architecture/README.md` |
| Architecture Audit Script | `scripts/check_arch.ts` |
| CI Pipeline | `.github/workflows/ci.yml` |
| Comprehensive CI Gate | `scripts/run_ci.ts` |
| Compile-Time Guards | `src-tauri/src/architecture.rs` |
| AGENTS.md (project identity & rules) | `AGENTS.md` |

---

**This freeze declaration is a living document. It is updated only through the RFC-to-ADR process.**
