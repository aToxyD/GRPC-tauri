# Backup & Restore Runbook

## 1. Creating a Backup
- Navigate to the **Backup** page.
- Click "Create Backup". The system performs an atomic SQLite backup ensuring no incomplete transactions are included.
- Wait for the "Success" notification. The system automatically tags the backup with the timestamp.

### WAL Snapshot Validation (Fail-Closed Policy)
- **Checkpoint Verification**: Before backup creation, the system performs a WAL checkpoint and verifies it completes successfully. If the checkpoint fails or leaves frames in the WAL, the backup is aborted immediately.
- **Snapshot Integrity Check**: After creating the temporary SQLite snapshot but before encryption, the system validates:
  - Temp file exists and is not empty
  - Temp file size is reasonable (not suspiciously small)
  - SQLite integrity_check passes
  - Page count and page size are valid
  - Snapshot size is consistent with source database
- **Fail-Closed Behavior**: If any validation fails, the backup is NOT created. The system logs the failure, writes an audit event, and securely deletes temporary files. No partial or corrupted backups are ever persisted.

## 2. Verifying Backup Integrity
- The system automatically performs heuristic validation of the backup file immediately after creation.
- Ensure no `Integrity Mismatch` errors are returned. If they are, discard the backup and retry.

## 3. Safe Storage Expectations
- Ensure the backup directory (`.grpc-data/backups`) is periodically synced to a secure, off-site location (e.g., encrypted flash drive or secure local offline server).
- Backups contain sensitive offline application data. Secure them physically.

## 4. Restore Workflow
1. Navigate to the **Backup** page.
2. Select the backup file to restore.
3. The system will prompt a critical confirmation dialogue requiring an explicit typed string (e.g., `RESTORE`). 
4. The system issues a short-lived execution token.
5. A full restart of the Tauri backend will occur (managed safely by the offline-first environment) applying the atomic rollback.

## 5. Restore Restrictions
- **Restore Blocked**: You cannot restore a backup from an older, archived fiscal year. The historical retention policies strictly prevent regressions.
- **Corrupted Backup**: If the backup hash fails or structure is deemed malformed, the system will instantly reject the file.
- **Integrity Mismatch**: If the system fingerprint does not match the backup signature, the action will be aborted.

## 6. Recovery Validation After Restore
- Review the `Audit Log` to confirm the restoration event was securely recorded in the newly restored environment.
- Run a System Health Check to confirm the database integrity is `Healthy`.

## Incident Response Procedures
- **Backup File Corrupted**: Discard the specific backup file. Attempt to restore from the immediate previous healthy backup.
- **Backup Verification Failed**: Do not attempt to use the backup. Verify hard drive health and memory limits, and initiate a new backup.
- **Restore Interrupted**: The system uses atomic temporary files. If power is lost during a restore, the system will rollback to the state prior to the restoration attempt.
- **Missing Backup Metadata**: Backups manually tampered with or renamed outside the application may lose metadata binding and will be rejected.
- **Backup Creation Failed (Checkpoint Validation)**: If backup creation fails with a checkpoint error, this indicates the WAL could not be flushed to the main database file. Check disk space, file permissions, and database lock status. Retry after resolving the underlying issue.
- **Backup Creation Failed (Snapshot Validation)**: If backup creation fails with a snapshot validation error, the temporary snapshot was corrupted or incomplete. This may indicate disk I/O issues or database corruption. Check system health and consider running database integrity checks before retrying. 
