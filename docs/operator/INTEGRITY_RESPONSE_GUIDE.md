# Integrity & Corruption Response Guide

The GRPC-tauri offline system enforces an absolute zero-trust verification of local data files to protect against tampering, bit-rot, and unauthorized physical disk modifications.

## 1. System Integrity States
- **Healthy**: Normal operational parameters. No anomalies detected.
- **Warning**: A non-fatal discrepancy was found (e.g., minor operational inconsistency). Operations continue, but a diagnostic review is recommended.
- **Corrupted**: Data integrity is compromised. The system enters a fail-closed protection state blocking critical actions.
- **Critical**: Severe structural or cryptographic failure. The system immediately halts operations.

## 2. Terminology
- **Anomaly**: Unusual statistical variance (e.g., sudden massive consumption spike) that does not necessarily mean corruption.
- **Integrity Failure**: A cryptographic hash or schema constraint was violated.
- **Corruption**: Actual damage to the data structure or files.
- **Operational Warning**: A procedural misstep was caught before application.

## 3. Incident Response Procedures

### Corruption Detected
- **Symptoms**: State transitions to `Corrupted`. File access denied warnings.
- **Logs Expected**: `CRITICAL_DATA_CORRUPTION` event.
- **Operator Action**: Halt all input. DO NOT attempt to write new data. Navigate to Backups and perform a verified restore from the last known `Healthy` backup.

### Audit Chain Failure
- **Symptoms**: The audit log chain validation rejects the current state.
- **Logs Expected**: `TAMPERING_DETECTED_AUDIT_CHAIN_INVALID`.
- **Operator Action**: Escalate to Wilaya Administration immediately. This indicates manual SQLite tampering or malicious activity. Restore required.

### Replay Rejection
- **Symptoms**: Applying a fiscal package results in "Replay Detected".
- **Logs Expected**: `REPLAY_ATTEMPT_REJECTED`.
- **Operator Action**: Ensure you are not re-importing an already applied package. Discard the package. Operations continue normally.

### Expired Fiscal Transition Package
- **Symptoms**: Import fails with "Validity Window Expired".
- **Operator Action**: Delete the `.pkg` file. Contact the Wilaya to issue a fresh, unexpired package.

### Orphan Snapshots
- **Symptoms**: Snapshot found without an associated transaction boundary.
- **Operator Action**: Run the diagnostic cleanup. The system will isolate the orphan data safely without impacting operations.

### Spike in Operational Findings
- **Symptoms**: Dashboard shows elevated warnings.
- **Operator Action**: Review the System Health and Conflict Center pages. Verify recent order data entry for typos or duplicate entries.

> **CRITICAL REMINDER**: The system does NOT magically auto-heal from cryptographic corruption. Its design is strictly deterministic and fail-closed. If it is broken, it stays broken to protect data until an operator explicitly rolls back to a safe, verified state.
