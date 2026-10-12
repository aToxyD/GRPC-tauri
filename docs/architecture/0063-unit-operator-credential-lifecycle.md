# ADR 0063: UNIT Operator Credential & Initialization Lifecycle

# Status
Accepted (2026-10-05); amended by owner-approved Errata E-1 and E-2 (2026-10-06) and
Erratum E-3 (2026-10-09); and by Errata E-4 and E-5 (2026-10-10) — see `# Errata` at the end
of this document. E-1/E-2 corrected
factual and implementation findings; **E-3 withdraws decision D7/§7.2** (the WILAYA-side
`set_unit_user_password` reset, now removed). **E-4 formalizes the mandatory ordered UNIT
initialization sequence and its dispatch gate; E-5 retires the package-based UNIT admin
disable/re-enable channel (`admin_enabled` is removed from `AdminAccessPayload`).**

> **Governance state.** This ADR is **Accepted** (2026-10-05) by owner ratification of the
> eighteen credential/lifecycle decisions enumerated in §2. It is the **authoritative
> amendment record** for the frozen documents listed in §1 — those files are NOT edited by
> this ADR (per `AGENTS.md` §8 C1 and `docs/architecture/ARCHITECTURE_FREEZE.md` §4).
> **Implementation is authorized but NOT yet performed**: this record is governance/design
> only. No Rust, Svelte, TypeScript, schema, or test change accompanies it. `# Verification
> Expectations` is the normative implementation checklist for the follow-on work.

# Date
2026-10-05

# Owner
Architecture / Security

# Reference

## 1. Documents amended or reconciled by this ADR (frozen files are NOT edited)

| Document | Exact locus | Relationship |
|----------|-------------|--------------|
| `docs/architecture/0040-identity-access-sync.md` | §"Invariant 13 — local password changes forbidden" (lines 100–108) | **Narrowly amended.** Invariant 13's prohibition on local mutation of a *Wilaya-governed synchronized* credential is narrowed to exclude the two UNIT-local credential transitions this ADR defines (§6, §7). Invariant 13 otherwise stands, including its rule that `set_fleet_admin_password` remains the only password-mutation surface for the **fleet `admin`** account (the former **WILAYA-side operator mirror** surface, `set_unit_user_password`, was **removed** by Erratum E-3, 2026-10-09). |
| `docs/architecture/0052-canonical-unit-operator.md` | §D1 (line 46), §D3 (line 60) | **Amended.** §D1's sentence *"`CreateUnitRequest` carries `{code, name, password}` only … The edit path rotates passwords only"* is superseded by §3 and §9 of this ADR: creation carries `{code, name}` and the edit path can no longer change `code` or rotate a password. §D1's canonical-name rule (`user`), §D2's node-scoped uniqueness, and §D3's structural rename prohibition are **retained unchanged**. |
| `docs/architecture/0051-admin-access-package.md` | §5 Account Ownership Invariant (lines 77–92), §8 Authorization (line 131) | **Reconciled, unchanged.** §5's byte-for-byte operator-preservation invariant is *preserved* and is the reason the initialization latch in §8 may not be expressed as operator-row state. §8's first-import authorization model is retained and given the additional lifecycle distinction in §10. |
| `docs/architecture/0044-unit-trust-first-v2-bootstrap.md` | §8 `.unit` V2 Contract (line 242), §8.3 Packaged-Identity Contract (line 285) | **Extended, unchanged.** §8/§8.3 remain the normative `.unit` envelope and packaged-identity contract. §11 adds one constraint on the credential sub-object only. §8.3's verification rule (3) *"`subject_id` == unit package's unit"* is the load-bearing fact for `identity_store` ownership in ADR-0064 §2. |
| `docs/architecture/0045-b8-first-identity-access-import.md` | §5 Fresh UNIT State (line 90), §26.1 decision A45-03 (line 457) | **Referenced, unchanged.** A45-03's anonymous-at-the-command-authorization-layer bootstrap grant applies to the legacy `identity_access` kind. It is **not** the authorization model of `admin_access`, which is `AuthenticatedOnly` (see §3 fact F13). |
| `docs/architecture/ARCHITECTURE_FREEZE.md` | §2.2 Authorization Model (line 106), §2.3 Audit Trail (line 113) | **Referenced, unchanged.** §2.2 is the frozen authorization model whose single choke point §5 relies on. §2.3's dual-write, hash-chain, transactional-audit, additive-only and single-repository rules are inherited unchanged by every audit obligation in this ADR. |
| `docs/architecture/0055-contract-centric-procurement.md` | line 83 (`historical fulfillment is never erased`) | Retention interaction is **not** decided here — see `docs/architecture/0064-unit-export-state-and-owned-data-deletion.md` §7. |
| `docs/technical/OPERATIONAL_LIMITATIONS.md` | §3 *One-Way Account Sync, B8* (line 47) | **Qualified in place.** Its "WILAYA is the sole source of truth for accounts on a UNIT node" claim is now explicitly scoped to **sync-delivered accounts only**, with an adjacent new bullet recording §7's local UNIT-admin authority, §6's self-change path, and §12's prohibition of anonymous recovery. |
| `docs/technical/CLAIM_VERIFICATION_MATRIX.md` | *Fail-Closed* and *Immutable* rows | **Qualified in place.** See ADR-0064 §9.2 U-4 for the full reconciliation record, which covers both rows and the added credential-lifecycle row. |
| `docs/governance/frontend/PROJECTION_OWNERSHIP_MAP.md` | *Product & Unit*, *Local UNIT Credential Lifecycle* | **Extended, human-readable.** Records that `password` is removed from `CreateUnitRequest` and registers an owner contract for §6/§7's two commands **before** they exist. Entries are marked *RATIFIED, NOT YET IMPLEMENTED* and are excluded from the summary counts. The generated FE-160 baseline is **not** modified — regenerating it belongs to the implementation phase. |
| `docs/security/THREAT_MODEL.md` | §2 threat matrix, `ت 10` (lines 104–117) | **Extended, unchanged existing threats.** §12's fixed bootstrap credential (`0000`) and its fail-closed gate at the single authorization choke point are enumerated as threat `ت 10` (*Fixed Bootstrap Credential & Forced-Password Lifecycle*), using the document's existing four-field per-threat template. `ت 1`–`ت 9` are byte-identical, and no new severity or risk-classification system was introduced, because the existing methodology provides none. The entry is marked as ratified-but-not-yet-implemented. Added on owner decision; this ADR itself did not author the security classification. |

## 2. Owner-ratified decision set (normative, 2026-10-05)

The following eighteen owner decisions are FINAL and AUTHORITATIVE and are encoded by
this ADR. They are reproduced here so the ADR is self-contained as a governance record.

