# ADR 0052: Canonical UNIT Operator Identity & Node-Scoped Account Uniqueness

# Decision Status

**Status: Accepted**
**Ratification Date: 2026-08-23**

**ACCEPTED — 2026-08-23 (Owner Ratification — Governance Gate).**

This ADR converts the ratified SEC-025 investigation verdict (**GO — BASELINE SCHEMA CHANGE**) into repository-level accepted governance and records the completed SEC-026 implementation.

| Item | Status |
|------|--------|
| Canonical UNIT operator `username = "user"` (server-derived) | **ACCEPTED / OWNER RATIFIED** |
| Node-scoped account uniqueness `UNIQUE(username, node_id)` | **ACCEPTED** |
| Baseline schema correction in `001_initial.sql` (no migration) | **ACCEPTED** |
| Removal of all operator-rename capability (`user_renamed`, `update_username`) | **ACCEPTED** |
| Role-aware login identity selection (WILAYA pinned / UNIT fixed set) | **ACCEPTED** |

---

## 1. Context / Problem

Before this decision, the `users` table enforced global username uniqueness (`UNIQUE(username)`). Every UNIT node's operator therefore competed for one namespace with every other node's operator and with the fleet admin. Consequences:

1. **Multi-unit collision class (SEC-023/024/025):** two units could not both provision an operator named `user`; provisioning required caller-supplied usernames, creating a phishing/confusion surface and a per-node naming policy problem.
2. **Legacy rename machinery:** ADR-0040-era `identity_access` apply logic renamed pre-existing operator rows to canonical names to satisfy the global constraint; ADR-0051 already closed the *synchronization* path (D1), but the repository layer retained `update_username` and rename-before-upsert remnants, keeping a mutation path alive that no certified flow may exercise.
3. **Login identity ambiguity:** the login page accepted free-text usernames on all node classes, inviting credential-stuffing against non-existent accounts and contradicting the fixed identity model the backend actually enforces.

The application is pre-release and all databases are development-local; the owner elected baseline schema correction over a migration chain.

## 2. Owner Requirement (normative)

> The UNIT operator SHALL be canonically named `user` on every unit node. Username uniqueness SHALL be scoped per node (`UNIQUE(username, node_id)`). No flow SHALL rename an operator account after creation. The login identity SHALL be selected from a fixed per-node-class set — never typed freely.

## 3. Decision

### D1 — Canonical operator name

`UnitService` derives the operator username server-side from the constant:

```text
OPERATOR_USERNAME = "user"
```

`CreateUnitRequest` carries `{code, name, password}` only. The frontend never transmits a username for unit creation or edit. The edit path rotates passwords only; it cannot change code-derived bindings or usernames.

### D2 — Node-scoped uniqueness

Baseline schema (`db/migrations/001_initial.sql`, users table):

```sql
username TEXT NOT NULL,
node_id  TEXT NOT NULL DEFAULT 'WILAYA',
CONSTRAINT users_username_node_unique UNIQUE (username, node_id)
```

Row model: each unit node holds `(user, <unit_code>, 'User')`; the fleet admin lives at `('admin', <local_scope>, 'Admin')`. All lookups are scope-bounded (`WHERE username = ? AND COALESCE(node_id,'WILAYA') = ?`); unscoped variants are deleted. All conflict targets are composite — `INSERT OR REPLACE` semantics are banned for accounts (explicit `ON CONFLICT(username, node_id) DO UPDATE`, row `id` never rewritten so child references such as `units.user_id` remain valid).

### D3 — Rename capability removed

Removed through every layer: repository (`update_username`, bare-key upserts), application (`ApplyIdentityAccessOutcome.user_renamed`, legacy rename-before-upsert hack), DTO/IPC (`IdentityAccessPackageImportResult.user_renamed`), contract surface (`src/lib/types.ts`), UI. The `.unit` package import reuses the existing `(username, node_id)` row identity on re-import instead of minting new ids.

### D4 — Role-aware login

* WILAYA nodes render a read-only identity field pinned to `admin`.
* UNIT nodes render a static selector with exactly `user` (default) and `admin`.
* No discovery IPC exists; unknown identities fail closed at authentication. The B8 provisioning ceremony gate (SEC-002-B) continues to pin `admin` via `BOOTSTRAP_ADMIN_USERNAME`.

## 4. Invariants Established

1. **I-A (canonical operator):** every provisioned unit has exactly one operator row named `user`, bound to its own node scope.
2. **I-B (scope isolation):** `set_account_status`, password rotation, lookups, and sync application can never cross node scopes; cross-scope access yields `ResourceNotFound`.
3. **I-C (row identity immutability):** account upserts never rewrite primary keys.
4. **I-D (fixed identity set):** the login surface offers no free-text identity entry on any node class.

## 5. Rejected Alternatives

* **Migration `011_*` preserving old rows** — rejected by owner: pre-release product, zero production databases; a migration chain would freeze a defective constraint into release history.
* **Per-unit prefixed usernames** (`unit12_user`) — leaks topology into credentials, breaks the fixed-selector login model, keeps the phishing surface.
* **Keeping `identity_access` dual-purpose with renames** — already superseded by ADR-0051 D1; reintroducing renames would violate the ADR-0051 owner invariant.

## 6. Consequences

Positive: multi-unit provisioning is collision-free by construction; the rename defect class is structurally eliminated; login UX matches backend reality; the schema expresses the domain fact (accounts are node-local).

Negative: existing dev databases predating the change must be recreated once (owner-authorized); documentation must keep describing `admin_access` (ADR-0051) as the sole post-cutover account-sync channel — unchanged by this decision.

## 7. Test Requirements (SEC-026 implementation gate)

Mandated and implemented: composite-conflict upsert determinism; coexistence of `('user', A)` / `('user', B)` / `('admin', local)`; scoped status toggling rejection across scopes; preserved row identity on re-import (no rename, id stable); ceremony single-admin invariant under scoped lookups; policy scoping matrix (WILAYA vs UNIT); frontend unit tests proving the WILAYA read-only pin and the UNIT fixed selector; e2e flows updated to the fixed-identity model without weakening lockout/negative-path coverage.

## 8. Freeze References

Governed by `docs/architecture/ARCHITECTURE_FREEZE.md`: the baseline schema was not a frozen artifact (§2); the change follows the documented SEC-025 → SEC-026 gate sequence.

Related: ADR-0051 (admin-only sync; UNIT-operator preservation invariant — completed here), ADR-0050 (normal-login model — extended, not altered), ADR-0040 (historical dual-purpose semantics — fully retired), ADR-0045 (bootstrap exemption precedent).
