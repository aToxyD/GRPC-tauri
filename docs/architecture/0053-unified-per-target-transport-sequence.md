# ADR 0053: Unified Per-Target Transport Sequence

# Decision Status

**Status: Accepted**
**Ratification Date: 2026-08-24**

**ACCEPTED — 2026-08-24 (Owner Ratification — Governance Gate, SEC-031 design approval / SEC-032 implementation).**

This ADR converts the SEC-031 architecture investigation verdict (**GO — IMPLEMENTATION DESIGN APPROVED**, "Design A") into accepted governance and directs a single canonical producer-side change: ONE contiguous transport sequence stream per `(issuer_identity_id, target_node_id)` across ALL pipeline-imported TransportGuard package kinds.

| Item | Status |
|------|--------|
| Unified producer allocator keyed `(issuer_identity_id, target_node_id)` | **ACCEPTED / OWNER RATIFIED** |
| Consumer `TransportGuard` / `sync_issuer_sequence` semantics | **FROZEN — byte-immutability required** |
| Retirement of fragmented producer allocator tables (006/009/010 streams) | **ACCEPTED** |
| Controlled pre-release development reset of producer sequence state | **ACCEPTED** |

---

## 1. Problem

The producer allocation scopes diverged from the frozen consumer TransportGuard scope.

The consumer side enforces one contiguous, kind-blind continuity check per issuing identity against its local ledger (`sync_issuer_sequence`): for an empty ledger the first accepted sequence MUST be exactly 1, and every subsequent acceptance MUST be exactly `last + 1`, regardless of package kind (`transport_guard.rs`). The producer side, however, allocates export sequences from THREE different scopes simultaneously:

1. **Global per-issuer ledger** (`sync_issuer_sequence_state`, migration 006) — products, daily_report, monthly_summary, stock_movements.
2. **Per-target ledger for one kind only** (`identity_access_export_sequence`, migration 009 — ADR-0045 §26.9 F-1 Option A) — `identity_access` only (now fail-closed/dead at the command boundary since the ADR-0051 D1 cutover).
3. **Dedicated issuer-only ledger for one kind** (`admin_access_export_sequence`, migration 010 — ADR-0051 §7) — `admin_access` only.

Consequence (empirically proven by SEC-030): a fresh UNIT that receives `admin_access #1` then rejects `products #1` as Replay (expected 2), or accepts `products #1` first and then rejects `admin_access #1`. The producer's fragmented counters cannot satisfy the consumer's single counter.

## 2. Root Cause

Two successive governance decisions each solved a local bootstrapping problem by fragmenting producer allocation scope, while the consumer remained a single per-issuer, kind-blind stream:

- **ADR-0045 §26.9 (F-1 Option A, 2026-08-15)** changed `identity_access` to per-target allocation `(issuer_identity_id, target_unit_code)` so every fresh UNIT could receive a sequence-1 bootstrap package. The other kinds were explicitly left on the global per-issuer ledger ("Decision point 2").
- **ADR-0051 §7 (2026-08-22)** introduced a *dedicated* issuer-only stream for `admin_access`, deliberately decoupled from every other kind's numbering, based on a broadcast model (one artifact, many consumers) that is incompatible with any lagging-UNIT scenario once kinds share one consumer continuity chain.

Both decisions preserved their local invariants but composed into a producer/consumer scope mismatch: three producer counters feed one consumer counter.

## 3. Decision

Replace the fragmented producer allocators with ONE canonical allocator scoped by `(issuer_identity_id, target_node_id)` across all TransportGuard pipeline-imported kinds:

```text
products, daily_report, monthly_summary, stock_movements,
admin_access, trust   (and any future pipeline kind)
```

Allocation semantics (unchanged contract, new key):

- First allocation for a pair ⇒ sequence `1`.
- Existing pair ⇒ `last_issued_sequence + 1`.
- Same issuer + different target ⇒ independent streams.
- Different issuer + same target ⇒ independent streams.

