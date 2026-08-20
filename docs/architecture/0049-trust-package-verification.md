# ADR 0049: Trust Package Verification Hardening — WILAYA-Pinned, Root-Verified, Anchor-Immutable (SEC-010)

# Decision Status

**ACCEPTED — 2026-08-19 (implementation-authorized ratification: SEC-010)**

This ADR ratifies the SEC-010 implementation mission: the trust package
import path (`kind = trust`, RFC `2026-08-04-node-identity-trust` §3.9) is
hardened so that payload certificates and revocations are **authority-verified
before any mutation**, eliminating the SEC-009-01 findings: a compromised
WILAYA signing key could previously forge WILAYA certificates (issuer None,
attacker-controlled public key, higher generation) or distribute UNIT/ADMIN
certificates inside a trust package, and a WILAYA could revoke identities it
did not issue or that are outside its authority.

| Item | Status |
|------|--------|
| Envelope issuer pin: package MUST be signed by the ACTIVE **local** WILAYA identity | **RATIFIED (mandatory, fail-closed)** |
| Payload certificates: WILAYA only — UNIT/ADMIN certificates REJECTED | **RATIFIED (mandatory, fail-closed)** |
| WILAYA certificates: subject Wilaya + issuer None + status Active + signature present + **Root signature verified** (`Ed25519SignatureVerifier.verify_certificate` against `resolve_root_public_key()`) | **RATIFIED (mandatory, fail-closed)** |
| Anchor immutability: identical anchor = idempotent no-op; anything else REJECTED — trust import never rotates | **RATIFIED (mandatory, fail-closed)** |
| Revocations: existing UNIT identities only, `issuer_identity_id == package signer == local WILAYA` | **RATIFIED (mandatory, fail-closed)** |
| Atomicity: ENTIRE payload validated before ANY mutation (all-or-nothing within the pipeline transaction) | **RATIFIED (mandatory)** |
| CredentialGuard | **UNCHANGED — monotonicity-only, ordered after authenticity** |
| UNIT bootstrap (`.unit`), ADMIN ceremony, challenge, R5, `install_wilaya_certificate` | **UNCHANGED** |
| Schema migration | **NOT REQUIRED** |
| Package format change | **NOT REQUIRED** |

# Date

2026-08-19

# Relation to Existing Documents

| Document | Relation |
|----------|----------|
| RFC `2026-08-04-node-identity-trust` §3.9 / ADR-0038 | **Strengthened** — the trust package authority model is closed: WILAYA anchors are Root-issued and immutable via this channel; UNIT/ADMIN certificates have no trust-package distribution path. |
| SEC-009-01 verification (verdict: PARTIAL / DIFFERENT ISSUE) | **Closed** — mechanism confirmed; WILAYA-local impact; UNIT/ADMIN/fleet impact REFUTED (authz denies trust import outside WILAYA-Admin); the residual WILAYA-anchor forgery path is eliminated by this ADR. |
| ADR-0044 (UNIT trust-first bootstrap) | **Unchanged** — the `.unit` bootstrap is the UNIT trust source; trust packages remain WILAYA-only. |
| ADR-0046 (UNIT↔WILAYA data sync trust) | **Unchanged** — data packages are unaffected; this ADR touches only `kind = trust`. |
| `ARCHITECTURE_FREEZE.md` | **Unchanged** — no frozen item is modified; this ADR is the sanctioned change record for the trust import authority rules. |

# 1. Executive Summary

SEC-009-01 demonstrated that the previous trust import path authenticated the
**envelope** (issuer certificate + Ed25519 package signature) but treated the
**payload** as trusted data: any WILAYA-signed package could upsert WILAYA
certificates (issuer None — the "Root-issued" marker was never checked against
the Root public key), distribute UNIT/ADMIN certificates, or revoke any
persisted identity regardless of issuance authority. The CredentialGuard
(monotonicity-only) was the sole payload control.

SEC-010 makes the authority model explicit and fail-closed:

1. **Issuer pin** — the envelope MUST be signed by the ACTIVE *local* WILAYA
   identity (`get_active_by_subject_type(Wilaya)` + `issuer_identity_id`
   equality). A package signed by any other WILAYA, a stale identity, or no
   identity is rejected before any payload processing.
2. **WILAYA-only, Root-verified certificates** — every payload certificate
   MUST be a WILAYA certificate (subject Wilaya, issuer None, status Active,
   signature present) whose signature verifies against the Authority Root
   public key (`resolve_root_public_key()`). `issuer None` alone is NOT
   authority: the Root signature over the canonical certificate bytes is.
3. **Anchor immutability** — the only certificate a trust package may carry is
   the ACTIVE WILAYA anchor itself, re-presented **verbatim**
   (`is_identical_to`). An identical re-presentation is an idempotent no-op;
   any successor, replacement, or second WILAYA identity is rejected. Trust
   import never rotates the anchor — rotation remains the
   `finalize_wilaya_rotation` ceremony.
4. **Issuer-scoped UNIT revocations** — a revocation may target ONLY an
   existing UNIT identity issued by the package signer (== local WILAYA).
   ADMIN/WILAYA/unknown/cross-issuer targets are rejected.
5. **Validation-before-mutation** — the ENTIRE payload (all certificates, all
   revocations) is validated before ANY write; the pipeline transaction
   (all-or-nothing) guarantees a rejected package leaves zero trace.

UNIT issuance remains the local R5 ceremony; ADMIN ceremony, challenge, `.unit`
bootstrap, and data sync packages are untouched.

# 2. Authority Model (post-SEC-010)