| # | Ratified decision | Encoded at |
|---|-------------------|-----------|
| D1 | WILAYA UNIT creation takes only `code` + `name`; the WILAYA MUST NOT supply a password. The canonical local UNIT operator is `username = user`, initial bootstrap password `0000`, `role = User`. `0000` is bootstrap-only and MUST NOT weaken the normal password policy; bootstrap handling uses a narrow, explicit bootstrap exception; the normal policy is unchanged for ordinary changes and resets. | §3, §4 |
| D2 | The first successful bootstrap login MUST force a password change before normal application use. Forced credential state MUST be enforced centrally in the backend; frontend routing/UI alone is insufficient. Only explicitly approved credential/bootstrap operations remain available while the forced state is active. | §5 |
| D3 | UNIT `user` can change their own password from UNIT Settings: verify current password, new password satisfies the normal policy, new password differs from the current one, Argon2 preserved, node/UNIT binding preserved, rate limiting preserved, password update and forced-state clear are atomic, the operation is audited with authenticated actor attribution, and the session is preserved appropriately. | §6 |
| D4 | A UNIT-local admin is established through the WILAYA-issued `admin_access` lifecycle. After the initial UNIT user completes the bootstrap password change, the UNIT MUST complete the `admin_access` initialization lifecycle so a local admin exists. This is part of initialization completion. | §8, §10 |
| D5 | Initialization completion is durable. The **existence** of the appropriate local UNIT admin establishes completion — NOT "currently active admin". If the WILAYA later disables/revokes the admin, the UNIT MUST NOT return to bootstrap state. The lifecycle states are distinguished: initial bootstrap, forced password change, admin establishment, initialization complete, later admin disable/revocation, explicit UNIT deletion. Explicit UNIT deletion is the lifecycle event that destroys the UNIT's identity/state. | §8, §10 |
| D6 | After initialization, a UNIT admin can reset the canonical local `user` password: authenticated local UNIT admin only; target is the canonical local `user`; the temporary credential satisfies the normal password policy; literal `0000` is NOT used for an ordinary admin reset; the reset sets forced-password-change state; the authenticated admin session is preserved; the operation is audited with authenticated admin attribution; rate limiting and security controls are preserved. No anonymous recovery mechanism is allowed. | §7, §12 |
| D7 | **Withdrawn by Erratum E-3 (2026-10-09, owner-approved).** The former decision retained the WILAYA-side UNIT user password reset on the incorrect premise that setting forced state on the WILAYA-side operator row would propagate through `.unit` export/replacement. The `.unit` package carries no authoritative forced-state flag and UNIT import enforces the state locally, so that premise is false. Under D1 the WILAYA does not supply the operator's operational password; the WILAYA-side reset command is **removed** and no WILAYA-side operator-password reset path remains. | §7.2 (withdrawn), E-3 |
| D8 | UNIT `code` is immutable after creation, because the password/node identity binding depends on UNIT code / node identity. Mutable UNIT code with compensating password changes is NOT preserved. Update flows MUST reject UNIT code changes. | §9 |
| D9 | The `.unit` package MUST NOT authoritatively carry a durable `must_change_password` flag. Imported/replaced operator credentials MUST enter the appropriate local forced-password lifecycle. A serialized stale forced-state value MUST NEVER override local security policy. | §11 |
| D10 | `admin_access` MUST distinguish initial bootstrap, initialization completion, normal post-initialization operation, later admin disable/revocation, and explicit UNIT deletion. `admin_access` MUST NOT silently reopen bootstrap after initialization completion. The existing operator-preservation rule is reconciled with this credential lifecycle without creating privilege escalation. **Amended by Erratum E-5 (2026-10-10): the package-carried `later admin disable/revocation` situation is retired — `admin_access` carries no UNIT-local enable/disable control; see `# Errata`.** | §10 |
| D11 | For WILAYA Units, a UNIT is **EXPORTED** if the WILAYA-side `identity_store` contains ANY historical identity row for that UNIT, regardless of status (`ACTIVE`, `REVOKED`, `SUPERSEDED`, `EXPIRED`). Once EXPORTED: Edit hidden/disabled, Export hidden/disabled, Delete remains. Enforced in the backend as well as the UI; not UI-only. | `0064` §1, §3 |
| D12 | WILAYA-side `identity_store` rows with `subject_type = 'UNIT'` are UNIT-owned. Successful UNIT deletion removes them atomically. A later recreation of the same UNIT code starts a clean identity lifecycle; deleted UNIT identity rows are NOT preserved merely to block code reuse. | `0064` §2 |
| D13 | UNIT deletion is an explicit destructive lifecycle operation: delete all legitimately UNIT-owned data, preserve global/shared data, atomically and fail closed, against an explicit ownership matrix covering indirect ownership where a child table has no `unit_id`. | `0064` §4 |
| D14 | `audit_log` is globally retained and `audit_log.user_id` is nullable with `ON DELETE SET NULL`; FK nullification MUST NOT be relied on silently. The canonical local UNIT operator may be hard-deleted only when zero `audit_log` rows reference that user; if any reference exists, UNIT deletion fails closed and rolls back atomically. No tombstone model is invented. The audit-reference check is inside the same deletion transaction. | `0064` §5 |
| D15 | UNIT deletion MAY purge UNIT-owned historical/domain rows even when their `fiscal_year` is archived — an explicit destructive lifecycle exception. `ARCHITECTURE_FREEZE.md` §2.4 closed-fiscal-year immutability is explicitly re-scoped for this feature. Node-global fiscal machinery, global fiscal aggregates, `fiscal_year_status`, `opening_balance_snapshots`, `fiscal_export_snapshots`, and any other node-global fiscal machinery identified by repository inspection remain protected. `FiscalHistoricalGuard` MUST NOT be extended to block the approved purge. The exception applies only to UNIT-owned destructive purge. | `0064` §6 |
| D16 | The UNIT purge explicitly qualifies the otherwise applicable historical-retention rules. The governance documentation MUST explicitly address interaction with ADR-0055 line 83, ADR-0061 §7, and `ARCHITECTURE_FREEZE.md` §2.4. The frozen documents themselves are NOT modified. The new ADR defines the precise UNIT destruction exception and its boundaries. Global audit/security records and global/cross-UNIT fiscal data remain subject to their existing retention rules. | `0064` §7 |
| D17 | There is no anonymous password recovery. No security questions, hidden master passwords, recovery codes, undocumented bypasses, or unauthenticated password reset paths are introduced. After initialization, the local UNIT admin is the recovery authority for the canonical local `user`. | §12 |
| D18 | The application has NOT been officially deployed or used; there is no legacy production database to preserve. Schema changes MAY directly modify the authoritative initial schema. Migrations MUST NOT be created merely for development-database compatibility; development databases may be recreated or reset. | §13, `0064` §8 |

# Context

## 3. Inspected repository facts

Every fact below was read from the working tree at commit `3e982990949cc666ccbdc697ee20b8111be6c16c`
(branch `feature/unit-operator-credential-lifecycle`). Line references are exact.

### 3.1 Canonical operator identity and node binding

* **F1** — `src-tauri/src/application/services/unit_service.rs:19` declares
  `pub const OPERATOR_USERNAME: &str = "user";`. The canonical name is server-derived and,
  per its own doc comment, "never caller-supplied".
* **F2** — `unit_service.rs:41-79` (`create_unit`) hashes with
  `self.password_port.hash_node(&req.password, node_id)` where `node_id = &req.code`, then
  calls `user_repo.upsert_user(&user_id, OPERATOR_USERNAME, &password_hash, UserRole::User,
  node_id, &now)` and `unit_repo.insert_unit(...)`.
* **F3** — `src-tauri/src/infrastructure/security/password_hash_provider.rs:44-59` shows the
  hash construction: `hash_bound` first computes `pre_hash(password, key)` and then applies
  `Argon2::default()` with a fresh `SaltString::generate(&mut OsRng)`.
* **F4** — `password_hash_provider.rs:66-70` shows `pre_hash` is
  `HMAC-SHA256(key = <domain>, message = password)` rendered as lowercase hex. For a UNIT
  operator the key is `node_id`, i.e. the **UNIT code**.
* **F5** — Consequence of F3+F4, verified by the provider's own test
  (`password_hash_provider.rs:88-101`): the same password hashed under two different
  `node_id` values produces pre-hashes that cannot verify against each other. A UNIT
  operator credential is therefore cryptographically bound to the UNIT code. This is the
  factual basis for D8.
* **F6** — `password_hash_provider.rs:16` defines
  `const GLOBAL_ADMIN_DOMAIN: &str = "offline-pos-admin-v1";` and `hash_admin` /
  `verify_admin` use it. The `admin` credential is fleet-wide and **node-independent**,
  unlike the operator credential.

### 3.2 The enforced password policy (and the two decoys)

* **F7** — `src-tauri/src/domain/validation.rs:798-816` defines
  `validate_change_password(new_password)`, which requires length ≥ 8, at least one ASCII
  uppercase, at least one ASCII lowercase, and at least one ASCII digit, and otherwise
  returns `AppError::Validation(ValidationError::WeakPassword { .. })`. **This is the
  production-enforced normal password policy.** Its only production caller is
  `application/services/user_account_sync_service.rs:91`.
* **F8** — `src-tauri/src/domain/validation.rs:608-670` defines
  `validate_create_unit_request`, which independently re-implements the same three
  composition rules for `req.password`. This function has **no production caller**; the
  WILAYA `create_unit` command path does not invoke it. Unit creation therefore currently
  applies **no** password validation.
