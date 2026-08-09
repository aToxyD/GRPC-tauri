# ADR 0042: Licensing Consumer Integration — Signed Licensing Artifact Consumption in GRPC

# Status
Accepted (2026-08-08)

# Amended
2026-08-09 — subject binding re-scoped to the opaque `subject.id` convention (raw node
Ed25519 public key, `base64url(URL_SAFE_NO_PAD)`) per R1–R5. Contract v1 and
`grpc-licensing` unchanged.

# Date
2026-08-08

# Owner
Architecture / Licensing Integration

# Reference
- `grpc-licensing/docs/architecture/ADR-0001..0005` (Accepted) — responsibilities,
  integration boundaries (Invariants 1–4), canonical model (Invariants 1–3), licensing vs
  identity/trust (Invariants 1–4), operating model (Invariants 1–6).
- `grpc-licensing/docs/architecture/ADR-0006` (Accepted) — artifact format:
  Canonical JSON + Ed25519, envelope, verification pipeline, provisioning package.
- `grpc-licensing/contracts/licensing/artifact-spec.md` **v1** (Published) — the **single
  canonical contract** this decision consumes. No other contract exists.
- `grpc-licensing/contracts/licensing/VERSION_MATRIX.md` — supported versions (`1` only).
- `grpc-licensing/docs/domain/ARTIFACT_MODEL.md` — 7-component → `payload` mapping.
- ADR-0038 (node identity & trust) — subject binding uses the node's Ed25519 public key.
- ADR-0041 — app-key provisioning; unchanged by this decision.

# Context

`grpc` is the designated **consumer** of Signed Licensing Artifacts produced by the
Licensing Authority (`grpc-licensing`). Today, `grpc` has **no licensing consumer layer**:
no trust anchor import, no artifact verification, no entitlement enforcement. The five
integration ADRs and the published artifact-spec v1 contract define *what* must be
consumed, but the consumer side — the code inside `grpc` that imports, verifies, binds,
and enforces — is entirely absent.

This decision installs that missing bounded context. It is deliberately **consumer-only**:
`grpc-licensing` is NOT modified, and `grpc` gains no ability to issue, modify, or
reinterpret licensing decisions (ADR-0002 Invariant 1, ADR-0004 Invariant 4).

# Decision

## 1. Scope: consumer-only bounded context (normative)

`grpc` implements **import + verify + bind + enforce + status** for Signed Licensing
Artifacts and Provisioning Packages, against the **published contract v1** only.

- **No changes to `grpc-licensing`** in any form (code, contracts, docs, schema).
- `grpc` holds only a **derived, replaceable view** of license state (ADR-0003 Invariant 2),
  never authoritative state.
- No local mutation of license state (ADR-0003 Invariant 3): every observable transition
  (Active → Suspended/Revoked/Expired) arrives as a **newly issued signed artifact**.
- No wall-clock participation in validity decisions (ADR-0005 §2.7): any temporal state is
  **signed payload data**.
- No network, no shared DB, no IPC between the projects (ADR-0002 Invariant 2,
  ADR-0005 Invariant 1).

## 2. Trust anchor lifecycle (provisioning-v1)

- Import format: Provisioning Package `provisioning-v1` (`artifact-spec.md` §9):
  `{ format: "provisioning-v1", key_id, algorithm: "Ed25519", public_key }`.
- The package carries the **anchor public key only** and grants no operational trust
  (ADR-0004 Invariant 3, ADR-0005 Invariant 5).
- **Exactly one active licensing anchor** per installation (ADR-0005 Invariant 6).
  Importing a newer package **replaces** the single active anchor (rotation);
  a second simultaneous active anchor is rejected.
- Anchor storage is a derived view: `licensing_anchor` table with `key_id`, public key,
  `installed_at`, `is_active` (single active enforced).
- Anchor installation is required before any artifact can be signature-verified; a
  presented artifact with **no installed anchor** is rejected (fail-closed).

## 3. Artifact verification pipeline (contract v1)

`grpc` verifies every artifact **locally and deterministically**, exactly per
`artifact-spec.md` §4, in this order — any failure at any step ⇒ **reject** (ADR-0005
Invariant 4):

1. **Structural** — envelope parses; fields present and typed (§2).
2. **Version** — `version == 1` per `VERSION_MATRIX.md`; anything else rejected.
3. **Identity** — recompute `artifact_id = hex(lowercase(SHA-256(UCR)))` (§3.1–3.2) and
   assert it equals `metadata.artifact_id`.
