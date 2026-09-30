# ADR 0062: Identity Data-Directory Override — `GRPC_IDENTITY_DATA_DIR`

# Status
Accepted (2026-09-30)

> **Governance state.** This ADR is **Accepted** (2026-09-30), promoted from `Proposed` after the
> Step 2 review required by `ARCHITECTURE_FREEZE.md` §4. Implementation was authorized under §4
> Step 3 and carried out against this record. The status progression is the repository convention
> `Draft → Proposed → Accepted → Deprecated → Superseded` (`docs/architecture/ADR_INDEX.md`,
> Policy §3). `# Verification Expectations` remains the normative checklist for the
> implementation; the two E2E-dependent items listed there are not yet executed.

# Date
2026-09-29

# Owner
Architecture / Security

# Reference
- RFC — `docs/architecture/rfcs/2026-09-29-identity-data-dir-override.md` — **Accepted**
  (2026-09-29), carried out by this ADR. The RFC is the historical proposal record and the
  traceability link to this decision; **this ADR is the authoritative status.**
- ADR-0039 (`docs/architecture/0039-adminkey-portable-scrypt.md`) — two-tier secret protection.
  **Unchanged.** The node signing key remains `age::x25519`; `.adminkey` remains the sole
  `age::scrypt` site for portable operator material. This ADR introduces **no** new scrypt site.
- ADR-0041 (`docs/architecture/0041-production-app-key-provisioning.md`) — **precedents
  referenced, not amended**: §1 (environment override is rank 1 and terminal when present —
  the precedence model for this ADR), §2 (`appkey.age` atomic write + `0600` + `create_dir_all`),
  §6 (`initialize_app_key` generation), §8 (fail-closed on key mismatch — the unchanged
  detection boundary), §9 (`get_security_status` already exposes `store_path`), §11.3 (App-Key
  unlock ≠ authentication — untouched).
- RFC `docs/architecture/rfcs/2026-08-04-node-identity-trust.md` — §3.6 item D1 (no re-issue of
  the first ADMIN after `.adminkey` loss) and §3.8 (`.adminkey`). Referenced, not amended.
- `docs/architecture/ARCHITECTURE_FREEZE.md` — §2.1 (infrastructure isolation), §2.6 (SQLite
  topology), §2.7 (two-tier encryption), §2.8 (production error exposure), §3.7 (tests — open),
  §4 (RFC-to-ADR process). **No frozen clause is amended by this decision.**
- `docs/architecture/ADR_INDEX.md` — decision record index; Policy §3 (status progression).
- `AGENTS.md` — P1 (determinism), P2 (single source of truth), P3 (explicit boundaries), R1
  (resource ownership / lock scope), §10 (agent verification protocol), §12 (exception mechanism
  — **not** invoked by this decision).

# Context

Identity-bearing and secret-bearing state has no environment override. The database has one:
`GRPC_DB_PATH`, at `src-tauri/src/db/mod.rs:156`. Identity state has no equivalent, and on
Windows there is no ambient-variable substitute, because `dirs::data_dir()` resolves through the
Known Folder API and ignores `APPDATA`, `LOCALAPPDATA`, and `XDG_DATA_HOME`.

The consequence is that the three identity files all resolve to the ambient platform data
directory and cannot be redirected:

| File | Role | Constant |
|---|---|---|
| `node_identity.key` | WILAYA/UNIT node signing key | `NODE_KEY_FILE_NAME`, `node_key_store.rs:25` |
| `node_identity.key.pending` | Staged (not yet promoted) rotation key | `NODE_KEY_PENDING_FILE_NAME`, `node_key_store.rs:28` |
| `.adminkey` | Portable operator key (ADMIN) | `ADMINKEY_FILE_NAME`, `adminkey_provider.rs:22` |
| `appkey.age` | Passphrase-protected App-Key store | `appkey_store.rs` |

Hermetic Windows E2E identity bootstrap is therefore not achievable. A test run reads and writes
the operator's real identity material, violating P1 (determinism) and risking issuance of
first-ADMIN material against a real installation — which RFC
`2026-08-04-node-identity-trust` §3.6 item D1 forbids after `.adminkey` loss.

**Prerequisite analysis only.** The Known Folder API behavior and the absence of any
`GRPC_IDENTITY_DATA_DIR` in the tree have been verified by inspection. The Windows E2E ceremony
itself has **not** been executed or verified, and this ADR does not claim it has.

