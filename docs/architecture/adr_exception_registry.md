# ADR Exception Registry — GRPC-Tauri

All `[arch:allow-*]` architectural exceptions must be registered here with ADR linkage, rationale, creation date, and expiration/review date.

**Policy:** Every exception expires 90 days after creation (per ARCHITECTURAL_INVARIANTS.md). Expired exceptions without a documented extension are violations.

---

## Exception: `[arch:allow-utc-now]`

| # | File | Line | Rule | ADR | Reason | Created | Expires | Owner |
|---|------|------|------|-----|--------|---------|---------|-------|
| 1 | `application/services/system_stats_service.rs` | 56 | 96 | ADR-007 | Stats report current month/year — inherently time-dependent operational metric | 2026-05-29 | 2026-08-27 | Architecture |
| 2 | `application/services/system_stats_service.rs` | 219 | 96 | ADR-007 | Stats report today's orders — inherently time-dependent operational metric | 2026-05-29 | 2026-08-27 | Architecture |
| 3 | `application/services/fiscal_closure_package_service.rs` | 387 | 1 | ADR-007 | Execution window validation — pre-flight check comparing against current time | 2026-05-29 | 2026-08-27 | Architecture |
| 4 | `infrastructure/security/mod.rs` | 218 | none | ADR-007 | Signing key deprecation deadline check — time-dependent security gate | 2026-05-29 | 2026-08-27 | Security |
| 5 | `repositories/orders.rs` | 54 | 1 | ADR-005 | Fallback fiscal year when order_date parsing fails — safe default for interoperability | 2026-05-29 | 2026-08-27 | Architecture |
| 6 | `domain/validation.rs` | 270 | none | ADR-007 | Date validation against current day — inherently time-dependent domain validation | 2026-05-29 | 2026-08-27 | Architecture |
| 7 | `errors/mod.rs` | 736 | none | ADR-007 | Fallback datetime when parsing empty string — error-handling utility | 2026-05-29 | 2026-08-27 | Architecture |
| 8 | `application/services/sync_import_execution_service.rs` | 46 | 96 | ADR-014 | Timestamp for sync package rejection audit entry | 2026-05-28 | 2026-08-26 | Sync |
| 9 | `application/services/sync_import_execution_service.rs` | 118 | 96 | ADR-014 | Timestamp for sync conflict audit entry | 2026-05-28 | 2026-08-26 | Sync |

## Exception: `[arch:allow-mutation-before-replay]`

| # | File | Line | Rule | ADR | Reason | Created | Expires | Owner |
|---|------|------|------|-----|--------|---------|---------|-------|
| 10 | `application/services/sync_import_execution_service.rs` | 43 | 5 | ADR-014 | Pre-existing legacy method — mutation must occur before replay check for idempotent import flow | 2026-05-28 | 2026-08-26 | Sync |
| 11 | `application/services/sync_import_execution_service.rs` | 105 | 5 | ADR-014 | Pre-existing legacy method — mutation before replay in conflict handling | 2026-05-28 | 2026-08-26 | Sync |
| 12 | `application/services/sync_import_execution_service.rs` | 143 | 5 | ADR-014 | Pre-existing legacy method — mutation before replay in validation | 2026-05-28 | 2026-08-26 | Sync |
| 13 | `application/services/sync_import_execution_service.rs` | 199 | 5 | ADR-014 | Pre-existing legacy method — mutation before replay check | 2026-05-28 | 2026-08-26 | Sync |
| 14 | `application/services/sync_import_execution_service.rs` | 306 | 5 | ADR-014 | Pre-existing legacy — mutation after replay check but before full validation | 2026-05-28 | 2026-08-26 | Sync |

## Exception: `[arch:allow-unwrap-or]`

| # | File | Line | Rule | ADR | Reason | Created | Expires | Owner |
|---|------|------|------|-----|--------|---------|---------|-------|
| 15 | `application/services/sync_import_execution_service.rs` | 287 | none | ADR-014 | Intended fallback to 0 for non-numeric conflict IDs — safe default | 2026-05-28 | 2026-08-26 | Sync |
| 16 | `application/services/sync_import_execution_service.rs` | 398 | none | ADR-014 | Intended fallback to 0 for non-numeric conflict IDs — safe default | 2026-05-28 | 2026-08-26 | Sync |
| 17 | `application/services/sync_import_execution_service.rs` | 464 | none | ADR-014 | False is safe default for "not imported yet" | 2026-05-28 | 2026-08-26 | Sync |
| 18 | `application/services/sync_import_execution_service.rs` | 488 | none | ADR-014 | False is safe default for "not imported yet" | 2026-05-28 | 2026-08-26 | Sync |
| 19 | `application/services/sync_import_execution_service.rs` | 566 | none | ADR-014 | Test assertion default — false is safe | 2026-05-28 | 2026-08-26 | Sync |
| 20 | `application/services/operation_execution_guard.rs` | 64 | none | ADR-007 | Safe default (0) when no open fiscal year exists — guard degrades gracefully | 2026-05-28 | 2026-08-26 | Architecture |
| 21 | `application/services/operation_execution_guard.rs` | 69 | none | ADR-007 | Safe default (0) when no archived years exist — guard degrades gracefully | 2026-05-28 | 2026-08-26 | Architecture |
| 22 | `application/services/system_health_service.rs` | 118 | none | ADR-007 | 0 bytes safe default when path/metadata unavailable — not error condition | 2026-05-28 | 2026-08-26 | Architecture |

## Exception: `[arch:allow-non-nested]`

| # | File | Line | Rule | ADR | Reason | Created | Expires | Owner |
|---|------|------|------|-----|--------|---------|---------|-------|
| 23 | `application/services/sync_import_execution_service.rs` | 264 | 5 | ADR-014 | Separate code path from handle_rejection — not a nested transaction | 2026-05-28 | 2026-08-26 | Sync |
| 24 | `application/services/sync_import_execution_service.rs` | 386 | 5 | ADR-014 | Separate code path from execute_import — not a nested transaction | 2026-05-28 | 2026-08-26 | Sync |

## Exception: `[arch:allow-sql]`

| # | File | Line | Rule | ADR | Reason | Created | Expires | Owner |
|---|------|------|------|-----|--------|---------|---------|-------|
| 25 | `application/services/sync_import_validation_service.rs` | 31 | 1 | ADR-011 | Pre-existing legacy method — SQL embedded in service before extraction policy was enforced | 2026-05-28 | 2026-08-26 | Sync |

## Exception: `[arch:allow-memory-unsafe]`

| # | File | Line | Rule | ADR | Reason | Created | Expires | Owner |
|---|------|------|------|-----|--------|---------|---------|-------|
| 26 | `infrastructure/backup/sqlite_backup_adapter.rs` | 749 | 31 | ADR-017 | Whole-file read in backup for SHA-256 hashing — required for cryptographic verification | 2026-05-28 | 2026-08-26 | Infrastructure |

---

## Summary

| Tag Type | Count | Primary ADR |
|----------|-------|-------------|
| `[arch:allow-utc-now]` | 9 | ADR-007, ADR-014, ADR-005 |
| `[arch:allow-mutation-before-replay]` | 5 | ADR-014 |
| `[arch:allow-unwrap-or]` | 8 | ADR-014, ADR-007 |
| `[arch:allow-non-nested]` | 2 | ADR-014 |
| `[arch:allow-sql]` | 1 | ADR-011 |
| `[arch:allow-memory-unsafe]` | 1 | ADR-017 |
| **Total** | **26** | |

**Review cadence:** All exceptions must be reviewed at least every 90 days. Expired exceptions must be renewed or closed.