4. **Signature** — recompute canonical digest over the **final unsigned bytes** (§3.3),
   verify Ed25519 against the single **active** anchor; `signature.key_id ==
   metadata.key_id == anchor.key_id`.
5. **Subject binding** — strict-decode `payload.subject.id` (base64url URL_SAFE_NO_PAD,
   exactly 32 bytes) and compare **byte-exact** to the current raw node Ed25519 public key
   (§4); mismatch ⇒ `LICENSE_NOT_FOR_THIS_NODE`. A correct signature alone is insufficient.
6. **Semantic** — payload consistent with the canonical model (ARTIFACT_MODEL.md): license
   id, `type_key` in the contract registry (`production`; unknown ⇒ reject), subject,
   entitlements, status, contract version.
7. **Enforce** — apply declared entitlements via the enforcement layer (§5).

## 4. Subject binding (LICENSE_NOT_FOR_THIS_NODE)

- Contract v1's `payload.subject` is an **opaque identifier**: it carries only
  `payload.subject.id`, which MUST equal `metadata.issued_for`. **No `subject.public_key`
  field exists** in artifact-spec v1; nothing in this section assumes one.
- **grpc-owned binding convention (normative):** `payload.subject.id` is the canonical
  encoding of the current node's raw Ed25519 public key:

  ```
  subject.id = base64url(URL_SAFE_NO_PAD, public_key_bytes)   # exactly 32 bytes, RFC 8032
  ```

  This is a contract-level encoding convention chosen by this ADR for node binding;
  `grpc-licensing` treats `subject.id` as opaque and is NOT modified.
- **Subject validation (fail-closed, at import):** a `subject.id` that is not strict
  base64url, contains padding, or does not decode to exactly 32 bytes is rejected.
- **Node key source (normative):** the raw local Ed25519 public key is derived from the
  node-scoped key material (`NodeKeyStore` → `derive_public_key`). A **certificate is NOT
  part of binding** — no trust-chain, no cert status, no certificate-liveness check. If the
  node key is absent, binding cannot be evaluated and the outcome is fail-closed
  (`NodeKeyMissing`).
- **Binding rule (normative):** a license is VALID for the current node only if
  `decode(subject.id)` equals the current raw node public key (**byte-exact**). A mismatch
  is the distinct fail-closed outcome: `LICENSE_NOT_FOR_THIS_NODE`.
- **Distinction (acceptance criterion):** `LICENSE_NOT_FOR_THIS_NODE` is a **post-signature**
  outcome — the artifact's authority signature is valid and the binding failed. It is never
  reported as `InvalidSignature`. **A correct signature alone is insufficient for validity.**
- The binding is evaluated at import and re-evaluated at every enforcement decision; it can
  never be skipped, overridden, or downgraded to a warning.

## 5. Entitlement enforcement

- Entitlements are declared **inside the signed payload** (`payload.entitlements`,
  ARTIFACT_MODEL.md §2) as an explicit list. `grpc` never infers or computes them.
- The declared set maps onto `grpc` operational feature groups via a **fixed, single
  mapping table** owned by the enforcement service (P2 — one owner, no duplication):

| Entitlement key | Feature group enforced in `grpc` |
|-----------------|----------------------------------|
| `core.auth` | authentication / identity access |
| `core.sync` | sync package import/export |
| `core.reports` | daily/monthly/Wilaya reporting |
| `core.stock` | inventory, stock movements, products |
| `core.consume` | orders / consumption operations |
| `core.admin` | administrative & governance actions |

- **Enforceable license**: a stored license is enforceable iff it is **Active**, **re-verifies
  valid** (full §3 pipeline), and **binding matches the current node** (§4). At least **one**
  stored enforceable license providing the required entitlement grants the action (multiple
  license ids allowed; no single-license restriction at the entitlement level).
- **No valid license ⇒ no entitlement**: with no enforceable license the node is restricted
  to the **exempt set** — authentication; app-key/security management; identity
  provisioning, sync, and rotation; licensing/anchor management. All other operations are
  denied (fail-closed default).
- **Denial surface**: the enforcement gate inside `authorize_command` denies with a dedicated
  `AuthorizationError` variant — `LicenseRequired` (no enforceable license),
  `EntitlementRequired` (license valid but missing the mapped entitlement), or
  `LicenseNotForThisNode` (binding mismatch) — fail-closed, logged, and audited.
- A license with status `Suspended`/`Revoked`/`Expired` (signed) ⇒ entitlement enforcement
  stops (LICENSE_LIFECYCLE.md statuses).