* **F9** — `src-tauri/src/domain/security.rs` defines a second, *different*
  `validate_password_strength` (length ≥ 8, one uppercase, one lowercase, one digit, plus a
  case-insensitive denylist `["password", "123456", "qwerty", "admin"]`). It also has **no
  production caller**. It is **not** the enforced policy and must not be treated as such.
* **F10** — Under F7, the bootstrap credential `0000` fails **all three** composition
  requirements: length 4 < 8, no ASCII uppercase, no ASCII lowercase. A narrow bootstrap
  exception is therefore mathematically mandatory — no ordinary policy relaxation can admit
  `0000`.

### 3.3 Account rows, disable semantics, and the bootstrap predicate

* **F11** — `src-tauri/src/db/migrations/001_initial.sql` `users` table columns are `id`,
  `username`, `password_hash`, `role`, `created_at`, `updated_at`, `node_id`, `deleted`,
  with `UNIQUE(username, node_id)`. There is **no** `must_change_password` column and
  **no** `is_active` column. Disable is expressed solely as `deleted = 1`.
* **F12** — `src-tauri/src/repositories/users.rs:261-266` (`set_deleted`) is a soft delete:
  `UPDATE users SET deleted = ?1, updated_at = ?2 WHERE id = ?3`, documented as
  "Disabling is a soft-delete: the row is preserved and re-enableable."
  `users.rs:301-320` (`upsert_synced_admin`) writes `deleted = excluded.deleted`, and its
  caller `application/usecases/sync/import_admin_access_package.rs:87-92` passes
  `!input.package.payload.admin_enabled` as that flag. **Disabling a UNIT admin therefore
  sets `deleted = 1` on an existing row; it never removes the row.** **Amended by Erratum E-5
  (2026-10-10), implemented by Slice A (2026-10-11, commit `7259e2e`): the supported
  `admin_access` import no longer passes a lifecycle-state flag, so the local UNIT admin's
  `deleted` is no longer writable by any supported operation.**
* **F13** — `users.rs:363-370` (`count_active_admins`) is
  `SELECT COUNT(*) FROM users WHERE deleted = 0 AND role = 'Admin'` — note it is **not**
  `node_id`-scoped.
* **F14** — `application/services/admin_access_first_import_predicates_service.rs:66` sets
  `no_active_admin = executor.users().count_active_admins()? == 0`, and
  `all_hold()` (lines 33-35) requires it together with `anchor_installed` and
  `anchor_is_issuer`.
* **F15** — The module doc comment at
  `admin_access_first_import_predicates_service.rs:17-19` asserts the gate is
  "Self-terminating: once the first import succeeds, a canonical Admin account exists and
  `no_active_admin` fails closed — the exemption cannot be replayed on a provisioned node."
  **Read against F12+F13+F14, that assertion holds only while the admin stays enabled.**
  Disabling the admin (`deleted = 1`) drives `count_active_admins()` back to `0`, which
  re-satisfies `no_active_admin` and re-opens the first-import exemption on a node that was
  already initialized. This is the concrete defect D5 forbids.
* **F16** — Because F12 guarantees disable is a soft delete, the predicate
  "a `users` row exists with `username = 'admin'` and `node_id` = the local UNIT code,
  **ignoring** `deleted`" is **monotonic**: no supported operation removes that row. This is
  the factual feasibility basis for the latch in §8.

### 3.4 `admin_access` import and authorization

* **F17** — the `admin_access` apply path carries exactly `{admin_password_hash,
  admin_enabled}` and the file header (lines 11-14) states it "rejects unknown fields at
  deserialization, so no target-unit or operator-account material can enter." **Amended by
  Erratum E-5 (2026-10-10): the payload becomes `{admin_password_hash}` only — `admin_enabled`
  is removed.**
* **F18** — `import_admin_access_package.rs:83-90` performs exactly two mutations:
  `executor.users().upsert_synced_admin(&Uuid::new_v4()..., &payload.admin_password_hash,
  input.local_unit_code.trim(), !payload.admin_enabled, &now)` followed by
  `registry.mark_imported(&package_id)`. Replay is refused earlier by
  `registry.has_imported(&package_id)` (lines 74-79) with
  `BusinessLogicError::DuplicateSyncPackage`. **Amended by Erratum E-5 (2026-10-10),
  implemented by Slice A (2026-10-11, commit `7259e2e`): the supported import path no longer
  writes `deleted` — no lifecycle-state flag is derived from the payload.**
* **F19** — The apply path has **no** dependency on `upsert_synced_user`, on any username
  mutation, or on any UNIT-user password mutation (file header lines 6-14). The operator row
  is structurally unreachable from this kind. This is the implementation of ADR-0051 §5 and
  is preserved unchanged by this ADR.
* **F20** — `src-tauri/src/commands/import_export.rs:1760` authorizes the
  `admin_access` import with `authorize_command(state, Action::AuthenticatedOnly, None)` —
  any authenticated session, **not** `AdminOnly`. Lines 1950-1957 add a structural guard:
  if `settings.node_type != crate::models::NodeType::Unit` the import is refused. The
  `AuthenticatedOnly` variant is declared at
  `src-tauri/src/application/authz/actions.rs:45`.

### 3.5 `.unit` import, `.unit` creation credential, and existing password commands

* **F21** — `src-tauri/src/commands/import_export.rs:662-683`: the `.unit` import authorizes
  as an anonymous `UserContext::new("system", "system_bootstrap", None)` when
  `SettingsService::is_setup_mode()` is true, and otherwise requires
  `authorize_command(&state, Action::AdminOnly, None)`.
* **F22** — `application/services/node_package_service.rs:134-192`
  (`import_unit_node_package`) validates the role before any write
  (`validate_unit_node_role`), rejects `role == UserRole::Admin`, calls
  `settings_repo.update_unit_node_settings(...)`, then resolves the operator row id with
  `get_user_by_username_raw(&package.user.username, node_id)` (reusing an existing row id so
  the unit→user link survives re-import) and calls
  `upsert_raw_user(&user_id, &package.user.username, &package.user.password_hash, &role.to_string(), node_id, &now)`.
  It then calls `upsert_raw_unit(...)` and `update_unit_user(&package.unit.id, &user_id)`.
* **F23** — F22 means the `.unit` payload's `password_hash` **overwrites** the local operator
  password hash on every import, including a re-import on an already-configured node (F21:
  `AdminOnly`). The WILAYA-side hash is node-bound to the unit code, and the import re-stamps
  `node_id = &package.unit.code` so the binding domain matches (F2, F22).
* **F24** — `src-tauri/src/models/unit.rs` declares `CreateUnitRequest { code, name, password }`
  and `src/lib/types.ts:91-95` mirrors it.
* **F25** — The existing WILAYA-side reset is
  `commands/import_export.rs:1569-1592` (`set_unit_user_password`), authorized
  `Action::ManageAccountSync` (WILAYA + `AdminOnly`; action declared at
  `authz/actions.rs:62`, policy at `authz/policies/mod.rs:186`), audited with
  `AuditAction::UnitUserPasswordUpdated` inside `AuditTxService::execute_with_audit`, and
  delegating to `application/services/user_account_sync_service.rs:90-108`, which calls
  `validate_change_password` (F7), resolves the unit by code, resolves the operator, hashes
  with `hash_node(password, unit_code)` (F4), and calls
  `users().change_password(&user.id, &password_hash, &now)`.
  **[Erratum E-3, 2026-10-09 — owner-approved]** This finding records the pre-removal state;
  the `set_unit_user_password` command described here has since been **removed** (see E-3).
* **F26** — `src/lib/contracts/sync.contract.ts:65-70` owns both frontend password commands
  (`setFleetAdminPassword`, `setUnitUserPassword`);
  `src/lib/contracts/inventory.contract.ts:31,43,47` owns `createUnit`, `updateUnit`,
  `deleteUnit`.
  **[Erratum E-3, 2026-10-09 — owner-approved]** `setUnitUserPassword` has since been
  removed from `sync.contract.ts` (see E-3).
* **F27** — `src/pages/UnitsPage.svelte` currently exposes a create modal requiring a
  password (`password` state at line 52, validation at line 104), an edit modal that
  re-submits `{ password: password || "" }` (lines 140-151), a delete modal (lines 88-90),
  and an export action (lines 176-189).
