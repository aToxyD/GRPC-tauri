# AGENTS.md

# GRPC-Tauri Governance Contract

Status: Active
Scope: Entire Repository

This document defines the permanent governance, architecture, ownership, and change-control rules of the GRPC-Tauri project.

Release-specific information, certifications, governance metrics, timelines, snapshots, and operational reports are maintained in the `/docs` hierarchy and are intentionally excluded from this contract.

---

# 0. Project Identity & Technology Stack

## Project Scope

GRPC-Tauri is an **offline-first desktop application** for managing food-service operations across Algerian Civil Protection (*Protection Civile*) units and Wilaya nodes.

Full name: *Gestion des Restaurants de la Protection Civile* (GRPC).  
Version: see `src-tauri/Cargo.toml` and `package.json`.

---

## Technology Stack

| Layer | Technology |
|-------|------------|
| Frontend | Svelte 5 + TypeScript + TailwindCSS |
| IPC Bridge | Tauri 2 (synchronous commands only) |
| Backend | Rust 2021 edition (MSRV defined in `Cargo.toml` and CI) |
| Database | SQLite — WAL mode — `Mutex<Option<Connection>>` single-writer |
| Package Manager | Bun |
| Build Tool | Vite |
| Testing | Vitest (unit / integration) + Playwright (E2E) |

---

## Runtime Tenets

These are non-negotiable invariants that govern all runtime behavior. Any code that violates them is a governance defect. All underlying architecture decisions are cataloged in `docs/architecture/ADR_INDEX.md`.

1. **Offline-first** — zero network dependencies at runtime.
2. **No async runtime** — all Rust code is synchronous. `async fn`, `await`, `tokio`, and `futures` are forbidden in any Rust layer.
3. **No background threads** — `std::thread::spawn` is forbidden in `sqlite_runtime`, `sqlite_runtime_review`, `sqlite_observability`, and all domain/application layers.
4. **Single-writer SQLite** — `Mutex<Option<Connection>>` serializes all DB writes. The only permitted secondary connection is the persisted rate limiter.
5. **WAL mode mandatory** — SQLite journal mode must remain WAL at all times.
6. **Single-instance enforcement** — via `tauri-plugin-single-instance`; a second process instance is rejected.
7. **Backend is source of truth** — frontend never owns authorization decisions or business calculations.

---

# 1. Core Principles

## P1 — Determinism First

Business outcomes must be deterministic and reproducible.

Given identical inputs and identical persisted state, the system must produce identical outputs.

---

## P2 — Single Source of Truth

Every business fact must have exactly one authoritative owner.

Duplication of business logic, ownership, contracts, projections, or runtime calculations is forbidden.

---

## P3 — Explicit Boundaries

All interactions between architectural layers must occur through documented contracts.

Cross-layer shortcuts are prohibited.

---

## P4 — Auditability

All critical operations must be traceable, reviewable, and explainable through persisted records, logs, or documented governance artifacts.

---

# 2. Architectural Invariants

## A1 — Domain Ownership

Every business domain owns:

* Its contracts
* Its projections
* Its runtime calculations
* Its persistence rules

Ownership must be unambiguous.

---

## A2 — Contract Ownership

Every IPC command, service contract, and externally consumed interface must belong to exactly one owner.

Duplicate ownership is forbidden.

---

## A3 — Projection Ownership

Every frontend projection must have exactly one owning domain.

Consumers may read projections but may not redefine them.

---

## A4 — Dependency Direction

Dependencies must flow inward toward business logic.

Infrastructure must not dictate domain behavior.

UI must not contain business rules.

---

## A5 — Runtime Purity

Business calculations belong to the backend domain layer.

Frontend code must not reimplement:

* inventory calculations
* financial calculations
* consumption calculations
* derived business metrics

except for presentation-only formatting.

---

# 3. Layer Map

Every business domain and infrastructure concern maps to an exact directory. Code must be placed in the correct layer; cross-layer shortcuts are governance violations.

