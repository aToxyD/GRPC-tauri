# ADR 0047: Sync Package V1/HMAC Removal — V2-Only (Ed25519) Enforcement (SEC-007)

# Decision Status

**ACCEPTED — 2026-08-18 (implementation-authorized ratification: SEC-007)**

This ADR ratifies the SEC-007 implementation mission: the legacy V1/HMAC
sync-package support is **permanently removed**, and every sync-package path is
enforced **V2-only** (Ed25519 signatures bound to node identities, issuer
identity + per-issuer transport sequence, mandatory integrity hash). V1/HMAC
references in the V1 architecture documents (ADR-0003, ADR-0007, ADR-0038,
ADR-0044, ADR-0046, RFC `2026-08-04-node-identity-trust`) become historical
where they describe the V1 window.

| Item | Status |
|------|--------|
| V1/HMAC sync-package support (produce, parse, verify, trusted-signer gate, env vars, diagnostics fields) | **REMOVED** |
| `signature_version = 2` (Ed25519) as the ONLY supported sync-package signature version | **RATIFIED (mandatory, fail-closed)** |
| Mandatory V2 metadata on every accepted package (integrity_hash, non-empty signature, issuer_identity_id, package_sequence) | **RATIFIED (mandatory)** |
| Schema migration for existing V2 columns | **NOT REQUIRED** |
| Package-format migration / V1-to-V2 conversion on import | **NOT PROVIDED — V1 files are rejected, not converted** |
| Fiscal closure package (`.fiscal-close.sync`) HMAC envelope | **DEFERRED — temporary exception `[arch:allow-hmac-fiscal]` (see §8)** |
| Follow-up ADR-0048: fiscal closure Ed25519 migration | **REQUIRED (tracked)** |

# Date

2026-08-18

# Relation to Existing Documents

| Document | Relation |
|----------|----------|
| RFC `2026-08-04-node-identity-trust.md` | **Amended (historical clauses)** — §3.10 remains the V2 signing authority; V1/HMAC window clauses are historical and are superseded by this ADR. |
| ADR-0003 (signature versioning) | **Superseded (V1/HMAC clauses)** — `signature_version = 2` / Ed25519 remains; `signature_version = 1` / HMAC is removed. |
| ADR-0007 (sync interchange) | **Superseded (V1 window clauses)** — the V1 compatibility window is closed; `MIN_SUPPORTED = V2`. |
| ADR-0038 (node identity & trust) | **Amended (historical clauses)** — Identity Store remains the single source of truth; V1/HMAC references in its appendices are historical. |
| ADR-0039 (crypto) | **Amended (historical clauses)** — HMAC-SHA256 sync signing is removed; age/Ed25519 stack unchanged. |
| ADR-0044 (`.unit` Trust-First V2) | **Amended** — the legacy V1/HMAC `.unit` acceptance window (A44-07) is closed; `.unit` imports are V2-only. |
| ADR-0046 (UNIT → WILAYA trust) | **Amended (historical clause)** — its §4 "V1 legacy data-package path UNCHANGED" statement is superseded: the V1 path is removed. |
| `ARCHITECTURE_FREEZE.md` | **Unchanged** — sync-package signature machinery is not frozen; this ADR is the sanctioned change record. |

# 1. Executive Summary

SEC-007 removes the legacy V1/HMAC sync-package machinery introduced before
the node-identity trust chain (RFC `2026-08-04-node-identity-trust`, ADR-0038)
existed. V1 packages carry no issuer identity and no per-issuer transport
sequence, which conflicts with B4 transport integrity (replay protection,
issuer accountability). After this ADR:

- every sync package must be V2 (Ed25519) with complete metadata — the
  deserializer, the import security gate, the compatibility window
  (`MIN_SUPPORTED = V2`), and the identity verification service all reject
  anything else, fail-closed;
- the HMAC signer, trusted-signer enforcement (`GRPC_ENFORCE_TRUSTED_SIGNERS`,
  `GRPC_TRUSTED_SIGNER_IDS`), accepted/deprecated key lists
  (`GRPC_ACCEPTED_SIGNING_KEY_IDS`, `GRPC_DEPRECATED_SIGNING_KEY_IDS`,
  `GRPC_SIGNING_KEY_DEPRECATION_DEADLINE_UTC`), and the diagnostics fields
  derived from them are deleted;
- no database migration and no package-format migration are performed: legacy
  V1 files are rejected at read time;
- the fiscal closure authorization package (`.fiscal-close.sync`) retains its
  own HMAC envelope for now, under a time-boxed exception (§8), with a
  dedicated follow-up mission (ADR-0048) to migrate it to the Ed25519 identity
  chain.

# 2. Problem Statement