Data-directory resolution is also duplicated, and the sites do **not** all serve
identity. Inspection of the current tree establishes **eight** relevant sites: **seven** (A–G)
identity or application-data resolutions, and one independent database resolution (H). Of the
seven, **five** (A, B, C, F, G) are **identity/secret** sites and are in scope for this decision;
**two** (D, E) are **operational-data** sites and are explicitly out of scope.

> **Survey basis.** The table below records the **pre-implementation** code locations and state
> reviewed during ADR-0062 acceptance. Its line numbers are **historical references to that
> surveyed state** and are intentionally **not** current implementation anchors; the shipped
> locations are recorded in `# Implementation Boundary`.

| # | Site | Resolution | Role | In scope? |
|---|---|---|---|---|
| A | `infrastructure/identity/adminkey_provider.rs:33-37` | `dirs::data_dir().join(GRPC_DATA_DIR)` — the original | Identity/secret (`.adminkey`) | **Yes** |
| B | `infrastructure/security/appkey_store.rs:70-74` | verbatim duplicate of (A) | Identity/secret (`appkey.age`) | **Yes** |
| C | `infrastructure/security/mod.rs:134` | `AppKeyStore::new(AppKeyStore::default_data_dir()?)` | Identity/secret (`store_path` projection) — **bypasses the hub** | **Yes** |
| D | `infrastructure/logging.rs:10` | `dirs::data_dir().join("GRPC").join("logs")` — hardcoded literal | **Operational data** (application logs) | **No** |
| E | `application/services/system_stats_service.rs:127,146` | `dirs::data_dir()` (backup-directory discovery) | **Operational data** (backup discovery) | **No** |
| F | `application/services/runtime_bootstrap.rs:69-72` | re-inlines the `common.rs` logic; comment: "mirroring `commands/common::node_key_store`" | Identity/secret (node key store) | **Yes** |
| G | `commands/common.rs:10-12` | `AdminKeyProvider::default_data_dir()` — **the only hub** | Identity/secret (all three stores) | **Yes** |

A separate, independent site governs the database: `db/mod.rs:156,165-167` (`GRPC_DB_PATH`, then
a hardcoded `data_dir().join("GRPC")`). It is out of scope for this decision.

**Why D and E are excluded (normative scope boundary).** Site D resolves `GRPC/logs`; site E
resolves `GRPC/backups` for `SystemStatsService`. Neither reads nor writes `node_identity.key`,
`.adminkey`, or `appkey.age`; both concern operational artifacts with a different lifecycle. The
actual backup writer does not even use `dirs::data_dir()` — `SqliteBackupAdapter::compute_backup_dir`
derives it from the database path (`infrastructure/backup/sqlite_backup_adapter.rs:113-118`) — so
site E is a *read-only discovery* of a directory whose real owner is the database path. Placing
operational directories under an environment contract named `GRPC_IDENTITY_DATA_DIR` would blur the
ownership boundary (`AGENTS.md` A1/A3) and widen the change beyond what hermetic identity bootstrap
requires. **This is a scope decision, not a behavior change:** D and E keep their current behavior
exactly, and this decision introduces **no** application-data resolver.

This duplication is a pre-existing P2 defect and it is the reason a naive env check in
`commands/common.rs` is **unsafe**: `node_key_store()`, `adminkey_provider()`, and
`appkey_store()` would write to the overridden directory while `app_key_status()` reported a
`store_path` from the ambient directory. That is a false "unprovisioned" projection and a
contract divergence.

A naming constraint also exists. `pub const GRPC_DATA_DIR: &str = "GRPC"` is already a live
constant at `adminkey_provider.rs:19`, re-exported at `infrastructure/identity/mod.rs:16`, and
cited by name in `docs/architecture/0041-production-app-key-provisioning.md` §2. Reusing it as an
environment-variable name would collide.

# Decision

## 1. One resolver, owned by `infrastructure/` (normative)

A **single resolver** owned by `infrastructure/` owns identity data-directory resolution. Its normative scope is **identity and secret state only**: the
stores that hold `node_identity.key` (including its `.pending` rotation slot), `.adminkey`, and
`appkey.age`.

Every in-scope consumer listed in `# Context` (A, B, C, F, G) must consume that resolver. The
duplicated `dirs::data_dir()` computations are removed, and hardcoded `"GRPC"` literals are
replaced with the existing `GRPC_DATA_DIR` constant where appropriate. Sites **D** (logs), **E**
(backup discovery), and **H** (database) are **not** consumers of this resolver and are unchanged by
this decision.

