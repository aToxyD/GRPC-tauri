# SQLite Connection Topology Evaluation

## Purpose

Evaluate whether the current single-connection SQLite architecture is appropriate for the application's operational and reporting workload without introducing runtime complexity.

## Current Architecture

```
┌──────────────┐     ┌──────────────────────┐     ┌──────────────┐
│ Application  │────▶│ Mutex<Option<Connection>>│────▶│   SQLite DB  │
│ Services     │     │   (serialized access) │     │   (WAL mode) │
└──────────────┘     └──────────────────────┘     └──────────────┘
```

All reads and writes pass through a single connection. SQLite WAL mode supports concurrent readers at the engine level, but the Rust mutex serializes all access at the application level.

## Evaluation Method

The evaluation is divided into four independent assessments:

### 1. Topology Assessment (`ConnectionModelEvaluator`)

Determines the current topology and whether a read connection would be beneficial. Inputs:

- `has_read_connection`: whether a read connection already exists
- `reporting_query_count`: number of reporting queries per cycle
- `wal_pressure`: classified WAL pressure level
- `concurrent_readers`: expected simultaneous read operations

### 2. Contention Assessment (`ContentionClassifier`)

Classifies WAL pressure and read contention from operational metrics. Outputs a fractional contention score [0.0, 1.0].

### 3. Read Connection Assessment (`ReadConnectionSuitabilityEvaluator`)

Maps contention and pressure to suitability level:

- **Low pressure + few queries → NotNeeded**
- **Medium pressure + moderate queries → Recommended**
- **High pressure + any queries → Recommended**

### 4. Orchestration Assessment (`OrchestrationEvaluator`)

Determines whether runtime actions (checkpoint, integrity check, topology review) are warranted under the current orchestration policy.

## Combined Evaluation

The `ReviewEvaluator` combines all four assessments into a single `ConnectionReviewEvaluation`, and the `RecommendationEngine` produces a deterministic priority-ordered set of recommendations.

## Decision Matrix

| Wal Pressure | Reporting Load | Contention | Recommended Action |
|-------------|---------------|------------|-------------------|
| Low | Low (<30 qps) | Low | No action needed |
| Low | High (>100 qps, >3 readers) | Low | Consider read connection |
| Medium | Moderate (>30 qps) | Medium | Add read connection |
| High | Any | High | Add read connection; schedule checkpoint |
| Any | Any | High | Full topology review |

## Guarantees

1. All evaluations are pure functions — no connections opened
2. All evaluations are deterministic — same input = same output
3. All evaluations are serializable — outputs can be cached, logged, or transmitted
4. No evaluation mutates application state
5. No evaluation depends on wall-clock time
6. No evaluation spawns threads or uses async