* **F28** — `src/pages/SettingsPage.svelte:8,20,125` places `setFleetAdminPassword` in the
  Settings surface. There is **no** existing local self-service password-change command and
  **no** local UNIT admin credential-reset command.

### 3.5 Rate limiting and session

* **F29** — `src-tauri/src/domain/rate_limiter.rs` fixes `DEFAULT_WINDOW_SECS = 300`,
  `DEFAULT_MAX_ATTEMPTS = 5`, `DEFAULT_GLOBAL_MAX_ATTEMPTS = 20`, and
  `GLOBAL_BREAKER_KEY = "__grpc_global_login_breaker__"`.
* **F30** — `src-tauri/src/commands/auth.rs:34-84`: `login_impl` consults
  `is_global_login_allowed()` **first**, then `is_allowed(&request.username)`, then drops the
  rate-limiter lock before touching the database.
* **F31** — `auth.rs:157-190` verifies with `verify_admin(&password, &hash)` for `admin`
  rows and `verify_node(&password, &user.node_id, &user.password_hash)` for operator rows —
  i.e. the node binding of F4/F5 is what the login path actually exercises — and records
  failures via `record_login_failure`.
* **F32** — `auth.rs:198-199` writes the established session into `state.current_session`;
  `auth.rs:318-334` (`get_current_user`) and `auth.rs:350` (`check_session`) read it.
* **F33** — `src-tauri/src/commands/guards.rs:110` exposes `authorize_command` as the single
  authorization entry point. All 181 `#[tauri::command]` functions in
  `src-tauri/src/commands/` reach it, directly or through the shared import-pipeline helpers
  `run_import_pipeline_core` / `run_import_pipeline_bootstrap`
  (`commands/import_export.rs:1871,1920`), which themselves call it. `guards.rs:35` exposes
  the separate maintenance gate `require_maintenance_allows`.
  **[Erratum E-1, 2026-10-06 — owner-approved]** The "All 181 … reach it" clause is
  **factually incorrect**. A non-empty set of registered commands never calls
  `authorize_command` — session/projection utilities (`logout`, `get_current_user`,
  `check_session`), the frontend bootstrap reads (`get_settings`, `is_configured`,
  `get_build_info`, `get_login_metrics`), the report/calculation readers and the pre-auth
  ceremony commands. `authorize_command` **remains** the single authorization entry point for
  every command that performs authorization (unchanged, frozen `§2.2`), but it is not a
  universal dispatch point, which is why §5's enforcement location was corrected by E-1.

# Decision

## 1. Scope of this ADR

This ADR governs the **credential and initialization lifecycle of the canonical local UNIT
operator account** (`user`) and the `admin_access` initialization handshake that follows it.
It governs credential *state* and *authorization*. It does not govern the WILAYA Units
lifecycle projection (`EXPORTED`) or UNIT data destruction; those are
`docs/architecture/0064-unit-export-state-and-owned-data-deletion.md`.

Two invariants are load-bearing for everything below and are **retained from ADR-0051 §5 and
ADR-0052 unchanged**:

```text
I-1  admin_access MUST NOT modify the UNIT operator row in any way.
I-2  The operator username is structurally immutable and canonically `user`.
```

I-1 is the reason §8's completion latch **must not** be expressed as operator-row state, and
the reason §10 may not make `admin_access` clear or set the operator's forced state.

## 2. Terminology (normative)

| Term | Meaning in this ADR |
|------|---------------------|
| **Bootstrap credential** | The single server-side literal `0000`, used only to mint the *initial* hash of a freshly created operator credential. Never caller-supplied, never transported from a frontend, never read from a package payload as an authoritative value. |
| **Normal password policy** | `domain::validation::validate_change_password` exactly as it exists today (F7). This ADR changes nothing about it. |
| **Forced credential state** | Persisted per-account flag `must_change_password`, meaning "this credential is not usable for normal application work until it is rotated." Storage and naming are specified in §11. |
| **Initialization** | The end-to-end sequence: bootstrap credential minted → forced change completed → local admin established. |
| **Initialization complete** | The durable state defined in §8. |
| **Local admin** | The `admin` row with `node_id` = the local UNIT code, established only by `admin_access` (F17, F18). |

## 3. D1 — UNIT creation takes `code` and `name` only

WILAYA UNIT creation is authorized by exactly two caller-supplied values: `code` and `name`.

* The creation contract MUST NOT carry a password field. This **supersedes**
  `docs/architecture/0052-canonical-unit-operator.md` §D1 line 46, which states
  `CreateUnitRequest` carries `{code, name, password}`.
* The WILAYA MUST NOT accept, forward, or derive a caller-chosen initial password.
* `src/lib/types.ts` and `src/lib/contracts/inventory.contract.ts` MUST stop carrying the
  field, and `src/pages/UnitsPage.svelte` MUST stop collecting it (F24, F27).
* Validation of `code` and `name` is unchanged: `validate_create_unit_request`
  (`domain/validation.rs:608-670`) requires a 6-character alphanumeric `code` (via
  `CODE_REGEX`), a 3–100 character `name`, and `is_sql_safe(&name)`. That function's
  **password clauses must be deleted**, because no password is supplied at creation (F8).
  The `code` and `name` clauses, and therefore the function's remaining purpose, are
  unchanged.
* The canonical operator row is still created server-side with
  `OPERATOR_USERNAME = "user"` and `role = User`, `node_id = <unit code>` (F1, F2).

## 4. D1 — The bootstrap credential and its narrow policy exception

* The initial operator credential is the literal `0000`. It is held in exactly one
  server-side constant owned by the UNIT credential domain. The constant's identifier is an
  implementation detail; its **value** is normative.
* `0000` is hashed with the ordinary node-bound construction — `hash_node("0000",
  <unit code>)` — so the resulting credential is indistinguishable in format from any other
  operator credential and is bound to the UNIT code exactly as in F2/F4/F5. **No new
  derivation domain, no new KDF parameter, and no new hash format is introduced.**
* The normal password policy (F7) MUST NOT be weakened. Concretely, the exemption is a
  **narrow, explicit, single-site exception**:

```text
The bootstrap value is exempt from validate_change_password at exactly one site:
the provisioning path that mints the initial operator credential hash.
Every other password-mutating entry point calls validate_change_password unchanged.
```

* Rationale recorded from fact: F10 shows `0000` violates all three composition rules of
  F7, so the exemption is strictly required and cannot be expressed as a policy change.
  The denylist in F9 is not in the enforced policy and is unaffected either way.
* `0000` MUST NOT become a general reset value, a recovery value, or an accepted input on
  any change/reset path. D6 forbids it for ordinary admin reset; D17 forbids it as a recovery
  mechanism.

## 5. D2 — First login forces a change, enforced centrally in the backend

* The first successful login of the canonical operator using the bootstrap credential MUST
  leave the account in the forced credential state. Frontend routing, guards, or modal
  dismissal MUST NOT be sufficient to clear it (D2).
* **[Erratum E-1, 2026-10-06 — owner-approved]** Enforcement is **central**, at the
  *dispatch* choke point `src-tauri/src/commands/registry.rs::get_invoke_handler`, which
  wraps the closure returned by `generate_handler!` and therefore runs before **every**
  registered command body — registration, not per-command discipline, is what the guard hangs
  off. This corrects the original text, which named `commands/guards.rs:110`
  (`authorize_command`) "because F33 establishes that every command reaches it": that
  premise is false (F33 erratum), so the original location would have left the commands that
  skip `authorize_command` unguarded. `authorize_command` and `ARCHITECTURE_FREEZE.md` §2.2
  are untouched; the forced-state check is an additive guard evaluated earlier in the
  dispatch path. This still satisfies `AGENTS.md` §2 P3 (explicit boundaries) and still
  creates no check a future command can forget: a command cannot be invoked unless it is in
  the registry, and the registry is exactly where the check lives.
* While the forced state is active, the set of operations that may proceed is a **closed
  allowlist**. Only these categories are approved:

| Allowed | Rationale |
|---------|-----------|
| Session establishment (`login`) | Must succeed to reach the change step. |
| Session teardown / inspection (`logout`, `get_current_user`, `check_session`) | The operator must be able to leave, and the frontend must be able to render the change screen (F32). |
| Self password change (§6) | The only way out of the forced state for the operator. |
| `admin_access` import (§10) | D4 requires initialization to complete, and D10 requires bootstrap to be open until it does. The import is already `AuthenticatedOnly`, so the forced-state operator can perform it. |

  Every other command MUST be refused while the forced state is active. The allowlist is
  closed: an operation not listed here is denied.
* **[Erratum E-2, 2026-10-06 — owner-approved]** The category table above is implemented as
  exactly this closed set of command names, in `commands/guards.rs`
  (`FORCED_STATE_ALLOWED_COMMANDS`): `login`, `logout`, `get_current_user`, `check_session`,
  `touch_session`, `change_own_password`, `import_admin_access_package`, `get_settings`,
  `is_configured`. Three names are made explicit here because the table states categories:
  `touch_session` is `Action::AuthenticatedOnly` and only advances the idle timestamp — the
  operator must be able to keep the session alive while completing the change; `get_settings`
  and `is_configured` are the unauthenticated-to-render frontend bootstrap reads the change
  surface needs before it can render. None of the three mutates business state. The set is
  closed: adding a name requires an approved amendment to this ADR.
* The refusal MUST be a backend error, not an empty result set, so the UI cannot mistake it
  for "no data".

## 6. D3 — UNIT `user` self password change

Available from the UNIT Settings surface (F27, F28 confirm no such command exists today).
All of the following are normative and each is individually fail-closed:

1. The actor MUST be the authenticated canonical local `user`. No other account, including a
   local admin, may use this operation; the admin path is §7.
2. The current password MUST be verified against the stored hash using the **node-bound**
   verifier `verify_node(current, <local unit code>, stored_hash)` (F4, F31). A mismatch
   MUST NOT disclose whether the account exists.
3. The new password MUST satisfy `validate_change_password` unchanged (F7).
4. The new password MUST NOT equal the current password. Because the stored value is an
   Argon2 hash with a random salt, equality is decided by verifying the proposed new password
   against the stored hash with the node-bound verifier (`verify_node(new, <local unit code>,
   stored_hash) == true` ⇒ reject as reuse). Reuse rejection MUST NOT be implemented by
   comparing plaintext.
5. The new hash MUST be produced by `hash_node(new, <local unit code>)` — Argon2 and node
   binding preserved (F2, F3, F4).
6. The password write and the forced-state clear MUST occur in **one** transaction, so a
   failure of either leaves both unchanged. `UsersRepository::change_password`
   (`users.rs:245-254`) is a single-statement UPDATE and does not touch the forced flag, so
   the forced-state clear is an additional statement that must share the caller's
   transaction; the existing `AuditTxService::execute_with_audit` boundary
   (`commands/import_export.rs:1580-1588`) is the pattern.
7. Authentication rate limiting MUST be preserved. The attempt that produced this change is
   an authenticated action, not a login; the F29/F30 login buckets are untouched by it, and
   the operation MUST NOT introduce a path that bypasses the login limiter.
8. The operation MUST be audited with authenticated actor attribution, using
   `AuditTxService::execute_with_audit` and an `AuditAction` whose `as_str()`/parse pair is
   registered (`domain/audit.rs`, which already registers `PasswordChange`,
   `UnitUserPasswordUpdated`). The audit write inherits `ARCHITECTURE_FREEZE.md` §2.3
   (dual-write, hash chain, transactional, additive-only, single repository).
9. The session MUST be preserved appropriately: the change does not invalidate the actor's
   established session (F32), and MUST NOT mint a new one.

## 7. D6 (D7 withdrawn) — one operator reset, no duplication

Only one operator-password reset is approved: the **local UNIT admin** reset (§7.1/D6). The
former second path — the WILAYA-side `set_unit_user_password` reset (D7/§7.2) — is
**withdrawn by Erratum E-3** (2026-10-09) and its command is removed; no WILAYA-side
operator-password reset remains.

### 7.1 Local UNIT admin reset of the canonical `user` (D6)

* Authorized actor: an **authenticated local UNIT admin** — an `admin` row with `node_id` =
  the local UNIT code (F17, F18). Any other caller is refused.
* Target: the canonical local `user` row for this node. The target is derived server-side
  from the local UNIT code; no caller-supplied username or unit id selects it.
* The temporary credential MUST satisfy the normal policy (F7). Literal `0000` MUST NOT be
  used.
* The reset MUST set the forced credential state on the target, so the operator must rotate
  before normal use — the same state §5 enforces.
* The hash MUST be node-bound: `hash_node(temp, <local unit code>)` (F2, F4).
* The authenticated admin's session MUST be preserved (F32); the operation MUST NOT log the
  admin out and MUST NOT mint a session.
* The operation MUST be audited with authenticated **admin** attribution, transactionally
  (§2.3).
* Rate limiting and existing security controls are preserved; the operation is authenticated
  and therefore outside the login limiter's scope by construction (F29, F30).

### 7.2 WILAYA reset of the UNIT `user` (D7) — withdrawn by E-3

**Withdrawn.** The former decision to retain and evolve the WILAYA-side reset was based on an
incorrect premise: setting the forced credential state on the WILAYA-side operator row does
**not** cause a subsequently exported/replaced `.unit` credential to "inherit" the forced
lifecycle, because the `.unit` package carries no authoritative forced-state flag, any
serialized value is inert, and UNIT import enforces the local state independently (§11).

Under D1 the WILAYA does not supply the operator's operational password, so the command has no
legitimate role. `set_unit_user_password` is **removed** (backend command + registry entry +
`sync.contract.ts` export), and there is no WILAYA-side operator-password reset path. The
**only** supported operator-password reset is the local UNIT admin reset in §7.1 (D6). Before a
local UNIT admin exists, the documented recovery limitation (§12) is unchanged; no recovery
path is invented.

## 8. D4/D5 — Durable initialization completion and the six lifecycle states

### 8.1 The latch

Initialization completion is established by the **existence** of the appropriate local UNIT
admin, defined as:

```text
EXISTS (SELECT 1 FROM users
        WHERE username = 'admin'
          AND node_id = <the local UNIT code>)
```

evaluated **without any `deleted` or status filter**.

* This is a durable existence fact, not an activity fact, exactly as D5 requires.
* It is expressible without a schema change, because F12 proves the admin row is only ever
  soft-disabled and the canonical admin upsert (F18; now the deleted-free
  `upsert_canonical_admin` — Slice A, commit `7259e2e`) is an upsert on
  `UNIQUE(username, node_id)` — no supported operation removes the row. F16 records this
  monotonicity. **Erratum E-5 (2026-10-10), implemented by Slice A (2026-10-11): the supported
  import path does not write `deleted` at all; because the latch ignores `deleted`, the
  monotonicity holds both before and after that change, and no other supported operation can
  soft-disable the local UNIT admin.**
* It MUST NOT be expressed as `count_active_admins() == 0` / `== 1` (F13, F14). F15 proves
  that predicate re-opens bootstrap after an admin disable, which D10 forbids and D5
  forbids.
* It MUST NOT be expressed as operator-row state, because I-1 forbids `admin_access` from
  writing that row.
* It MUST NOT be expressed as a UI-derived flag, because D2 requires backend enforcement.
* The first-import exemption that consumes this latch is the `admin_access` first-import
  path. `AdminAccessFirstImportPredicatesService`
  (`admin_access_first_import_predicates_service.rs`) keeps `anchor_installed` and
  `anchor_is_issuer` unchanged (F14) and its `no_active_admin` predicate is redefined from
  "no enabled admin" to the §8.1 existence test. Its module doc comment (F15) must be
  corrected to state the real termination property.

### 8.2 The six lifecycle states

