# ADR 0048: Fiscal Closure Ed25519 Migration — Identity-Bound Signing (SEC-008)

# Decision Status

**ACCEPTED — 2026-08-18 (implementation-authorized ratification: SEC-008)**

This ADR ratifies the SEC-008 implementation mission: the fiscal closure
authorization package (`.fiscal-close.sync`) is migrated from the shared-secret
HMAC-SHA256 envelope to an **Ed25519 signature bound to the WILAYA node
identity** (RFC 8032, `signature_version = 2` per RFC
`2026-08-04-node-identity-trust` §3.10 / ADR-0038). The temporary exception
`[arch:allow-hmac-fiscal]` (ADR-0047 §8, registry #29) is **CLOSED**: all HMAC
machinery is removed and no shared-secret signing path remains anywhere in the
repository.

| Item | Status |
|------|--------|
| Fiscal closure package HMAC envelope (`.fiscal-close.sync`, `FISCAL_CLOSURE_PACKAGE_VERSION = 2`) | **REMOVED** |
| Ed25519 identity-bound fiscal closure envelope (v3: `package` + `signature_version` + `signer_public_key_hex` + `signature_hex`) | **RATIFIED (mandatory, fail-closed)** |
| Signer: ACTIVE WILAYA node identity (R5-proven key via `NodeIdentityResolver`) | **RATIFIED (only signer; Root/UNIT/ADMIN cannot authorize)** |
| Verifier: UNIT resolves issuer from Identity Store, requires trusted ACTIVE WILAYA, verifies Ed25519 over canonical JSON bytes | **RATIFIED (mandatory, fail-closed)** |
| `GRPC_PACKAGE_SIGNING_KEY` / `GRPC_ACTIVE_SIGNING_KEY_ID` env reads, prod gate, `D_signing_key` HMAC semantics, diagnostics fields | **REMOVED** |
| Schema migration | **NOT REQUIRED** |
| Package-format migration / backward-compat window / dual HMAC+Ed25519 period | **NOT PROVIDED — pre-deployment source-level cutover; legacy v2/HMAC files are rejected, not converted** |
| Sync package pipeline (`issuer_sequence`, Transport Guard, V2) | **UNCHANGED** |
| `GRPC_APP_KEY` / `GRPC_ENV` | **UNCHANGED** |

# Date

2026-08-18

# Relation to Existing Documents

| Document | Relation |
|----------|----------|
| ADR-0047 §8 (fiscal HMAC exception, `[arch:allow-hmac-fiscal]`) | **Closed** — the deferred follow-up is now implemented; registry row #29 marked CLOSED. |
| RFC `2026-08-04-node-identity-trust` §3.10 / ADR-0038 | **Extended** — the fiscal closure package joins the identity-bound Ed25519 signing surface; verification binds to the WILAYA trust anchor. |
| ADR-0039 (two-tier secrets) | **Strengthened** — no shared signing secret remains; `age::x25519` node keys + identity Ed25519 cover the fiscal surface. |
| ADR-0041 (production app key) | **Unchanged** — `GRPC_APP_KEY`/`GRPC_ENV` semantics untouched. |
| `ARCHITECTURE_FREEZE.md` | **Unchanged** — fiscal closure semantics (authorized transitions, replay, retention, maintenance confirmation) are preserved; this ADR is the sanctioned change record for the signing envelope. |

# 1. Executive Summary

SEC-008 removes the last HMAC-based authorization path in the system. The
fiscal closure authorization package — the offline WILAYA → UNIT authorization
for `close_year` — was previously signed with a fleet-shared 32-byte key
(`GRPC_PACKAGE_SIGNING_KEY`) carried in environment configuration. Under the
node-identity trust chain (ADR-0038) a shared secret is both weaker and
architecturally foreign: identity-based Ed25519 gives issuer accountability
(the signer is a resolvable WILAYA identity), fails closed against unknown or
untrusted signers, and removes the production-env mandatory-key coupling.

This is a **clean source-level cutover**: the application is pre-deployment,
there are no persisted signature columns, no deployed fleet, and no
backward-compat requirement. No DB migration and no package-format migration
are performed; a legacy v2/HMAC envelope is rejected at read time.

# 2. Envelope v3 (`.fiscal-close.sync`)

```
FiscalClosureEnvelope {
  package: FiscalClosurePackage,        // schema_version = 3
  signature_version: u32,               // must equal SIGNATURE_VERSION_ED25519 (2)
  signer_public_key_hex: String,        // 32-byte Ed25519 public key, hex
  signature_hex: String,                // 64-byte Ed25519 signature, hex
}
```

`FiscalClosurePackage` gains `issuer_identity_id` (UUID of the signing WILAYA
identity, covered by the signature and the fingerprint). `signing_key_id`
semantics change from "HMAC key rotation id" to **the signer's Ed25519 public
key hex** (registry + preview + audit fields unchanged in shape).

Canonicalization: `serde_json::to_vec(package)` — the existing deterministic
fiscal convention, identical on both sides.

# 3. Signing (WILAYA)

- `export_fiscal_closure_package` resolves the local signer via
  `NodeIdentityResolver::resolve_local_signer(db, node_key_store, SubjectType::Wilaya)`
  — the R5-proven Ed25519 provider paired with the ACTIVE WILAYA certificate.
- `Ok(None)` (no node key or no ACTIVE WILAYA certificate) → export refused
  with an Arabic operator error. `Err` (R5 mismatch, revoked, non-Ed25519
  algorithm version) → export refused.
- The package is signed over the canonical JSON bytes with
  `Ed25519PackageSigner`; `signer_public_key_hex` and `signing_key_id` both
  carry the signer's public key.
- **No environment variable is ever read for signing.** Root must not sign
  fiscal packages; only the WILAYA identity may.

# 4. Verification (UNIT)

`read_and_verify_envelope` (instance method on `FiscalClosurePackageService`)
rejects fail-closed, in order, with a distinct `[FISCAL_CLOSURE_PACKAGE_REJECTED]
reason=...` log line:

1. `schema_version != 3` → `unsupported_schema_version`;
2. `signature_version != SIGNATURE_VERSION_ED25519` → `unsupported_signature_version`;
3. signature empty → `missing_signature`; malformed → `malformed_signature`;
4. signer public key malformed → `malformed_signer_public_key`;
5. `issuer_identity_id` not a UUID → `malformed_issuer_identity_id`;
6. identity not found in Identity Store → `unknown_signer`;
7. issuer `subject_type != Wilaya` → `signer_not_wilaya`;
8. issuer status != Active → `signer_not_active`;
9. issuer certificate expired (`not_after`) → `signer_certificate_expired`;
10. envelope key ≠ certificate public key → `signer_key_mismatch`;
11. issuer is not the ACTIVE WILAYA trust anchor (`get_active_by_subject_type`,
    Invariant 6) → `signer_not_trusted_anchor`;
12. Ed25519 verification over the canonical bytes fails → `invalid_signature`.

There is no fallback to any shared-secret scheme at any step.

# 5. Deletions (HMAC Surface)

| Removed | Where |
|---------|-------|
| `resolve_package_signing_key_32_impl` / `resolve_package_signing_key_32` (incl. zero-key dev fallback) | `infrastructure/security/mod.rs` |
| `resolve_active_signing_key_id` | `infrastructure/security/mod.rs` |
| `GRPC_PACKAGE_SIGNING_KEY` / `GRPC_ACTIVE_SIGNING_KEY_ID` validation in `validate_production_security_environment` | `infrastructure/security/mod.rs` |
| `compute_hmac` / `verify_hmac` / `hmac_hex` envelope field | `fiscal_closure_package_service.rs` |
| `SyncSecurityDiagnostics.has_package_signing_key_env` / `active_signing_key_id` | `models/dto.rs`, `src/lib/types.ts` |
| HMAC-based `D_signing_key` check | `deployment_readiness_service.rs` |
| `GRPC_PACKAGE_SIGNING_KEY` e2e injection (`testSigningKey`) | `src/tests/e2e/orchestration/processManager.ts` |

Remaining HMAC usages (password pre-hash in `password_hash_provider.rs` and
its domain docs) are unrelated to package signing and are **not** affected by
this ADR.

# 6. Re-pointed Consumers

- **`D_signing_key`** (deployment readiness): WILAYA nodes must have a
  resolvable signing identity (`resolve_local_signer` → `Ok(Some)`); UNIT /
  UNCONFIGURED nodes pass as "not applicable". Blocking in production,
  warning otherwise.
- **`export_reproducibility_helper`**: `signing_key_id` audit field becomes the
  resolved WILAYA public key hex; "unset" documents unprovisioned nodes.
- **`get_sync_security_diagnostics`**: `bootstrap_would_fail = production &&
  !has_app_key_env` only.

# 7. Replay & Retention (Unchanged)

Replay protection remains `fiscal_transition_id` (UNIQUE in
`applied_fiscal_transitions`), pre-flight `is_applied` check, and the
`DuplicateSyncPackage` error path. No `issuer_sequence` is introduced into the
fiscal surface (that ledger belongs to the sync V2 pipeline, which is
untouched). Retention lifecycle (ACTIVE/ARCHIVED/RETIRED, registry,
transition history) is unchanged. The pre-existing asymmetry where
`close_fiscal_year_confirmed` does not register a pending package is
unchanged by this ADR.

# 8. Protected Boundaries

No migrations, no fiscal schema changes, `FiscalClosingService::close_year`
semantics, `GRPC_APP_KEY` / age stack, `NodeKeyStore` internals,
`AdminKeyProvider`, `AppKeyStore`, identity provisioning/rotation,
challenge-response, Transport Guard, Credential Guard, sync V2 pipeline,
`.unit` bootstrap, backup/restore, password pre-hash HMAC, or authz policies.

# 9. Implementation Boundary (Binding)

Allowed to change (already applied by SEC-008):

- `src-tauri/src/application/services/fiscal_closure_package_service.rs`
- `src-tauri/src/commands/fiscal.rs`
- `src-tauri/src/infrastructure/security/mod.rs`
- `src-tauri/src/models/dto.rs` (diagnostics fields)
- `src-tauri/src/application/services/deployment_readiness_service.rs` and its
  call sites
- `src-tauri/src/application/services/export_reproducibility_helper.rs` and
  its call sites
- `src-tauri/src/domain/identity/signing.rs` (stale HMAC comment)
- frontend contracts (`types.ts`, `tauri.ts`, `FiscalManagementPage.svelte`)
- e2e harness (`processManager.ts`)
- tests (retention rewrite, new Ed25519 security matrix)
- docs (this ADR, ADR_INDEX, exception registry #29 CLOSED, THREAT_MODEL,
  runbooks, README)

Not allowed: anything listed in §8. In particular no SQL, no migration file,
no new Tauri command, no async, no unrelated refactor.

# 10. Test Coverage

- `fiscal_closure_ed25519_security_tests.rs` — the SEC-008 security matrix:
  v3 metadata, valid verify/apply, tamper, invalid signature, unknown signer,
  key mismatch, expired/revoked signer, missing signature, unsupported
  signature/schema version, legacy v2/HMAC envelope rejection, UNIT forgery
  rejection, unrelated-WILAYA rejection, env-var irrelevance (HMAC env vars
  neither consulted nor required), replay + retention unchanged.
- `fiscal_transition_retention_tests.rs` — rewritten against the seeded WILAYA
  identity signer; lifecycle/fingerprint/preview semantics unchanged.
- `authorized_transition_execution_tests.rs` — build-signature updates only.
- `operator_reliability_tests.rs` — `DeploymentReadinessService` constructor
  updates; fresh-DB `D_signing_key` passes on UNCONFIGURED default.
- `infrastructure/security/mod.rs` unit tests — HMAC resolver/gate tests
  deleted; app-key gate tests retained; leftover env-var irrelevance asserted.

# 11. Verification

- Full regression: `cargo test --workspace`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `bun run check`,
  `bun run test:unit`, `bun run test:e2e`, plus the governance gates
  (check:arch zero warnings, check_docs_governance, check_secrets,
  check_release_integrity, `git diff --check`).
- Repo-wide searches must confirm zero active production fiscal HMAC
  dependencies and zero active `[arch:allow-hmac-fiscal]` tags.