# ADR 0043: Backup Command Async Exception (Permanent)

# Status
Accepted (2026-08-09)

# Date
2026-08-09

# Owner
Architecture

# Reference
- RFC — `docs/architecture/rfcs/2026-08-09-backup-async-exception.md` — **approved** (2026-08-09).
- ADR-006 (`docs/architecture/0026-no-async-runtime.md`) — **partially superseded** by this
  decision for the two backup command handlers only (decision items 1–2).
- ARCHITECTURE_FREEZE.md §2.6 ("🔒 No async", line 144) — **amended** by this decision.
- ADR-0030 (`docs/architecture/0030-adr-exception-governance.md`) — **amended** by this
  decision for the `[arch:allow-async]` exception: registered as **Permanent**, exempt from
  the 90-day renewal cycle. All other exceptions keep the 90-day policy.
- Rule 117 (`scripts/check_arch.ts` §1677) — **amended** by this decision.
- `AGENTS.md` Runtime Tenet #2 — unchanged as a general rule; this ADR is the sanctioned
  carve-out (per `AGENTS.md` §13, an accepted ADR supersedes the contract where conflict exists).

# Context

`src-tauri/src/commands/backup.rs` contains the **only** async surface in the Rust tree:

- `create_backup` — `pub async fn` at line 46.
- `restore_backup` — `pub async fn` at line 357.
- `tauri::async_runtime::spawn_blocking` at lines 193 and 456.
- `.await` at lines 194 and 459.

The async is used exclusively to offload blocking backup/restore file I/O (database copy +
encryption) from the Tauri command thread via `spawn_blocking`. In Tauri 2, synchronous
commands run on the main thread; a synchronous backup/restore implementation would freeze the
UI during the operation. This trade-off was already anticipated in ADR-006
("Consequences-Harder": CPU-bound operations block the Tauri command thread).

The violation of the no-async freeze (ARCHITECTURE_FREEZE.md §2.6, tenet #3, AGENTS.md Tenet #2,
ADR-006) is currently **undetected** by governance tooling: Rule 117 only scans
`src-tauri/src/infrastructure/sqlite_runtime_review/**/*.rs` and does not cover `commands/`.
This is undrift per AGENTS.md C3 — present since the initial commit (`a93a96b`); the backup
deadlock fix is in `e0f0cca` (2026-06-03).

# Decision

Grant a **narrow, permanent** architectural exception to the no-async freeze, via the
RFC-to-ADR process (ARCHITECTURE_FREEZE.md §4):

1. **Async is permitted at exactly two command entry points** in
   `src-tauri/src/commands/backup.rs`: `create_backup` and `restore_backup`.
2. **Permitted construct only:** `tauri::async_runtime::spawn_blocking` for blocking
   backup/restore file I/O. This is the sanctioned reason — keeping the UI responsive while
   the blocking I/O runs off the command thread.
3. **No async may leak** to Domain (`src-tauri/src/domain/ports/backup.rs` stays fully
   synchronous), to Application/Services, or to any other Command.
4. The exception is **permanent**: `[arch:allow-async]` tags are registered as `Permanent` in
   `docs/architecture/adr_exception_registry.md` and are exempt from the 90-day renewal cycle
   (ADR-0030 amendment). All other suppressions keep the 90-day policy.
5. **Enforcement:** Rule 117 is broadened to scan all `src-tauri/src/**/*.rs`, with a
   **file-specific** exclusion for `src-tauri/src/commands/backup.rs` (exact path, not a
   `commands/*` pattern). Engine enforcement routes `[arch:allow-async]` through the same
   metadata + ADR-linkage lifecycle as all other suppressions (B3-2).

# Scope

**In scope:**
- `src-tauri/src/commands/backup.rs` — the two command handlers `create_backup` (:46) and
  `restore_backup` (:357), and their `spawn_blocking` calls (:193/:456).

**Out of scope (remains forbidden):**
- Any other command, service, repository, domain, application, or infrastructure layer.
- `async fn`, `await`, `tokio`, `futures`, `async-std`, `smol` anywhere outside the two entry
  points.
- Any secondary async runtime, background threads, or new `spawn_blocking` call sites.

