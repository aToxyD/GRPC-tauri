# ADR 0046: UNIT → WILAYA Data Sync Trust — Kind-Scoped UNIT Issuer Acceptance (DESIGN B)

# Decision Status

**ACCEPTED — 2026-08-16 (implementation-authorized ratification: SYNC-005 DESIGN B)**

This ADR ratifies DESIGN B of the SYNC-001/SYNC-002 remediation: UNIT-issued V2
(Ed25519) data packages are intentional architecture for exactly three data
kinds, imported by the same-WILAYA coordinator node under strict membership and
issuer↔payload binding constraints. All other kinds remain WILAYA-only.
Implementation is authorized by the SYNC-005 directive and bounded by §9.

| Item | Status |
|------|--------|
| UNIT → WILAYA data-package signing (stock_movements / daily_report / monthly_summary) | **RATIFIED** |
| products / identity_access / trust / registry / `.unit` remain WILAYA-only | **RATIFIED** |
| Membership predicate (`cert.subject_id → units → wilaya_code == settings.wilaya_code`) | **RATIFIED (mandatory)** |
| Issuer ↔ import-target binding (`import unit_id == cert.subject_id`, post-signature) | **RATIFIED (mandatory)** |
| stock_movements payload binding (signed `unit_id`, when present/non-empty, == `cert.subject_id`) | **RATIFIED (mandatory)** |
| Signature-first ordering (no subject_id-derived authority before Ed25519 authentication) | **RATIFIED (mandatory)** |
| SYNC-001-03 trust-install issuer hardening | **CLASSIFIED defense-in-depth (optional, not in scope)** |
| V1 legacy data-package behavior | **OUT OF SCOPE (unchanged)** |
| Schema migration / package format / crypto / B8 / AppKey / authz matrix | **NOT CHANGED** |

# Date

2026-08-16

# Relation to Existing Documents

| Document | Relation |
|----------|----------|
| RFC `2026-08-04-node-identity-trust.md` | **Amended (minimal, §3.10)** — SEC-003-01 issuer-subject scope: UNIT issuers accepted for the three data kinds on WILAYA importers with membership + binding. |
| ADR-0003 (signature versioning) | **Unchanged** — `signature_version = 2` / Ed25519 / canonical bytes unchanged. |
| ADR-0038 (node identity & trust) | **Unchanged** — Identity Store remains the single source of truth; no new trust authority; no identity-store WILAYA dimension. |
| ADR-0039 (crypto) | **Unchanged** — no algorithm change; no new key material. |
| ADR-0040 / ADR-0045 (identity_access / B8) | **Unchanged** — `identity_access` remains WILAYA-only; B8 first-import untouched. |
| ADR-0041 (App Key) / ADR-0042 (licensing) | **Unchanged** — AppKey/encryption untouched. |
| ADR-0044 (`.unit` Trust-First V2) | **Unchanged** — `.unit` remains WILAYA-only, anchor-first, fixed sequence 1; the dedicated path never reaches the new UNIT branch. |
| `ARCHITECTURE_FREEZE.md` §2.7 | **Amended (minimal)** — §2.7 records this ratified exemption (pattern of ADR-0044/0045 entries). |

# 1. Executive Summary

UNIT nodes sign their own data packages (stock_movements, daily_report,
monthly_summary) with their own Ed25519 identity (`IdentitySignedExportService`,
`SubjectType::Unit`). The WILAYA import pipeline previously rejected **every**
non-WILAYA issuer (SEC-003-01 gate in `sync_package_identity_verification_service.rs`),
breaking the legitimate UNIT → WILAYA data-sync direction (SYNC-001-01, HIGH).

DESIGN B replaces the unconditional WILAYA-only issuer rule with a
**kind-scoped** UNIT issuer policy: a UNIT certificate is accepted only for the
three data kinds, only when the importer node is a WILAYA node, only after
Ed25519 signature authentication, only when `cert.subject_id` resolves to a
local `units` row in the importer's WILAYA, and only when the import target and
every signed stock-movement payload `unit_id` equal the authenticated
`cert.subject_id`. Critical kinds (identity_access, trust, registry, `.unit`)
and `products` remain WILAYA-only. The implementation is confined to the
verification service, the import pipeline context passing, a new small
application-layer membership helper, tests, and this governance record.

# 2. Problem Statement

See SYNC-001 (discovery) and SYNC-002 (design) for the full evidence trail.
Root cause: `validate_v2_issuer_certificate` rejects any certificate with
`subject_type != Wilaya` before signature verification, so UNIT-issued V2 data
packages (produced legitimately by `export_subject_type(NodeType::Unit)`) are
rejected with «المُصدِر ليس عقدة ولاية (WILAYA)» at `import_export.rs:1589`.

