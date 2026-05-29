# WAL Lifecycle Management

## Overview

WAL (Write-Ahead Log) lifecycle management provides deterministic runtime primitives for managing SQLite WAL state. This module operates within the `sqlite_runtime` infrastructure layer and is purely execution-oriented — it does not schedule, monitor, or automatically trigger operations.

## Architecture

```
sqlite_runtime/
  checkpoint.rs      — WalCheckpointExecutor, CheckpointMode, CheckpointResult
  idle_checkpoint.rs — IdleCheckpointEvaluator, IdleWindow, IdleCheckpointDecision
  policies.rs        — CheckpointPolicy
```

## Deterministic Behavior

All operations are deterministic given the same inputs:

- `WalCheckpointExecutor::check_eligibility()` produces the same decision for same (pages, seqno, policy)
- `IdleCheckpointEvaluator::evaluate()` produces the same recommendation for same (window, eligibility, policy)
- No wall-clock time is used — idle duration is supplied by the caller
- No random values are used

## Checkpoint Modes

| Mode      | PRAGMA Value | Behavior |
|-----------|-------------|----------|
| Passive   | PASSIVE     | Checkpoint as much WAL as possible without blocking |
| Full      | FULL        | Block until checkpoint completes, truncate WAL |
| Restart   | RESTART     | Like FULL but also restarts WAL file |
| Truncate  | TRUNCATE    | Like FULL but truncates WAL to zero bytes |

## Checkpoint Eligibility

A checkpoint is eligible when:

1. The database has pages (`page_count > 0`)
2. The WAL has content (`checkpoint_seqno > 0`)
3. Either: WAL size exceeds `max_wal_size_bytes` (default 100MB), OR checkpoint is needed (seqno > 0 with non-passive state)

## Idle Checkpoint Evaluation

The idle checkpoint evaluator is a pure function:

1. If `idle_duration < 30s` → `ShouldNotCheckpoint` (below threshold)
2. If not eligible for checkpoint → `ShouldNotCheckpoint`
3. If policy disables idle checkpoint → `ShouldNotCheckpoint`
4. Otherwise → `ShouldCheckpoint`

The evaluator **recommends** — it never executes. The caller must invoke checkpoint execution separately.

## Operational Limitations

- No automatic scheduling — caller drives all checkpoint decisions
- No background threads or daemons
- No hidden retries
- No automatic VACUUM
- No WAL file deletion by path (only PRAGMA wal_checkpoint operations)
