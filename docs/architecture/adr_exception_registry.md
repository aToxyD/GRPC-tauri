# ADR Exception Registry — GRPC-Tauri

All `[arch:allow-*]` architectural exceptions must be registered here with ADR linkage, rationale, creation date, and expiration/review date.

**Policy:** Every exception expires 90 days after creation (per ARCHITECTURAL_INVARIANTS.md). Expired exceptions without a documented extension are violations.

> **B3-2 renewal (2026-08-09):** Rows #1–7 (decorative `[arch:allow-utc-now]` tags in files not scanned by any wall-clock rule) were closed — tags removed, code unchanged. All remaining rows renewed with inline `Reason/Date/Owner` metadata (`Date: 2026-08-09`) and re-synced to current line numbers. New expiry: 2026-11-07.

## Exception: `[arch:allow-utc-now]`

| # | File | Line | Rule | ADR | Reason | Created | Renewed | Expires | Owner |
|---|------|------|------|-----|--------|---------|---------|---------|-------|
| 8 | `application/services/sync_import_execution_service.rs` | 46 | 96 | ADR-014 | Timestamp for sync package rejection audit entry | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |
| 9 | `application/services/sync_import_execution_service.rs` | 118 | 96 | ADR-014 | Timestamp for sync conflict audit entry | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |

## Exception: `[arch:allow-mutation-before-replay]`

| # | File | Line | Rule | ADR | Reason | Created | Renewed | Expires | Owner |
|---|------|------|------|-----|--------|---------|---------|---------|-------|
| 10 | `application/services/sync_import_execution_service.rs` | 43 | 88 | ADR-014 | Pre-existing legacy method — mutation must occur before replay check for idempotent import flow | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |
| 11 | `application/services/sync_import_execution_service.rs` | 105 | 88 | ADR-014 | Pre-existing legacy method — mutation before replay in conflict handling | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |
| 12 | `application/services/sync_import_execution_service.rs` | 143 | 88 | ADR-014 | Pre-existing legacy method — mutation before replay in validation | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |
| 13 | `application/services/sync_import_execution_service.rs` | 199 | 88 | ADR-014 | Pre-existing legacy method — mutation before replay check | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |
| 14 | `application/services/sync_import_execution_service.rs` | 306 | 88 | ADR-014 | Pre-existing legacy — mutation after replay check but before full validation | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |

## Exception: `[arch:allow-unwrap-or]`

| # | File | Line | Rule | ADR | Reason | Created | Renewed | Expires | Owner |
|---|------|------|------|-----|--------|---------|---------|---------|-------|
| 15 | `application/services/sync_import_execution_service.rs` | 287 | 6 | ADR-014 | Intended fallback to 0 for non-numeric conflict IDs (replay-detect path) — safe default | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |
| 16 | `application/services/sync_import_execution_service.rs` | 400 | 6 | ADR-014 | Intended fallback to 0 for non-numeric conflict IDs (conflict path) — safe default | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |
| 17 | `application/services/sync_import_execution_service.rs` | 466 | 6 | ADR-014 | False is safe default for "not imported yet" (build_replay_detector) | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |
| 18 | `application/services/sync_import_execution_service.rs` | 490 | 6 | ADR-014 | False is safe default for "not imported yet" (build_replay_detector_from_ctx) | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |
| 19 | `application/services/sync_import_execution_service.rs` | 568 | 6 | ADR-014 | Test assertion default — false is safe | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |
| 20 | `application/services/operation_execution_guard.rs` | 64 | 6 | ADR-007 | Safe default (0) when no open fiscal year exists — guard degrades gracefully | 2026-05-28 | 2026-08-09 | 2026-11-07 | Architecture |
| 21 | `application/services/operation_execution_guard.rs` | 69 | 6 | ADR-007 | Safe default (0) when no archived years exist — guard degrades gracefully | 2026-05-28 | 2026-08-09 | 2026-11-07 | Architecture |
| 22 | `application/services/system_health_service.rs` | 118 | 6 | ADR-007 | 0 bytes safe default when path/metadata unavailable — not error condition | 2026-05-28 | 2026-08-09 | 2026-11-07 | Architecture |

## Exception: `[arch:allow-non-nested]`

| # | File | Line | Rule | ADR | Reason | Created | Renewed | Expires | Owner |
|---|------|------|------|-----|--------|---------|---------|---------|-------|
| 23 | `application/services/sync_import_execution_service.rs` | 264 | 89 | ADR-014 | Separate code path from handle_rejection — not a nested transaction | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |
| 24 | `application/services/sync_import_execution_service.rs` | 388 | 89 | ADR-014 | Separate code path from execute_import — not a nested transaction | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |

## Exception: `[arch:allow-sql]`

| # | File | Line | Rule | ADR | Reason | Created | Renewed | Expires | Owner |
|---|------|------|------|-----|--------|---------|---------|---------|-------|
| 25 | `application/services/sync_import_validation_service.rs` | 31 | 1 | ADR-011 | Pre-existing legacy method — SQL embedded in service before extraction policy was enforced | 2026-05-28 | 2026-08-09 | 2026-11-07 | Sync |

## Exception: `[arch:allow-memory-unsafe]`

| # | File | Line | Rule | ADR | Reason | Created | Renewed | Expires | Owner |
|---|------|------|------|-----|--------|---------|---------|---------|-------|
| 26 | `infrastructure/backup/sqlite_backup_adapter.rs` | 749 | 31 | ADR-017 | Whole-file read in backup for SHA-256 hashing — required for cryptographic verification | 2026-05-28 | 2026-08-09 | 2026-11-07 | Infrastructure |

## Exception: `[arch:allow-async]` — PERMANENT (ADR-0043)

| # | File | Line | Rule | ADR | Reason | Created | Expires | Owner |
|---|------|------|------|-----|--------|---------|---------|-------|
| 27 | `commands/backup.rs` | 45 | 117 | ADR-0043 | `create_backup` offloads blocking DB-copy + encryption via `tauri::async_runtime::spawn_blocking` to avoid UI freeze on the Tauri main thread — sanctioned exception, no async beyond the two backup command handlers | 2026-08-09 | Permanent | Architecture |
| 28 | `commands/backup.rs` | 357 | 117 | ADR-0043 | `restore_backup` offloads blocking DB restore + verification via `tauri::async_runtime::spawn_blocking` to avoid UI freeze on the Tauri main thread — sanctioned exception, no async beyond the two backup command handlers | 2026-08-09 | Permanent | Architecture |

---

## Summary

| Tag Type | Count | Primary ADR |
|----------|-------|-------------|
| `[arch:allow-utc-now]` | 2 | ADR-014 |
| `[arch:allow-mutation-before-replay]` | 5 | ADR-014 |
| `[arch:allow-unwrap-or]` | 8 | ADR-014, ADR-007 |
| `[arch:allow-non-nested]` | 2 | ADR-014 |
| `[arch:allow-sql]` | 1 | ADR-011 |
| `[arch:allow-memory-unsafe]` | 1 | ADR-017 |
| `[arch:allow-async]` (Permanent) | 2 | ADR-0043 |
| **Total** | **21** | |

**Review cadence:** All exceptions must be reviewed at least every 90 days. Expired exceptions must be renewed or closed. All rows renewed 2026-08-09; next review window closes 2026-11-07. `[arch:allow-async]` is **Permanent** per ADR-0043 — exempt from the 90-day renewal cycle.
