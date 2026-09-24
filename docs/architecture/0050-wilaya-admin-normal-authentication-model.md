# ADR 0050: WILAYA Admin Normal Authentication Model — B6-B Reversal (SEC-013)

# Decision Status

**ACCEPTED — 2026-08-19 (architectural ratification — SEC-013 Phase 0)**
**IMPLEMENTED — 2026-08-20 (SEC-013 Phase 1)**

This ADR is an explicit security-policy decision. It reverses, **for normal
WILAYA Admin login only**, the B6-B permanent gate that forced Challenge–
Response when a usable ADMIN identity exists, and records the accepted
residual-risk trade-off. Architectural acceptance was ratified 2026-08-19
(implementation NOT AUTHORIZED at that point). Implementation was authorized
and landed 2026-08-20 (SEC-013 Phase 1): the password gate became an
account-credential predicate (`IdentityAuthenticationPolicy::password_login_allowed`
now takes the username), the login command routes only hash-less identity-only
accounts to Challenge–Response, and the login page keeps the password tab on
every node with `.adminkey` Challenge–Response retained as the recovery path.

| Item | Status |
|------|--------|
| Normal WILAYA Admin login = username + password (B8 fleet-admin credential) | **DECIDED (mandatory target)** |
| Presence of ACTIVE ADMIN identity + `.adminkey` disables password login | **SUPERSEDED** for normal login (B6-B gate reversal) |
| `.adminkey` / Challenge–Response mechanism | **RETAINED** — recovery / high-assurance / future step-up only |
| Step-up authentication implementation | **NOT AUTHORIZED** in this phase (documented as available future mechanism) |
| Authorization layer (role / ResourceContext / node scoping) | **UNCHANGED — authoritative** |
| WILAYA Admin → UNIT login | **UNCHANGED** — B8 local-account model (no session transfer) |
| ADR-0040 (B8) | **UNCHANGED** — remains the fleet-admin credential authority |
| ADR-0041 (App-Key) | **UNCHANGED by this ADR** (amended separately, §11) |
| Migration (DB / package / re-provisioning) | **NOT REQUIRED** |

# Date

2026-08-19

# Owner

Architecture / Security

# Reference

- ADR-0038 (`docs/architecture/0038-node-identity-and-trust.md`) — B6-A/B6-B
  authentication cutover; **the B6-B permanent password gate for provisioned
  WILAYA normal login is superseded by this decision** (historical text preserved).
- RFC `docs/architecture/rfcs/2026-08-04-node-identity-trust.md` §3.12 (B6-A/B6-B
  closure) — historical record; the B6-B normal-login clause is superseded here.
- ADR-0040 (`docs/architecture/0040-identity-access-sync.md`) — B8 fleet-wide
  admin credential; authoritative for the credential model (unchanged).
- ADR-0041 (`docs/architecture/0041-production-app-key-provisioning.md`) —
  App-Key lifecycle (unchanged by this ADR).
- SEC-012 / SEC-013 reports — evidence base for this decision.

# 1. Decision

For a provisioned WILAYA node:

**NORMAL ADMIN LOGIN is `username + password`.**

- The presence of an ACTIVE ADMIN identity and `.adminkey` **MUST NOT by itself
  disable the normal password login path**.
- The B8 fleet-admin password is a **valid normal authentication credential** for
  the local WILAYA `admin` account, exactly as it is for the local UNIT `admin`
  account.
- The decision applies to **normal post-provisioning login only**. Recovery and
  emergency flows are outside this decision and may legitimately require stronger
  secrets.

## 1.1 Exact policy superseded

The B6-B clause superseded by this decision is:

> `password_login_allowed = !has_active_admin_identity` — the password path
> remains open only while no ACTIVE ADMIN identity is usable; WILAYA nodes with
> a usable ADMIN identity route exclusively to Challenge–Response
> (ADR-0038 Commit ③; `IdentityAuthenticationPolicy::password_login_allowed`,
> `src-tauri/src/application/services/identity_authentication_policy.rs:106-111`).

Superseded **for normal WILAYA Admin login**. The predicate's other effects —
UNIT nodes keep the password path, unprovisioned nodes keep it, identity-only
accounts (empty password hash) remain Challenge–Response-only — are unchanged.

# 2. `.adminkey` role

- The `.adminkey` / Challenge–Response mechanism is **NOT deleted**.
- It remains available for:
  - recovery;
  - high-assurance authentication;
  - administrative recovery flows;
  - any explicitly protected operation that later requires possession-based
    authentication.
- **No step-up authentication implementation is introduced in this phase.**
  Step-up is documented as a future/available security mechanism: a later
  decision may attach Challenge–Response to explicitly protected operations.

# 3. Critical security boundary

**Authentication method ≠ authorization authority.**