| Layer | Path | Responsibility |
|-------|------|----------------|
| Frontend pages | `src/pages/` | Svelte 5 UI — presentation only |
| Frontend components | `src/components/` | Reusable UI components |
| IPC Contract | `src/lib/tauri.ts` | **Only** file permitted to call `invoke` / `@tauri-apps/*` |
| Frontend types | `src/lib/types.ts` `src/lib/contracts/` | Shared TypeScript types and domain contracts |
| Commands | `src-tauri/src/commands/` | Thin IPC handlers — auth guard + dispatch only. No business logic. |
| Application services | `src-tauri/src/application/services/` | Orchestration, fiscal, sync, backup, report services |
| Authorization | `src-tauri/src/application/authz/` | Policies, principal resolution, resource context |
| Reporting | `src-tauri/src/application/reporting/` | Report cache, inventory valuation, stock ledger |
| Oversight | `src-tauri/src/application/oversight/` | KPIs, benchmarks, anomaly detection |
| Sync | `src-tauri/src/application/sync/` + `sync_integrity/` | Package build, replay, sequencing, conflict resolution |
| Domain | `src-tauri/src/domain/` | Pure business rules — must not import infrastructure, application, or commands |
| Repositories | `src-tauri/src/repositories/` | SQL **only** — row mapping, no arithmetic, no cross-repo calls, no business logic |
| Infrastructure | `src-tauri/src/infrastructure/` | DB adapters, backup, encryption, observability, sync signing |
| App wiring | `src-tauri/src/app/` | Tauri `AppState` — no business logic |
| Models / DTOs | `src-tauri/src/models/` | Data transfer objects — no business logic |
| DB / Migrations | `src-tauri/src/db/` | `ConnectionFactory`, schema migrations |

## Critical Isolation Rules

| Rule | Constraint |
|------|------------|
| **Frontend isolation** | All `invoke` / `@tauri-apps/api` / `@tauri-apps/plugin-*` imports are confined to `src/lib/tauri.ts`. Direct invocation from pages, components, or any other file is a violation. |
| **SQL confinement** | SQL strings (`SELECT`, `INSERT`, `UPDATE`, `DELETE`) belong exclusively in `src-tauri/src/repositories/`. Zero SQL in commands, services, or domain. |
| **Password confinement** | `verify_password_argon2` and all password hash operations are confined to `commands/auth.rs`. Identity-based authentication (Challenge–Response, Ed25519) is confined to the identity layer per RFC `2026-08-04-node-identity-trust` / ADR-0038. |
| **Identity ownership** | Identity Store is the sole source of truth for identity state. ADMIN/UNIT/WILAYA are identities of the same class. Trust distribution flows exclusively through `trust`/`registry` packages (ADR-0038). |
| **Domain purity** | `domain/` must not import `infrastructure/`, `application/`, or `commands/`. |
| **Application isolation** | `application/` must not import `app/` (Tauri managed state). |
| **AppState ownership** | `State<AppState>` may only be created, owned, or mutated within `src-tauri/src/app/`. No other layer may hold or pass `AppState` directly. |
| **Repository isolation** | Repositories may not call other repositories. Cross-repository orchestration belongs to application services only. |
| **Crypto algorithm** | Two-tier secret protection (ADR-0039): `age::x25519` mandatory for node-managed secrets; `age::scrypt` permitted exclusively for portable operator key material (`.adminkey`) and only in `src-tauri/src/infrastructure/identity/adminkey_provider.rs`. Signing: Ed25519 (RFC 8032) bound to node identity via `signature_version = 2` (RFC `2026-08-04-node-identity-trust`, ADR-0038). |
| **Recovery Mode** | Recovery access is operational recovery and NOT part of the identity trust chain — it issues no certificates (RFC `2026-08-04-node-identity-trust`). |

---

# 4. Frontend Governance

## F1 — Projection Consumption

Pages consume data through approved projections.

