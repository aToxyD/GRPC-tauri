# Runtime Orchestration Semantics

## Overview

Runtime orchestration in this system is defined as **caller-driven evaluation** of whether runtime actions (checkpoint, integrity check, connection review) are warranted. There is no automatic scheduling, no background loop, and no daemon.

## Orchestration Policy

Two policies define how orchestration decisions are made:

| Policy | Description |
|--------|-------------|
| `ManualOnly` | No automatic action recommendations; caller must explicitly request each action |
| `PolicyDriven` | Evaluation compares current metrics against thresholds and recommends actions |

## Thresholds

| Metric | Threshold | Action |
|--------|-----------|--------|
| WAL size | >100MB | Checkpoint recommended |
| Checkpoint seqno | >20 | Checkpoint recommended |
| WAL pressure | High | Checkpoint recommended |
| Operations since last integrity check | ≥100 | Integrity check needed |

## Evaluation Scope

The `OrchestrationEvaluator` evaluates four categories:

1. **Checkpoint required**: wal pressure/threshold exceeds limits under PolicyDriven
2. **Integrity check required**: operation count since last check exceeds interval
3. **Connection review required**: high pressure or large WAL triggers topology re-evaluation
4. **Policy assessment**: whether the current policy is adequate for observed load

## Determinism

Orchestration evaluations are pure functions with no side effects. The same inputs always produce the same assessment. No wall-clock time is used — all decision inputs are caller-supplied.

## Safety Constraints

- Orchestration evaluation never executes checkpoints
- Orchestration evaluation never runs integrity checks
- Orchestration evaluation never opens connections
- Orchestration evaluation never spawns threads
- Orchestration evaluation never uses async
