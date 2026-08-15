# ADR 0045: B8 First `identity_access` Import — Bootstrap Authorization Exemption

# Decision Status

**ACCEPTED — 2026-08-14 (Freeze §4 Step 3 — Final Freeze/ADR Status Governance Gate)**

> **Historical record (preserved):** this document began **PROPOSED — Decision for Owner Approval** (2026-08-14). Owner ratification of all decisions (A45-01..12) completed via the Owner Ratification Completion gate (canonical record §26.6); the RFC amendment was executed documentation-only (§26.7, RFC §3.10/§3.12 + ADR-0038 §6 mirror); **Freeze §4 Step 3 is now closed**: status **ACCEPTED** + freeze document updated (§2.2 B8 exemption).

Acceptance is **architectural only**: implementation is **NOT AUTHORIZED** (requires a separate implementation-authorization gate) and production readiness remains **BLOCKED** by the external prerequisite A44-06 (production Root-key certification). No code, tests, or configuration were modified. Status vocabulary used throughout: **DECIDED / BLOCKED / PENDING / NOT AUTHORIZED / OUT OF SCOPE / REJECTED**.

| Item | Status |
|------|--------|
| Option A — first-import authorization exemption (architecture) | **OWNER RATIFIED** with conditions (owner ratification gate 2026-08-14 — see §26) |
| Implementation of any option | **NOT AUTHORIZED** |
| RFC amendment | **NOT AUTHORIZED** at this gate |
| ADR-0044 amendment | **NOT AUTHORIZED** at this gate |
| `.unit role=Admin` as bootstrap mechanism | **REJECTED** (owner-confirmed — see §26 A45-08) |
| Options C / D / E | **REJECTED** (see §9) |
| Cross-ADR consistency (ADR-0044 ↔ ADR-0045) | **CONSISTENT** (reconciled — see §25) |
| ADR status | **ACCEPTED** (2026-08-14 — Freeze §4 Step 3 closed: status transition + freeze update §2.2/§2.7/§2.8; RFC linkage complete; RFC/ADR-0038 amended §26.7) |

# Date

2026-08-14

# Relation to Existing Documents

| Document | Relation |
|----------|----------|
| [docs/architecture/0040-identity-access-sync.md](docs/architecture/0040-identity-access-sync.md) (Accepted) | **Amends (proposed)** — adds a first-import authorization carve-out to the B8 flow (§3, Invariant 11). The accepted text is NOT modified at this gate. |
| [docs/architecture/0044-unit-trust-first-v2-bootstrap.md](docs/architecture/0044-unit-trust-first-v2-bootstrap.md) (Proposed) | **Companion decision** — resolves the separate B8 blocker identified in ADR-0044 §12.1. ADR-0044 is NOT modified. |
| [docs/architecture/rfcs/2026-08-04-node-identity-trust.md](docs/architecture/rfcs/2026-08-04-node-identity-trust.md) | **No amendment at this gate** — RFC amendment requires owner approval of this ADR first (Freeze §4). |
| [docs/architecture/ARCHITECTURE_FREEZE.md](docs/architecture/ARCHITECTURE_FREEZE.md) | **Interaction documented** — Freeze §2.2 (Authorization Model: fail-closed, structured authorization) is frozen; an approved ADR is the required instrument. Freeze file is NOT modified. |
| ADR-0038 (Accepted) | **Unaffected** — no identity-store change is proposed. |

# 1. Executive Summary

A freshly provisioned UNIT cannot perform its first `identity_access` import on the governed path, because the import requires a UNIT-scoped Admin session (SEC-003-06-b) while the canonical UNIT Admin account is delivered **by** that same import. This is a real circular authorization dependency — not a documentation misunderstanding. Independently, the import is V2-only and SEC-003-01 requires an ACTIVE, non-expired WILAYA certificate in the UNIT Identity Store, which the `.unit` package does not install; the existing pre-auth `install_wilaya_certificate` mechanism covers that prerequisite and MUST run first.

The recommended architecture is **Option A**: a narrowly scoped, package-authenticated, self-terminating first-import authorization exemption. It introduces no new trust model: the package's WILAYA issuer (Root-anchored certificate + Ed25519 V2 signature) remains the sole authenticity authority, and the exemption is exhausted by its own success (Admin account created + ledger advanced), after which the normal AdminOnly authorization resumes unchanged. The `.unit role=Admin` acceptance path is a governance defect and is explicitly REJECTED as a solution.

# 2. Problem Statement

Governed UNIT bootstrap state:

1. `.unit` provisioning creates exactly one normative **User** account (WILAYA-side producer hard-codes `UserRole::User`, `unit_service.rs:57-60`).
2. A fresh UNIT has **no Admin session** (no Admin-role row, no identity, no `.adminkey`).
3. `Action::ImportIdentityAccessPackage` requires a UNIT-scoped **AdminOnly** session (`policies/mod.rs:132-138`, SEC-003-06-b).
4. The canonical UNIT Admin account is created **only** by the `identity_access` apply (`upsert_synced_admin`, `user_account_sync_service.rs:182-191`).
5. Therefore the governed path contains a circular authorization dependency: importing requires an Admin that only the import can create.

A second, independent dependency: `identity_access` is a SECURITY_CRITICAL kind → V2-only (SEC-003-02), and `verify_v2_signature` requires an ACTIVE WILAYA certificate in the Identity Store (SEC-003-01). That certificate is available through the existing pre-auth `install_wilaya_certificate` mechanism and MUST be installed before the first `identity_access` import.

The `.unit role=Admin` acceptance (`validate_unit_node_role` accepts `"Admin"`, `node_package_service.rs:16-25`, persisted verbatim at `:161`) is an ungoverned escape hatch that conflicts with RFC:490 (no ADMIN identity on UNIT) and ADR-0040 Invariant 11, and is deliberately reconciled away by the first apply (rename). It MUST NOT be adopted as the solution.

# 3. Problem: Exact Authorization Deadlock (verified)

```
Fresh UNIT (one User-role row, no sessions, empty Identity Store, no ledger)
   ↓
import_identity_access_package → run_import_pipeline → authorize_command (import_export.rs:1246-1251)
   ↓
Action::ImportIdentityAccessPackage → ResourceContext::UnitNode → AdminOnly (policies/mod.rs:132-138)
   ↓
Admin session requires an Admin-role users row + successful login (auth.rs:110-190)
   ↓
The only governed creator of an Admin-role row on UNIT is the identity_access apply
   (upsert_synced_admin, user_account_sync_service.rs:182-191)
   ↓
circular dependency — CONFIRMED (FACT)
```

Facts that bound the cycle:

- **FACT** — No local Admin-creation command exists (no `create_user`); the only user-write surfaces on UNIT are `.unit` import (`node_package_service.rs:136-176`) and `identity_access` apply (`user_account_sync_service.rs:162-205`).
- **FACT** — `issue_first_admin_key` (SEC-002) cannot run on a fresh UNIT: it hard-requires an ACTIVE WILAYA certificate in the local Identity Store (`identity_provisioning_service.rs:616-623`) and mints an ADMIN **identity**, which RFC:490 forbids on UNIT ("لا ADMIN على عقدة UNIT إطلاقًا"; UNIT chain `Uninitialized → UnitWaitingForCertificate → UnitActive`).
- **FACT** — Login is the only session path; Challenge–Response is mandatory only when an ACTIVE ADMIN identity exists (`auth.rs:102`), and no `.adminkey` exists on a fresh UNIT.
- **FACT** — `.unit` sets `configured = 1` (`settings.rs`, `node_package_service.rs:148`), so `is_setup_mode()` returns false afterward (`settings_service.rs:67`); the anonymous `system_bootstrap` context is confined to the `.unit` command itself (`import_export.rs:454-506`).

# 4. Separate Trust-Anchor Dependency (verified)

- **FACT** — `identity_access` ∈ SECURITY_CRITICAL_KINDS (`import_export.rs:115`); SEC-003-02 requires `signature_version=V2` + signature + `issuer_identity_id` + `package_sequence` + `integrity_hash` (`import_export.rs:168-196`).
- **FACT** — `verify_v2_signature` looks up the issuer certificate by `issuer_identity_id` and rejects with "المُصدِر غير موجود في مخزن الهويات" when absent (`sync_package_identity_verification_service.rs:72-90`); SEC-003-01 requires `subject_type == Wilaya`, `status == Active`, `not_after > now` (`:42-65`), then Ed25519 over canonical bytes (`:106-130`).
- **FACT** — `.unit` import writes settings, users, and units only; it installs **no** certificate (`node_package_service.rs:136-176`) — the UNIT Identity Store is empty after `.unit`.
- **FACT** — The WILAYA anchor is installed by the pre-auth `install_wilaya_certificate` command (`commands/identity.rs:386-408`, no `authorize_command`) via `IdentityTrustAnchorService` (`identity_trust_anchor_service.rs:58-108`): requires a signed, ACTIVE, WILAYA-subject, Root-issued certificate (`issuer None`), verifies the Root signature with `resolve_root_public_key()` + Ed25519, is idempotent only for the literally identical certificate (`is_identical_to`), and fails closed on any differing ACTIVE anchor. No Admin session is required.
- **FACT** — The B8 integration suite seeds the issuer certificate directly into the Identity Store before any import (`identity_access_sync_tests.rs:93-114`), confirming SEC-003-01 as a hard prerequisite.

**Corollary (DECIDED in this proposal):** `install_wilaya_certificate` MUST occur before the first `identity_access` import. The single-anchor invariant (conflicting ACTIVE anchor fails closed) means the store holds at most one ACTIVE WILAYA certificate, so "issuer == installed anchor" is the cross-WILAYA check.