`infrastructure/` **must not** import `commands/` (`ARCHITECTURE_FREEZE.md` §2.1). The existing
dependency direction — commands consuming infrastructure — is preserved; `commands/common.rs:10-12`
becomes a consumer of the resolver rather than a second definition of it.

## 2. The `GRPC_IDENTITY_DATA_DIR` contract (normative)

> **Precedence:** `GRPC_IDENTITY_DATA_DIR` (absolute) → `dirs::data_dir()/GRPC`.

| Condition | Behavior |
|---|---|
| Unset | Platform default — identical to current behavior |
| Empty / whitespace-only | Treated as **unset**; `PathBuf::from("")` is prohibited |
| Non-absolute | **Rejected** — `AppError::Configuration` (the current working directory is not a stable base) |
| Absolute, not yet existing | Created on first write (`create_dir_all`), matching current behavior |
| Absolute, existing | Used verbatim |

The variable name is **not** `GRPC_DATA_DIR` (see the naming constraint in `# Context`).

Any error raised by this contract must respect the production error-exposure gate
(`ARCHITECTURE_FREEZE.md` §2.8, ADR-0012): details are gated behind
`#[cfg(debug_assertions)]`. No absolute claims (Rules 32, 36) are permitted in the messages.

## 3. Process-lifetime stability and testability (normative)

Resolution is **stable for the process lifetime**. After the first successful resolution, a later
environment change **must not** cause a second directory to be observed by any consumer in the same
process (`AGENTS.md` R1; P1 — identical inputs must yield identical outputs).

Implementation constraints (to be verified at implementation time; these are **not** claims about
the current tree):

- The **caching mechanism is an implementation-time decision.** The established process-global
  pattern in this tree is `static` + `Mutex` (e.g. `infrastructure/security/mod.rs:32,40-41`).
  `OnceLock` / `OnceCell` / `lazy_static` are **not** established repository patterns — no occurrence
  exists in the tree — and must not be presented as such.
- **No production reset capability.** This decision does not authorize a public, release-build
  function that re-initializes the resolved directory, and tests must not weaken the
  process-lifetime invariant to obtain a fresh value.
- **Tests must use existing isolation patterns.** Environment-mutating tests must use the
  repository's existing synchronization pattern, `SECURITY_TEST_ENV_LOCK`
  (`infrastructure/security/mod.rs:353-372`).
- The implementation must provide a **deterministic test strategy** covering the unset, empty,
  relative, and absolute cases plus the stability rule, **without** exposing a production reset.
  The choice of strategy (per-case isolated test context versus a subprocess per case) is an
  implementation-time decision.

## 4. Profile independence (normative)

The override is honored in **all** profiles, including release. It follows the `GRPC_APP_KEY`
precedent (`docs/architecture/0041-production-app-key-provisioning.md` §1: environment is rank 1
and terminal when present).

The override is **not** gated on `GRPC_ENV`, and **neither is the shadow guard (§5)**. A
profile-gated security condition is silently inert in any environment that does not set that
profile, which is a fail-open on misconfiguration.

**What `GRPC_ENV` actually determines today (verified in the current tree):**

| Check | Actual behavior | Source |
|---|---|---|
| `is_production_mode()` | `production` or `prod` only, case-insensitive | `infrastructure/security/mod.rs:21-26` |
| Deployment readiness | `production` only | `application/services/deployment_readiness_service.rs:239-242` |
| Value `test` | **consumed by no backend branch**; set only by the E2E harnesses | `src/tests/e2e/orchestration/processManager.ts:56`; `src/tests/e2e/drivers/webkitTauriDriver.ts:144` |

`test` is therefore **not** an established backend profile. This decision does **not** create one,
does not add a runtime security gate, and does not weaken any security condition when
`GRPC_ENV=test` is present.

When the override is active, it is logged as a warning and the resolved directory is observable
through the existing `get_security_status` → `store_path` field (ADR-0041 §9). **No secret value is
ever logged.** No new observability surface is added.

## 5. Fail-closed guard against accidental cross-directory shadowing (normative)

**Normative identity files for this decision are, exclusively:** `node_identity.key` (the active
key) and `node_identity.key.pending` (the staged rotation key) — together the node key store —
plus `.adminkey` and `appkey.age`. No other file counts (`*.tmp` staging files, `grpc.db`,
`backups/`, `logs/` are excluded).

