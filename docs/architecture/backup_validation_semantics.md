# Backup Validation Semantics

## Overview

The backup validation module provides deterministic runtime primitives for validating SQLite backup files. It opens a backup file in a temporary read-only SQLite connection, runs `PRAGMA integrity_check`, and returns a structured validation summary.

## Architecture

```
sqlite_runtime/
  backup_validation.rs — BackupValidationRunner, BackupValidationResult, BackupValidationFailure
```

## Validation Flow

1. Caller provides path to a plaintext SQLite backup file
2. `BackupValidationRunner::validate()` opens the file with `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NO_MUTEX`
3. Runs `PRAGMA integrity_check`
4. Runs `PRAGMA page_count` and `PRAGMA page_size` for metadata
5. Parses integrity output into structured result
6. Closes the temporary connection
7. Returns `BackupValidationResult` or `BackupValidationFailure`

## Result Structure

```rust
BackupValidationResult {
    passed: bool,                    // true if integrity_check returned "ok"
    issues: Vec<IntegrityIssue>,     // detailed issues if not passed
    integrity_raw_output: String,    // raw PRAGMA output
    page_count: u64,                 // page count of backup
    page_size: u64,                  // page size of backup
}
```

## Safety Guarantees

1. **Read-only**: The backup file is opened in read-only mode
2. **No mutation**: The module never modifies the backup or the production database
3. **No permanent restore**: The backup is only opened temporarily for validation
4. **Temporary connection**: The SQLite connection is dropped after validation completes
5. **Fail-closed**: Any error opening or validating results in `BackupValidationFailure`

## Operational Limitations

- Backup files must be plaintext SQLite databases (decrypt before validation if encrypted)
- No encryption/decryption handling — caller is responsible for providing a valid SQLite file
- No automatic restore — validation is read-only
- No production database mutation
- No cross-connection state sharing