# 5. Fresh UNIT State (verified)

Immediately after `.unit` provisioning (`NodePackageService::import_unit_node_package`, `node_package_service.rs:136-176`):

| Aspect | State | Evidence |
|--------|-------|----------|
| settings | `node_type='UNIT'`, `unit_name`, `wilaya_code`, `configured=1` | `node_package_service.rs:148`; settings repository |
| users | exactly 1 row (fresh UUID), username/hash/role from package, `node_id=unit code` | `node_package_service.rs:156-164` |
| role (normative) | `User` | producer `unit_service.rs:57-60` |
| role (consumer-accepted) | `Admin` or `User` (verbatim) | `node_package_service.rs:16-25, 142, 161` |
| units | 1 row + `unit_user` link | `node_package_service.rs:166-173` |
| Identity Store | **empty** (no WILAYA/UNIT/ADMIN certificate) | `node_package_service.rs:136-176` |
| sessions / `.adminkey` | none | — |
| import ledger | empty (`.unit` bypasses `run_import_pipeline`) | `import_export.rs:454-506` |
| setup mode | exited (`configured=1`) | `settings_service.rs:67` |
| IdentityBootstrapState | UNIT chain `Uninitialized → UnitWaitingForCertificate → UnitActive`; no ADMIN on UNIT | RFC:490 |

# 6. Existing Bootstrap Mechanisms (verified)

| Mechanism | Location | Scope | Reusable for exactly-one first import? |
|-----------|----------|-------|------------------------------------------|
| `system_bootstrap` anonymous UserContext | `import_export.rs:454-506` | `.unit` only; gated on `is_setup_mode()` | Pattern yes; gate trigger no (`configured=1` after `.unit`) |
| Pre-auth commands (no session): `begin_wilaya_provision`, `finalize_wilaya_provision`, `generate_identity_request`, `install_wilaya_certificate`, `issue_first_admin_key`, `finalize_identity_provision` | `commands/identity.rs:64-110, 386-408` | offline bootstrap, D2 anchor install | **`install_wilaya_certificate`: YES** (pre-auth, idempotent, Root-verified, fail-closed) |
| SEC-002 first-admin identity | `identity_provisioning_service.rs:583-640` | WILAYA / legacy-ADMIN nodes only (needs ACTIVE WILAYA in store; ADMIN identity forbidden on UNIT) | No |
| Session creation | `auth.rs:110-190` (password), Challenge–Response (`.adminkey`) | login only | No — no credentials exist pre-import |
| `.unit role=Admin` acceptance | `node_package_service.rs:16-25, 161` | package-controlled field | Technically yes — but ungoverned, violates RFC:490 / ADR-0040 Invariant 11, renamed by the first apply |

**Conclusion:** the repository already establishes the principle "package authenticity substitutes for session authority in setup mode" (`.unit` precedent: anonymous + SEC-003-05-D + signature). No mechanism today authorizes exactly one `identity_access`; the missing piece is a first-import carve-out, not a new trust model.

# 7. Options Considered

### Option A — First-import authorization exemption (RECOMMENDED, PROPOSED)

Allow exactly one `identity_access` import without an Admin session when ALL bootstrap predicates hold (§10). Evaluated in §10–§15.

### Option B — Deliver Admin through `.unit` (REJECTED)

- **FACT** — the consumer accepts `"Admin"` (`node_package_service.rs:16-25`) and persists it (`:161`); login would accept the per-unit hash via the `verify_admin` → `verify_node` fallback (`auth.rs:132-143`).
- **FACT** — adopting it violates RFC:490 (no ADMIN on UNIT), ADR-0040 Invariant 11 (canonical admin SHALL originate from the Wilaya via `identity_access`), and creates a duplicate legacy bootstrap path.
- **FACT** — the first `identity_access` apply renames a legacy admin-named unit user to `user`, preserving row identity (`user_account_sync_service.rs:172-181`; test `identity_access_sync_tests.rs:701`) — i.e., the architecture deliberately reconciles this collision away.
- **REJECTED** — it weakens the User-only UNIT invariant and would make the governed flow depend on an ungoverned producer field. No repository evidence contradicts this rejection.

### Option C — Separate WILAYA bootstrap artifact (REJECTED)

- **FACT** — the existing `identity_access` package already carries: WILAYA Ed25519 signature (V2), `issuer_identity_id`, payload-bound `unit_code` (`models/identity_access.rs:18-28`; export `user_account_sync_service.rs:121-146`), `package_sequence`, package-id idempotency, and the Admin credentials (hashes + enabled flags).
- A new artifact provides **no additional security property**; it would duplicate the transport, verification, and apply machinery. **REJECTED as redundant.**

### Option D — New bootstrap package kind (REJECTED)

- `identity_access` already satisfies the required semantics (authenticity, unit binding, sequence, idempotency, credentials). A parallel bootstrap kind would create a second apply path for zero gain. **REJECTED.**

### Option E — Reorder provisioning (REJECTED)

- **FACT** — no local Admin-creation surface exists on UNIT (no `create_user`; SEC-002 requires an ACTIVE WILAYA in the store and mints a forbidden ADMIN identity); credentials originate at the WILAYA. Admin/session creation cannot legitimately precede `.unit`/`identity_access`. **REJECTED as impossible on the governed path.**

# 8. Option A — Detailed Architecture (PROPOSED)

The exemption is an authorization carve-out at the import entry gate. It reuses the complete existing verification chain unchanged and adds **one new binding check**. No new trust model.

**Bootstrap predicates (ALL must hold):**

| # | Predicate | Current state | Evidence |
|---|-----------|---------------|----------|
| 1 | local node is UNIT (`node_type='UNIT'`) | exists (settings) | settings repository |
| 2 | local WILAYA trust anchor installed and valid (ACTIVE, non-expired, Root-verified) | exists — pre-auth `install_wilaya_certificate` | `identity_trust_anchor_service.rs:58-108` |
| 3 | package kind is exactly `identity_access` | exists (kind check) | `import_export.rs:168-196` |
| 4 | package is V2 (`signature_version=2`) | exists (SEC-003-02) | `import_export.rs:115, 179` |
| 5 | SEC-003-01 passes (ACTIVE WILAYA issuer, non-expired, Ed25519 valid) | exists | `sync_package_identity_verification_service.rs:42-130` |
| 6 | SEC-003-02 passes (V2 + signature + issuer + sequence + integrity) | exists | `import_export.rs:168-196` |
| 7 | issuer `identity_id` == installed anchor `identity_id` (cross-WILAYA) | **implied, not explicit** — single-anchor invariant makes it true; the exemption MUST enforce it explicitly | `identity_trust_anchor_service.rs:90-101` |
| 8 | `payload.unit_code` == local unit code (settings/units row) | **ABSENT today** — `apply` binds accounts to `payload.unit_code` (`user_account_sync_service.rs:174, 185, 198`) with no cross-check; `_importer_wilaya` is unused in the B8 closure (`import_export.rs:1199`). The exemption MUST add this check. | `user_account_sync_service.rs:162-205` |
| 9 | no canonical Admin account exists (username `admin`, role Admin, `node_id=unit code`, `deleted=0`) | checkable | ADR-0040 Invariant 11; users repository |
| 10 | no prior `identity_access` ledger entry for the issuer (`last_applied_sequence` is None) | checkable | `sync_package_identity_verification_service.rs` (ledger) |
| 11 | `package_sequence == 1` (first sequence semantics) | exists — first package from an issuer MUST be 1 | `transport_guard.rs:44-69`, test `:77-93` |
| 12 | apply is atomic (single transaction: verify → guard → apply → ledger advance → audit) | exists | `import_export.rs:1290-1363` |
| 13 | success creates the Admin account and advances the ledger → exemption exhausted | apply behavior | `user_account_sync_service.rs:182-203` |

**Boundary rules (DECIDED in this proposal):**

