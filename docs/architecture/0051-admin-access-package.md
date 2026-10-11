# ADR 0051: Admin-Only B8 Account Synchronization (`admin_access`)

# Decision Status

**Status: Accepted**
**Ratification Date: 2026-08-22**

**ACCEPTED — 2026-08-22 (Owner Ratification — Governance Gate).**

This ADR converts the ratified Admin-Only B8 architecture (SEC-019 investigation; SEC-020 RFC/ADR & freeze-gate analysis) into repository-level accepted governance.

| Item | Status |
|------|--------|
| `admin_access` package-kind architecture | **ACCEPTED / OWNER RATIFIED** |
| Decision D1 — legacy `identity_access` cutover | **RATIFIED** |
| ARCHITECTURE_FREEZE §2.2 amendment (parallel bootstrap exemption) | **ACCEPTED** |
| ARCHITECTURE_FREEZE §2.7 amendment (WILAYA-only security-critical kind scope) | **ACCEPTED** |
| Implementation of any of the above | **NOT AUTHORIZED by this document alone** (separate implementation gate: SEC-021) |

Acceptance is architectural/governance only. This artifact was created documentation-only; no source, tests, migrations, contracts, or configuration were modified by this decision.

---

## 1. Context / Problem

ADR-0040 certified a dual-purpose `identity_access` B8 package that synchronizes both the fleet-wide `admin` account and the UNIT `user` account. Its apply path (`UserAccountSyncService::apply`) canonically renames the UNIT-local operator account to `user` and replaces its password hash via `upsert_synced_user (ON CONFLICT(username))` with the WILAYA-issued node-bound credential.

Field behavior demonstrated that operators lose their provisioning credentials on every import (SEC-018 root cause): the pre-import username ceases to exist, and the effective password becomes whatever WILAYA currently stores for that unit. This behavior is certified but rejected by the owner:

* `.unit` provisioning owns the UNIT operator account;
* importing an **Admin synchronization** package must not modify that account.

The owner's requirement therefore invalidates the UNIT-user half of the dual-purpose payload while leaving the fleet-admin half intact.

## 2. Owner Requirement (normative)

> The UNIT operator account created during `.unit` provisioning SHALL remain exactly as provisioned. Importing the fleet Admin synchronization package SHALL synchronize only the canonical `admin` account and SHALL NOT rename, replace, disable, delete, or otherwise modify the UNIT operator account.

## 3. Decision

Adopt a new SECURITY-CRITICAL package kind:

```text
admin_access
```

Semantics:

```text
direction : WILAYA → all UNIT nodes
purpose   : fleet-wide synchronization of the canonical `admin` account ONLY
target    : none — the ABSENCE of a target is the broadcast semantic
```

It is NOT a targeted UNIT package. No sentinel target values (`"*"`, `"ALL"`, null markers) are permitted. The same signed artifact is independently importable by every authorized UNIT.

## 4. Payload (normative)

> **Erratum E-1 (2026-10-10, owner-approved):** `admin_enabled` is **removed** from the
> normative payload below — the payload is the fleet-wide administrator password hash **only**
> (`{admin_password_hash}`), exactly as the exporter has always emitted it. See `# Errata` at
> the end of this document.

```text
AdminAccessPayload {
    admin_password_hash
}
```

> **Implementation status (Slice A — required, not yet performed):** the current official
> `admin_access` importer still applies the payload through the shared `upsert_synced_admin`,
> whose conflict path writes `password_hash`, `role`, `deleted`, and `updated_at`. The
> deleted-free write set stated below is the **required Slice A target**, not current runtime
> behavior.

`admin_access` carries **no** UNIT-local enable/disable control: a successful import creates
the local administrator when absent and, on re-import against an existing UNIT-local
administrator, may update **only** the row's `password_hash` and `updated_at`; it MUST NOT
update or depend on the existing value of `id`, `created_at`, `node_id`, `role`, `deleted`,
or `must_change_password`, must not enable, disable, delete, or otherwise change the
lifecycle state of an existing UNIT-local administrator account, and never relies on reading
or preserving an existing `deleted` value. Pre-change payloads carrying `admin_enabled` are
not compatibility requirements (the application has not been officially deployed — ADR-0063
D18); strict unknown-field rejection (`deny_unknown_fields`) makes them fail deserialization,
fail-closed (Erratum E-1).

The payload MUST NOT contain any of:

* `unit_code` (or any target/reference to a specific UNIT);
* UNIT-user username;
* UNIT-user password;
* UNIT-user password hash;
* UNIT-operator identity or operator-selected username;
* any other UNIT-local operator-account material.

The synchronized username is structurally canonical (`admin`) — it is hard-coded by the repository upsert and is never transported in the payload. Implementation MUST enforce strict deserialization (`deny_unknown_fields`) plus a negative-deserialization test so that an `admin_access` payload is structurally incapable of representing or carrying a UNIT-user credential.

## 5. Account Ownership Invariant (normative)

```text
.unit        owns the UNIT-local operator account.
admin_access owns ONLY the canonical `admin` account.
```