Pages must not reconstruct domain models.

---

## F2 — Contract Usage

Frontend-to-backend communication must occur through documented contracts.

Direct command invocation outside approved contract layers is forbidden.

---

## F3 — Derived State Discipline

Frontend-derived values must remain presentation-oriented.

Business-critical derived values belong to the backend.

---

## F4 — Ownership Integrity

Cross-domain usage requires documented ownership approval.

---

# 5. Backend Governance

## B1 — Domain Logic Authority

Business rules must reside in domain, application, or service layers.

Infrastructure and presentation layers may not become business-rule owners.

---

## B2 — Persistence Integrity

Persisted state must remain internally consistent.

Mutation paths must be explicit and traceable.

---

## B3 — Deterministic Reporting

Reports, aggregates, and summaries must remain reproducible from persisted data.

---

## B4 — Float Safety

Direct equality comparisons on floating-point values representing domain quantities are forbidden.

Use epsilon-based comparisons.

Examples:

* value.abs() < f64::EPSILON
* (a - b).abs() < f64::EPSILON

---

# 6. Runtime Safety

## R1 — Resource Ownership

Long-running operations must not hold shared locks unnecessarily.

Locks should be held only for the minimum required scope.

The primary SQLite connection is serialized through `Mutex<Option<Connection>>`. The only permitted secondary SQLite connection is the persisted rate limiter (ADR-0029); all other secondary write connections are forbidden.

---

## R2 — Failure Isolation

Failures must remain localized.

Partial failures must not silently corrupt domain state.

---

## R3 — Security Controls

Authentication, authorization, and rate-limiting rules must be enforced through dedicated security layers.

Security rules must not be duplicated across unrelated layers.

---

# 7. Documentation Governance

## D1 — Architecture Decisions

Significant architectural decisions require ADR documentation.

All ADRs live under `docs/architecture/` with the naming convention `NNNN-title.md`. The current ADR catalog is tracked in `docs/architecture/ADR_INDEX.md`.

---

## D2 — Canonical Documentation

Only one canonical source may exist for any architectural rule.

Historical documents must clearly identify themselves as historical.

---

## D3 — Documentation Consistency

Documentation must not contradict certified architecture.

When contradictions exist, architecture documentation takes precedence until reconciliation occurs.

---

# 8. Change Control

## C1 — Governance Preservation

Changes must preserve architectural invariants unless an approved architecture decision explicitly changes them.

Items listed in `docs/architecture/ARCHITECTURE_FREEZE.md` Section 2 are frozen and require the RFC-to-ADR process (documented in `docs/architecture/ARCHITECTURE_FREEZE.md` Section 4) before any modification.

---

## C2 — Controlled Exceptions

Exceptions require:

* documented rationale
* documented owner
* documented approval
* defined review lifecycle

Temporary exceptions must be tagged with `[arch:allow-*]` inline comments referencing a tracking issue. All approved exceptions expire after **90 days** unless renewed through a follow-up ADR.

---

## C3 — Drift Prevention

Architecture, ownership, and contract drift must be detectable through governance tooling.

The authoritative drift-detection tool is `scripts/check_arch.ts` (`bun run check:arch`). Zero warnings are tolerated; any warning is treated as a build error.

---

# 9. Certification Requirements

A release may be considered certifiable only when:

* `bun run check:arch` passes with zero warnings
* architecture validation passes
* governance validation passes (`scripts/check_docs_governance.ts`)
* tests pass (`cargo test`, `vitest`, Playwright)
* build passes (`tauri build`)
* secrets check passes (`scripts/check_secrets.ts`)
* release integrity passes (`scripts/check_release_integrity.ts`)
* documented exceptions remain valid and within their expiry window

Additional release requirements are defined in the documentation repository.

---

# 10. Agent Responsibilities

Any human contributor, AI agent, automation workflow, or maintenance process operating in this repository must:

1. Preserve architectural invariants.
2. Respect ownership boundaries.
3. Avoid introducing duplicate business logic.
4. Maintain deterministic behavior.
5. Keep documentation synchronized with architectural reality.
6. Prefer minimal, reversible changes.
7. Avoid speculative refactoring.
8. Treat governance violations as defects.

## Agent Verification Protocol

Before proposing or applying any code change, an agent must:

1. **Read `docs/architecture/ARCHITECTURE_FREEZE.md`** — identify whether the target area is frozen (Section 2). If frozen, the RFC-to-ADR process is required before implementation.
2. **Run `bun run check:arch`** — confirm zero architecture warnings after changes.
3. **Confirm layer placement** — verify the change is in the correct layer per the Layer Map (Section 3 of this document).
4. **Confirm isolation rules** — no SQL outside `repositories/`, no `invoke` outside `src/lib/tauri.ts`, no `async fn` / `std::thread::spawn` in Rust.
5. **Tag exceptions** — if a temporary violation is unavoidable, use `[arch:allow-*]` with an inline comment and a tracking issue reference before submitting.
6. **Do not suppress governance tooling** — disabling or bypassing `check_arch.ts` rules is forbidden.

---

# 11. Governance Tooling

The following tools are the authoritative enforcement mechanisms for this repository:

| Tool | Command | Policy |
|------|---------|--------|
| Architecture audit | `bun run check:arch` | Zero warnings. CI fails on any warning. |
| TypeScript / Svelte | `bun run check` (svelte-check) | Zero errors. |
| Rust lints | `cargo clippy -D warnings` | Zero warnings. Compilation fails. |
| Unit / integration tests | `cargo test` + `vitest` | Zero failures. |
| Secrets detection | `scripts/check_secrets.ts` | Zero hardcoded secrets. |
| Documentation governance | `scripts/check_docs_governance.ts` | Zero broken links / missing refs. |
| Release integrity | `scripts/check_release_integrity.ts` | Zero integrity violations. |
| CI pipeline | `.gitlab-ci.yml` | Full gate — all tools above. |

Reference: `docs/architecture/ARCHITECTURE_FREEZE.md` Section 5 (Zero-Warning Policy).

---

# 12. Exception Mechanism

When a frozen invariant cannot be immediately satisfied, the following process applies:

1. **Tag the code** — add an `[arch:allow-*]` inline comment with a brief rationale and a tracking issue reference.
2. **File an ADR** — create or update `docs/architecture/NNNN-title.md` within 7 days.
3. **Expiry** — all temporary exceptions expire after **90 days**. After expiry, the code must be refactored to comply or a renewal ADR must be filed.
4. **Emergency path** — for production-critical bugs, a temporary fix with `[arch:allow-*]` is permitted, but the full RFC-to-ADR process must follow within 7 days.

Reference: `docs/architecture/ARCHITECTURAL_INVARIANTS.md` Section 3, `docs/architecture/ARCHITECTURE_FREEZE.md` Section 4.

---

# 13. Source of Authority

This document defines permanent repository governance.

Release-specific artifacts, certifications, metrics, approvals, snapshots, observability reports, maintenance policies, and operational procedures are maintained separately within the `/docs` hierarchy.

When a conflict exists between a temporary document and this contract, this contract takes precedence unless superseded by a formally accepted architecture decision record (ADR).

The following documents extend and complement this contract:

| Document | Location | Purpose |
|----------|----------|---------|
| Architecture Freeze Declaration | `docs/architecture/ARCHITECTURE_FREEZE.md` | Frozen contracts and RFC-to-ADR process |
| Architectural Invariants Charter | `docs/architecture/ARCHITECTURAL_INVARIANTS.md` | Technical invariants and exception policy |
| ADR Index | `docs/architecture/ADR_INDEX.md` | All accepted architecture decisions |
| Architecture Audit Script | `scripts/check_arch.ts` | Authoritative governance enforcement tool |
| CI Pipeline | `.gitlab-ci.yml` | Full pre-merge gate |