**Trigger condition.** When the override is active and the resolved directory contains **none** of
the normative identity files while the default directory contains **at least one**, the application
**must not** report the node as unprovisioned. This is evaluated **before** any identity read or
write, and applies in **all** profiles.

| # | Case | Behavior |
|---|---|---|
| 1 | Nothing in the overridden directory / nothing in the default | Normal first-run behavior; no error |
| 2 | Nothing in the overridden directory / identity material in the default | **Hard error** (fail-closed guard) |
| 3 | Something in the overridden directory / something in the default | **No file comparison.** Each store is read from its own resolved directory. `.adminkey` in the default together with `appkey.age` in the override is **not** an error under this decision. *Implementation-time decision:* if a later review concludes this case warrants treatment, it is recorded in a follow-up ADR |
| 4 | All identity files in both | Normal operation; **no** cryptographic cross-check and **no** content comparison between directories (the tree performs no such cross-directory verification) |
| 5 | Exactly one store present in either location | No pre-emptive error; derived from reading each store from its own directory. *Implementation-time decision:* if "partially populated" must be an explicit case, it is recorded in a follow-up ADR |
| 6 | Directory does not exist | "Does not exist" — no error; created on first write by the existing `create_dir_all` |
| 7 | Filesystem inspection returns an actual I/O error (not "not found") | **Hard error** — never treated as "empty" or "absent" |
| 8 | A path expected to be a directory exists as a non-directory | **Hard error** (type ambiguity) |
| 9 | An identity file path exists as an unexpected filesystem type (directory, link, or device) | **Hard error** (type ambiguity) |

Rationale: RFC `2026-08-04-node-identity-trust` §3.6 item D1 forbids re-issuing the first ADMIN after
`.adminkey` loss, and ADR-0041 §6 would otherwise generate a new App Key against a wrong directory.

**Limitation of the guarantee (normative).** This is a **fail-closed guard against accidental
cross-directory shadowing** — a **point-in-time** filesystem inspection performed before identity
access. It is **not** an atomic filesystem invariant and it provides **no** protection against
concurrent modification (TOCTOU). **No cross-process filesystem lock is introduced by ADR-0062.** The
security claim is confined to detecting misconfiguration before the node is reported unprovisioned.

## 6. Preserved custody, crypto, and write invariants (normative — unchanged)

- `node_identity.key` — `age::x25519`, node-managed, never leaves the node (ADR-0039 §1).
- `.adminkey` — `age::scrypt`, portable operator material (ADR-0039 §1, §3). Contents and format
  unchanged, so operator portability is preserved.
- `appkey.age` — atomic write, `0600`, `create_dir_all` (ADR-0041 §2).
- A mismatched key surfaces as undecryptable node material — the correct detection boundary is
  unchanged (ADR-0041 §8).
- **Fail-closed identity resolution — the one behavior this decision changes at the command
  layer (see `# Consequences`).** The `temp_dir().join("GRPC")` fallback that formerly stood in
  for a resolution failure at site G (`commands/common.rs`) is **removed**: the three identity
  store helpers return `AppResult` and propagate `AppError::Configuration` raised by the §2
  contract or tripped by the §5 guard, so a rejected or shadow-conflicting override can never
  redirect identity material to an unintended directory. Site F
  (`application/services/runtime_bootstrap.rs`) **retains** its pre-existing
  `temp_dir().join("GRPC")` fallback for the single condition it was written for — an
  unresolvable platform data directory, i.e. `AppError::Internal(_)` — and propagates every
  other error class. All custody, algorithm, atomicity, and `0600` invariants listed above are
  unchanged.
- **No new `age::scrypt` site.** The two-site allow-list in `ARCHITECTURE_FREEZE.md` §2.7 and
  Rule 38 is untouched; no `scripts/check_arch.ts` change is required or permitted.
- **No format or filename changes** for any secret.
- The App-Key resolution hierarchy (ADR-0041 §1, §11.4) is untouched; this decision does not
  touch authentication or sessions (ADR-0041 §11.3, ADR-0050).

## 7. `GRPC_DB_PATH` remains independent (normative)

`GRPC_DB_PATH` (`db/mod.rs:156`) stays a separate, unchanged override. It is **not** unified with
identity data-directory resolution. SQLite topology (`ARCHITECTURE_FREEZE.md` §2.6), WAL mode, and
the single-writer `Mutex<Option<Connection>>` are unaffected: this decision changes a path, not a
connection.