After `.unit` provisioning, the operator-selected username remains authoritative and the provisioning-established node-bound password remains authoritative. After `admin_access` import, the canonical local `admin` account is created when absent; on re-import only its `password_hash` and `updated_at` may change. `admin_access` never enables, disables, deletes, or restores the account (Erratum E-1); the UNIT operator account MUST remain unchanged.

Explicitly normative:

* `admin_access` MUST NOT rename the UNIT operator to `user`.
* `admin_access` MUST NOT replace the UNIT operator password (hash or domain).
* `admin_access` MUST NOT modify the UNIT operator row in any way — id, username, role, `node_id`, password hash, enabled/deleted state.

The invariant is byte-for-byte and holds on BOTH acceptance and rejection branches of every import attempt. It is enforced structurally at the implementation boundary: the `admin_access` application path reaches only the Admin-only repository operation (`upsert_synced_admin`) and contains no dependency on UNIT-user synchronization code (`upsert_synced_user`, `update_username`, unit-user password mutation). (That repository operation is the **current** implementation; the required Slice A change replaces it with a dedicated deleted-free `admin` upsert that preserves the same operator-row isolation.)

## 6. Security Model

`admin_access` reuses the certified V2 pipeline without modification:

* V2 package envelope (`signature_version = 2`);
* Ed25519 signature bound to an ACTIVE WILAYA issuer identity;
* issuer == installed anchor verification;
* canonical integrity hash;
* encrypted `.sync` transport via the existing age provider;
* SECURITY_CRITICAL kind registration (V2-only enforcement inherited automatically);
* fail-closed validation at every gate;
* replay protection via each node's local imported-package registry;
* strict sequence continuity via the existing TransportGuard;
* atomic single-transaction import.

No new cryptographic primitive. No new trust primitive. No parallel weaker verification path exists for this kind.

## 7. Sequence Model — Dedicated Issuer-Only Stream

Producer side: a dedicated additive migration creates an issuer-scoped producer stream (conceptually `admin_access_export_sequence(issuer_identity_id PRIMARY KEY, last_issued_sequence, updated_at)`) with the advance-on-success allocation contract identical to migrations 006/009, executed under the single-writer SQLite mutex.

Consumer side: existing per-node ledger and registry machinery unchanged — replay/ordering state is strictly local per UNIT.

Broadcast property: WILAYA emits sequence N once; UNIT-A accepts N and UNIT-B independently accepts N against their own local ledgers, without cross-UNIT collision or invalidation. Rotation numbering for `admin_access` is deliberately decoupled from `products`, `trust`, `registry`, `data-report`, and legacy `identity_access` streams.

> **Supersession (2026-08-24 — ADR-0053):** the dedicated issuer-only
> `admin_access` producer stream above is **superseded** by ADR-0053:
> `admin_access` joins the **unified per-target transport stream**
> `(issuer_identity_id, target_node_id)` with `target_node_id = units.code`
> (authoritative, validated), and the migration-010 producer table is retired.
> The historical rationale recorded here (broadcast simplicity) is preserved;
> SEC-030 empirically demonstrated that independent per-kind counters cannot
> satisfy the frozen kind-blind consumer continuity once kinds interleave, so
> the broadcast emission model becomes explicit per-target delivery. All other
> clauses of this ADR (payload, ownership invariant, predicates, D1 cutover,
> authz, V2 signing) remain unchanged and in force.

## 8. Authorization

EXPORT (WILAYA):

```text
WilayaNode + AdminOnly session + required unlocked state.
No unit_code parameter. No UNIT selection anywhere in the export path.
```

IMPORT (UNIT), post-bootstrap:

```text
UnitNode + AdminOnly session.
```

Denied at all times: unauthenticated callers; UNIT `User` sessions after bootstrap exists.

