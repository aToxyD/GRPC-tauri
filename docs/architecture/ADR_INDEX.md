# ADR Index — GRPC-Tauri

## Canonical ADR Location

**All ADRs live under `docs/architecture/`** using the naming convention `NNNN-title.md` (4-digit zero-padded number, kebab-case title).

ADR-001 through ADR-007 have been migrated into the unified numbering below as ADR-0021 through ADR-0027. The canonical source is `docs/architecture/`.

---

## ADR Catalog

### Series 0001–0020: Architecture & Protocol (Legacy, Stabilized)

| ADR | File | Title | Status | Date | Owner |
|-----|------|-------|--------|------|-------|
| 0001 | `0001-read-layer-extraction-policy.md` | Read Layer Extraction Policy | Accepted | 2025-Q4 | Architecture |
| 0002 | `0002-sync-package-boundary.md` | Sync Package Boundary | Accepted | 2025-Q4 | Sync |
| 0003 | `0003-sync-protocol-versioning-and-integrity.md` | Sync Protocol Versioning & Integrity | Accepted | 2025-Q4 | Sync |
| 0004 | `0004-protocol-changes-are-breaking-changes.md` | Protocol Changes Are Breaking | Accepted | 2025-Q4 | Sync |
| 0005 | `0005-canonical-serialization-contract.md` | Canonical Serialization Contract | Accepted | 2025-Q4 | Sync |
| 0006 | `0006-signing-key-rotation-skeleton.md` | Signing Key Rotation Skeleton | Accepted | 2025-Q4 | Security |
| 0007 | `0007-signing-key-deprecation-window-policy.md` | Signing Key Deprecation Window Policy | Accepted | 2025-Q4 | Security |
| 0008 | `0008-trusted-signer-identity-skeleton.md` | Trusted Signer Identity Skeleton | Accepted | 2025-Q4 | Security |
| 0009 | `0009-sync-package-canonical-json-v2.md` | Sync Package Canonical JSON V2 | Accepted | 2025-Q4 | Sync |
| 0010 | `0010-sync-package-only-transport.md` | Sync Package Only Transport | Accepted | 2025-Q4 | Sync |
| 0011 | `0011-unified-architecture-migration.md` | Unified Architecture Migration | Accepted | 2025-Q4 | Architecture |
| 0012 | `0012-production-error-exposure-policy.md` | Production Error Exposure Policy | Accepted | 2025-Q4 | Architecture |
| 0013 | `0013-observability-layer.md` | Observability Layer | Accepted | 2025-Q4 | Infrastructure |
| 0014 | `0014-sync-conflict-intelligence.md` | Sync Conflict Intelligence | Accepted | 2025-Q4 | Sync |
| 0015 | `0015-streaming-encryption-and-verification.md` | Streaming Encryption & Verification | Accepted | 2025-Q4 | Security |
| 0016 | `0016-memory-aware-sync-pipeline.md` | Memory-Aware Sync Pipeline | Accepted | 2025-Q4 | Sync |
| 0017 | `0017-atomic-secure-restore.md` | Atomic Secure Restore | Accepted | 2025-Q4 | Infrastructure |
| 0018 | `0018-fail-closed-authorization.md` | Fail-Closed Authorization | Accepted | 2025-Q4 | Security |
| 0019 | `0019-legacy-crypto-isolation.md` | Legacy Crypto Isolation | Accepted | 2025-Q4 | Security |
| 0020 | `0020-single-instance-runtime-enforcement.md` | Single-Instance Runtime Enforcement | Accepted | 2025-Q4 | Architecture |

### Series 0021–0030: Determinism & Governance (Stabilized)

These ADRs were originally created as ADR-001 through ADR-007 and are cross-referenced here by their unified number.