# 3. Security Invariants (Ratified)

## I1 — Kind-scoped UNIT authority

A UNIT certificate does NOT gain generic signing authority. It gains only
kind-scoped authority to sign its own authenticated data for its own WILAYA:

```text
WILAYA
  ├── identity_access
  ├── trust
  ├── registry
  ├── .unit
  └── products, plus all kinds currently authorized
        │
        ▼
UNIT
  └── signs only:
        ├── stock_movements
        ├── daily_report
        └── monthly_summary
              │
              ▼
        WILAYA importer
```

## I2 — Accepted UNIT-issued kinds (exhaustive)

| Kind | UNIT issuer | WILAYA issuer |
|------|-------------|---------------|
| `stock_movements` | ACCEPTED (with membership + binding) | ACCEPTED (unchanged) |
| `daily_report` | ACCEPTED (with membership + binding) | ACCEPTED (unchanged) |
| `monthly_summary` | ACCEPTED (with membership + binding) | ACCEPTED (unchanged) |
| `products` | **REJECTED** | ACCEPTED (unchanged) |
| `identity_access` | **REJECTED** | ACCEPTED (unchanged) |
| `trust` | **REJECTED** | ACCEPTED (unchanged) |
| `registry` | **REJECTED** | ACCEPTED (unchanged) |
| `.unit` | **REJECTED** | ACCEPTED (unchanged, dedicated path) |

There is no generic «ACTIVE UNIT signs V2 packages» rule. The allowed kind
set is explicit in the verification decision.

## I3 — Acceptance predicate for UNIT-issued data packages (ALL mandatory)

For a UNIT-issued `stock_movements` / `daily_report` / `monthly_summary`
package imported on a WILAYA node:

1. `issuer_identity_id` present in metadata;
2. issuer certificate exists in the Identity Store;
3. importer node is a WILAYA node (`settings.node_type == Wilaya`);
4. certificate `status == Active`;
5. certificate `not_after` is `None` or in the future (advisory verification-time guard, Invariant 8);
6. Ed25519 package signature verifies against the certificate public key over canonical bytes;
7. **only after signature authentication**:
   a. `cert.subject_id` resolves to a local `units` row;
   b. that row's `wilaya_code == local settings.wilaya_code` (trusted local persisted state — never package/renderer/filename metadata);
   c. import target `unit_id == cert.subject_id`;
   d. for `stock_movements`: every signed payload movement `unit_id`, when present and non-empty, equals `cert.subject_id`; genuinely absent/empty values keep the existing restamp semantics, whose authoritative source is now the authenticated certificate subject.

## I4 — Signature-first ordering

No `cert.subject_id`-derived decision (membership, import-target selection,
payload binding, package acceptance) may occur before the package signature has
been successfully authenticated. Resolving the certificate itself is permitted
(the verifier needs it for verification).

## I5 — No authority broadening

- No UNIT ↔ UNIT trust/data authority: UNIT-issued acceptance exists only on
  WILAYA importer nodes (I3.3).
- UNITs do not gain identity issuance authority.
- No UNIT may sign `products`, `identity_access`, `trust`, `registry`, or `.unit`.
- The verifier does not substitute for command authorization (`authorize_reports`
  unchanged; trust/registry imports remain WILAYA-admin only).
- WILAYA remains the sole issuer of UNIT identities (ADR-0038 chain
  Root → WILAYA → UNIT) — unchanged.

# 4. Unchanged Contracts (Explicitly Out of Scope)

| Contract | Status |
|----------|--------|
| Package format / canonical JSON / integrity hash / signature algorithm | UNCHANGED |
| Identity Store schema, `identity_store` repository | UNCHANGED — no WILAYA dimension |
| `TransportGuard` + per-issuer sequence ledger (`issuer_identity_id`, `package_sequence`) | UNCHANGED — the accepted UNIT issuer is just another legitimate per-issuer stream |
| Database migrations | NONE |
| B8 first-import predicates | UNCHANGED |
| App Key / encryption (ADR-0041/0039) | UNCHANGED |
| Authorization matrix (`reports.rs`, `authorize_reports`) | UNCHANGED |
| V1 legacy data-package path | UNCHANGED (out of scope; no UNIT identity semantics introduced into V1) |

# 5. Verification Order (implementation contract)

```text
1. issuer_identity_id present
2. issuer certificate exists
3. package-kind issuer policy (kind-scoped; fail-closed for unknown kinds)
4. importer is WILAYA node (for UNIT issuers)
5. certificate ACTIVE
6. certificate not_after validation
7. Ed25519 package signature verification
8. ONLY IF issuer is UNIT:
      a. membership: cert.subject_id → units.get_unit → wilaya_code == settings.wilaya_code
      b. import unit_id == cert.subject_id
      c. stock_movements: every signed payload unit_id (present, non-empty)
         == cert.subject_id
9. TransportGuard
10. domain validation
11. mutation
12. advance issuer sequence
```

