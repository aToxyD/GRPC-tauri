# ADR 0040: Identity & Access Synchronization — Two Static Accounts per UNIT (B8)

# Status
Draft (2026-08-07)

# Date
2026-08-07

# Owner
Architecture / Security

# Reference
- RFC `docs/architecture/rfcs/2026-08-04-node-identity-trust.md` §5 (B8 row).
- ADR-0038 (Node Identity & Trust) — B8 is **separate** from Node Identity: it
  synchronizes **application accounts only** (`users` rows), never identity
  certificates or node keys.
- ADR-0039 §2 (two-tier secret protection) — password hashes are node-bound
  application credentials, never age-encrypted node secrets.
- ADR-0002 / ADR-0010 (sync package boundary / package-only transport) — B8 rides
  the existing V2 (Ed25519) pipeline with a new `identity_access` package kind.
- `scripts/check_arch.ts` Rule 127 (ManageUnits node-type guard) — closed by B8 ①.
- ARCHITECTURE_FREEZE.md §2.7 — unchanged by this decision (no DDL, no new tables).

# Context

After B5–B7, the WILAYA node authenticates its ADMIN through the identity trust
chain (`.adminkey` + Challenge–Response), while UNIT nodes continue to use
**Application User Authentication** (username + password) for their local users.
Unit users today are created through the `.unit` bootstrap package (`UserExport {
username, password_hash, role }`) or `create_user` — a local, unit-owned lifecycle
that cannot be governed centrally.

B8 introduces central account governance: the Wilaya is the **single source of
truth** for exactly two accounts per UNIT node (`admin` + `user`), distributed
one-way Wilaya→UNIT through the existing V2 (Ed25519) sync pipeline as a new
`identity_access` package kind.

The administrative password is **fleet-wide** (the same username, password, and
role on every node) so a single emergency credential is uniform across the
fleet; the user password is **per-unit** (each UNIT receives its own `user`
hash).

# Decision

## 1. Scope (normative)

- B8 synchronizes **application accounts only**. Node Identity (B6/B7), identity
  certificates, node keys, and the `.adminkey` operator identity are **out of
  scope** and are never carried in `identity_access` packages.
- The existing V2 (Ed25519) pipeline carries the new `identity_access` package
  kind. No new transport, no new crypto, no new signing scheme.

## 2. Account model (normative)

- Every UNIT node has exactly **two** synchronized accounts:
  - `admin` — role `Admin`, username `admin`, node-bound to the unit
    (`node_id = unit code`), password derived with the fleet-wide admin domain.
  - `user` — role `User`, username `user`, node-bound to the unit, per-unit
    password derived with the node-bound domain.
- `deleted` column (0=active, 1=disabled) is reused as a **soft-delete** toggle.
  Rows are never hard-deleted by the sync path.

## 3. Flow (normative)

1. Wilaya sets the fleet admin password (`set_fleet_admin_password`) and each
   unit's user password (`set_unit_user_password`); an admin/user may be
   disabled (`set_account_status`).
2. Wilaya exports one `identity_access` package per unit
   (`export_identity_access_package(unit_code, path)`), fail-closed if the admin
   is unset or disabled.
3. The unit imports the package (`import_identity_access_package(path)`), which
   applies it transactionally.

## 4. Apply-time reconciliation (normative)

- If a local unit-bound user exists under a different username (e.g. a legacy
  `admin` collision) and no `user` row exists yet, it is **renamed** to `user`;
  its row id is preserved.
- The canonical `admin` (role `Admin`, node_id = unit code) and `user`
  (node_id = unit code) are upserted by username; an existing row keeps its id.
- `enabled=false` maps to `deleted=1`.

# Invariants (normative, user-mandated)

## Invariant 11 — exactly one synced Admin per node

> Each node has exactly one synchronized `Admin` account. Its username SHALL be
> `admin`, its role SHALL be `Admin`, its password SHALL originate from the
> Wilaya, and it MUST NOT be locally modified.

- Applies to UNIT nodes that have applied an `identity_access` package.
- The `admin` row is always bound to its unit (`node_id = unit code`).

## Invariant 12 — exactly one synced User per UNIT

> Each UNIT has exactly one synchronized `User` account. Its username SHALL be
> `user`, its password SHALL be per-unit and originate from the Wilaya, and it
> SHALL be node-bound.

## Invariant 13 — local password changes forbidden

> Local password changes for synchronized accounts MUST fail closed. Any local
> attempt to change a synced account's password outside the sync path is
> rejected.

- Applies to `admin` and `user` rows governed by the Wilaya.
- The Wilaya `set_fleet_admin_password` / `set_unit_user_password` commands are
  the only password-mutation surface.

# Consequences

- UNIT credentials become centrally governable and deterministic (same inputs →
  same accounts), satisfying P1/P2.
- Soft-delete keeps rows traceable (P4); no destructive DDL (B2/Persistence).
- The fleet-wide admin domain is an exception confined to password derivation
  inside `Argon2PasswordHashProvider`; account ownership stays unit-bound.
- Sync is strictly one-way (Wilaya→UNIT); there is no reverse path.

# Out of scope

- RBAC, additional roles, third accounts.
- Node Identity / identity certificates / node keys (B6/B7).
- Reverse sync (UNIT→Wilaya account reporting).
- Local self-service password change for synced accounts.

# Implementation

- B8 ①: port split (`hash_admin`/`verify_admin` + `hash_node`/`verify_node`),
  users repository soft-delete + canonical upserts, `UserAccountSyncService`,
  `identity_access` payload, AuthZ actions, audit events, close Rule 127.
- B8 ②: IPC cutover, frontend contracts, snapshot recertification.
- B8 ③: integration suite, final ADR acceptance, 8-gate pass, `tauri build`.
