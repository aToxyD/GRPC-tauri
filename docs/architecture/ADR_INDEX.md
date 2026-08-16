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
| 0041 | `0041-production-app-key-provisioning.md` | Production Application-Key Provisioning — GRPC_APP_KEY via Passphrase-Protected Store | Accepted | 2026-08-08 | Architecture / Security |
| 0042 | `0042-licensing-consumer-integration.md` | Licensing Consumer Integration — Signed Licensing Artifact Consumption in GRPC | Accepted | 2026-08-08 | Architecture / Licensing Integration |
| 0043 | `0043-backup-async-exception.md` | Backup Command Async Exception (Permanent) — narrow carve-out to the no-async freeze for `spawn_blocking` in `commands/backup.rs` | Accepted | 2026-08-09 | Architecture |
| 0044 | `0044-unit-trust-first-v2-bootstrap.md` | `.unit` Bootstrap Migration — Trust-First V2 (Ed25519) / HMAC-V1 Retirement (amends RFC 2026-08-04-node-identity-trust §3.10/§3.12, supersedes ADR-0008) | Accepted | 2026-08-14 | Architecture / Security |
| 0045 | `0045-b8-first-identity-access-import.md` | B8 First `identity_access` Import — Bootstrap Authorization Exemption (companion to ADR-0044 §12.1; proposed amendment to ADR-0040) | Accepted | 2026-08-14 | Architecture / Security |
| 0046 | `0046-unit-to-wilaya-data-sync-trust.md` | UNIT → WILAYA Data Sync Trust — Kind-Scoped UNIT Issuer Acceptance for `stock_movements`/`daily_report`/`monthly_summary` (amends RFC 2026-08-04-node-identity-trust §3.10, SEC-003-01 scope) | Accepted | 2026-08-16 | Architecture / Security |

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
