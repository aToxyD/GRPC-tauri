# Runtime Connection Access Review

## Context

Phase 6.C of the SQLite observability roadmap evaluates whether the current single-`Mutex<Option<Connection>>` topology is suitable for the application's reporting workload, and whether a dedicated read connection would provide measurable benefit.

## Topology Model

The system currently uses a single SQLite connection serialized through `Mutex<Option<Connection>>`. All reads and writes share this connection. SQLite WAL mode supports concurrent readers, but the current architecture does not utilize this property.

### Evaluated Topologies

| Topology | Description |
|----------|-------------|
| `SingleWriter` | One connection handles all reads and writes, serialized via Mutex |
| `SingleWriterWithReadConnection` | One write connection + one or more read-only connections for reporting |

## Evaluation Criteria

### WAL Pressure

WAL pressure is classified from two metrics:

- **WAL file size**: >100MB = High, >10MB or seqno >10 = Medium, else Low
- **Checkpoint seqno**: high seqno indicates WAL growth without checkpoint

### Reporting Load

- **Query volume**: number of reporting queries expected per cycle
- **Concurrent readers**: expected simultaneous read operations
- **Reporting frequency**: how often reports are generated

### Contention

Contention is classified from write frequency and reporting frequency:

| Write Frequency | Reporting Frequency | Contention Level |
|----------------|-------------------|-----------------|
| ≤50 | ≤30 | Low |
| >50 or >30 | Medium | |
| >100 and >50 | High | |

## Deterministic Evaluation

All evaluations are pure functions. No wall-clock dependencies, no mutable state, no connection opening during evaluation.

### ConnectionModelEvaluator

Evaluates the current topology and determines whether a read connection would be beneficial based on WAL pressure, reporting query count, and concurrent readers.

### ReadConnectionSuitabilityEvaluator

Produces one of three outcomes:

- **NotNeeded**: single writer sufficient
- **Beneficial**: read connection would help but not critical
- **Recommended**: read connection strongly recommended

### ContentionClassifier

Pure function classifying both WAL pressure and read contention from input metrics.

## Recommendation Engine

The `RecommendationEngine` produces a set of deterministic recommendations:

- **NoActionNeeded**: all metrics within threshold
- **AddReadConnection**: reporting load justifies a second connection
- **ScheduleCheckpoint**: WAL pressure suggests checkpoint
- **ScheduleIntegrityCheck**: operations since last integrity check exceed interval
- **UpgradeOrchestrationPolicy**: manual policy constrains runtime decisions

## Guarantees

1. All evaluations are reproducible (same input = same output)
2. No connections are opened during evaluation
3. No wall-clock time is used
4. No mutations occur during evaluation
5. All output is serializable via serde
6. Recommendation order is deterministic
