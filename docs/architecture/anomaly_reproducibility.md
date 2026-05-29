# Anomaly Detection Reproducibility

## Determinism Guarantee

All anomaly detection computations are **strictly deterministic**: given the same `BenchmarkDistribution`, all detectors produce identical output every time.

This is enforced through:

1. **Pure functions** — detectors consume only `BenchmarkDistribution` data. No SQL, no database access, no external state.
2. **No randomness** — no random initialization, no sampling, no shuffling.
3. **No wall-clock dependency** — no `Utc::now`, `SystemTime`, or `Instant`.
4. **Fixed thresholds** — severity boundaries are hard-coded constants, not configuration parameters.
5. **Stable arithmetic** — all computations follow the same order of operations.

## Reproducibility by DB State

The system-wide reproducibility guarantee is:

```
same DB state → same BenchmarkDistribution → same AnomalyBatch
```

The DB state fully determines the `BenchmarkDistribution` (see [benchmark_reproducibility.md](benchmark_reproducibility.md)). The distribution in turn fully determines the anomaly output. Therefore, restoring a DB to a previous state produces identical anomaly reports.

## Ordering

Anomaly reports within an `AnomalyBatch` follow the same order as the input `BenchmarkDistribution.values` (sorted descending by value, ascending by unit_id for ties). This ordering is stable across identical distributions.

## Serialization

`AnomalyReport` and `AnomalyBatch` derive `Serialize` and `Deserialize` from serde. The JSON representation preserves all fields and can be used for:
- Archiving anomaly snapshots
- Comparing anomaly states across fiscal periods
- Debugging and audit logs

## Limitations

1. **Floating-point precision** — Different hardware architectures or compiler optimizations may produce slightly different results for operations involving very small or very large numbers. The formulas used (two-pass mean/stddev) minimize this risk but do not eliminate it.
2. **Not suitable for real-time alerting** — The system is designed for periodic (daily/weekly) anomaly detection over frozen or stable dataset. It does not support streaming or incremental computation.
3. **No cross-distribution comparisons** — Each `BenchmarkDistribution` is processed independently. There is no mechanism to detect anomalies across fiscal years or across different benchmark types in a single pass.

## Verification

Reproducibility is verified by integration test `identical_distribution_produces_identical_anomaly_batch`, which constructs an identical distribution twice and asserts byte-identical anomaly batches.