## Consequences

**Enables:**
- Hermetic Windows E2E identity bootstrap: `node_identity.key`, `.adminkey`, and `appkey.age`
  can all relocate to a per-run temporary directory. This is the prerequisite for the E2E
  ceremony; the ceremony itself is verified only during implementation and testing.
- P2 improves: the five duplicated **identity** resolution sites (A, B, C, F, G) collapse into one
  owned resolver. This is independently valuable and landable as a pure refactor with no behavior
  change.

**Costs and obligations:**
- A wrong override in production now fails closed instead of silently presenting a fresh,
  unprovisioned node. This is a deliberate behavior change relative to the ambiguous status quo.
- The command-layer degradation path is removed: a resolver error at site G previously produced a
  store rooted at `temp_dir()/GRPC`, which could have directed identity writes at a directory no
  consumer intended. The helpers now propagate, so such a failure aborts the operation. Site F's
  `Internal`-only fallback is unchanged (§6).
- The guard is point-in-time and introduces no cross-process lock; concurrent modification during
  the check is outside its guarantee (§5).
- The resolver becomes a single point of resolution **for identity/secret state**; future
  identity-bearing files must use it rather than calling `dirs::data_dir()` directly. Operational
  directories (logs, backup discovery) are explicitly **not** covered and remain unchanged.
- A new environment contract exists in the process environment. It grants no privilege the
  environment does not already grant: an actor who can set the process environment can already set
  `GRPC_APP_KEY` (ADR-0041 §1, rank 1) and thereby reach every node-managed secret.
- Reversal is trivial — unset the variable.
- `docs/architecture/ADR_INDEX.md` must gain an entry when this ADR is accepted.

# Explicitly Out of Scope

1. **Unifying `GRPC_DB_PATH`** with identity data-directory resolution — a separate decision.
2. **Any format or filename change** for `.adminkey`, `node_identity.key`, or `appkey.age`.
3. **Any `age::scrypt` allow-list change** (`ARCHITECTURE_FREEZE.md` §2.7, Rule 38). No new site.
4. **Reparse-point / junction hardening** of the override path — **warn-only, no additional
   enforcement.** The directory the operator names carries its own security; constraining the
   operator's choice is beyond this change.
5. **`node_identity.key` file-mode hardening** — **not changed and not decided by this ADR.** The
   existing file-mode question is carried forward as a separate follow-up (see below). No parity
   with `appkey.age`'s `0600` is asserted or required here.
6. **Any additional environment variable**, a new backend profile, or a new gating pattern on
   `GRPC_ENV` — including any weakening of a security condition when `GRPC_ENV=test` is present
   (§4, §5).
7. **Any modification to `scripts/check_arch.ts`** or any other governance script.
8. **Any `[arch:allow-*]` exception.** None is required or added: this decision fills an
   unspecified item rather than excusing a frozen-contract violation.
9. **Any change to the security model or authentication path** (ADR-0041 §11.3, ADR-0050).
10. **Verification that the Windows E2E ceremony passes.** Only the prerequisite analysis has
    been verified.
11. **Relocating operational data directories** — `GRPC/logs` (site D) and `GRPC/backups`
    (site E) — under the identity contract. A separate decision; this ADR introduces no
    application-data resolver.

# Implementation Boundary

Per `docs/architecture/ARCHITECTURE_FREEZE.md` §4, implementation is gated on acceptance
(Step 3) and must pass the full governance gate (Step 4).

**In scope for a later implementation change:**

| Area | Sites |
|---|---|
| New resolver | one module under `src-tauri/src/infrastructure/` — placement resolved at implementation time to `infrastructure/identity/data_dir.rs` (see below) |
| Convergence (identity only) | `commands/common.rs:11-12`; `infrastructure/identity/adminkey_provider.rs:34-36`; `infrastructure/security/appkey_store.rs:70-72`; `infrastructure/security/mod.rs:135`; `application/services/runtime_bootstrap.rs:133-140` |
| Command-layer propagation (required by the §2/§5 fail-closed change) | `commands/common.rs` — the three store helpers now return `AppResult` and the `temp_dir()/GRPC` fallback is removed (§6); and the following files, changed **only** to propagate that resolver result at existing call sites, with no new business logic and no new command: `commands/fiscal.rs`; `commands/identity.rs`; `commands/import_export.rs`; `commands/operational.rs`; `commands/security.rs` |
| E2E harness | `src/tests/e2e/orchestration/processManager.ts`; `src/tests/e2e/drivers/webkitTauriDriver.ts`; the direct-spawn environment in `src/tests/e2e/runtime/startup.spec.ts` |
| Governance docs | `docs/architecture/ADR_INDEX.md` (on acceptance); `docs/architecture/rfcs/2026-09-29-identity-data-dir-override.md` (status reconciled on acceptance); changelog |