### 3.1 Consumer remains unchanged (normative)

The receiving node keeps:

```text
sync_issuer_sequence          — consumer ledger, keyed by issuer_identity_id
expected = last_applied + 1   — kind-blind continuity
```

Each receiving node's local ledger implicitly represents `target_node_id = self`: a node only ever observes packages addressed to it, so the consumer has always been per-target-by-self. The unified producer scope therefore makes producer and consumer scopes structurally identical. NO consumer file changes: `transport_guard.rs`, `sequencing.rs`, `sync_applied_packages.rs`, migration 004, `import_*_package_impl`, `run_import_pipeline`/`run_import_pipeline_core` must remain semantically untouched. Any required consumer change is a STOP condition of this decision's implementation.

### 3.2 `.unit` remains outside the system

`.unit` bootstrap packages keep fixed `package_sequence = 1`, allocate nothing, commit nothing, and never touch any transport ledger (RFC §7 Bootstrap exception, ADR-0044 A44-08). Unchanged.

### 3.3 Canonical target resolution (normative)

The effective allocation key `target_node_id` MUST be resolved authoritatively from local server-side state:

| Recipient class | `target_node_id` source |
|-----------------|------------------------|
| UNIT recipient (WILAYA-issued kinds: products, admin_access, trust) | `units.code` — validated against authoritative `units` rows |
| WILAYA recipient (UNIT-issued kinds: daily_report, monthly_summary, stock_movements) | `settings.wilaya_code` |

NEVER permitted as a transport target:

- `unit_name` (display name, not a security identifier);
- renderer-provided arbitrary target identifiers that bypass backend validation;
- `issuer_identity_id` used as a target;
- `source_node_id` used as a target (it is the sender dimension).

A selected code that does not resolve to an authoritative row fails closed. There is no silent fallback to unit names or placeholder codes.

### 3.4 Allocation discipline (reaffirmed, normative)

1. **Advance-on-success remains mandatory** — the ledger advances ONLY after the package file is successfully built, signed, encrypted, and written (`commit()`).
2. **Failed package generation MUST NOT consume a sequence** — a dropped pending token leaves the ledger untouched; a retry reuses the same number.
3. **No gaps are permitted** — combined with advance-on-success, accepted producer sequences for a live stream are issued without holes.
4. **Single-writer serialization remains relied upon** — allocation spans `begin_export → build → commit` under the process-wide SQLite writer mutex (`Mutex<Option<Connection>>`); no separate concurrency mechanism is introduced, and none is needed.

## 4. Core Invariant

> For every issuer `I` and target `T`, the set of package sequences accepted by target `T` from issuer `I` forms exactly `1..n` — contiguous, gapless, replay-protected — **independent of package kind**.

Producer issuance mirrors this invariant per `(issuer_identity_id, target_node_id)` stream so that a conforming consumer never observes a violation.

## 5. What Remains Untouched (normative exhaust list)

- `TransportGuard` and its sequencing logic (`application/sync_integrity/transport_guard.rs`, `sequencing.rs`);
- consumer ledger table `sync_issuer_sequence` and `sync_applied_packages` (migration 004 semantics);
- import pipeline semantics (`run_import_pipeline`, `run_import_pipeline_core`, every `import_*_package_impl`);
- V2 verification and Ed25519 signature verification (ADR-0047/0048);
- authorization layer (`application/authz/`) — policies, actions, role gates;
- B8 first-import predicates (`admin_access_first_import_predicates_service.rs`);
- D1 `identity_access` fail-closed stubs (export AND import remain dead — this ADR does NOT revive them);
- `.unit` bootstrap semantics (ADR-0044);
- package wire format, metadata schema, canonical JSON, integrity hash, age encryption.

## 6. Security Rationale

This decision fixes a **producer/consumer scope mismatch** — a correctness defect in the transport ordering plane. It does NOT relax replay protection anywhere:

- The consumer guard still rejects duplicates (Replay), regressions (Replay), and skips (OutOfOrder) exactly as before.
- Per-target producer streams do not create issuer-substitution, cross-UNIT escalation, or trust-bypass surfaces: the target dimension selects which continuity chain a package belongs to, while authentication remains entirely V2-signature/anchor-based.
- Sequence numbers carry no authorization meaning; aligning producer scope with consumer scope removes an availability/liveness failure (fresh UNITs unable to bootstrap through legitimate ordered traffic) without granting any new accept capability.
- Targets are validated authoritatively server-side, so a compromised renderer cannot steer sequence allocation to a forged stream.

## 7. Rejected Alternatives

### 7.1 Design C — kind-scoped consumer streams (REJECTED)

Splitting the consumer ledger per package kind (`(issuer, kind)` keys, one continuity chain per kind) would let the fragmented producer scopes stand as-is. Rejected because:

1. It modifies the FROZEN consumer mechanism (`ARCHITECTURE_FREEZE §2.7` Trust/Registry Transport Guard contract; RFC §3.4.1) — requiring weakening of a certified security control rather than correcting the deviating side.
2. It breaks the ratified cross-kind continuity guarantee demonstrated by RFC §3.4.3 (`Registry→Trust→Inventory→Trust = 100..103` on one chain) and re-tested by `rfc_343_sequence_is_consistent_across_kinds`.
3. It multiplies replay-ledger state per node by the kind count and re-opens SEC-030-class composition questions at every future kind addition (each new kind would need a consumer-side migration).
4. The consumer is the certified, tested, frozen half of the protocol; the producer allocator is internal implementation detail. Fixing the unfrozen side is strictly safer.

### 7.2 Retaining independent `admin_access` / products counters (REJECTED)

Keeping the dedicated `admin_access` issuer-only counter alongside a global products counter is mathematically incompatible with the frozen consumer continuity. Formal statement:

Let consumer ledger state be `L(I) = last accepted sequence from issuer I` (single value, kind-blind). Let producer counters be `P_admin(I)`, `P_data(I)` evolving independently. For the consumer to accept both streams, every interleaving must satisfy `next_of_stream == L(I) + 1`. After both streams issue at least once (`P_admin ≥ 1`, `P_data ≥ 1`), whichever counter issues second presents a value already consumed by the other stream's acceptance ⇒ guaranteed Replay rejection for some legal emission order (empirically confirmed by SEC-030 scenarios C/B/G). No assignment of independent starting offsets can satisfy `∀ interleaving: seq = L+1`, because two monotone producers cannot track one shared consumer variable. Therefore the counters MUST share one stream; given targets exist for all WILAYA-issued kinds (§3.3), the shared stream is the unified per-target allocator.

## 8. Compatibility / Reset Policy

- This is a **pre-release controlled development reset**. The application has no production deployments (production remains gated by A44-06 Root-key certification), so there is no deployed fleet whose artifacts must survive the schema change.
- All pre-SEC-031 sync artifacts are declared **VOID**: previously exported packages (any V2 kind) must be regenerated after this change if they are to be imported into post-change nodes. Import paths reject stale-sequence legacy artifacts deterministically (Replay/OutOfOrder) — which is correct behavior for void artifacts, not data corruption.
- No compatibility bridge, alias table, or wire-format version bump is introduced: the package format itself is unchanged; only producer-side numbering state is reset.
- Mixed old-producer/new-consumer fleets cannot arise pre-release; the single-instance offline-first deployment model means a node upgrades as a unit.

## 9. Migration & Recovery

**Migration 011** (`011_transport_export_sequence.sql`) executes inside the standard transactional migration runner:

```sql
CREATE TABLE transport_export_sequence (
    issuer_identity_id TEXT NOT NULL,
    target_node_id     TEXT NOT NULL,
    last_issued_sequence INTEGER NOT NULL CHECK (last_issued_sequence >= 1),
    updated_at TIMESTAMP,
    PRIMARY KEY (issuer_identity_id, target_node_id)
);

DROP TABLE IF EXISTS sync_issuer_sequence_state;        -- migration 006 producer stream
DROP TABLE IF EXISTS identity_access_export_sequence;   -- migration 009 producer stream
DROP TABLE IF EXISTS admin_access_export_sequence;      -- migration 010 producer stream
```