| ADR | Legacy Ref | File | Title | Status | Date | Owner |
|-----|-----------|------|-------|--------|------|-------|
| 0021 | ADR-001 | `0021-fifo-determinism.md` | FIFO Determinism | Accepted | 2026-05-29 | Domain |
| 0022 | ADR-002 | `0022-domain-event-ordering.md` | Domain Event Ordering | Accepted | 2026-05-29 | Domain |
| 0023 | ADR-003 | `0023-reproducible-reporting.md` | Reproducible Reporting | Accepted | 2026-05-29 | Reporting |
| 0024 | ADR-004 | `0024-audit-dual-write.md` | Audit Dual-Write | Accepted | 2026-05-29 | Audit |
| 0025 | ADR-005 | `0025-sqlite-single-writer.md` | SQLite Single-Writer | Accepted | 2026-05-29 | Infrastructure |
| 0026 | ADR-006 | `0026-no-async-runtime.md` | No Async Runtime | Accepted | 2026-05-29 | Architecture |
| 0027 | ADR-007 | `0027-runtime-evaluation-purity.md` | Runtime Evaluation Purity | Accepted | 2026-05-29 | Architecture |

### Series 0028+: Service Governance, Exception Policy (New)

| ADR | File | Title | Status | Date | Owner |
|-----|------|-------|--------|------|-------|
| 0028 | `0028-service-size-governance.md` | Service Size Governance Policy | Accepted | 2026-05-29 | Architecture |
| 0029 | `0029-secondary-sqlite-connections.md` | Secondary SQLite Connection Policy | Accepted | 2026-05-29 | Infrastructure |
| 0030 | `0030-adr-exception-governance.md` | ADR Exception Governance Policy | Accepted | 2026-05-29 | Architecture |

### Series 0038+: Identity & Trust (New)

