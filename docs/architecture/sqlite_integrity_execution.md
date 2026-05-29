# SQLite Integrity Execution

## Overview

The integrity execution module provides deterministic runtime primitives for running SQLite integrity checks. It wraps `PRAGMA integrity_check` and `PRAGMA quick_check` with structured result parsing, severity classification, and bounded execution history.

## Architecture

```
sqlite_runtime/
  integrity_runner.rs — IntegrityRunner, QuickCheckRunner, IntegrityExecutionResult
  runtime_metrics.rs  — IntegrityExecutionHistory (bounded)
```

## Integrity Execution

### IntegrityRunner

Runs both `PRAGMA integrity_check` and `PRAGMA quick_check`, then classifies the combined severity:

| Check Result | Quick Result | Severity |
|-------------|--------------|----------|
| Passed      | Passed       | Ok       |
| Failed      | Passed       | Warning  |
| Passed      | Failed       | Error    |
| Failed      | Failed       | Error/Critical/Fatal based on issue count |

### QuickCheckRunner

Runs `PRAGMA quick_check` only. Returns structured `QuickCheckResult`.

## Severity Classification

Severity is determined by the number and type of issues:

| Condition | Severity |
|-----------|----------|
| All checks passed | Ok |
| integrity_check failed, quick_check passed | Warning |
| quick_check failed, <5 issues | Error |
| 5-10 issues | Critical |
| >10 issues | Fatal |

## Corruption Severity Interpretation

- **Ok**: No corruption detected
- **Warning**: integrity_check found minor issues (e.g., index inconsistencies) but quick_check passed — data likely intact
- **Error**: Both checks found issues — probable data corruption
- **Critical**: Multiple structural issues — index/page corruption likely
- **Fatal**: Widespread corruption — database integrity compromised

## Integrity Guarantees

1. **No automatic repair**: Integrity checks are read-only. The module never attempts to fix corruption.
2. **Deterministic parsing**: Same raw output produces identical structured results.
3. **Issue ordering**: Issues are ordered by line number from the PRAGMA output — deterministic and reproducible.
4. **Bounded history**: `IntegrityExecutionHistory` keeps a configurable maximum number of past results (FIFO eviction).

## Operational Limitations

- No mutation beyond SQLite integrity PRAGMAs
- No automatic repair or recovery
- No self-healing
- No wall-clock dependency
