# Integrity Monitoring Foundations

## Overview

The integrity module parses SQLite's `PRAGMA integrity_check` and
`PRAGMA quick_check` output into structured, deterministic types for
corruption classification and alerting.

## PRAGMA Output Semantics

### PRAGMA integrity_check

- On success: returns a single row with value `"ok"`
- On failure: returns one row per issue, each containing a description
  of the problem (e.g., `"row 5 missing from index idx_products"`)
- Each failed row references database pages, indexes, or tables

### PRAGMA quick_check

- On success: returns `"ok"`
- On failure: returns the first issue found (abbreviated check)

## Integrity Classification

### Issue Severity

| Severity | Criteria |
|----------|----------|
| Ok | Both checks pass |
| Warning | Integrity check fails but quick_check passes |
| Error | One check fails |
| Critical | Both checks fail with ≤10 issues |
| Fatal | Both checks fail with >10 issues |

### Issue-Level Classification

| Keyword | Severity |
|---------|----------|
| "missing from index" | Error |
| "wrong", "invalid", "corrupt" | Critical |
| "missing", "null" | Warning |
| Other error text | Error |

## Determinism

- Same PRAGMA output → same `IntegrityCheckResult` / `QuickCheckResult`
- Issues are ordered by line number in the PRAGMA output
- Snapshot severity is a pure function of the two check results
- No time-based fields in any integrity type

## Limitations (Phase 6.A)

- No scheduled execution — caller must invoke `from_connection` explicitly
- No automatic repair
- No trend tracking across snapshots
- No backup validation (planned for later phases)
- No integration with domain event system for integrity alerts