| ADR | File | Title | Status | Date | Owner |
|-----|------|-------|--------|------|-------|
| 0038 | `0038-node-identity-and-trust.md` | Node Identity & Trust Architecture | Accepted | 2026-08-04 | Architecture / Security |
| 0039 | `0039-adminkey-portable-scrypt.md` | Two-Tier Secret Protection (x25519 node / scrypt `.adminkey`) | Accepted | 2026-08-04 | Architecture / Security |
| 0040 | `0040-identity-access-sync.md` | Identity & Access Synchronization — Two Static Accounts per UNIT | Accepted | 2026-08-07 | Architecture / Security |
| 0041 | `0041-production-app-key-provisioning.md` | Production Application-Key Provisioning — GRPC_APP_KEY via Passphrase-Protected Store (amended 2026-08-19 §11 post-provisioning startup model; amended 2026-08-20 §11.4 OS keyring source IMPLEMENTED — SEC-013 Phase 2) | Accepted | 2026-08-08 | Architecture / Security |
| 0042 | `0042-licensing-consumer-integration.md` | Licensing Consumer Integration — Signed Licensing Artifact Consumption in GRPC | Accepted | 2026-08-08 | Architecture / Licensing Integration |
| 0043 | `0043-backup-async-exception.md` | Backup Command Async Exception (Permanent) — narrow carve-out to the no-async freeze for `spawn_blocking` in `commands/backup.rs` | Accepted | 2026-08-09 | Architecture |
| 0044 | `0044-unit-trust-first-v2-bootstrap.md` | `.unit` Bootstrap Migration — Trust-First V2 (Ed25519) / HMAC-V1 Retirement (amends RFC 2026-08-04-node-identity-trust §3.10/§3.12, supersedes ADR-0008) | Accepted | 2026-08-14 | Architecture / Security |
| 0045 | `0045-b8-first-identity-access-import.md` | B8 First `identity_access` Import — Bootstrap Authorization Exemption (companion to ADR-0044 §12.1; proposed amendment to ADR-0040) | Accepted | 2026-08-14 | Architecture / Security |
| 0046 | `0046-unit-to-wilaya-data-sync-trust.md` | UNIT → WILAYA Data Sync Trust — Kind-Scoped UNIT Issuer Acceptance for `stock_movements`/`daily_report`/`monthly_summary` (amends RFC 2026-08-04-node-identity-trust §3.10, SEC-003-01 scope) | Accepted | 2026-08-16 | Architecture / Security |
| 0047 | `0047-sync-v1-hmac-removal.md` | Sync Package V1/HMAC Removal — V2-Only (Ed25519) Enforcement (SEC-007; supersedes V1 window clauses of ADR-0003/0007/0038/0044/0046; fiscal closure HMAC exception `[arch:allow-hmac-fiscal]`, follow-up ADR-0048) | Accepted | 2026-08-18 | Architecture / Security |
| 0048 | `0048-fiscal-closure-ed25519-migration.md` | Fiscal Closure Ed25519 Migration — Identity-Bound Signing (SEC-008; closes exception #29 `[arch:allow-hmac-fiscal]` of ADR-0047; removes `GRPC_PACKAGE_SIGNING_KEY` / `GRPC_ACTIVE_SIGNING_KEY_ID`) | Accepted | 2026-08-18 | Architecture / Security |
| 0049 | `0049-trust-package-verification.md` | Trust Package Verification Hardening — WILAYA-Pinned, Root-Verified, Anchor-Immutable (SEC-010; eliminates SEC-009-01 payload-forgery path; closes trust-package UNIT/ADMIN distribution and cross-issuer revocation) | Accepted | 2026-08-19 | Architecture / Security |
| 0050 | `0050-wilaya-admin-normal-authentication-model.md` | WILAYA Admin Normal Authentication Model — B6-B Reversal for Normal Login; `.adminkey` Retained as Recovery/High-Assurance Mechanism (SEC-013 Phase 0; supersedes the B6-B normal-login clause of ADR-0038) | Accepted | 2026-08-19 | Architecture / Security |
| 0051 | `0051-admin-access-package.md` | Admin-Only B8 Account Synchronization — `admin_access` Security-Critical Package Kind; Fleet-Wide Admin Sync Without UNIT Target; UNIT-Operator Account Preservation Invariant; Legacy `identity_access` Cutover (D1) (SEC-019/SEC-020 owner ratification; partially supersedes the UNIT-user synchronization semantics of ADR-0040) | Accepted | 2026-08-22 | Architecture / Security |
| 0052 | `0052-canonical-unit-operator.md` | Canonical UNIT Operator Identity & Node-Scoped Account Uniqueness — server-derived `user` operator, `UNIQUE(username, node_id)` baseline correction (no migration), rename-capability removal, role-aware login identity selection (SEC-023/024/025 → SEC-026; completes the ADR-0051 UNIT-operator preservation invariant) | Accepted | 2026-08-23 | Architecture / Security |
| 0053 | `0053-unified-per-target-transport-sequence.md` | Unified Per-Target Transport Sequence — ONE canonical producer allocator `(issuer_identity_id, target_node_id)` across all TransportGuard pipeline kinds; consumer guard byte-frozen; retires fragmented 006/009/010 producer streams (migration 011); controlled pre-release reset; voids pre-SEC-031 artifacts (SEC-030/031 → SEC-032; amends RFC §3.4.1 producer side, supersedes ADR-0045 §26.9 pattern + ADR-0051 §7 stream) | Accepted | 2026-08-24 | Architecture / Security |
| 0055 | `0055-contract-centric-procurement.md` | Contract-Centric Procurement — Supplier first-class with UNIT M:N; Contract as pricing + entitlement authority; per-UNIT obligations; WILAYA-only obligation release; Old-obligation priority (X → Y); one ACTIVE entitlement per (UNIT, Product, FY) via materialized entitlement_state + SQLite-valid partial unique index; full-order rejection on over-request/mixed-supplier; backend-authoritative pricing (no base_price fallback); TVA one immutable rate per FY; ContractCatalog V2 additive sync kind; single-baseline edit, no migrations; trust/SEC-009 unchanged (SEC-087-C/D/E → SEC-087-F). **Partially superseded by ADR-0056 (§3.5 no-split clause, §3.6 no-automatic-split clause); "one supplier per SupplierOrder" retained** | Accepted | 2026-09-01 | Architecture / Security |
| 0056 | `0056-multi-supplier-request-splitting.md` | Multi-Supplier Request Splitting & Portion-Granularity Order Items — automatic per-supplier split of a CreateOrderRequest; at most one SupplierOrder per supplier; one SupplierOrderItem per allocation portion; same-product multi-item rows within one SupplierOrder permitted for same-supplier multi-allocation; every item has exactly one allocation leg; greedy-drain portion selection (oldest eligible allocation to full effective remaining); confirmation order-independent by construction (own-reservation netting); explicit deterministic confirmation-query ORDER BY (allocation priority key + item-id tie-break); portions materialized in resolver-priority order; whole-request atomicity via single AuditTxService transaction; current-FY header anchor for all produced orders; shared non-unique reference_number; no schema change; Phase 4A confirmation semantically unchanged; sync/eligibility/authorization unchanged; partially supersedes ADR-0055 §3.5/§3.6 (SEC-087 Phase 4B) | Accepted | 2026-09-09 | Architecture |
| 0057 | `0057-sync-protocol-v3-schema-gate.md` | Sync Protocol V3 Schema Gate — V3-Only Import Compatibility Window (SEC-087 Phase 6A) — envelope-level `schema_version` V3 cutover with a closed `[V3, V3]` import window enforced by a single kind-blind pipeline gate replacing the per-kind window checks; `SYNC_PACKAGE_SCHEMA_VERSION` = V3; V0/V1/V2 → `PACKAGE_TOO_OLD`, V4+ → `PACKAGE_TOO_NEW`; V3 products payload contract RATIFIED (config REQUIRED: `purchase_unit` / `consumption_unit` / `conversion_factor` / `tva_classification`; fail-closed via `validate_product_units`; enforced before Product mutation in Phase 6B; 6A-era config-free packages = non-conforming pre-release artifacts); no V4; no timestamp / build / version / heuristic boundary; transport sequencing remains permanently retired (SEC-056D/SEC-057); replay protection = exact `package_id` dedup; `.unit` bootstrap path untouched (distinct artifact, exempt from the pipeline gate); amends ADR-0047 import-window clauses and the ADR-0003 window clause; complies with ADR-0004 steps 1–4; Phase 6A does NOT authorize Product payload/import implementation (SEC-087 Phase 6B/6C remain separate workstreams) | Accepted | 2026-09-11 | Architecture / Security |
| 0058 | `0058-product-unit-config-immutability.md` | Product Unit-Config Immutability After First Stock Movement — the Product unit/factor tuple (`purchase_unit` / `consumption_unit` / `conversion_factor`) is FROZEN once a `stock_movements` row exists for the product; trigger = `EXISTS(stock_movements WHERE product_id)`; a V3 Product record changing any frozen field on a product with movement history rejects the ENTIRE sync package, fail-closed and pre-mutation, at the single Product V3 import gate (`validate_products_package_for_import`); invariant applies to every future write path (currently reachable production mutation path = V3 sync upsert); `tva_classification` remains under existing fiscal-year governance (ADR-0055 §3.9), `base_price`/`name` not frozen; no schema migration, no new envelope version, no inventory re-keying/merging, no partial package application (SEC-087 Phase 6C follow-on; extends ADR-0057 §3.4 enforcement gate; ADR-0055/0056 precedents) | Accepted | 2026-09-12 | Architecture / Security |
| 0059 | `0059-unit-scoped-contract-catalog-export.md` | Unit-Scoped Contract Catalog Export — explicit export mode model `ExportContractCatalogMode` (`FleetRestore` / `UnitDistribution { target_unit_code }`), eliminating the `Option<TargetUnit>` ambiguity; target identity = `units.code` via the fail-closed `resolve_unit_transport_target` (never `units.id`/filename/renderer label); per-UNIT status policy `Accepted`/`Active`/`Ended` with `Ended(B)` never in unit A's artifact (unit scope trumps Ended eligibility) vs FleetRestore all-status retention (separate policy); additive `SyncPackageMetadata.target_node_id` (serde `default` + `skip_serializing_if = None`) with `SYNC_PACKAGE_SCHEMA_VERSION` kept **V3** (no bump; ADR-0057 closed `[V3,V3]` window; FleetRestore canonical bytes unchanged, UnitDistribution intentional target-bearing bytes); zero-contract UNIT artifact; shared `GRPC_APP_KEY` age encryption retained, `target_node_id` = authenticity not confidentiality, isolation by payload selection; importer target matrix enforced (UNIT A + A.code accept; UNIT A + B.code/None reject; WILAYA + None accept; WILAYA + A.code reject); SQL `?1` binding hardening; no migration / no V4 / no transport reactivation — **implementation complete as of C1–C4** (metadata field + serde canon-signature coverage C1 `26b6dd8`; export reproducibility persistence C2 `54d52bf`; exporter wiring / input wrapper / SQL hardening C3 `917f0ad`; importer target binding + empty-`UnitDistribution` allowance C4 `f14ca73`) (SEC-087-F follow-on; extends ADR-0055 §3.10; complies with ADR-0057/0041; ADR-0053/SEC-056D transport retirement). **Partially superseded by ADR-0060 (Contract Catalog FleetRestore form clauses only — the `FleetRestore` export-mode variant/status policy/WILAYA+None import acceptance are retired; the `UnitDistribution` model is retained)** | Accepted | 2026-09-17 | Architecture / Security |
| 0060 | `0060-retire-contract-catalog-fleet-restore.md` | Retire the Contract Catalog FleetRestore Export Form — `ExportContractCatalogMode` becomes single-form (`UnitDistribution { target_unit_code }` only); Contract Catalog `FleetRestore` variant/producer (`export_contract_catalog_fleet`)/command (`export_contract_catalog_package` + registry entry)/`execute_fleet` removed; importer rejects `PackageExportMode::FleetRestore` for `contract_catalog` packages UNCONDITIONALLY on every node, with or without `target_node_id` (ADR-0059 §13 WILAYA+None ACCEPT cell closed); `Action::ExportContractCatalogPackage` retained as the authz guard for the sole `export_contract_catalog_to_units` command; generic `PackageExportMode::FleetRestore`/fleet loop for products and admin access untouched; no schema/envelope change/migration; legacy FleetRestore artifacts rejected fail-closed, never reinterpreted; **partially supersedes ADR-0059** (§3 `FleetRestore` variant + invariant 3, §7 FleetRestore Status Policy, §13 WILAYA+None ACCEPT cell, §19 testing item 2, §10 fleet empty-catalog references — UnitDistribution/target identity/status policy/target_node_id serde/V3/zero-contract/SQL hardening retained) | Accepted | 2026-09-24 | Architecture / Security |
| 0061 | `0061-unit-wilaya-allocation-fulfillment-state-sync.md` | UNIT → WILAYA Allocation-Level Fulfillment State Synchronization — new UNIT-issued sync kind `contract_fulfillment` (ADR-0046 allowlist extended under ADR-0051 conditions: V2-only, Ed25519, ACTIVE non-expired cert, WILAYA-only importer, membership + `unit_id` binding); `FulfillmentFact` is an **allocation-level cumulative state snapshot**, NOT a historical event; synchronization identity = `allocation_id` + absolute cumulative `fulfilled_quantity` (not a random UUID, not a `SupplierOrder` id); `order_id`/`order_item_id`/`fact_id` absent from the payload — the repo persists **no durable historical fulfillment ledger** (leg table is a pricing/reservation record; `try_increment_fulfilled` is a legless path), so per-order causality is **intentionally not reconstructed**; `fulfilled_quantity` is **purchase-unit** (proven from `order_service.rs:685` pre-conversion write path), unit triple carried as fail-closed cross-check, no unit column added to `contract_allocations`; fiscal attribution = `contract_allocations.fiscal_year` as a **cross-check, never an application input** (older-obligation X→Y fulfilment preserved); resolution filters `deleted=0` only, **never** `entitlement_state` (ADR-0055:83 "historical fulfillment is never erased"), no re-keying/re-parenting/archival; export = **complete current set**, no watermark/transport-sequence/per-fact-ledger migration — a deliberate choice **coupled** to the state-based idempotency that it licenses; idempotency = content-derived `package_id` (`sha256(canonical_json(dataset))`) + guarded monotone absolute `SET` (`fulfilled_quantity < ?absolute`), giving at-least-once delivery + at-most-once effect + order-insensitive convergence (**not** "exactly-once"); import all-or-nothing fail-closed, all facts validated pre-mutation; new domain-owned `wilaya_executable_remaining = contracted − fulfilled − released` on the WILAYA-only `ContractAllocationView`, never includes `reserved_quantity`; `effective_remaining` **unchanged**; `UnitContractEntitlement` deliberately not given the field (single owner); reuse `IdentitySignedExportService` (only change to existing prod code = optional caller-supplied `PackageId` in `build_and_write`, defaulting to `Uuid::new_v4()`); `DATA_PACKAGE_KINDS` registration mandatory (`import_export.rs:194` gates V2 enforcement on it); V3 unchanged, no schema change/migration/new table/new column; extends ADR-0055 §3.10/§3.5, complies with ADR-0056/0057/0058/0059/0060, amends ADR-0046 allowlist | Accepted | 2026-09-26 | Architecture / Security / Sync |

| 0062 | `0062-identity-data-dir-override.md` | Identity Data-Directory Override `GRPC_IDENTITY_DATA_DIR` — one `infrastructure/`-owned resolver for identity/secret state (`node_identity.key` + `node_identity.key.pending`, `.adminkey`, `appkey.age`); precedence `GRPC_IDENTITY_DATA_DIR` (empty/whitespace = unset, relative → `AppError::Configuration`, never resolved against the CWD) → `dirs::data_dir()/GRPC`; honored in **all** profiles and never gated on `GRPC_ENV`; process-lifetime-stable resolution with no production reset; fail-closed point-in-time guard against an empty override against a populated platform default (no content comparison, no cryptographic cross-check, no cross-process lock); converges identity sites A/B/C/F/G only — logs (D), backup discovery (E), and `GRPC_DB_PATH` (H) unchanged; no new `age::scrypt` site, no secret format/filename/custody change; enables hermetic Windows E2E identity bootstrap (ceremony not yet executed) | Accepted | 2026-09-30 | Architecture / Security |

---

## Cross-Reference Map

The following legacy ADRs have been consolidated:

| Legacy Ref | Unified Number | Canonical File |
|-----------|---------------|----------------|
| ADR-001 | ADR-0021 | `0021-fifo-determinism.md` |
| ADR-002 | ADR-0022 | `0022-domain-event-ordering.md` |
| ADR-003 | ADR-0023 | `0023-reproducible-reporting.md` |
| ADR-004 | ADR-0024 | `0024-audit-dual-write.md` |
| ADR-005 | ADR-0025 | `0025-sqlite-single-writer.md` |
| ADR-006 | ADR-0026 | `0026-no-async-runtime.md` |
| ADR-007 | ADR-0027 | `0027-runtime-evaluation-purity.md` |

---

## Policy

1. **Canonical location:** `docs/architecture/NNNN-title.md`
2. **Numbering:** Sequential, 4-digit zero-padded (0028, 0029, ...)
3. **Statuses:** Draft → Proposed → Accepted → Deprecated → Superseded
4. **All ADRs must have:** status, date, owner
5. **Superseded ADRs:** Must reference `superseded_by` in front matter

---

## Unused / Reserved Numbers

| ADR Range | Purpose |
|-----------|---------|
| 0031 | `0031-rate-limiter-persistence.md` | Rate Limiter Persistence | Accepted | 2026-06-03 | Infrastructure |
| 0032–0037 | CANCELLED / DEFERRED | Not implemented in v1.2.0 baseline | — | — | — |
| 0044–0099 | Available for future governance ADRs |
| 0100–9999 | Available for domain/feature ADRs |