| State | Entered when | May leave via |
|-------|--------------|---------------|
| **initial bootstrap** | operator row created with the bootstrap credential hash (F2) | first login |
| **forced password change** | first successful bootstrap login, or the §7.1 reset (D2, D6) | successful self change (§6) |
| **admin establishment** | bootstrap change completed and the `admin_access` import succeeds (D4) | — |
| **initialization complete** | the §8.1 existence test holds (D5) | terminal until admin disable, UNIT deletion, or code reuse after deletion |
| **later admin disable / revocation** | **retired by Erratum E-5 (2026-10-10)**: `admin_access` carries no enable/disable control, so this state cannot be entered through any supported operation; the WILAYA-side `set_account_status` disable (F12) targets the WILAYA-scope fleet admin only, and its export fails closed with no UNIT-delivery channel | — (no package-based exit exists) |
| **explicit UNIT deletion** | the owner-approved destructive operation | see `0064` |

Two transitions are forbidden outright:

* **initialization complete → initial bootstrap** is forbidden. F15 is the concrete defect;
  §8.1 is its fix.
* **forced password change → normal use** without a successful §6 change is forbidden, and is
  enforced centrally per §5.

## 9. D8 — UNIT `code` is immutable

* UNIT `code` is immutable after creation. Update flows MUST reject a request that attempts
  to change it. Rationale is factual, not stylistic: F4/F5 show the credential hash is
  derived under an HMAC key equal to the UNIT code, so a code change invalidates every
  existing operator credential.
* This **supersedes** the `update_unit` behavior at `unit_service.rs:88-118`, which today
  calls `unit_repo.update_unit(unit_id, &req.code, &req.name, &now)` — writing `code` — and
  then re-hashes the operator password under the *new* code.
* The compensating design ("keep code mutable and rotate passwords alongside it") is
  **rejected by the owner** (D8) and is not implemented.
* The update flow retains its remaining capability: `name` remains editable.
* Ownership of the immutability check is the unit domain's validation layer
  (`domain/validation.rs`), not the command layer and not the repository, per
  `AGENTS.md` §3 (SQL confinement, layer map).

## 10. D10 — `admin_access` lifecycle reconciliation

`admin_access` MUST distinguish all five lifecycle situations in §8.2's middle rows. The
mapping onto the existing single predicate set (F14) is:

| Lifecycle situation | Existing predicate set | Required behavior |
|---------------------|------------------------|-------------------|
| initial bootstrap | `anchor_installed` ∧ `anchor_is_issuer` ∧ **§8.1 existence test false** | admit the first `admin_access` import |
| admin establishment | same set, existence test false, but an import already succeeded | a second, distinct `package_id` still applies via the canonical admin upsert (`upsert_synced_admin` at ADR time, now the deleted-free `upsert_canonical_admin` — F18/Slice A; last-arriving-wins); exact-`package_id` replay is still refused (F18) |
| initialization complete | existence test **true** | the first-import exemption is closed; later distinct `admin_access` packages continue to apply as normal post-initialization operation, never as bootstrap |
| later admin disable / revocation | existence test **true** (the row survives — F12) | the first-import exemption **stays closed**; the package carries **no** enable/disable control — a re-import updates password material only and does not write `deleted` (Erratum E-5, implemented by Slice A — commit `7259e2e`; the importer no longer writes it via the shared upsert) |
| explicit UNIT deletion | the §8.1 test can no longer be true on a deleted unit | see `0064` |