# Alternatives

## Option B — Refactor backup/restore to fully synchronous commands (rejected)
Removing async/spawn_blocking would make the code literally compliant with the freeze with no
exceptions. **Rejected** because Tauri sync commands run on the main thread: blocking
DB-copy + encryption would freeze the UI during backup/restore — the exact regression that the
async implementation was introduced to prevent (documented in ADR-006 Consequences-Harder).

## Option C — Use `std::thread::spawn` instead of async (rejected)
Background threads are also forbidden (AGENTS.md Tenet #3; Rules 101/108/116 ban
`std::thread::spawn` in observability/runtime/runtime-review layers). This would substitute one
frozen-contract violation for another, with additional join/handoff complexity.

## Option D — Keep the drift undocumented (rejected)
Leaves a frozen-contract violation invisible to `check:arch` — exactly the undetected-drift
state C3 exists to prevent. No governance enforcement, no audit trail.

# Consequences

**Easier:**
- The deviation becomes declared, bounded, and governable: documented in the freeze §2.6,
  registered as `Permanent`, and enforced by a Rule 117 that now sees async across all Rust.
- UI responsiveness during backup/restore is preserved (the operational reason for async).
- The 90-day renewal cycle is not diluted for ordinary exceptions — only the single
  `[arch:allow-async]` tag family is exempt, and only when it satisfies ADR linkage.

**Harder:**
- The freeze §2.6 text must be kept precise so the carve-out cannot be read as "async is now
  optional everywhere".
- Rule 117's file-specific exclusion must remain an exact-path match; any broadened pattern
  would re-open the gate.
- Future code review must not extend the async surface beyond the two entry points; the
  governance test suite guards this.

# Governance / Enforcement

1. **Rule 117 (`scripts/check_arch.ts`):** broaden patterns to
   `["src-tauri/src/**/*.rs"]`, keep the async regex (extended with `spawn_blocking`), and add
   the file-specific exclusion:
   ```ts
   (f) => !f.includes("src-tauri/src/commands/backup.rs")
   ```
2. **Suppression metadata (`src-tauri/src/commands/backup.rs`):** add `[arch:allow-async]` tags
   above `create_backup` (:46) and `restore_backup` (:357) with `Reason`/`Date`/`Owner` and
   `see ADR-0043` linkage.
3. **Engine (`scripts/governance/invariants/runtimeSafety.ts`):** add `"async"` to
   `rustTagPatterns` so the tags flow through `validateSuppressionMetadata` (B3-2 lifecycle).
4. **Registry (`docs/architecture/adr_exception_registry.md`):** add an `[arch:allow-async]`
   section with rows for the two call sites, marked `Permanent`, ADR-0043.
5. **Validator (`scripts/governance/suppression.ts`):** a tag is exempt from 90-day expiry
   **only** when it carries a `Permanent: ADR-NNNN` marker referencing a registered/valid ADR
   in `PERMANENT_ADRS` (seeded with `0043`), has full metadata (Reason/Date/Owner), and
   satisfies `see ADR-0043` linkage. The validator must resolve the referenced ADR — it must
   **not** grant permanence based on the marker string alone. All other tags keep the 90-day
   default.

# Migration / Expiry Policy

- The exception is **permanent** — no 90-day renewal.
- `PERMANENT_ADRS` is a curated set that must be kept in sync with registered permanent ADRs;
  `0043` is its first entry. Any future permanent exception requires its own accepted ADR and
  a corresponding registry row.
- Temporary exceptions (all other `[arch:allow-*]` tags) remain on the 90-day cycle per
  ADR-0030.

# Rollback

If the exception must be withdrawn: remove the `[arch:allow-async]` tags and registry rows,
revert the freeze §2.6 amendment, revert Rule 117 patterns, and revert the validator change.
Backup commands remain functional throughout — the tags are comments only; no behavior change.

# Backward Compatibility

Zero behavior change. The tags are comment-only; the `spawn_blocking` flow is untouched;
frontend contracts (`create_backup`/`restore_backup` via `src/lib/tauri.ts` and
`src/lib/contracts/backup.contract.ts`) are unchanged.