- The exemption authorizes **only** the bootstrap transition; it is not a general unauthenticated import path (self-termination, §12).
- The apply semantics, payload shape, transport, encryption (age/App Key), signing (Ed25519), and post-bootstrap AdminOnly authorization are unchanged.
- Predicate 8 (local-unit binding) and predicate 7 (explicit anchor==issuer) are **new checks required by this decision**; today's implicit binding is insufficient for a session-less first import.
- The exemption MUST be recorded in an audit event dedicated to the bootstrap transition (open decision #4).

# 9. Authentication vs Authorization Boundary

The proposal preserves the distinction between three independent authorities:

| Authority | What it provides | Source |
|-----------|------------------|--------|
| **Cryptographic authenticity** | The package is exactly what the WILAYA signed: issuer cert ACTIVE WILAYA, non-expired, Ed25519 valid, kind V2-critical | SEC-003-01/02, `sync_package_identity_verification_service.rs:42-130` |
| **Operational authorization** | The WILAYA (single source of truth for the synced accounts) issued the package for THIS unit: payload `unit_code` binding + issuer == installed anchor + sequence=1 | predicates 7, 8, 11 |
| **Session-based authorization** | After bootstrap, an Admin session is required for every subsequent import | `policies/mod.rs:132-138` |

A valid signature alone is NOT declared equivalent to general Admin authorization. The exemption is scoped to the exact bootstrap transition (no Admin row + empty ledger + sequence 1) and is consumed by its own success. There is no claim that a signed package may bypass authorization in any other state.

# 10. Bootstrap State Machine (PROPOSED)

```
FRESH                       (configured=0; no users; empty Identity Store; setup-mode)
  │  .unit import — anonymous system_bootstrap + SEC-003-05-D (import_export.rs:454-506)
  ▼
UNIT_PACKAGE_ACCEPTED       (configured=1; User-role row; EMPTY Identity Store; no ledger)
  │  install_wilaya_certificate — PRE-AUTH, Root-verified, idempotent, fail-closed (commands/identity.rs:386-408)
  ▼
ROOT_ANCHORED               (ACTIVE WILAYA anchor; still no Admin session)
  │  FIRST identity_access — exemption predicates 1-13 (PROPOSED)
  ▼
ADMIN_AVAILABLE             (canonical admin row + ledger seq=1; exemption exhausted)
  ▼
NORMAL_OPERATION            (Admin/User sessions; imports via AdminOnly — unchanged)
```

- **Legal transitions**: FRESH → UNIT_PACKAGE_ACCEPTED → ROOT_ANCHORED → ADMIN_AVAILABLE → NORMAL_OPERATION; FRESH → … → ROOT_ANCHORED → NORMAL_OPERATION (User-scope operation without B8 — the current degradation path).
- **Illegal**: any `identity_access` before ROOT_ANCHORED (SEC-003-01 issuer absent); any second first-import (predicates 9/10/13).
- **Irreversible**: ledger advance and Admin creation (soft-delete only; rows keep identity — ADR-0040 §2).
- **Replay**: same/older sequence → Transport Guard Replay, ledger not consumed (tests `identity_access_sync_tests.rs:559-622`); future sequence → OutOfOrder (first must be 1).
- **Partial failure**: single transaction rolls back; same-sequence retry succeeds (test `:882-928`).
- **Rollback**: re-provisioning (re-import `.unit`, fresh UUIDs) unless separately decided otherwise (open decision #9).

> **Amendment (2026-08-15 — packaged-identity bootstrap, ADR-0044 §8.3):** the
> `UNIT_PACKAGE_ACCEPTED` transition may now also install the packaged UNIT
> identity (certificate + guarded key install + executor identity install)
> inside the same `.unit` import audit transaction, reaching
> `UnitActive` without a separate CSR ceremony. This does NOT change the B8
> `identity_access` exemption (predicates 1-13 unchanged), the anchor-first
> requirement (predicate 2, §10 `ROOT_ANCHORED`), or any other invariant in
> §11. The packaged UNIT identity is Ed25519-authenticated by the installed
> ACTIVE WILAYA anchor, exactly like `finalize_unit_provision`.

# 11. Security Invariants (NORMATIVE — preserved by this decision)

1. Fresh UNIT does not possess the WILAYA private key. — CONFIRMED (RFC trust chain)
2. Fresh UNIT does not possess `GRPC_PACKAGE_SIGNING_KEY`. — CONFIRMED
3. Fresh UNIT does not possess `.adminkey`. — CONFIRMED
4. Fresh UNIT does not possess any fleet-wide signing secret. — CONFIRMED
5. Root signs WILAYA identities only. — CONFIRMED (RFC:184-185)
6. WILAYA trust anchor is installed before the first `identity_access`. — DECIDED (this ADR, §4)
7. Conflicting ACTIVE WILAYA anchors fail closed. — CONFIRMED (`identity_trust_anchor_service.rs:90-101`)
8. `identity_access` remains V2-only. — CONFIRMED (SEC-003-02)
9. SEC-003-01 remains mandatory. — CONFIRMED
10. SEC-003-02 remains mandatory. — CONFIRMED
11. Cross-WILAYA imports fail closed. — DECIDED (predicate 7)
12. `.unit` remains User-only normatively. — CONFIRMED (ADR-0044 §8; producer `unit_service.rs:57-60`)
13. The `.unit role=Admin` escape hatch is NOT an approved bootstrap mechanism. — DECIDED (REJECTED, §7-B)
14. The first-import exemption is single-use. — DECIDED (predicates 9, 10, 13)
15. The first-import exemption is self-terminating. — DECIDED (§12)
16. Subsequent `identity_access` imports remain AdminOnly. — CONFIRMED (unchanged policy)
17. Replay protection remains mandatory. — CONFIRMED (Transport Guard)
18. Import is atomic. — CONFIRMED (single transaction)
19. Encryption remains age/App Key. — CONFIRMED (ADR-0039/0041)
20. Signing remains Ed25519. — CONFIRMED (Freeze §2.7)
21. Encryption and signing keys remain separate domains. — CONFIRMED
22. UNIT compromise does not expose fleet-wide signing capability. — CONFIRMED (no signing secret on UNIT)
23. No silent trust-anchor replacement. — CONFIRMED (`is_identical_to` idempotency, fail-closed on conflict)
24. Rollback remains re-provisioning unless separately decided otherwise. — DECIDED (open decision #9)

# 12. Self-Termination Proof

Why the exemption cannot become a permanent unauthenticated import path:

1. **Precondition (9)**: no canonical Admin account → the exemption is unavailable once the admin row exists.
2. **Precondition (10)**: no prior `identity_access` ledger entry → the exemption is unavailable once the ledger advanced.
3. **Success (13)**: the first import itself creates the Admin account AND advances the ledger, atomically.
4. **Post-state**: every later import attempt fails predicates 9/10 (and Transport Guard enforces continuity from sequence 1), so the normal AdminOnly authorization is the only remaining path.

Single-use by construction; the exemption cannot be re-triggered by replay (guard), by a second package (predicates), or by deletion (soft-delete only).

# 13. Threat Model

| # | Threat | Prevented? | Basis |
|---|--------|-----------|-------|
| 1 | Attacker at local UNIT UI before first import | Partially — no Admin session exists; User-scope commands only; import denied | authz policy |
| 2 | Valid WILAYA package for another UNIT | Yes, with predicate 8 — payload `unit_code` MUST equal local unit code (NEW check; today implicit) | §8 predicate 8 |
| 3 | Valid package for this UNIT, contents modified | Yes — Ed25519 over canonical bytes | SEC-003-01 |
| 4 | Replay same `identity_access` | Yes — Transport Guard Replay; ledger not consumed | tests :559 |
| 5 | Replay older `identity_access` | Yes — Replay (incoming ≤ last applied) | tests :559-622 |
| 6 | Forge an Admin not authorized by WILAYA | Yes — hashes originate from WILAYA export, fail-closed on unset/disabled admin; forging requires WILAYA private key | export fail-closed tests :423-470 |
| 7 | Replace an existing Admin | Yes — post-bootstrap AdminOnly; keep-id upserts; Invariant 13 (no local password change) | ADR-0040 |
| 8 | Cross-WILAYA provisioning | Yes — single-anchor invariant + predicate 7 (issuer == installed anchor) | §8 predicate 7 |
| 9 | Clone UNIT database/files | Partially — clone inherits honest state; no device binding (see §20); replay bound to ledger | §20 |
| 10 | Interrupted provisioning | Yes — atomic transaction rollback; retry same sequence | tests :882-928 |
| 11 | Restart during first import | Yes — same as 10 (WAL, single transaction) | — |
| 12 | Race two first-imports | Yes — single-writer mutex + predicates 9/10/11 inside one transaction; second fails fail-closed | R1 / single-writer |
| 13 | Valid package, unexpected local UNIT identity | Partially — no UNIT-certificate binding exists; binding is payload/node-level; documented limitation (open decision #7) | §20 |
| 14 | Downgrade to legacy V1 format | Yes — SEC-003-02 rejects V1 for critical kinds | import_export.rs:179 |

# 14. Replay / Ordering / Concurrency

- **Replay**: same sequence → Replay; older sequence → Replay; ledger NOT consumed on failure (test `:882-928`). Future sequence before 1 → OutOfOrder (`transport_guard.rs:77-93`). A second "first" import cannot exist because the ledger advance and admin-row creation are atomic with the apply.
- **Ordering**: anchor (pre-auth) → first import (sequence 1) → subsequent imports (AdminOnly, contiguous sequences).
- **Concurrency**: two simultaneous first-import attempts serialize on the single-writer mutex; inside one transaction both predicates 9/10/11 and the ledger advance hold, so exactly one succeeds and the second fails closed.

# 15. Recovery / Failure Handling

| Failure | Recovery |
|---------|----------|
| Missing WILAYA anchor | Install via pre-auth `install_wilaya_certificate` before any import (predicate 2) |
| Failed first import (signature/guard/parse) | Fix artifact; retry same sequence; ledger not consumed |
| Interrupted first import | Atomic rollback; retry |
| Wrong `.sync` file / corrupted package | Rejected at decrypt/parse; no state written |
| Wrong-UNIT package | Rejected by predicate 8 (NEW check) |
| Conflicting WILAYA anchor | Fail-closed already; operator must reconcile before import |
| Post-import admin unusable | Re-import (ledger-contiguous, AdminOnly) with corrected WILAYA-side state; re-provisioning is the fallback (open decision #9) |

# 16. SEC-002 Relationship

- SEC-002 governs first Admin **identity** provisioning on WILAYA / legacy-ADMIN nodes (`issue_first_admin_key`, `identity_provisioning_service.rs:583-640`).
- B8 governs first Admin **account** synchronization on UNIT (`identity_access`, ADR-0040 Invariant 11).
- These are different domains: identity-class vs account-class. SEC-002 cannot be reused to create the UNIT Admin account (requires ACTIVE WILAYA in the local store — absent on fresh UNIT — and mints an ADMIN identity forbidden by RFC:490).
- The B8 exemption does NOT weaken, bypass, or duplicate SEC-002: it operates entirely in the account domain on UNIT nodes; SEC-002's WILAYA-side gates (canonical username SEC-002-B, one-time gate via `AdminCredentialState`, recovery session gate SEC-002-R) are untouched.
- No evidence contradicts this separation; no contradiction found.

# 17. SEC-003 Relationship

| Control | Role in first import | Status |
|---------|----------------------|--------|
| SEC-003-01 issuer validation (ACTIVE WILAYA, non-expired) | Mandatory predicate 5 | CONFIRMED — reusable as-is; requires anchor installed (predicate 2) |
| SEC-003-02 kind policy (V2 mandatory for critical kinds) | Mandatory predicates 3/4/6 | CONFIRMED — reusable as-is |
| Ed25519 signature verification | Mandatory | CONFIRMED |
| Certificate expiration (`not_after`) | Mandatory | CONFIRMED |
| Transport Guard sequence/replay | Mandatory (first sequence = 1) | CONFIRMED |
| `ImportedPackageRegistry` idempotency | Mandatory | CONFIRMED |
| Session / Admin principal | **Absent in bootstrap — replaced by predicates 7/8/9/10** | This ADR's carve-out |
| Audit of the pre-auth anchor install | **Absent today** (`install_wilaya_certificate` has no audit write) | Open decision #3 |

# 18. ADR-0044 Relationship

- ADR-0044 (`.unit` Trust-First V2) is a separate decision: it governs `.unit` authenticity (HMAC→V2) and the trust ordering (anchor before `.unit`). It identifies B8 as a separate blocker with an owner decision pending (§12.1).
- This ADR is the **companion decision** that addresses that B8 blocker: it proposes the first-import exemption without depending on ADR-0044's V2 migration (the exemption operates on the existing V2 `identity_access` pipeline regardless of `.unit`'s signature scheme).
- ADR-0044 does NOT authorize implementation, and this ADR also does NOT authorize implementation. Neither document's text is modified by this gate.

# 19. `.unit role=Admin` Escape-Hatch Decision (REJECTED as mechanism)

- **FACT** — `validate_unit_node_role` accepts `"Admin"` and `"User"` and rejects only unknown/malformed strings (`node_package_service.rs:16-25`); the accepted role is persisted verbatim (`:161`); tests assert both parse (`:247-272`).
- **FACT** — the WILAYA-side producer always emits `User` (`unit_service.rs:57-60`), so the acceptance is a latent, ungoverned capability rather than a produced path.
- **FACT** — first apply renames a legacy admin-named unit user to `user`, preserving row identity (test `identity_access_sync_tests.rs:701`) — the architecture reconciles this collision away by design.
- **DECISION**: the escape hatch is a governance defect / legacy acceptance path. It is **REJECTED** as a bootstrap mechanism (conflicts with RFC:490, ADR-0040 Invariant 11, and ADR-0044 §8 normative User-only role). Whether to close it (narrow SEC-003-05-D to User-only) is a separate owner decision (open decision #8); closing is NOT authorized here.

# 20. Audit Requirements

- The exemption MUST be traceable: a dedicated audit event for the first `identity_access` import, under an explicit bootstrap context (analogous to `system_bootstrap`), recording package id, issuer, sequence, and outcome (open decision #4).
- The pre-auth `install_wilaya_certificate` path currently performs no audit write; reuse of it as a hard prerequisite raises the question of whether its audit must be fixed first (open decision #3). This ADR does not authorize changing it.

# 21. Clone / Device Binding

- **FACT** — there is no cryptographic device binding in the current architecture: binding is payload/node-level (`payload.unit_code` → account `node_id`; `user_account_sync_service.rs:174-198`) plus issuer-ledger continuity. This matches ADR-0044 §8.1 (target-node binding ACCEPTABLE BY DESIGN for offline bootstrap).
- **DECISION** — the clone limitation is **accepted by design at this gate** (operator ceremony + confirmation step bound the risk), with future binding mechanisms deferred (open decision #7). This ADR does not claim device binding.

# 22. Open Owner Decisions

1. Exact first-import predicate set (§8 predicates 1-13) — approve, amend, or reject individual predicates.
2. Whether the exemption is anonymous (package-authenticated only, `.unit`-style) or requires the existing User session as a weak authentication signal.
3. Whether `install_wilaya_certificate` audit must be fixed before reuse as a prerequisite.
4. Whether the first `identity_access` import requires a dedicated audit event and its exact shape.
5. Exact first `package_sequence` (proposed: 1, per Transport Guard semantics) and the ledger baseline.
6. Whether the exemption belongs in ADR-0040 (proposed: amend), ADR-0044 (not recommended), or a new ADR (this document).
7. Whether package-to-UNIT binding (predicate 8) is sufficient without UNIT-certificate binding (future mechanism).
8. Whether `.unit role=Admin` acceptance is separately closed by an architecture amendment (SEC-003-05-D narrowing).
9. Recovery / re-provisioning behavior (rollback semantics; UUID churn on `.unit` re-import).
10. Clone-risk acceptance (deferred to a future decision; accepted by design here).
11. Relationship to the ADR-0044 Root-key blocker: the exemption requires a functional Root for anchor verification; production readiness remains BLOCKED until the production Root public key is certified (BLOCKING PREREQUISITE, ADR-0044 §9.4).

None of these is resolved by assumption in this document.

# 23. Implementation Boundary

> **NO IMPLEMENTATION AUTHORIZED**
>
> This architecture decision does not authorize source-code changes.
> No implementation planning is authorized unless and until the required owner decisions and governance gates are completed.

High-level implementation implications (documented only to define architectural boundaries, NOT as a task list):

- The exemption touches a frozen surface (Freeze §2.2 Authorization Model: fail-closed default, structured authorization) → an approved ADR via the Freeze §4 RFC-to-ADR process is required before any code.
- The change surface, once authorized, would be confined to: the import entry authorization predicate (new bootstrap branch), the new predicates 7/8 (binding checks), and audit additions — with the verification chain and apply path unchanged.
- No RFC text may change until owner approval of this ADR (and the corresponding freeze process step).

# 24. Governance Status

| Check | Status |
|-------|--------|
| ADR status | **ACCEPTED** (2026-08-14 — Freeze §4 Step 3 closed; previously PROPOSED — historical) |
| Implementation | NOT AUTHORIZED |
| RFC amendment | EXECUTED documentation-only (§26.7) — further changes require a new governance gate |
| ADR-0044 / ADR-0040 / Freeze text | ADR-0044 Accepted; ADR-0040 NOT MODIFIED; Freeze updated (§2.2/§2.7/§2.8) by the Final Freeze/ADR Status gate |

> **Historical record:** this table previously recorded "PROPOSED — Decision for Owner Approval" and "RFC amendment: NOT AUTHORIZED" at this point (pre-ratification state). Preserved per audit-history rule; superseded by the rows above.
| Requires owner decision before any approval | YES (Freeze §4 process; §22 open decisions) |
| Production readiness of the exemption | BLOCKED until the ADR-0044 Root-key blocker is resolved (open decision #11) |

# 25. Owner Decision Resolution Record (2026-08-14)

This section is the **canonical resolution record** for the owner-controlled decisions of ADR-0044 and ADR-0045. It reconciles both proposals into one auditable state. Vocabulary: **DECIDED / BLOCKED / OWNER DECISION REQUIRED / OUT OF SCOPE / NOT AUTHORIZED / REJECTED**. It does NOT ratify anything; it records what is and is not decided, and why.

> **Historical record — superseded (2026-08-14, Owner Ratification Completion)**: the statuses in this section reflect the state before owner ratification. They are superseded by the completed matrix §26.1 and the canonical **OWNER RATIFICATION RECORD — COMPLETED** in §26.6. Preserved for audit history; do not read §25.1 as current state.

## 25.1 Decision Matrix (canonical)

| ID | Decision | Status | Resolution | Evidence / Source |
|----|----------|--------|------------|-------------------|
| A44-01 | Approve Trust-First `.unit` V2/Ed25519 architecture | **OWNER DECISION REQUIRED** | No owner ratification exists anywhere in the repository. Prior gates produced review verdicts only ("APPROVED WITH BLOCKERS" = proposal review, NOT owner approval). | ADR-0044 §6, §28.1-28.2; ADR_INDEX status "Proposed" |
| A44-02 | RFC §3.10 amendment (replace `.unit` HMAC exception with Trust-First V2) | **NOT AUTHORIZED** | Gated on A44-01 acceptance + Freeze §4 Step 3 (ADR status Accepted). RFC text untouched. | ADR-0044 §6.1; Freeze §4 |
| A44-03 | RFC §3.12 D2 ordering (`install_wilaya_certificate` before `.unit`) | **NOT AUTHORIZED** (text) / DECIDED (capability fact) | The capability already exists today (anchor installable on a certificate-empty node); the normative RFC text change is gated on A44-01 acceptance. | `identity_trust_anchor_service.rs:58-108`; ADR-0044 §7 S1.6 |
| A44-04 | ADR-0038 mirror update | **NOT AUTHORIZED** | Follows RFC §3.10/§3.12 amendment (gated on A44-01). | ADR-0044 §6.1 |
| A44-05 | ADR-0008 supersession | **NOT AUTHORIZED** | Gated on A44-01 acceptance + A44-07 (V1 window closure) + RFC amendment. | ADR-0044 §6.1, §14 |
| A44-06 | Production Root public-key certification | **CERTIFIED — PRODUCTION ROOT KEY CERTIFICATION COMPLETE (2026-08-15)** | `PROD_ROOT_PUBLIC_KEY` is the certified Production Authority Root key: ceremony artifacts (`root-secret.hex` 0600 / `root-public.key`, 2026-08-14 05:11, outside the repo) derive to the exact embedded key; independent verification: SHA-256 fingerprint `7b38f2e1584353f9a2bf4fcc578345db3660208fecfd4c042d4efe6270072f20`; secret never in repo. Supersedes the prior TEST-2-vector status (no key replaced). | certification record `docs/security/A44-06-production-root-key-certification.md`; `root_public_key.rs:37` |
| A44-07 | V1 data-import window closure (date/condition) | **OWNER DECISION REQUIRED** | No closure date/condition decided (RFC:476 undecided; ADR-0044 §13.2). Proposed evidence criteria (fleet on V2, no V1 in circulation, V1-free E2E) are PROPOSED only. | RFC:476; ADR-0044 §13.2-13.3 |
| A44-08 | First `.unit` V2 `package_sequence` | **OWNER DECISION REQUIRED** | `Some(1)` or "WILAYA ledger start" is a proposal with an explicit "decided at implementation" note; not owner-decided. Distinct from the `identity_access` first sequence (A45-06), which is code-enforced. | ADR-0044 §8, §26#2 |
| A44-09 | `.unit` V1/V2 transition strategy (dual generation vs fleet-sync) | **OWNER DECISION REQUIRED** | Neither strategy is chosen by any documented owner intent. | ADR-0044 §17, §26#3 |
| A44-10 | Root rotation policy | **OWNER DECISION REQUIRED** | Single-root is the current implementation (DECIDED fact); the rotation policy (timeline, acceptance windows, explicit cutover) is undecided. No multi-root behavior invented. | `root_public_key.rs`; ADR-0044 §10.1, §26#7 |
| A44-11 | Target-node binding strategy | **DECIDED** (current operative model) + **OWNER DECISION REQUIRED** (future mechanisms) | Current model: payload + operator ceremony + mandatory confirmation step; NO device binding exists (cryptographic device binding is impossible offline at bootstrap). Future device-bound mechanisms are deferred, explicitly NOT a present requirement. | ADR-0044 §8.1, §26#9; `node_package_service.rs` |
| A44-12 | Rollback / recovery semantics | **DECIDED** (principle) | Rollback = full re-provisioning; no silent downgrade. Consistent with ratified posture (Freeze §2.7 legacy-read-only, fail-closed isolation). Operational recovery-point details are proposal content pending A44-01. | ADR-0044 §20.1; Freeze §2.7; ADR-0018/0019 |
| A44-13 | App Key interchange | **OUT OF SCOPE** | Confirmed separate workstream; not opened or closed by these decisions. | ADR-0044 §12, §26#5; ADR-0041 |
| A45-01 | Approve Option A (first `identity_access` import exemption) | **OWNER DECISION REQUIRED** | No owner ratification exists. Recommendation stands as PROPOSED. | this ADR §8-§13 |
| A45-02 | Exact exemption predicate | **DECIDED** (predicates enforced by existing code: kind, V2, SEC-003-01/02, atomic apply, self-termination-by-apply, first-seq=1) + **OWNER DECISION REQUIRED** (new exemption-trigger checks: UNIT node, explicit anchor==issuer, payload `unit_code` == local unit code, no canonical Admin row, empty B8 ledger) | No predicates added beyond the 13 evidence-anchored ones. | this ADR §8; `transport_guard.rs:77-93`; `user_account_sync_service.rs:162-205` |
| A45-03 | Anonymous vs User-session first import | **OWNER DECISION REQUIRED** | Not decided; not inferred. | this ADR §22#2 |
| A45-04 | Audit semantics (anchor install; first bootstrap import) | **DECIDED** (fact: `install_wilaya_certificate` performs no audit write today) + **OWNER DECISION REQUIRED** (policy: fix anchor audit; exemption audit requirements) | Implementation remains NOT AUTHORIZED. | `identity_trust_anchor_service.rs:58-108`; `commands/identity.rs:386-408`; this ADR §20 |
| A45-05 | Dedicated audit event taxonomy | **OWNER DECISION REQUIRED** | Undecided. | this ADR §22#4 |
| A45-06 | First-import sequence baseline | **DECIDED** | First `identity_access` from an issuer MUST be sequence 1 (TransportGuard enforcement, code-tested). Consistent with ADR-0044 (`.unit` setup path has no ledger; ledger begins after bootstrap). No cross-ADR contradiction. **Refined 2026-08-15 (F-1 Option A, §26.9): first `identity_access` import sequence = 1 per target UNIT stream.** | `transport_guard.rs:44-69, 77-93`; ADR-0044 §4.6 |
| A45-07 | UNIT certificate binding at first `identity_access` | **DECIDED** | No UNIT certificate is required at first import: the UNIT certificate is generated LATER via CSR (D2, after `.unit`). Binding = payload `unit_code` + installed anchor/issuer. Future mechanisms deferred (A44-11). | RFC:490; ADR-0044 §7 S4 |
| A45-08 | `.unit role=Admin` acceptance (narrow to User-only) | **OWNER DECISION REQUIRED** | No accepted governing document explicitly prohibits the V1 consumer acceptance; ADR-0044 §8 (Proposed, not accepted) makes User-only normative for V2. Recommended: adopt the narrowing with ADR-0044 acceptance. Implementation NOT AUTHORIZED. | `node_package_service.rs:16-25, 161`; RFC:490; ADR-0044 §8 |
| A45-09 | Recovery / re-provisioning semantics | **DECIDED** (facts: interrupted import → atomic rollback; same-sequence retry succeeds; `.unit` re-import churns user UUIDs) + **OWNER DECISION REQUIRED** (accepted recovery posture policy) | Facts from code/tests; policy (e.g., UUID churn acceptance, re-import rules) undecided. | `identity_access_sync_tests.rs:882-928`; users repository upsert semantics |
| A45-10 | Clone-risk acceptance | **DECIDED** (fact: no device binding exists) + **OWNER DECISION REQUIRED** (acceptance as normative posture) | No device-binding requirement invented; risk bounded by operator ceremony + confirmation. | this ADR §21; ADR-0044 §8.1 |
| A45-11 | Relationship to ADR-0044 Root-key blocker | **DECIDED** | Consistency rule: the exemption requires a functional Root for anchor verification; ADR-0045 is NOT production-ready while A44-06 remains blocked. | this ADR §24; ADR-0044 §9.4 |
| A45-12 | Relationship to SEC-002 | **DECIDED** | SEC-002 = first Admin IDENTITY provisioning (WILAYA/legacy-ADMIN nodes); B8 = first Admin ACCOUNT synchronization (UNIT). Distinct domains; B8 does not replace, modify, or bypass SEC-002; SEC-002 cannot create the UNIT Admin account. | RFC:490; ADR-0040 Invariant 11; `identity_provisioning_service.rs:616-623`; this ADR §16 |

## 25.2 Cross-ADR Reconciliation

The 14 reconciliation points were checked explicitly; **all are CONSISTENT** (no conflict requiring a cross-ADR owner decision):

| # | Point | ADR-0044 | ADR-0045 | Verdict |
|---|-------|----------|----------|---------|
| 1 | Trust anchor installation order | anchor before `.unit` (§7 S1.6) | anchor before first `identity_access` (§4) | CONSISTENT — anchor precedes both |
| 2 | `.unit` V2 authenticity | Ed25519 trust-first | agnostic (works with either scheme) | CONSISTENT |
| 3 | Root → WILAYA hierarchy | single anchor (RFC §3.11) | same | CONSISTENT |
| 4 | WILAYA issuer binding | issuer == installed anchor (§11.1) | predicate 7 | CONSISTENT |
| 5 | `unit_code` binding | payload + confirmation step (§8.1) | predicate 8 (explicit cross-check) | CONSISTENT — complementary |
| 6 | First `package_sequence` | open for `.unit` V2 (§26#2) | seq=1 for `identity_access` (enforced) | CONSISTENT — different ledgers |
| 7 | V1 retirement boundary | designated for retirement, not removed (§14) | untouched | CONSISTENT |
| 8 | `.unit` role policy | NORMATIVE User-only (proposed, §8) | role=Admin REJECTED as mechanism | CONSISTENT — same direction |
| 9 | B8 first-import authorization | separate blocker (§12.1) | companion decision (Option A) | CONSISTENT — resolved-by-proposal |
| 10 | Admin identity vs account | preserved | preserved | CONSISTENT |
| 11 | SEC-002 scope | untouched | untouched (A45-12) | CONSISTENT |
| 12 | App Key / encryption scope | age/App Key unchanged (§12) | unchanged (§11 #19-21) | CONSISTENT |
| 13 | Rollback semantics | re-provisioning only (§20.1) | re-provisioning (§15) | CONSISTENT |
| 14 | Production Root-key prerequisite | BLOCKED (§9.4) | BLOCKED (A45-11) | CONSISTENT |

## 25.3 Resolution-Gate Verdict

- **Option A**: PROPOSED — **OWNER DECISION REQUIRED** (A45-01). Not ratified, not rejected.
- **ADR-0044 architecture**: PROPOSED — **OWNER DECISION REQUIRED** (A44-01). Review verdicts ("APPROVED WITH BLOCKERS") are NOT owner ratification.
- **RFC amendment state**: **RFC AMENDMENT BLOCKED** — owner decisions remain (A44-01, A45-01); §3.10/§3.12 D2 NOT amended; ADR-0038 NOT updated; ADR-0008 NOT superseded; Freeze NOT modified.
- **Implementation**: **NOT AUTHORIZED** by this gate or any prior gate.

> **Update (Owner Ratification Gate — 2026-08-14)**: the verdicts above are superseded where §26 (Owner Ratification Record, same date) explicitly ratifies: **A45-01** and **A45-02** are now **OWNER RATIFIED** (with conditions — §26.1); **A44-01** remains **OWNER DECISION REQUIRED**. All other matrix rows below remain as recorded in §25.1.
>
> **Update (Owner Ratification Completion — 2026-08-14)**: fully superseded by §26.6 — **all remaining decisions now OWNER RATIFIED** (A44-01/07/08/09/10/11/12, A45-03/04/05/08/09/10/11/12); A44-06 remains BLOCKED — EXTERNAL PREREQUISITE; A44-13 OUT OF SCOPE. See completed matrix §26.1.

# 26. Owner Ratification Record (2026-08-14)

**Canonical combined owner-ratification record for ADR-0044 + ADR-0045.** This is the single authoritative owner-decision record for both documents (ADR-0044 references this section instead of duplicating a matrix). Status vocabulary: **OWNER RATIFIED / OWNER DECISION REQUIRED / BLOCKED — EXTERNAL PREREQUISITE / DECIDED — FACT / DECIDED — ARCHITECTURAL PRINCIPLE / OUT OF SCOPE / NOT AUTHORIZED**.

**Decision source**: the owner-ratification gate instruction (2026-08-14) and the owner-ratification-completion gate instruction (2026-08-14 — §26.6). Prior review verdicts ("APPROVED WITH BLOCKERS", "PROPOSED", "DECISION FOR OWNER APPROVAL") are NOT owner decisions. "OWNER RATIFIED" below is recorded ONLY where the owner's own instruction explicitly states the decision; nothing is inferred.

## 26.1 Decision Matrix (completed — supersedes §26.5 statuses)

| ID | Decision | Status | Owner Decision | Conditions | Evidence |
|----|----------|--------|----------------|------------|----------|
| A44-01 | Approve Trust-First `.unit` V2 / Ed25519 architecture | **OWNER RATIFIED** | Owner explicitly approved `.unit` HMAC-V1 → Trust-First V2/Ed25519: V2/Ed25519 authenticity; WILAYA issuing authority; UNIT installs WILAYA certificate before accepting V2 `.unit`; installed certificate verified against production Root anchor; V2 verification requires ACTIVE valid WILAYA certificate; HMAC-V1 legacy and designated for retirement; no HMAC fleet secret on UNIT; no WILAYA private key or fleet secret on UNIT; signing/encryption separate; age/App Key encryption unchanged; no silent anchor replacement; rollback = re-provisioning; cross-WILAYA acceptance fail-closed; V2 security-critical verification mandatory; B8 uses separately ratified Option A. **Architectural ratification only — does NOT authorize implementation.** | 15-point architecture as listed; no weakening | owner gate §1 (A44-01); ADR-0044 §6/§8 |
| A44-06 | Production Root public-key certification | **CERTIFIED (2026-08-15)** | Owner: certification gate executed. Ceremony artifacts found outside the repo (root-secret.hex / root-public.key, 2026-08-14 05:11) and independently verified: secret-derived public key == embedded `PROD_ROOT_PUBLIC_KEY` byte-exact; SHA-256 fingerprint `7b38f2e1584353f9a2bf4fcc578345db3660208fecfd4c042d4efe6270072f20`; private key absent from repository. Remaining requirements satisfied: ceremony executed, pinning verified, custody documented, distribution per runbooks, verification recorded. | no key replaced or invented; certification record `docs/security/A44-06-production-root-key-certification.md` | owner gate §0-§8 (2026-08-15); `root_public_key.rs`; ADR-0044 §9.4 |
| A44-07 | V1 data-import window closure | **OWNER RATIFIED** | Owner ratified **evidence-based** (not date-based) closure: window closes only when ALL of — all supported fleet nodes V2-capable; no supported production UNIT requires V1 data import; no V1 data package legitimately in circulation; V2 end-to-end interoperability green; rollback/recovery evidence available for V2 path; closure recorded as a governance event before V1 acceptance/removal is implemented. No calendar date invented. | closure is evidence-based; implementation/removal separately unauthorized | owner gate §3 |
| A44-08 | First `.unit` V2 `package_sequence` | **OWNER RATIFIED** | Owner ratified the initial sequence baseline: first `.unit` V2 package has `package_sequence = 1`. TransportGuard semantics not altered. | seq=1 baseline; TransportGuard unchanged | owner gate §4; ADR-0044 §8 |
| A44-09 | V1/V2 transition strategy | **OWNER RATIFIED** | Owner ratified **fleet-sync transition** (not indefinite dual-format acceptance): upgrade/provision fleet to V2; verify fleet readiness; close V1 window per A44-07 criteria; only then authorize V1 retirement/removal. Not implemented now; V1 not removed now. | fleet-sync; V1 removal only after A44-07 closure | owner gate §5 |
| A44-10 | Root rotation policy | **OWNER RATIFIED** | Owner ratified **explicit acceptance window** policy: root changes are explicit governance events; old/new trust roots must have a defined acceptance transition; no silent anchor replacement; rotation fail-closed if trust state ambiguous; multi-root implementation = future concern. Multi-root support NOT implemented now. | acceptance window; no silent replacement; fail-closed | owner gate §6 |
| A44-11 | Target-node binding posture / future mechanism | **OWNER RATIFIED — CURRENT MODEL** | Owner ratified current architecture binding: `payload.unit_code`; WILAYA issuer/anchor; provisioning ceremony. Device-bound marker **NOT required** for current architecture. Future device-bound mechanisms deferred to a separate owner decision; not added now. | current model accepted; device binding deferred | owner gate §7 |
| A44-12 | Rollback/recovery operational policy | **OWNER RATIFIED** | Owner ratified the recovery principle **rollback = re-provisioning**: no silent downgrade; no V2→V1 rollback as recovery shortcut; interrupted atomic operations may retry; failed provisioning recovered through governed re-provisioning. No recovery changes implemented in this gate. | re-provisioning only; no downgrade shortcut | owner gate §8 |
| A44-13 | App Key interchange scope | **OUT OF SCOPE** | Scope classification, not a ratification; separate workstream, untouched here. | — | ADR-0044 §12, §26#5 |
| A45-01 | Approve Option A — first `identity_access` import exemption | **OWNER RATIFIED** | Option A approved (original ratification + reaffirmed): package-authenticated, self-terminating first `identity_access` import exemption, with mandatory prior `install_wilaya_certificate` and the A45-02 control set. **No alternative bootstrap mechanism authorized**: no new bootstrap artifact; no new package kind; no `.unit role=Admin`; no SEC-003 weakening; no V2-only weakening. Post-bootstrap AdminOnly unchanged. RFC authorized: **NO**. Implementation authorized: **NO**. | controls non-negotiable; no alternative mechanism; no role=Admin substitute | owner gates §1 (2026-08-14 #1), §9 (A45-01) |
| A45-02 | Exact exemption predicate | **OWNER RATIFIED** | 15 mandatory controls, unchanged: UNIT node only; prior WILAYA anchor installation mandatory; installed anchor valid; anchor issuer == package issuer; kind `identity_access`; V2 signature mandatory; SEC-003-01 mandatory; SEC-003-02 mandatory; `payload.unit_code` == local UNIT code; no canonical Admin before first import; B8 ledger empty before first import; first package sequence = 1; atomic and self-terminating apply; post-import normal AdminOnly; failure of any prerequisite fail-closed. No additional bootstrap trust mechanism may replace these controls. | controls must remain unchanged; no replacement | owner gate §10 (A45-02) |
| A45-03 | Authorization context (anonymous vs User-session) | **OWNER RATIFIED** | Owner ratified: first `identity_access` bootstrap import is **anonymous at the command authorization layer** (no Admin session exists yet); authorization provided by the complete bootstrap predicate + package authenticity chain. Exemption: narrowly scoped, single-use, self-terminating, fail-closed. After first successful import, normal authenticated AdminOnly applies. **No temporary bootstrap Admin account.** | anonymous; single-use; fail-closed; no temp Admin account | owner gate §11 (A45-03) |
| A45-04 | Audit semantics | **OWNER RATIFIED** | Owner ratified auditability of the bootstrap transition: pre-import WILAYA anchor installation; first `identity_access` bootstrap transition; success/failure outcomes relevant to the bootstrap boundary. Existing audit guarantees not weakened; no unaudited trust transition. Implementation remains unauthorized. | auditable transition; no weakening | owner gate §12 (A45-04) |
| A45-05 | Audit event taxonomy | **OWNER RATIFIED** | Owner ratified explicit dedicated governance/audit event semantics for: WILAYA anchor installation; first `identity_access` bootstrap import; bootstrap success/failure. Exact enum/constants/event identifiers = **implementation detail**, NOT invented or implemented in this gate. | explicit distinguishable auditability required; identifiers deferred to implementation gate | owner gate §13 (A45-05) |
| A45-06 | First-import sequence baseline | **DECIDED — FACT** | First `identity_access` sequence MUST be 1 (TransportGuard enforcement; owner-listed control "first package sequence is 1" in A45-02). Consistent with A44-08 (different ledger, own decision). **Refined 2026-08-15 (F-1 Option A, §26.9): per target UNIT stream.** | — | `transport_guard.rs`; ADR-0045 §8 |
| A45-07 | UNIT certificate binding at first import | **DECIDED — FACT** | No UNIT certificate at first import: binding = `payload.unit_code` + anchor/issuer (A45-02 controls confirm exactly this). UNIT cert is generated later via CSR (RFC:490, ADR-0044 §7 S4). Future mechanisms: see A44-11. | — | RFC:490; ADR-0045 §8; ADR-0044 §7 |
| A45-08 | `.unit role=Admin` | **OWNER RATIFIED** | Owner ratified: `.unit` package role is **User-only**; a `.unit` package carrying `role=Admin` is **invalid** for the governed Trust-First architecture; the escape hatch is rejected and must not be used as first-admin bootstrap, B8 bootstrap, recovery, or migration shortcut. Validation change NOT implemented in this gate. | User-only normative; role=Admin invalid; not implemented | owner gate §14 (A45-08) |
| A45-09 | Recovery / re-provisioning policy | **OWNER RATIFIED** | Owner ratified operational posture: interrupted atomic import → transaction rollback; retry of the valid first sequence permitted; no downgrade to V1; failed/invalid provisioning recovery → governed re-provisioning; no silent credential substitution; no hidden local Admin creation. UUID churn from `.unit` re-provisioning accepted as current architectural consequence unless separately redesigned. No reset mechanism implemented now. | 6-point posture; UUID churn accepted | owner gate §15 (A45-09) |
| A45-10 | Clone-risk posture | **OWNER RATIFIED** | Owner explicitly accepted: **no device-bound UNIT identity at first `identity_access` import**; accepted binding = local `unit_code`, anchored WILAYA issuer, package authenticity, provisioning ceremony. Device binding deferred to a future architecture decision; not added now. | clone-risk accepted; device binding deferred | owner gate §16 (A45-10) |
| A45-11 | Root-key relationship | **OWNER RATIFIED — PRINCIPLE ONLY** | Owner ratified: B8 Option A is **not production-ready until the production Root key certification blocker (A44-06) is resolved** — `A44-06 BLOCKED → B8 production deployment BLOCKED`. Dependency not weakened. | production deployment blocked by A44-06 | owner gate §17 (A45-11) |
| A45-12 | SEC-002 relationship | **OWNER RATIFIED** | Owner ratified: SEC-002 remains separate; B8 governs UNIT account bootstrap, does NOT create an ADMIN identity, does not replace/bypass SEC-002, does NOT authorize `issue_first_admin_key` on UNIT. Identity provisioning and account synchronization remain separate domains. | SEC-002 untouched; no ADMIN identity via B8 | owner gate §18 (A45-12) |

## 26.2 Ratified Decision — Explicit Content and Boundaries

For the originally owner-ratified rows (A45-01, A45-02):

- **What was approved**: the Option A architecture (package-authenticated, self-terminating first `identity_access` import exemption) with mandatory prior `install_wilaya_certificate`, and the exact predicate/control set of §26.1 A45-02, as fixed by the owner's instruction.
- **What was NOT approved**: no implementation; no RFC amendment (§3.10 / §3.12 D2); no different bootstrap mechanism; no `.unit role=Admin` substitute; no weakening of any listed control; no predicate additions.
- **Conditions**: the controls in A45-02 are non-negotiable ("Do NOT weaken these controls"); the exemption is UNIT-scoped, package-authenticated, atomic, self-terminating; post-bootstrap AdminOnly remains unchanged.
- **Dependencies**: functional production Root (A44-06) is a blocking external prerequisite for production readiness (A45-11); Freeze §4 Step 3 (ADR acceptance + RFC/Freeze linkage) is not executed by this gate.
- **RFC amendment authorized by these decisions**: **NO**.
- **Implementation authorized**: **NO**.

## 26.3 RFC-to-ADR Status (end of this gate)

| Item | Status |
|------|--------|
| RFC §3.10 amended | **NO** |
| RFC §3.12 D2 amended | **NO** |
| ADR-0038 mirror updated | **NO** |
| ADR-0008 superseded | **NO** |
| ARCHITECTURE_FREEZE modified | **NO** |

## 26.4 Implementation Status

> **NO IMPLEMENTATION AUTHORIZED.**
>
> Owner ratification of A45-01/A45-02 does not equal implementation authorization. A separate RFC-amendment governance gate and a separate implementation-authorization gate are required before any implementation.

## 26.5 Owner Decision Completion Gate — Outcome (2026-08-14)

The Owner Decision Completion Gate re-examined every remaining decision against its own instruction text, under the rule that **only an explicit owner instruction in that prompt may become `OWNER RATIFIED`**.

- **Result**: the completion gate supplied **NO new explicit owner decisions**. The §26.1 matrix is unchanged: A45-01/A45-02 remain OWNER RATIFIED (reaffirmed by the gate's §1, including the mandatory control set and the `.unit role=Admin` substitute rejection); **A44-01** and every other pending row (A44-07/08/09/10 policy halves, A45-03/04/05/08-narrowing/09/10-policy halves) remain **OWNER DECISION REQUIRED**; **A44-06** remains **BLOCKED — EXTERNAL PREREQUISITE** (no certification decision was supplied).
- **Facts vs ratification preserved**: technical feasibility ("APPROVED WITH BLOCKERS", "architecturally sound", seq=1 support, single-root model, no device binding, atomic rollback) was NOT converted into ratification.

> **Superseded (Owner Ratification Completion — 2026-08-14)**: this historical record is preserved as-is. Its statuses are superseded by §26.6 — every previously-unresolved decision was subsequently explicitly ratified by the owner (see §26.1 completed matrix).
- **Boundaries**: no implementation, no RFC amendment, no Freeze modification, no ADR-0038/ADR-0008 change, no index change (ADR status remains **PROPOSED**).

## 26.6 OWNER RATIFICATION RECORD — COMPLETED (2026-08-14)

Canonical completed owner-ratification record for ADR-0044 + ADR-0045. Source of every row: the owner's explicit decisions in the Owner Ratification Completion gate instruction (2026-08-14), sections 1–18. Ratifications are **architectural only**: implementation authorization = **NO** and RFC amendment authorization = **NO** for every row.

| ID | Explicit owner decision | Status | Boundaries | Dependencies |
|----|-------------------------|--------|------------|--------------|
| A44-01 | Approve Trust-First `.unit` V2/Ed25519 (15-point architecture: V2 authenticity, WILAYA authority, anchor-first install, Root-verified, ACTIVE cert required, HMAC-V1 legacy/retiring, no fleet secret on UNIT, signing/encryption separate, App Key encryption unchanged, no silent anchor replacement, rollback = re-provisioning, cross-WILAYA fail-closed, V2 verification mandatory, B8 via Option A) | **OWNER RATIFIED** | architectural ratification only; NO implementation; NO RFC amendment | A44-06 for production |
| A44-06 | Owner certification gate executed (2026-08-15) — ceremony artifacts found + independently verified | **CERTIFIED — EXTERNAL PREREQUISITE SATISFIED** | no key replaced or invented; private key never in repo | certification record: `docs/security/A44-06-production-root-key-certification.md` (fingerprint `7b38f2e1…`, ceremony 2026-08-14, pin match byte-exact) |
| A44-07 | Evidence-based V1 window closure: all 6 criteria (fleet V2-capable; no production UNIT needs V1; no legitimate V1 packages in circulation; V2 interop green; rollback/recovery evidence; governance-event record) | **OWNER RATIFIED** | no calendar date; implementation/removal separately unauthorized | evidence per criteria |
| A44-08 | First `.unit` V2 `package_sequence = 1` | **OWNER RATIFIED** | TransportGuard semantics unchanged | — |
| A44-09 | Fleet-sync transition strategy (upgrade fleet → verify readiness → close V1 window per A44-07 → then retire V1) | **OWNER RATIFIED** | not implemented now; V1 not removed now | A44-07 closure |
| A44-10 | Root rotation via explicit acceptance window; root changes = governance events; no silent anchor replacement; fail-closed on ambiguity; multi-root = future implementation concern | **OWNER RATIFIED** | multi-root not implemented now | — |
| A44-11 | Current binding accepted: `payload.unit_code` + WILAYA issuer/anchor + provisioning ceremony; device-bound marker NOT required now | **OWNER RATIFIED — CURRENT MODEL** | device binding deferred to separate future owner decision | — |
| A44-12 | Rollback = re-provisioning; no silent downgrade; no V2→V1 recovery shortcut; interrupted atomic ops may retry | **OWNER RATIFIED** | no recovery changes implemented | — |
| A44-13 | App Key interchange — separate workstream | **OUT OF SCOPE** | untouched | — |
| A45-01 | Option A approved: package-authenticated, self-terminating first `identity_access` import exemption; mandatory prior `install_wilaya_certificate`; no new bootstrap artifact/kind; no `.unit role=Admin`; no SEC-003 weakening; no V2-only weakening | **OWNER RATIFIED** | NO implementation; NO RFC amendment; post-bootstrap AdminOnly unchanged | A44-06 for production readiness (A45-11) |
| A45-02 | 15 mandatory controls unchanged (UNIT only; prior valid anchor; issuer==anchor; kind `identity_access`; V2; SEC-003-01; SEC-003-02; unit_code match; no canonical Admin; empty B8 ledger; seq=1; atomic+self-terminating; post-import AdminOnly; fail-closed) | **OWNER RATIFIED** | controls must remain unchanged; no replacement mechanism | — |
| A45-03 | Bootstrap import anonymous at command-authorization layer; authorization = full predicate + authenticity chain; single-use, self-terminating, fail-closed; NO temporary bootstrap Admin account | **OWNER RATIFIED** | narrow scope; post-import normal AdminOnly | — |
| A45-04 | Bootstrap transition must be auditable: anchor installation, first import, success/failure outcomes; no weakening of audit guarantees; no unaudited trust transition | **OWNER RATIFIED** | implementation unauthorized | — |
| A45-05 | Explicit dedicated audit event semantics for anchor install / first bootstrap import / success-failure; exact identifiers = implementation detail | **OWNER RATIFIED** | identifiers NOT invented/implemented here | — |
| A45-06 | First-import sequence baseline = 1 (code-enforced fact) | **DECIDED — FACT** | Refined 2026-08-15 (F-1 Option A, §26.9): first `identity_access` import sequence = 1 **per target UNIT stream**; producer allocates from `(issuer_identity_id, target_unit_code)` (migration 009) so every fresh UNIT receives its own sequence-1 package. Consumer rule unchanged. | TransportGuard + migration 009 |
| A45-07 | No UNIT certificate at first import; binding = unit_code + anchor/issuer (fact) | **DECIDED — FACT** | UNIT cert later via CSR | RFC:490 |
| A45-08 | `.unit` role **User-only**; `role=Admin` on `.unit` invalid; escape hatch rejected for bootstrap/recovery/migration shortcuts | **OWNER RATIFIED** | validation change NOT implemented | — |
| A45-09 | Recovery posture: transactional rollback; retry valid first sequence; no V1 downgrade; governed re-provisioning; no silent credential substitution; no hidden local Admin; UUID churn accepted | **OWNER RATIFIED** | no reset mechanism implemented | — |
| A45-10 | Clone-risk explicitly accepted: no device-bound UNIT identity at first import; binding = unit_code + issuer + authenticity + ceremony; device binding deferred | **OWNER RATIFIED** | device binding not added | — |
| A45-11 | Principle: A44-06 BLOCKED → B8 production deployment BLOCKED | **OWNER RATIFIED — PRINCIPLE ONLY** | dependency not weakened | A44-06 |
| A45-12 | SEC-002 separate: B8 governs UNIT account bootstrap; no ADMIN identity; no replace/bypass of SEC-002; no `issue_first_admin_key` on UNIT | **OWNER RATIFIED** | identity provisioning vs account sync remain separate domains | — |

**Global boundaries (all rows)**: implementation authorization = **NO**; RFC amendment authorization = **NO**; ARCHITECTURE_FREEZE modification = **NO**; ADR-0038/ADR-0008 changes = **NO**; ADR status remains **Proposed** (Freeze §4 Step 3 not executed); no key material generated or modified.

> **Update (2026-08-14 — RFC-Amendment Governance Gate)**: the "RFC amendment authorization = NO" boundary above refers to the ratification gate itself. In the subsequent RFC-Amendment Governance Gate, the RFC amendment WAS executed as a documentation-only delta (§26.7): RFC §3.10/§3.12 amended and ADR-0038 §6 mirror updated, reflecting these ratified decisions. Implementation authorization remains **NO**; ADR status remains **Proposed**; Freeze modification remains **NO**.

## 26.7 RFC-Amendment Governance Gate — Outcome (2026-08-14)

- **Authority**: Freeze §4 Step 3 — "Once the RFC is approved": owner ratification (completed 2026-08-14) = RFC approval; the minimum documentation delta reflecting the ratified architecture was executed. Step 4 (implement) NOT activated.
- **RFC `2026-08-04-node-identity-trust.md` amended (docs only)**:
  - §3.10: permanent Bootstrap exception for `.unit` replaced by normative Trust-First V2/Ed25519 text (WILAYA authority; ACTIVE anchor installed before acceptance; root-verified; User-only role — `role=Admin` invalid; first seq = 1; no fleet secret on UNIT; no silent anchor replacement; cross-WILAYA fail-closed; rollback = re-provisioning; root rotation via explicit acceptance window; HMAC-V1 legacy/retiring under evidence-based window A44-07 with fleet-sync transition A44-09). Historical rationale preserved.
  - §3.12: new decided rows — "الترتيب المعياري للثقة" (anchor before V2 `.unit` and before first `identity_access`) and "أول استيراد `identity_access` على عقدة UNIT جديدة (B8 bootstrap)" carrying the ratified Option A predicates (A45-01..05/12), no invented identifiers.
  - ADR-0038 §6 mirror updated with the same amendment (marked "تعديل مرآة 2026-08-14").
- **NOT executed**: ARCHITECTURE_FREEZE modification (prohibited this gate — remains for the final Step 3 documentation gate) · ADR-0008 supersession (deferred — A44-07 evidence criteria unmet; RFC keeps HMAC-V1 legacy-readable) · ADR-0045 status (remains **Proposed**) · any source/test/config change.
- **A44-06 impact**: does NOT block RFC amendment (decision document) nor ADR amendment — blocks **production readiness only**; no certification claimed, no key material touched.
- **Boundaries**: RFC amendment ≠ implementation authorization; RFC amendment ≠ production authorization; nothing staged or committed.

## 26.8 Final Freeze/ADR Status Governance Gate — Outcome (2026-08-14)

- **Freeze §4 Step 3 closed**: RFC approved (Accepted + owner ratification of the architecture) → ADR status transitioned **PROPOSED → ACCEPTED** (Step 3.2); RFC linkage complete (Step 3.3); freeze document updated to reflect the change (Step 3.4 — §2.2 B8 exemption, §2.7 `.unit` Trust-First V2, §2.8 A44-06 production blocker).
- **Distinctions preserved**: architectural acceptance ≠ implementation authorization ≠ production readiness. Implementation **NOT AUTHORIZED** (separate gate required). Production was **BLOCKED** by A44-06 — **resolved 2026-08-15: root key CERTIFIED** (record: `docs/security/A44-06-production-root-key-certification.md`); production readiness is checked in a separate final gate.
- **ADR-0008**: unchanged — supersession deferred pending the ratified V1-closure evidence criteria (A44-07).
- **Historical gate records**: all prior gates (§25/§26.1–§26.7, HARD STOP history) preserved as-is.
- **Boundaries**: documentation only; no code/tests/migrations/keys; nothing staged or committed.

## 26.9 F-1 Sequence-Ledger Resolution — Option A (2026-08-15)

- **Problem (F-1, P1, Security Closure Audit)**: the producer global per-issuer
  ledger (RFC §3.4.1, `sync_issuer_sequence_state`) is shared by all five V2
  kinds (products, daily_report, monthly_summary, stock_movements,
  identity_access); the consumer rule "first import on an empty ledger MUST be
  1" (A45-06 / B8 control 11) is enforced per issuing identity. Composition:
  only ONE sequence-1 package per WILAYA issuer could ever exist → a second
  fresh UNIT (and even the first, after any other V2 export) could never
  bootstrap. All B8 tests bypassed the producer ledger by hand-crafting
  `package_sequence = 1`; no multi-UNIT producer test existed.
- **Owner decision (2026-08-15, Owner Decision / F-1 Resolution Gate)**: **Option
  A — per-(issuer, target) producer stream for `identity_access`**. Ratified
  semantics (binding):
  1. `identity_access` export sequences are allocated from a stream keyed by
     **(issuer_identity_id, target_unit_code)**; each fresh UNIT receives its
     own `1, 2, 3, ...` stream from the same WILAYA issuer.
  2. The other V2 kinds keep the **global per-issuer producer ledger** unchanged
     (`sync_issuer_sequence_state`).
  3. Consumer-side semantics unchanged: node-side `sync_issuer_sequence`,
     empty-ledger ⇒ first accepted sequence = 1, contiguous enforcement, replay
     protection, duplicate detection, B8 predicates, atomic application,
     post-bootstrap AdminOnly. No consumer control is weakened.
  4. `.unit` V2 (ADR-0044 A44-08) unchanged: fixed `package_sequence = 1`,
     outside the Transport Guard ordering domain; MUST NOT advance or consume
     the `identity_access` producer stream.
- **A45-06 refinement (this amendment)**: "first `identity_access` import
  sequence = 1" now means **per target UNIT stream**. Control 11 is NOT
  weakened — it is preserved per unit and is now satisfiable fleet-wide. The
  historical decision is preserved; this refinement implements the ratified
  F-1 Option A resolution.
- **Implementation (2026-08-15, IMPLEMENTATION-AUTHORIZATION gate)**: migration
  009 (dedicated additive table `identity_access_export_sequence` with composite
  PK, matching the migration conventions; no alteration of existing tables);
  repository `IdentityAccessExportSequenceStateRepository` with the same
  begin/commit advance-on-success contract and fail-closed regression check;
  `IdentitySignedExportService::export_v2_identity_access_package` (target
  `payload.unit_code` — authoritative server-side value built from the local
  `units` row); command `export_identity_access_package` routed to it. Proven
  by the real-producer multi-UNIT test (`f1_multi_unit_producer_tests.rs`):
  UNIT-A seq 1 and UNIT-B seq 1 ACCEPTED from the same WILAYA issuer; seq 2/3
  per unit ACCEPTED; replay/cross-target/gap/seq-0/seq-2-first rejected;
  migration 009 upgrade/fresh verified.
- **Boundaries**: A44-06 remains BLOCKED — EXTERNAL PREREQUISITE (production
  blocked); F-2 (cross-WILAYA Admin-import binding) remains P3, pre-existing,
  unchanged; ADR-0044 requires no amendment (`.unit` semantics untouched);
  no production release authorized by this amendment.

# 27. HARD STOP

> **HARD STOP — OWNER RATIFICATION COMPLETED.**
>
> All explicitly supplied owner architectural decisions have been recorded.
>
> Production remains blocked by A44-06 Production Root-key certification.
>
> RFC amendment is NOT authorized by this gate.
>
> Implementation is NOT authorized by this gate.
>
> The next governance step is a separate RFC-AMENDMENT GOVERNANCE GATE, followed only later by an independent IMPLEMENTATION-AUTHORIZATION GATE.