FIRST IMPORT — narrow fail-closed bootstrap exemption (parallel to ADR-0045's shape, governed solely by this ADR). Admitted only when ALL of the following hold, evaluated inside the protected transaction boundary:

```text
anchor_installed   — an ACTIVE WILAYA certificate is installed locally
anchor_is_issuer   — the package issuer equals the installed anchor identity
no_active_admin    — the ADR-0063 §8.1 local-admin EXISTENCE test: no `users` row with
                     `username = 'admin'` AND `node_id` = the local UNIT code, evaluated with
                     NO `deleted` filter (Erratum E-1 — the latch is deletion-insensitive)
```

`unit_code_matches` is deliberately absent: the package has no target binding to check. Its protective role is replaced — not weakened — by issuer pinning over the V2 signature chain. This is not an authentication bypass: authentication remains carried entirely by the certificate↔signature chain. The exemption self-terminates: once the §8.1 existence test holds (a local UNIT admin row exists), imports revert permanently to AdminOnly (Erratum E-1). A predicate service parallel to the existing one evaluates these checks; the certified `identity_access` predicate service remains untouched.

## 9. Decision D1 (ratified)

At `admin_access` cutover:

1. legacy `identity_access` **issuance = disabled**;
2. legacy `identity_access` **import = fail closed / rejected**, at the package-kind/policy boundary, BEFORE any account mutation;
3. rejection is atomic — every rejection path performs **zero writes**;
4. the legacy kind is **not silently reinterpreted** as `admin_access`: no partial application, no "accept and ignore its User portion", no aliasing.

Legacy `identity_access` remains a historical/legacy kind only. It is NOT an alternative Admin-only mechanism and MUST NOT acquire new semantics. Already-issued artifacts are not reinterpreted: at cutover they are rejected fail-closed. Recovery of already-renamed UNIT nodes is explicitly out of scope of this ADR and requires its own separate remediation decision.

## 10. Backward Compatibility

Existing already-issued `identity_access` packages remain documented historical formats (ADR-0040 stands as their record). At D1 cutover they are rejected at the import boundary with an explicit failure message and zero mutation. `admin_access` is the new authoritative fleet-Admin synchronization mechanism from cutover onward.

## 11. Rejected Alternatives

* **Reuse `identity_access` with Admin-only semantics** — redefines frozen payload semantics mid-stream, strands already-issued artifacts with contradictory meanings, breaks backward compatibility, and leaves the largest silent governance drift.
* **Generic broadcast credential package** — over-generalizes representational capacity and invites future user-credential material back into a broadcast channel.
* **Transport outside the certified package pipeline** — bypasses V2 verification, transport guarding, and replay protection; introduces a new trust path.

## 12. Consequences

Positive: the owner invariant becomes structurally guaranteed rather than conventionally avoided; package semantics are self-describing; exposure shrinks (single Argon2 hash instead of two); UNIT authentication code is untouched; blast radius is additive.

Negative: dual-kind documentation until cleanup; legacy apply code persists behind a closed boundary pending physical removal; UI copy changes required; freeze amendments were prerequisites of this acceptance (completed 2026-08-22).

## 13. Test Requirements (implementation gate)

Implementation authorization (SEC-021) carries a mandated matrix derived from SEC-020 §18, including at minimum: operator-row byte-snapshot preservation on accepted AND rejected branches; admin create/update/rotation lifecycle; fleet-wide multi-node import of one artifact; whole-users-table isolation snapshot; negative deserialization; signature/issuer/replay/tamper/kind-confusion/authz negatives; D1 rejection tests proving zero mutation; new predicate suite; policy allow/deny matrix.

## 14. Freeze References

Governed by the ARCHITECTURE_FREEZE amendments dated 2026-08-22:

* §2.2 — parallel narrow bootstrap exemption for `admin_access`;
* §2.7 — `admin_access` added to the WILAYA-only security-critical kind scope.

Related: ADR-0040 (historical dual-purpose semantics; superseded in part by §5/§9 here), ADR-0044/0045 (bootstrap-exemption precedent), ADR-0046 (kind-scoping model), ADR-0047 (V2-only), ADR-0050 (normal-login model — untouched).

# Errata

Amendments to this record's **own** text, approved by the owner and entered here rather than
in a second document, so that exactly one canonical source exists (`AGENTS.md` §7 D2, D3).

| ID | Date | Locus | What was wrong | Correction |
|----|------|-------|----------------|------------|
| E-1 | 2026-10-10 | §4 (normative payload); §8 first-import predicates | §4 specified `AdminAccessPayload { admin_password_hash, admin_enabled }`, implying `admin_access` carries a UNIT-local enable/disable control, and §8's `no_active_admin` ("zero active Admin accounts") implied the bootstrap exemption depends on *enabled* admins. Neither matches the authorized design: the exporter never emits `admin_enabled = false` (`user_account_sync_service.rs::export_admin_access` fails closed on a missing or disabled fleet admin), no supported import path may change the local admin's lifecycle state, and ADR-0063 §8.1 defines the latch as an **existence** test (`admin_exists_for_node`, no `deleted` filter) that a soft-disabled local admin still satisfies (ADR-0063 D5). | `admin_enabled` is **removed** from the normative payload; the payload is exactly `{admin_password_hash}`. Strict unknown-field rejection (`deny_unknown_fields`) is retained and remains mandatory. Pre-change payloads are **not** compatibility requirements: the application has not been officially deployed (ADR-0063 D18) and no producer release emits `admin_enabled = false`; a pre-change artifact carrying `admin_enabled` fails deserialization fail-closed. `admin_access` carries **no** UNIT-local enable/disable control: import creates the local administrator when absent and, on re-import, may update only the row's `password_hash` and `updated_at` — it must not update or depend on the existing value of `id`, `created_at`, `node_id`, `role`, `deleted`, or `must_change_password`, and never enables, disables, deletes, or restores an account. The shared `deleted` column/model field remains for other user-account and legacy behavior, but the supported UNIT import path must not write it (**Slice A target**, not current behavior — the current importer still writes `deleted` via the shared upsert; see ADR-0063 E-5). §8 `no_active_admin` is reconciled to the ADR-0063 §8.1 existence test. §13 test-matrix and §14 freeze-reference requirements are unchanged; no envelope change and no version bump. |