The resolver landed as `src-tauri/src/infrastructure/identity/data_dir.rs`, exported from
`infrastructure/identity/mod.rs`. Placement satisfies `ARCHITECTURE_FREEZE.md` §2.1: it lives in
`infrastructure/` and imports no `commands/` module.

**Out of scope for implementation:** `infrastructure/logging.rs` (site D) and
`application/services/system_stats_service.rs` (site E) — **no change**; `db/mod.rs`
(`GRPC_DB_PATH` untouched); every authentication, session, and crypto code path; all
product/front-end behavior.

**Suggested sequence.** Step 1 (resolver convergence, no behavior change) → Step 2 (the
environment contract) → Step 3 (E2E harness) → Step 4 (governance). Steps 1–3 are additive.

# Verification Expectations

The implementation must satisfy the full governance gate (`AGENTS.md` §11): `bun run check:arch`
with zero warnings, `cargo test`, `cargo clippy -D warnings`, `bun run check`, `vitest`,
`scripts/check_docs_governance.ts`, `scripts/check_secrets.ts`,
`scripts/check_release_integrity.ts`, and `bun run tauri build`.

Additionally, this decision requires at minimum:

1. **Unset fallback** — resolution equals `dirs::data_dir()/GRPC`.
2. **Empty value** — empty and whitespace-only are treated as unset; `PathBuf::from("")` is never
   constructed.
3. **Relative-path rejection** — a relative value yields `AppError::Configuration`, with details
   gated per §2.8.
4. **Absolute-path resolution** — an absolute value is used verbatim.
5. **Process-lifetime stability** — repeated resolution in one process returns an identical result
   even if the environment changes mid-process, and the test strategy (§3) verifies this without a
   production reset capability.
6. **Shadow guard** — an empty override directory against a populated default directory is a hard
   error **in every profile**, including when `GRPC_ENV=test` is present.
7. **Filesystem edge cases** — per §5: an actual I/O error, a directory path that is a non-directory,
   and an identity path of an unexpected type each produce a hard error; a missing directory does not.
8. **Cross-store coherence** — the node key store, the `.adminkey` provider, and the `appkey.age`
   store resolve to the same directory within one process.
9. **`app_key_status().store_path` coherence** — the projection reflects the override. This
   regression test targets the bypass at `infrastructure/security/mod.rs:135`.
10. **Operational sites unchanged** — `infrastructure/logging.rs:10` and
    `application/services/system_stats_service.rs:127,146` still resolve `dirs::data_dir()/GRPC/**`
    regardless of `GRPC_IDENTITY_DATA_DIR`.
11. **No ambient identity files created by Windows E2E** — a run creates no identity file outside
    its temporary directory.
12. **Unset-variable regression** — with the variable unset, behavior is identical to the current
    behavior.

Items 11 and 12 must not be reported as passing until the E2E ceremony has actually been executed;
this ADR records them as required, not as verified.

# Follow-Up Carried Forward

**1. `node_identity.key` file mode.** In the write path observed at
`src-tauri/src/infrastructure/identity/node_key_store.rs:70`, `std::fs::write` is used **without an
explicit mode**, unlike `appkey.age`, which enforces `0600` per ADR-0041 §2. **This ADR does not
change, decide, or require parity for that mode.** A full inventory of write paths was not
completed for this record; a **separate follow-up ADR** must determine the intended mode on POSIX
platforms, accounting for the differing filesystem semantics on Windows. This item must not be
bundled into ADR-0062.

**2. Operational data directories.** Site D (`GRPC/logs`) and site E (`GRPC/backups` discovery) are
outside the identity contract by this decision. If operational directories later require their own
override, that is a separate ADR. Note for that future record: the backup **writer** derives its
directory from the database path (`infrastructure/backup/sqlite_backup_adapter.rs:113-118`), so site E
is currently a read-only discovery that can already disagree with the writer when `GRPC_DB_PATH` is
set — a pre-existing observation, not created or addressed by ADR-0062.