V1/HMAC sync packages were produced with a shared symmetric key
(`GRPC_PACKAGE_SIGNING_KEY`) and carried no issuer identity or transport
sequence. They therefore:

1. cannot be attributed to an issuing node (no `issuer_identity_id`);
2. cannot participate in the per-issuer replay ledger (`package_sequence`);
3. rely on a fleet-wide shared secret instead of the certified Ed25519
   identity chain (ADR-0038);
4. force a compatibility window (A44-07, ADR-0007) that weakens fail-closed
   semantics on every import path.

The coexistence of two signing schemes doubles the attack surface and keeps
deployment guidance (HMAC key rotation, trusted-signer lists) that no longer
matches the identity-first architecture.

# 3. Removed Capabilities (Exhaustive)

| # | Removed item | Location (before removal) |
|---|--------------|---------------------------|
| 1 | `HmacPackageSigner` (HMAC-SHA256 signing) | `infrastructure/sync/packages/signing/hmac_package_signer.rs` (deleted) |
| 2 | HMAC verification path in the deserializer + "missing field means V1" allowances | `package_deserializer.rs` |
| 3 | V1 acceptance in the import security gate (data kinds and `.unit`) | `commands/import_export.rs` |
| 4 | `schema_version` compatibility window for V1 | `application/sync/compatibility` (`MIN_SUPPORTED = V2`) |
| 5 | Trusted-signer enforcement in the production environment gate | `infrastructure/security/mod.rs` |
| 6 | `GRPC_ENFORCE_TRUSTED_SIGNERS`, `GRPC_TRUSTED_SIGNER_IDS`, `GRPC_ACCEPTED_SIGNING_KEY_IDS`, `GRPC_DEPRECATED_SIGNING_KEY_IDS`, `GRPC_SIGNING_KEY_DEPRECATION_DEADLINE_UTC` | environment read path |
| 7 | `resolve_accepted_verification_key_ids`, `is_signing_key_deprecated`, `resolve_trusted_signer_ids`, `should_enforce_trusted_signers`, `is_trusted_signer` | `infrastructure/security/mod.rs` |
| 8 | `SyncSecurityDiagnostics` fields: `accepted_verification_key_ids`, `deprecated_signing_key_ids`, `deprecation_deadline_utc`, `enforce_trusted_signers`, `trusted_signer_ids` | `models/dto.rs` + `src/lib/types.ts` |
| 9 | No-op V1 bypasses in the identity verification service | `sync_package_identity_verification_service.rs` |
| 10 | V1 golden fixtures (`tests/snapshots/sync/*_package_plain.json`) | deleted; replaced by V2 roundtrip + rejection tests |

**Leftover environment variables:** `GRPC_PACKAGE_SIGNING_KEY` and
`GRPC_ACTIVE_SIGNING_KEY_ID` remain in use (fiscal closure, §8). The removed
variables (§3 rows 6) may still exist in external environments; they are
**ignored** and must NOT cause the application to fail merely because they are
unknown/unused.

# 4. Mandatory V2 Contract (All Layers, Fail-Closed)

An accepted sync package MUST satisfy, on every import path (data kinds,
`.unit`, trust, registry, identity_access):

1. `schema_version` within the supported window — `MIN_SUPPORTED = V2`;
2. `integrity_hash` present and matching SHA-256 over canonical bytes
   (ADR-0009);
3. `signature_version == 2` (Ed25519) — any other value or absence is a
   rejection;
4. `signature` present and non-empty;
5. `issuer_identity_id` present — package attributable to a node identity;
6. `package_sequence` present — per-issuer transport ledger participation;
7. Ed25519 signature verified against the issuer's certificate public key
   from the Identity Store (ADR-0038/0046), with ACTIVE status and valid
   `not_after`;
8. `.unit` bootstrap additionally requires the WILAYA-only dedicated path
   (ADR-0044) with fixed sequence 1, anchor-first, and the source pin
   (SEC-003-05-D).

Enforcement points (all reject, none bypass):

- `package_deserializer.rs` — metadata contract (1–4);
- `import_validation.rs` + `compatibility` — schema window (1);
- `commands/import_export.rs` — security gate (1–6), per kind;
- `sync_package_identity_verification_service.rs` — (7), including the
  kind-scoped UNIT issuer policy (ADR-0046) and `.unit` WILAYA gate.

# 5. No-Migration Decision

- **No DB migration:** existing columns (`sync_applied_packages`
  `package_sequence` / `issuer_identity_id`, and all package metadata fields)
  remain nullable in the schema; every production import now populates them
  (V2-only). Nullability is schema stability, not a V1 allowance.
- **No package-format migration:** V1 files on disk are rejected at read time
  with a fail-closed error; no automatic conversion is provided. Operators
  must re-export data through the V2 pipeline.

# 6. Deployment Guidance