- An entitlement key **not in the mapping table** is rejected at import (unknown state ⇒
  fail-closed), matching "unknown type ⇒ reject" semantics; `grpc` never ignores a declared
  entitlement.

## 6. Import & replace semantics (derived view)

- Import of an artifact for a license id **replaces** any previously held derived view for
  that id (ADR-0003 Invariant 2). Replacement is a full overwrite of the derived row, never
  a merge.
- Revocation is delivered as a new signed artifact with `status: "revoked"`; `grpc` applies
  it by denying the previously granted entitlements. There is no local "revoke" command.
- A re-import of the identical artifact is idempotent (same `artifact_id`).

## 7. Persistence (derived, replaceable)

Migration `007_licensing.sql` adds consumer-view tables (no authoritative semantics):

- `licensing_anchor` — active anchor state (exactly one active).
- `licensing_license` — per-license derived view: artifact_id, license_id, type_key,
  subject_id, entitlements (JSON), status, imported_at.
- `licensing_events` — append-only audit of imports (P4), recording outcome
  (verified / rejected / not-for-this-node).

All writes flow through repositories; no SQL outside `repositories/`.

## 8. Command surface

New IPC commands (registered in `commands/registry.rs`):

- `get_licensing_status` → `{ anchor: {installed, key_id}, licenses: [...], summary }`
- `import_trust_anchor(package_json)` → install/replace the single active anchor
- `import_license(artifact_json)` → verify + bind + store derived view
- `verify_license()` → deterministic re-verification of stored licenses (no re-import)
- `dry_run_verify_license(artifact_json)` → verify without persisting (operator preview)

All are thin handlers (auth guard + dispatch); verification logic lives in an application
service (`licensing/`), crypto byte-work in infrastructure.

## 9. Layer placement & isolation

| Concern | Location |
|---------|----------|
| Contracts / types | `src-tauri/src/models/` + `src/lib/contracts/licensing.contract.ts` |
| IPC handlers | `src-tauri/src/commands/licensing.rs` (thin, registered in `registry.rs`) |
| Orchestration (import/verify/bind/enforce) | `src-tauri/src/application/licensing/` (services) |
| Crypto bytes only (canonical JSON, SHA-256, Ed25519 verify) | `src-tauri/src/infrastructure/licensing/` |
| SQL | `src-tauri/src/repositories/` only |
| UI | `src/pages/` + routing in `src/App.svelte` |

The licensing bounded context is **isolated** from the identity trust chain (ADR-0004
Invariant 1, Invariant 2): it reuses the node's **raw Ed25519 public key**
(`NodeKeyStore` → `derive_public_key`) for binding — **no certificate, no cert status, and
no trust hierarchy participate** in licensing verification, and no signing keys, anchors,
or trust relationships are shared with identity/sync signing.

# Consequences

- `grpc` gains the full consumer path defined by the five licensing ADRs, with zero
  modification to `grpc-licensing`.
- Node-bound licensing: a license minted for Node A is VALID on A and
  `LICENSE_NOT_FOR_THIS_NODE` on Node B (acceptance criterion).
- Subject binding uses the ADR-0042 `subject.id` convention — `base64url(URL_SAFE_NO_PAD)`
  of the raw 32-byte node Ed25519 public key — on the **unchanged** opaque `subject.id` of
  artifact-spec v1; `grpc-licensing` and the contract are not modified.
- Enforcement is deterministic and fail-closed; no local mutation; no wall-clock; no
  network; derived view only.
- Future license types (contract v1 `production` only today) require no `grpc` code change
  as long as they remain within the contract (ADR-0002 §6).
- Anchor rotation is operator-driven physical provisioning, single active anchor at all
  times.

# Out of scope

- Any change to `grpc-licensing` (authority, issuance, revocation, registry, storage).
- Offline generation of licenses, extension, or renewal within `grpc`.
- Network activation, phone-home, auto-update of licenses.
- Encrypting artifacts (ADR-0006 §6: no confidentiality).
- X.509 / PKI; anything beyond Ed25519 + SHA-256 (ADR-0007).

# Implementation

- B2/B1: migration `007_licensing.sql` + repositories; `application/licensing/` services;
  `infrastructure/licensing/` canonical-JSON + verification; `commands/licensing.rs`;
  `licensing.contract.ts`; status page + routing.
- Subject binding uses the raw node key material (`NodeKeyStore` → `derive_public_key`);
  no certificate or trust-chain state is required (ADR-0038 key material, not its trust
  chain).
- D1: ADR_INDEX.md updated with this ADR.