Privilege-escalation guard (D10's second clause):

* I-1 is preserved verbatim. `admin_access` reaches only the canonical admin upsert and
  remains structurally incapable of touching the operator row (F19); Slice A (commit
  `7259e2e`) implemented the dedicated deleted-free `upsert_canonical_admin` in place of the
  earlier shared `upsert_synced_admin` call, preserving the same operator-row isolation. This
  ADR therefore does **not** let
  `admin_access` clear, set, or observe the operator's forced state.
* The forced state after an `admin_access`-driven admin lifecycle change is governed by §7
  (local admin reset) and §11 (`.unit` import) only.
* The structural UNIT-node guard (F20) and the `AuthenticatedOnly` authorization (F20) are
  retained unchanged.

## 11. D9 — The `.unit` credential lifecycle

* The `.unit` package MUST NOT authoritatively carry a durable `must_change_password` value.
  A serialized forced-state value, if present in any artifact at all, is **inert**: it MUST
  NEVER override local security policy (D9). Local policy is the only authority for the
  local account's forced state.
* Storage: the forced state is persisted on the local account row in the authoritative
  initial schema, as `users.must_change_password` (integer, default set to the forced value
  for an operator row created by `.unit`). This is the flag the owner names in D9. No
  separate table and no second source of truth is introduced (P2).
* Import/replacement behavior (F22, F23): because `.unit` supplies the operator's password
  hash, an import or re-import **replaces** the operator credential. Whenever that happens —
  first provisioning or re-provisioning on an already-configured node — the local forced
  state MUST be set so the newly supplied credential cannot be used for normal work without
  a rotation. This is the "appropriate local forced-password lifecycle" D9 requires.
* The row-id reuse in F22 (`get_user_by_username_raw`) is retained so the unit→user link
  survives re-import; only the credential and the forced flag change.
* `.unit` remains governed by `docs/architecture/0044-unit-trust-first-v2-bootstrap.md` §8
  and §8.3 for its envelope, packaged identity, and install ordering. §8.3's rule (3)
  (`subject_id` == the package's unit) is unaffected.
* Authorization for `.unit` is unchanged: anonymous `system_bootstrap` in setup mode,
  otherwise `AdminOnly` (F21).

## 12. D17 — Recovery authority and prohibited mechanisms

* There is **no anonymous password recovery**. Prohibited by this ADR, explicitly:
  security questions; hidden master passwords; recovery codes; undocumented bypasses; any
  unauthenticated password-reset path.
* After initialization is complete, the **local UNIT admin is the sole recovery authority**
  for the canonical local `user`, through §7.1.
* Before initialization is complete, the WILAYA is the only party that can mint a new
  operator credential, and it does so by re-issuing `.unit` (§11).
* The `0000` bootstrap credential is not a recovery mechanism and MUST NOT be usable as one
  after the forced state is cleared.

## 13. D18 — Schema-change posture

* The authoritative initial schema is `src-tauri/src/db/migrations/001_initial.sql`, the
  repository's only migration file. Per D18 the changes required by this ADR — the
  `users.must_change_password` column of §11 — are made **directly in that file**.
* No migration is created for development-database compatibility. Development databases may
  be recreated or reset.
* Precedent: `docs/architecture/0052-canonical-unit-operator.md` §2 reached the same
  conclusion for the `UNIQUE(username)` → `UNIQUE(username, node_id)` correction, stating
  "The application is pre-release and all databases are development-local; the owner elected
  baseline schema correction over a migration chain."

# Consequences

## Security

* The operator credential becomes unusable-by-default: it is born in a forced state and
  cannot reach normal application work until rotated. `0000` is a published constant, so the
  bootstrap credential offers no secrecy; the forced state is what converts it from a
  liability into a defined starting point.
* F15 is a real, reachable re-open of the `admin_access` bootstrap exemption after an admin
  disable. §8.1 closes it without weakening any other predicate.
* No new cryptographic primitive, derivation domain, KDF parameter, trust primitive, or
  verification path is introduced. The `admin` fleet-wide domain (F6) and the operator
  node-bound domain (F4) are both untouched.

## Lifecycle and state

* The account table gains one column that is the single source of truth for "this credential
  must be rotated". Frontend, `.unit` artifacts, and `admin_access` payloads are all
  explicitly barred from being a competing authority for it (D2, D9, I-1).
* Deleting a UNIT destroys the local admin row and therefore the §8.1 latch. That is why
  deletion is the terminal lifecycle event (D5) and why `0064` must delete the operator row
  in the same atomic operation.

## Compatibility

* `CreateUnitRequest` loses a field; `updateUnit` loses both code mutation and password
  rotation. Both are breaking changes to the `inventory` contract surface and to
  `src/tests/unit/**` and `src-tauri/tests/**` expectations.
* Because `bun run check:arch` enforces FE-158/FE-160 against
  `docs/governance/frontend/baselines/*.snapshot.json`, the implementation phase MUST
  regenerate those generated baselines. They are out of scope for this ADR and were not
  touched.

# Explicitly Out of Scope

* Any application, schema, migration, or test implementation. This ADR changes no Rust, no
  Svelte, no TypeScript, no SQL, and no test.
* `must_change_password` enforcement code, the closed allowlist of §5, the self-change
  command, and the local admin reset command — all authorized here, implemented later.
* UNIT data destruction, the `EXPORTED` projection, the ownership matrix, the archived
  fiscal-year exception, and historical-retention qualification — all in `0064`.
* `identity_store` row deletion, the `.unit` envelope, trust-anchor handling, challenge–response,
  `.adminkey`, App-Key custody, and backup/restore — all unchanged.
* Any modification to the frozen documents in §1. This ADR is their amendment record.
* A `docs/architecture/rfcs/` proposal. The owner ratified the decisions directly; the
  repository's RFC-to-ADR path (`ARCHITECTURE_FREEZE.md` §4) requires a proposal before an
  ADR, and this record is written as the ADR that carries those ratified decisions.

# Implementation Boundary

Ordered work items, each independently verifiable. Listed for traceability only; none is
performed by this ADR.

1. **Schema** — add `users.must_change_password` to `001_initial.sql`; delete the now-dead
   password clauses of `validate_create_unit_request` (§3).
2. **Types/contract** — remove `password` from `CreateUnitRequest` in `src/lib/types.ts`
   (§3); remove the field from `createUnit`/`updateUnit` in `inventory.contract.ts`.
3. **Domain** — hold the bootstrap constant (§4); add the bootstrap exemption at exactly one
   call site (§4); add reuse rejection by node-bound verification (§6.4); add the
   code-immutability check (§9).
4. **Repository** — add the forced-flag column to the read/upsert/changepassword paths of
   `src-tauri/src/repositories/users.rs`; add the §8.1 existence predicate (not
   `count_active_admins`); add the `audit_log` reference-count predicate required by
   `0064` §5.
5. **Guards** — implement the §5 closed allowlist as the dispatch-level guard named by
   Erratum E-1: the allowlist constant and the enforcement function live in
   `commands/guards.rs`, and they are invoked from the registry wrapper
   `commands/registry.rs::get_invoke_handler` (not from inside `authorize_command`, per E-1);
   correct the `admin_access_first_import_predicates_service.rs` doc comment (F15) and
   redefine `no_active_admin`.
6. **Commands/services** — add the self-change command (§6) and the local admin reset
   command (§7.1); **remove** the WILAYA-side `set_unit_user_password` command (D7 withdrawn
   by E-3 — it is not evolved); enforce `.unit` forced state on import (§11).
7. **Frontend** — remove password collection from `UnitsPage.svelte` (§3); add the change
   and reset surfaces to `SettingsPage.svelte` (§6, §7.1); render the backend forced-state
   refusal (§5).
8. **Governance snapshots** — regenerate `docs/governance/frontend/baselines/*.snapshot.json`
   (FE-158/FE-160), which the contract change in item 2 requires.

# Verification Expectations

Normative checklist for the implementation phase.

1. Creating a UNIT with only `code` + `name` succeeds and yields a `user` row with role
   `User`, `node_id = code`, and forced state set.
2. `validate_change_password` still rejects `0000`, `weak`, and any 3-class-incomplete
   value on every non-bootstrap path.
3. The bootstrap exemption is reachable from exactly one call site (assert by test).
4. Logging in with `0000` succeeds and yields a session whose forced state blocks every
   command outside the §5 allowlist, with a backend error (not an empty result).
5. The self change rejects: wrong current password, policy-failing new password, and reuse
   of the current password.
6. The self change atomically writes the new hash and clears the forced flag; the new hash
   verifies under `verify_node(new, <code>, hash)` and fails under any other `node_id`.
7. A UNIT admin can reset `user` to a policy-passing temporary credential; `0000` is
   refused; forced state is set; the admin's session survives; the audit row carries the
   admin's identity.
8. A non-admin authenticated caller cannot reset `user`.
9. After initialization completes, re-importing an `admin_access` package updates the
   administrator's password material only; the local admin's lifecycle state is unchanged
   (not enabled, disabled, deleted, or restored), the first-import exemption stays closed,
   and the exemption predicates do not report "bootstrap available" (amended by Erratum E-5 —
   the package carries no enable/disable control).
10. A soft-disabled local admin row (`deleted = 1`, seeded directly — after E-5 no supported
    `admin_access` operation may write that field) leaves initialization complete on restart;
    the initialization latch remains deletion-insensitive.
11. Changing `code` through any update path is refused.
12. A `.unit` import on a configured node forces a password change, and any serialized
    `must_change_password` value in an artifact does not change the resulting local state.
13. `audit_log` has no plaintext password, hash, or pre-hash in any column or in `details`.
14. `bun run check:arch` reports zero warnings; `cargo clippy -D warnings`, `cargo test`,
    `vitest`, and the release-integrity and documentation-governance scripts pass.
15. E-4 — On a fresh UNIT (post-`.unit`), the mandatory ordered sequence is: (1) UNIT key
    import/install, (2) UNIT identity/signature import/install, (3) successful `admin_access`
    import establishing the local administrator (§8.1 existence test true), (4) successful
    mandatory operator password change. Initialization is complete **only** when all four
    stages have succeeded in that order; no stage is skippable or reorderable.
16. E-4 — The operator MUST NOT succeed at `change_own_password` before stage (3) succeeds:
    while no local admin exists, the incomplete-initialization gate refuses the change with a
    backend error (not an empty result), even though the forced credential state is active.
    A successful `admin_access` import establishes the local admin before password change is
    permitted.
17. E-4 — The gate is implemented at the existing command-dispatch boundary
    (`commands/registry.rs::get_invoke_handler`) and keys on the repository-backed local-admin
    existence predicate `admin_exists_for_node` for the local UNIT code — **never** on
    `admin_updated` (which is `true` on every successful import and is not a validity signal)
    and never on a new persistent flag or schema field.
18. E-4 — While no local admin exists, a **session-bearing** invocation is admitted only
    against the §5 list **minus** `change_own_password`: `login`, `logout`,
    `get_current_user`, `check_session`, `touch_session`, `import_admin_access_package`,
    `get_settings`, `is_configured` — the only way to leave the state is a successful
    `import_admin_access_package`. `change_own_password` is refused with a backend error
    until the §8.1 latch is true, and every other command invoked from a session is refused
    fail-closed with a backend error. Invocations without a session are NOT bypassed: they
    proceed to the command's own guards, which remain responsible for each command's
    authentication, authorization, cryptographic verification, and state-transition checks.
    In particular, the pre-authentication UNIT bootstrap ceremonies required by stages 1-2 —
    `get_security_status`, `initialize_app_key`, `import_app_key`, `unlock_app_key`,
    `get_identity_status`, `begin_unit_provision`, `finalize_unit_provision`,
    `install_wilaya_certificate` — and the login-screen reads (`get_build_info`,
    `is_configured`) remain reachable on a fresh UNIT with no session, each gated by its own
    checks; the E-4 gate relaxes none of them. (`sign_unit_identity_request` is NOT one of
    these: it is the WILAYA-side post-authentication signing ceremony, WILAYA Admin-only.)
    Once the latch is true, the §5 forced-state allowlist applies unchanged (including
    `change_own_password`).
19. E-4 — A failed `admin_access` import (signature verification, authorization, authz policy,
    predicate, replay, or validation failure) performs zero mutation (pipeline atomicity) and
    leaves the gate in the incomplete-initialization state; repeated failure does not change
    the allowlist. A repeated successful import after completion does not re-open the gate:
    the §8.1 latch is monotonic and deletion-insensitive (E-5).
20. E-5 — (Slice A target) An `admin_access` re-import against an existing local admin changes
    the administrator password material and `updated_at` bookkeeping only; it does not read,
    write, or preserve-check `deleted`, and does not enable, disable, delete, or restore the
    account.
21. E-5 — The exporter continues to fail closed on a missing password hash or a disabled fleet
    admin (unchanged); the exported payload carries `admin_password_hash` only; `admin_enabled`
    is neither serialized nor deserialized and pre-change payloads fail closed on
    `deny_unknown_fields` (no backward compatibility — D18).
22. E-5 — On a WILAYA node, the WILAYA-scope `set_account_status` disable of the fleet admin
    continues to work locally and continues to fail export closed; there is no package-based
    delivery of an enable/disable state to UNIT nodes.

# Errata

Amendments to this record's **own** text, approved by the owner and entered here rather than
in a second document, so that exactly one canonical source exists (`AGENTS.md` §7 D2, D3).
E-1 and E-2 correct one factual finding and one implementation locus and make one category
table's contents normative; **E-3 withdraws decision D7/§7.2** (the WILAYA-side reset, now
removed). **E-4 formalizes the mandatory ordered UNIT initialization sequence and its dispatch
gate; E-5 retires the package-based UNIT admin disable/re-enable channel.** No other decision
is withdrawn.

| ID | Date | Locus | What was wrong | Correction |
|----|------|-------|----------------|------------|
| E-1 | 2026-10-06 | Context F33; §5 enforcement bullet; Implementation Boundary item 5 | F33 states that "All 181 `#[tauri::command]` functions … reach `authorize_command`", and §5 derived its enforcement locus from that claim. The claim is **false**: a non-empty set of registered commands (session/projection utilities, frontend bootstrap reads, report/calculation readers, pre-auth ceremony commands) never calls it. | The §5 forced-state enforcement is implemented at the **dispatch** choke point `src-tauri/src/commands/registry.rs::get_invoke_handler`, which runs before every registered command body. `authorize_command` (`commands/guards.rs:110`) is unchanged and remains the single authorization entry point; the forced-state check is additive and sits earlier in the dispatch path. |
| E-2 | 2026-10-06 | §5 allowlist table | The table enumerated **categories** of allowed operations only, which left the implemented set unverifiable against the record. | The closed allowlist is now normative and enumerated by name: `login`, `logout`, `get_current_user`, `check_session`, `touch_session`, `change_own_password`, `import_admin_access_package`, `get_settings`, `is_configured` (`commands/guards.rs::FORCED_STATE_ALLOWED_COMMANDS`). |
| E-3 | 2026-10-09 | Decision D7; §7 heading and introduction; §7.2; Implementation Boundary item 6; F25/F26 | D7/§7.2 retained the WILAYA-side `set_unit_user_password` reset and asserted that setting the forced credential on the WILAYA-side operator row makes "a subsequently exported/replaced `.unit` credential inherit the forced lifecycle". That premise is **false**: the `.unit` package carries **no** authoritative `must_change_password`, a serialized value is inert (§11), and UNIT import enforces the local forced state independently. A WILAYA-side reset therefore cannot transmit a forced-credential state through export/import, and under D1 the WILAYA does not supply the operator's operational password. | D7 and §7.2 are **withdrawn**; the WILAYA-side `set_unit_user_password` command is **removed** (backend command + registry entry + `sync.contract.ts` export), leaving no WILAYA-side operator-password reset path. The **only** supported operator-password reset is the UNIT-local authenticated **admin** reset in §7.1 (D6). Before a local UNIT admin exists, the documented recovery limitation (§12) is unchanged; no recovery path is invented. |
| E-4 | 2026-10-10 | §8/§10 (D4/D5/D10); §5 allowlist (`commands/guards.rs::FORCED_STATE_ALLOWED_COMMANDS`); Verification Expectations | The record stated D4's mandate that "the UNIT MUST complete the `admin_access` initialization lifecycle" and that initialization completion is the §8.1 existence latch, but it did not constrain **ordering**: nothing forbade the operator from completing the mandatory password change *before* a local admin existed, so a node could clear the forced state and reach normal operational commands with no local admin while `change_own_password` remained on the §5 allowlist. | The UNIT initialization sequence is **mandatory and ordered**: (1) UNIT key import/install → (2) UNIT identity/signature import/install → (3) successful `admin_access` import establishing the local administrator → (4) successful mandatory operator password change — and only then is initialization complete. No stage may be skipped or reordered. The operator MUST NOT succeed at `change_own_password` before stage (3) succeeds. The intended implementation gate sits at the existing command-dispatch boundary (`commands/registry.rs::get_invoke_handler`) and keys on the repository-backed §8.1 existence predicate `admin_exists_for_node` for the local UNIT code — **never** `admin_updated` (always `true` after any successful import), never a new persistent flag or schema field. While no local admin exists, the admitted allowlist for **session-bearing** invocations is the §5 list minus `change_own_password` — `login`, `logout`, `get_current_user`, `check_session`, `touch_session`, `import_admin_access_package`, `get_settings`, `is_configured`; `change_own_password` cannot complete the mandatory password-change stage before a successful `admin_access` import establishes the local administrator, and every other command invoked from a session is refused fail-closed. Invocations without a session are NOT bypassed: they proceed to the command's own guards (authentication, authorization, cryptographic verification, and state-transition checks), which keeps the pre-authentication UNIT bootstrap ceremonies of stages 1-2 reachable on a fresh UNIT while relaxing none of their checks. Acceptance criteria are appended to `# Verification Expectations` (items 15-19). This record specifies the intended seam and behavior only; documentation alone does not implement or enforce the gate. Implementation remains a separately scoped slice (Slice B), not part of payload-field removal. |
| E-5 | 2026-10-10 | Decision D10; F12; F17; F18; §8.2 state table ("later admin disable / revocation"); §10 table; Verification Expectations item 9; ADR-0051 §4 | The record (following ADR-0051) treated `admin_enabled = false` as a package-deliverable UNIT-admin disable and `admin_enabled = true` as a re-enable — a complete package-driven enable/disable lifecycle. That channel is **latent by construction**: `export_admin_access` hard-codes `admin_enabled: true` and fails closed for a missing or disabled fleet admin (`user_account_sync_service.rs:154-175`), so no production artifact can carry `false`; no supported import path may change the local admin's lifecycle state. | `admin_enabled` is **removed** from `AdminAccessPayload` (ADR-0051 E-1). `admin_access` carries no UNIT-local enable/disable control: a successful import creates the local administrator when absent and, on re-import against an existing UNIT-local administrator, may update **only** the row's `password_hash` and `updated_at` — it must not update or depend on the existing value of `id`, `created_at`, `node_id`, `role`, `deleted`, or `must_change_password` (INSERT initializes a new canonical admin with `deleted = 0`, via an explicit value or the verified database default); it does not enable, disable, delete, or restore an existing account. The shared `deleted` column/model field is retained for other user-account and legacy `identity_access` behavior, and the legacy shared-upsert behavior is unchanged, but the supported UNIT `admin_access` import path must not write it. Text describing `admin_enabled = false`, package-driven disable/re-enable, and false→true transitions is corrected herein (D10, F17, F18, §8.2, §10, VE item 9; acceptance criteria appended as VE items 20-22). Unrelated local/WILAYA authorization and security guarantees are unchanged; exporter fail-closed conditions for a missing/disabled fleet admin or empty password hash are preserved; no envelope change and no version bump. **Implementation status:** Slice A (the payload-field removal and the deleted-free canonical upsert above) is implemented in commit `7259e2e` (2026-10-11) and owner-accepted per the `GOVERNANCE_APPROVALS.md` 2026-10-11 entry; the official `admin_access` importer now applies the payload through the deleted-free `upsert_canonical_admin`, whose conflict path updates only `password_hash` and `updated_at` (a newly inserted row inherits the schema default `deleted = 0`), while the earlier shared `upsert_synced_admin` conflict path (which additionally wrote `role` and `deleted`) remains only for the legacy `identity_access` apply path. |

Provenance: E-1/E-2 recorded with the §5 + §6 implementation (Slice 2); E-3 recorded with the
removal of the WILAYA-side `set_unit_user_password` command. Approval and scope are recorded in
`docs/governance/frontend/GOVERNANCE_APPROVALS.md` (2026-10-06 and 2026-10-09 entries).
E-4 and E-5 are governance/documentation records (2026-10-10; approval and scope in
`docs/governance/frontend/GOVERNANCE_APPROVALS.md` 2026-10-10 entry); the implementation
slice A (payload-field removal) is implemented and owner-accepted (2026-10-11, commit
`7259e2e`; approval in the `GOVERNANCE_APPROVALS.md` 2026-10-11 entry), while slice B
(initialization-order gate) remains separately scoped and unapproved.