- `GRPC_PACKAGE_SIGNING_KEY` remains **mandatory in production** — but now
  exclusively for the fiscal closure package (`.fiscal-close.sync`), whose
  HMAC envelope still requires it (§8). Sync packages no longer consume it.
- `GRPC_ACTIVE_SIGNING_KEY_ID` remains read for the fiscal closure export
  metadata and the export-reproducibility audit field.
- Removed variables may be left in place; they are ignored.
- Deployment readiness check `D_signing_key` remains active and reflects the
  fiscal closure requirement.

# 7. Testing Contract

- Rejection tests: V1-shaped package rejected by the deserializer
  (products/daily/monthly), `signature_version = 1` rejected by the identity
  verification service, V1 data-kind and `.unit` packages rejected by the
  import gate, `PACKAGE_TOO_OLD` for `schema_version < V2`.
- V2 roundtrip determinism with a fixed Ed25519 secret (RFC 8032 determinism).
- Full regression: `cargo test` (workspace), `vitest`, Playwright, plus the
  governance gates (check:arch zero warnings, check_docs_governance,
  check_secrets, check_release_integrity).
- The compatibility window tests were rewritten to assert `V2` as the minimum.

# 8. Temporary Exception: Fiscal Closure HMAC Envelope (`[arch:allow-hmac-fiscal]`)

The fiscal closure authorization package (`.fiscal-close.sync`) is a
**separate production fiscal envelope** with its own versioning
(`FISCAL_CLOSURE_PACKAGE_VERSION = 2`, unrelated to sync schema V2). It signs
fiscal authorization data with the fleet HMAC key and is consumed by
`fiscal_closure_package_service.rs` (export/verify), `export_reproducibility_helper.rs`
(audit field), and `deployment_readiness_service.rs` (`D_signing_key`).

Under SEC-007 the sync V1 machinery is removed, but the fiscal closure HMAC
dependencies are **kept intact** under the tag `[arch:allow-hmac-fiscal]`:

- `resolve_package_signing_key_32_impl` / `resolve_package_signing_key_32`
  (including the zero-key debug fallback);
- `resolve_active_signing_key_id`;
- the `GRPC_PACKAGE_SIGNING_KEY` / `GRPC_ACTIVE_SIGNING_KEY_ID` environment
  reads and the production gate clauses that validate them;
- the `D_signing_key` deployment readiness check;
- `SyncSecurityDiagnostics.has_package_signing_key_env` /
  `active_signing_key_id`.

**Expiry:** 90 days from 2026-08-18 (i.e. 2026-11-16). Registered in
`adr_exception_registry.md`. **Follow-up:** ADR-0048 (separate mission) will
migrate the fiscal closure package to the Ed25519 identity chain
(Root → WILAYA signing identity) and remove this exception. The follow-up is
also tracked in the exception registry row.

# 9. Implementation Boundary (Binding)

Allowed to change (already applied by SEC-007):

- `infrastructure/sync/packages/signing/hmac_package_signer.rs` (deleted) and
  `signing/mod.rs` re-exports;
- `infrastructure/sync/packages/package_deserializer.rs`;
- `application/sync/compatibility/mod.rs`;
- `commands/import_export.rs` (gate + `.unit` path);
- `application/services/sync_package_identity_verification_service.rs`;
- `infrastructure/security/mod.rs` (removals + fiscal-closure retagging);
- `application/services/deployment_readiness_service.rs` (message only);
- `models/dto.rs` + `src/lib/types.ts` (`SyncSecurityDiagnostics`);
- `repositories/sync_applied_packages.rs`, `application/sync/package_metadata.rs`,
  `application/sync/import_provenance.rs` (comments/fixtures only);
- test fixtures: `tests/sync_package_kinds_parse_tests.rs`,
  `tests/sync_package_import_parse.rs`, `tests/sync_v2_contract_fixtures.rs`,
  deleted V1 goldens;
- docs: this ADR, `ADR_INDEX.md`, `adr_exception_registry.md`,
  `security/THREAT_MODEL.md`, `runbooks/security-production-keys.md`,
  `runbooks/trial-deployment-checklist.md`.

Forbidden (unless a separate decision explicitly authorizes):

- removing the fiscal closure HMAC dependencies listed in §8 before ADR-0048;
- any migration of V1 data or packages (no conversion path);
- weakening any V2 acceptance predicate from §4.

# 10. Governance Records

- SEC-007 analysis report (Q1–Q10) precedes this ADR.
- The V1/HMAC window clauses in ADR-0003, ADR-0007, ADR-0038, ADR-0044,
  ADR-0046 and RFC `2026-08-04-node-identity-trust` are **historical** from
  the acceptance date of this ADR; where they conflict with §3–§4, this ADR
  takes precedence (per AGENTS.md §13).