- The runner applies migrations via `execute_batch` inside `unchecked_transaction` (db/migrations.rs), so multi-statement scripts including `DROP TABLE` are supported natively — no framework extension is required.
- **Consumer tables are NOT touched**: `sync_issuer_sequence` and `sync_applied_packages` (migration 004) remain byte-identical.
- Recovery: because producer sequence rows are regenerable issuance bookkeeping (not business data), recovery after a failed migration is the standard pre-migration backup restore path (`maybe_pre_migration_backup` runs automatically before applying version 11). Post-reset, streams rebuild naturally from sequence 1 per `(issuer, target)`, consistent with §8 void-artifact policy.
- Rollback of the DECISION (not the migration) would require a new ADR; migration 011 is not reversible in place (retired tables stay retired).

## 10. Superseded Governance (historical preservation)

- **RFC 2026-08-04-node-identity-trust §3.4.1** originally described a single per-issuer producer stream. That historical record is preserved; the canonical post-ADR-0053 allocation scope is `(issuer_identity_id, target_node_id)` across all pipeline kinds (amendment noted in-place in the RFC).
- **ADR-0045 §26.9 (F-1 Option A)**: the per-target allocation pattern for `identity_access` was CORRECT in direction and is hereby generalized into the canonical allocator. `identity_access` itself remains dead (D1); nothing in this ADR revives it.
- **ADR-0051 §7**: the dedicated `admin_access` producer stream is superseded; `admin_access` joins the unified per-target transport stream with `target_node_id = units.code`. Its historical rationale (broadcast simplicity) is preserved above (§2, §7.2).

## 11. Consequences

- One producer repository (`TransportExportSequenceRepository`) replaces three; call sites migrate mechanically; obsolete repositories are deleted after call-site migration.
- Export commands gain explicit, backend-validated UNIT target selection where the exporter issues to UNITs (products, admin_access); UNIT-issued exporters infer the WILAYA target from `settings.wilaya_code`.
- WILAYA rotation emits one trust package per authoritative active UNIT (per-target streams), replacing the single rotation trust artifact.
- Frontend export flows for products/admin gain authoritative UNIT selection consistent with existing UI conventions.
- Test obligations: unified-stream regression suite (first-is-one, per-target isolation, interleaving across kinds, replay/gap negatives, no-burn-on-failure, `.unit` isolation, trust rotation per-UNIT, D1 deadness) plus all existing suites remaining green (SEC-021/022/026/029 included).

> **Amendment (2026-08-26, SEC-033)** — the two bullets above describing
> operator-side target selection are refined as follows: products and
> admin_access exports are **fleet-level** at the command/UI boundary. The
> renderer expresses fleet intent only (no `unit_code` parameter, no UNIT
> selector); the backend enumerates the authoritative target set from local
> `units` rows and emits one signed artifact per target (`-<unit_code>`
> suffix for multi-target, requested path preserved for single-target;
> zero registered UNITs fail closed). Every normative element of this ADR —
> per-target `(issuer_identity_id, target_node_id)` allocation, validated
> authoritative targets, frozen consumer guard — is unchanged. The historical
> text above is preserved verbatim.

## 12. Freeze References

- Amends (producer side only): RFC `2026-08-04-node-identity-trust` §3.4.1; ARCHITECTURE_FREEZE §2.7 (reference note); ADR-0045 §26.9; ADR-0051 §7.
- Preserves unchanged: ARCHITECTURE_FREEZE §2.2 (B8 exemptions), §2.7 (consumer guard contracts), ADR-0038, ADR-0044, ADR-0046, ADR-0047/0048, ADR-0051 (all non-§7 clauses), ADR-0052.