```
Root (offline Authority, Ed25519; resolve_root_public_key)
 │  ── signs WILAYA anchors (issuer None, canonical bytes)
 ▼
WILAYA anchor (ACTIVE local identity — the ONLY distributable certificate)
 │  ── signs trust packages (envelope, kind=trust) and issues UNIT/ADMIN certs (R5)
 ▼
UNIT / ADMIN certificates (NEVER distributed via trust packages)
```

Trust-package import rules (all fail-closed, `OperationNotPermitted`):

| Element | Accepted | Rejected |
|---------|----------|----------|
| Envelope issuer | ACTIVE local WILAYA identity | other WILAYA, stale/revoked, missing |
| Certificate subject | WILAYA | UNIT, ADMIN |
| Certificate issuer | None (Root-issued) | Some(...) |
| Certificate status | Active | Revoked / any other |
| Certificate signature | present + Root-verified | missing, garbage, imposter, WILAYA-key-signed |
| Certificate identity | identical to ACTIVE local anchor | successor, tampered, second WILAYA |
| Revocation target | existing UNIT issued by package signer | ADMIN, WILAYA, unknown, cross-issuer |

# 3. Verification Order (importer, `import_trust_package::execute`)

1. `source_node_id` present; package not previously imported
   (`DuplicateSyncPackage` fail-closed).
2. **Issuer pin**: ACTIVE local WILAYA identity exists; `issuer_identity_id`
   equals it.
3. **Certificate pass (read-only)**: for every certificate — signed
   (`require_signed`), subject WILAYA, issuer None, status Active, Root
   signature verified against `resolve_root_public_key()`, `is_identical_to`
   the local anchor.
4. **Revocation pass (read-only)**: for every revocation — target exists,
   subject UNIT, `issuer_identity_id == package signer`.
5. **Apply pass**: CredentialGuard (monotonicity-only, unchanged) →
   `identity_store.upsert` for certificates; status→Revoked upsert for
   revocations; `mark_imported`.

Steps 3–4 complete before step 5 begins. The pipeline runs the whole sequence
inside one transaction (`run_import_pipeline`, `with_event_persistence`), so a
failure in ANY step rolls back the entire package — zero partial writes.

# 4. Changes

| File | Change |
|------|--------|
| `src-tauri/src/application/usecases/sync/import_trust_package.rs` | `execute()` hardened per §2–§3; test module rewritten (24 tests). |
| `src-tauri/src/application/authz/policies/mod.rs` | Added `trust_package_import_remains_wilaya_admin_only` (UNIT nodes cannot import trust packages). |
| `src-tauri/tests/sync_trust_registry_import_tests.rs` | Happy path updated to anchor re-presentation; added `trust_import_unit_certificate_is_rejected_sec010`. |

No migrations. No package-format changes. No changes to UNIT bootstrap, ADMIN
ceremony, challenge, R5, or `install_wilaya_certificate`.

# 5. Alternatives Considered

| Alternative | Rejected Because |
|-------------|------------------|
| Minimal verification: require issuer None + ACTIVE status only | SEC-009-01 forgery path remains — issuer None is a marker, not proof of Root issuance. |
| Separate artifact (e.g., signed certificate ledger file) | New transport channel violates ADR-0010 (sync-package-only transport) and duplicates authority state; trust packages are already the single trust channel (§3.4.4). |
| Allow trust-package rotation of the WILAYA anchor | Introduces a remote key-change path for the local trust root; rotation belongs to the operator ceremony. Rejected for determinism and tamper-resistance. |
| Allow ADMIN/other-issuer revocations | WILAYA authority is scoped to its own issuance (ADR-0038); cross-authority revocation creates a DoS surface. |

# 6. Invariants (unchanged, reaffirmed)

- Trust package is the ONLY trust distribution channel; `.unit` bootstrap is
  the UNIT trust source (ADR-0044).
- CredentialGuard remains monotonicity-only (replay/rollback control), never
  an authenticity substitute.
- Backend is source of truth for authorization; no frontend changes.
- Determinism: identical package + identical state → identical outcome.
- Security controls stay in dedicated layers (authz policy + usecase + repo).

# 7. Residual Risks

| Risk | Mitigation |
|------|------------|
| Production Root public key is a placeholder pin (`resolve_root_public_key` env → compiled → dev fallback) | A-44-06 production Root key certification flow (operator-controlled); SEC-010 inherits it — the pin, not the verification logic, is the remaining deployment concern. |
| Compromised local WILAYA key can still sign revocations for UNITs it issued | Inherent to the authority model; mitigated by revocation audit records and the R5 custody chain. |
| Envelope signature verification for `kind = trust` runs in the pipeline service (`verify_v2_signature` path) | Unchanged by this ADR; the usecase adds the payload authority gates that were missing. |

# 8. Test Coverage

- 24 unit tests in `import_trust_package.rs` (acceptance of identical anchor;
  rejection of UNIT/ADMIN certs, forged WILAYA certs (stolen key + imposter
  signature), unsigned/garbage/tampered signatures, successor/different-WILAYA
  certs, cross-issuer packages, missing issuer, missing local WILAYA; scoped
  revocations (accepted local-UNIT, rejected ADMIN/WILAYA/cross-issuer/
  unknown); mixed-payload zero-write atomicity; duplicate determinism).
- Integration: `sync_trust_registry_import_tests` (pipeline round-trip with
  anchor re-presentation + ledger advance; UNIT-cert rejection with zero
  writes; authz WILAYA-Admin-only).
- Full gates: `cargo test --workspace`, clippy `-D warnings`, `bun run
  check:arch`, vitest, svelte-check, Playwright.