Changing normal WILAYA Admin login from Challenge–Response to username+password
MUST NOT change:

- `Principal.role` semantics;
- `ResourceContext` semantics;
- `WilayaNode` authorization;
- `UnitNode` authorization;
- `AdminOnly` policy;
- `ManageUnits` authorization;
- `ManageAccountSync` authorization;
- `SignUnitIdentityRequest` authorization;
- trust-package authorization;
- session portability rules.

The authorization layer remains authoritative: decisions are role-only or
node-context-only (`src-tauri/src/application/authz/policies/mod.rs:38-165`,
`policies/system.rs:16-24`, `policies/reports.rs:39-148`) and never depend on
the authentication method used to establish the session.

# 4. WILAYA → UNIT behavior

WILAYA Admin → UNIT login is **NOT session transfer**. The intended flow:

```
WILAYA fleet admin password
        ↓
UNIT local users table (B8-synchronized `admin` row)
        ↓
local password verification (verify_admin, fleet-wide domain)
        ↓
local UNIT CurrentSession
        ↓
role = Admin
```

- The WILAYA session, Principal, authority, App-Key, or runtime state are
  **never transported to UNIT**.
- The UNIT Admin session is **independently established by the UNIT node**.
- Sessions remain non-portable (`domain/session.rs` — not serializable).

# 5. UNIT authorization boundary

Preserve exactly:

- UNIT Admin **MAY** perform AdminOnly operations allowed to the Admin role.
- UNIT Admin **MUST NOT** obtain WilayaNode authority, and **MUST NOT**:
  - `ManageUnits`;
  - `ManageAccountSync` / `ExportIdentityAccessPackage`;
  - `SignUnitIdentityRequest`;
  - `CloseFiscalYearAuthority`;
  - WILAYA-only reporting/authority operations (`ReadWilayaReports`,
    `ImportTrustPackage`, `ImportRegistryPackage`).

This distinction derives from **LOCAL NODE CONTEXT** (`ResourceContext` built
from local settings, `commands/guards.rs:115,127-164`), **not** from the origin
of the password.

# 6. B8 relationship

- B8 remains the authority for the fleet-wide Admin credential (ADR-0040).
- ADR-0040 is **NOT modified** by this ADR.
- Unchanged: password synchronization, `identity_access` package format,
  export/import behavior, password hashing (`GLOBAL_ADMIN_DOMAIN`), password
  propagation, WILAYA-only password management.

# 7. Security rationale

B6-B originally enforced possession of `.adminkey` for provisioned WILAYA Admin
login (high-assurance, possession-based). This decision separates:

- **normal authentication** — the B8-managed fleet Admin password
  (username + password), and
- **high-assurance / recovery authentication** — `.adminkey` Challenge–Response.

This provides authentication-model parity between WILAYA and UNIT while
preserving the stronger credential for recovery/high-assurance use.

**This decision is NOT claimed to be "more secure" than B6-B.** Accurately:

- B6-B provided stronger possession assurance for normal ADMIN login.
- The new model intentionally trades that property for the required
  username+password normal-login model.
- The residual risk is accepted explicitly by this ADR (§8).
- Argon2 password hashing, mandatory strong-password validation, and
  rate limiting remain existing controls.
- Authorization boundaries remain unchanged (§3).

# 8. Explicit residual risk

A local attacker capable of obtaining or modifying the WILAYA users database
has a **materially different authentication path** under this decision: the
stored `admin` password hash becomes a usable (offline-attackable / replaceable)
gate to a normal WILAYA Admin session, where B6-B previously required possession
of `.adminkey` material.

This is an **intentional policy decision**, not minimized or hidden. Mitigations
that remain: Argon2 (memory-hard, per-hash random salt), strong-password
validation, rate limiting, audit logging, and OS-level file access control. The
equivalent risk class already exists on UNIT nodes today.

# 9. Migration

- Existing `.adminkey` remains (signing + recovery).
- No re-provisioning required.
- No database migration required.
- No package migration required.
- Existing UNIT behavior remains unchanged.
- Existing B8 synchronization remains unchanged.
- Operational runbooks (e.g., `docs/runbooks/admin-bootstrap.md`) remain
  accurate until the implementation phase, at which point they are updated.

# 10. Governance cross-references

- ADR-0038: B6-B normal-login clause **superseded** by this ADR (status note
  added; historical text preserved).
- RFC `2026-08-04-node-identity-trust` §3.12: B6-B closure clause — normal-login
  portion superseded by this ADR; RFC historical text preserved.
- ADR-0040: **No change** — cross-reference only (§6).

# 11. Scope exclusions

- No step-up implementation.
- No `.adminkey` removal or re-issuance changes.
- No session portability.
- No App-Key changes (addressed by the ADR-0041 amendment, SEC-013 Phase 0).
- No authorization predicate changes.