# 6. Trust Model

The trust graph is unchanged (RFC `2026-08-04-node-identity-trust` §3.4):

```text
Root (production-certified) → WILAYA → UNIT certificates
```

UNIT data packages are self-signed by the UNIT identity (the UNIT signs its own
data, not other units' data). Acceptance on the WILAYA importer additionally
requires that the UNIT belong to the importer's WILAYA (membership) and that
the package's target equals the signer (binding). This is NOT a new trust
authority: the Identity Store remains the single source of truth for identity
state (ADR-0038), and the same ACTIVE/not_after/Ed25519 gates apply.

# 7. SYNC-001-03 Classification

The optional trust-install hardening (verify at trust-package install time that
`cert.issuer_identity_id` equals the local ACTIVE WILAYA anchor) is classified
**defense-in-depth** and is NOT part of this implementation. It remains tracked
(SYNC-001-03, LOW/MEDIUM defense-in-depth) and may be hardened by a separate
decision later. This classification does not weaken I3: membership and binding
are enforced on every UNIT-issued import regardless.

# 8. Alternatives Rejected

- **Design A (unconditional ACTIVE-UNIT acceptance)** — rejected: grants generic
  signing authority; violates I1/I5.
- **Design C (WILAYA counter-signature per package)** — rejected: requires
  package-format change and online WILAYA participation; breaks offline-first.
- **Design D (provenance-only protection)** — rejected: provenance is
  defense-in-depth, not an issuer policy; violates §3 (products exclusion must
  be explicit in the issuer policy itself).
- **Keeping the status quo** — rejected: SYNC-001-01 HIGH breaks the governed
  UNIT → WILAYA data direction.

Full comparison: SYNC-002 audit §Design Comparison.

# 9. Implementation Boundary (Binding)

Allowed to change:

- `src-tauri/src/application/services/sync_package_identity_verification_service.rs`
  — kind-scoped issuer policy + post-signature UNIT checks (I3, §5).
- `src-tauri/src/commands/import_export.rs` — pass trusted local context into
  the verification layer: package kind (command constant), local
  `settings.wilaya_code`, `settings.node_type`, import target `unit_id`
  (renderer-supplied, validated against the authenticated certificate), and a
  payload-unit-id extractor for `stock_movements`. Authoritative context never
  originates from package metadata or filenames.
- `src-tauri/src/application/sync/unit_issuer_membership.rs` (new) — smallest
  application-layer membership helper (`verify_unit_issuer_membership`); no SQL
  beyond the existing `units.get_unit` repository call; no schema fields.
- `src-tauri/tests/sync_unit_to_wilaya_import_tests.rs` (new) — real-pipeline
  integration suite.
- Focused unit tests inside the verification service.
- This ADR + RFC §3.10 amendment + `ARCHITECTURE_FREEZE.md` §2.7 entry +
  `ADR_INDEX.md`.

Forbidden (unless a separate decision explicitly authorizes):

- `identity_store.rs`, `identity_provisioning_service.rs`,
  `identity_rotation_coordinator.rs`, `node_package_service.rs`,
  `b8_first_import_predicates_service.rs`, `file_encryption.rs`,
  `package_builder.rs`, `package_deserializer.rs`, `transport_guard.rs`,
  `import_provenance.rs`, `reports.rs` / authorization policies, migrations,
  AppKey implementation, `canonical_json.rs`, `signing.rs`, `Cargo.lock`.

# 10. Migration

NOT REQUIRED. `units.wilaya_code` and `settings.wilaya_code` already exist; the
TransportGuard ledger is already per-issuer and issuer-agnostic; the Identity
Store already holds UNIT certificates with `subject_id = units.id`.

# 11. Testing Contract

Real-pipeline integration tests (see implementation) must cover at minimum:
legitimate UNIT → WILAYA imports of the three data kinds; same-WILAYA
membership; sequence 1 then 2; and rejection of revoked/superseded/expired/
unknown/foreign-WILAYA UNIT issuers, renderer `unit_id != cert.subject_id`,
forged issuer, tampered payload/signature, wrong signed stock-movement
`unit_id`, UNIT-issued `products`/`identity_access`/`trust`/`registry`/`.unit`,
replay, out-of-order, and wrong-WILAYA imports. Existing regression suites
(SEC-001/002/003-02, B8, AppKey, identity_access, trust, registry, `.unit`,
products, idempotency, TransportGuard, multi-unit producer streams) must remain
green.