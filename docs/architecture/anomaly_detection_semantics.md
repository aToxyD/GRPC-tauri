# Anomaly Detection Semantics

## Overview

Anomaly detection in the oversight layer identifies units whose KPI values deviate significantly from the population. All computations are deterministic, reproducible, and consume only `BenchmarkDistribution` data — no SQL, no wall-clock dependency.

## Detection Methods

### 1. Z-Score Detector (`ZScoreDetector`)

For a population of N values, computes:

```
mean = Σ(x_i) / N
stddev = √( Σ(x_i - mean)² / N )        // population stddev (not sample)
z_i = (x_i - mean) / stddev
```

**Key properties:**
- Uses population standard deviation (divisor N) since the full unit population is available.
- Two-pass formula for numerical stability (mean first, then sum squared deviations).
- When stddev ≈ 0 (all identical values), z = 0 for all units (no anomaly).

**Severity thresholds (absolute z-score):**

| Range              | Severity   |
|--------------------|------------|
| \|z\| < 2.0       | Normal     |
| 2.0 ≤ \|z\| < 3.0 | Warning    |
| \|z\| ≥ 3.0       | Critical   |

**Justification:** These thresholds correspond to approximately 95% (2σ) and 99.7% (3σ) of values in a normal distribution. They are symmetric around the mean.

### 2. Percentile Outlier Detector (`PercentileOutlierDetector`)

For each value, computes its percentile rank within the population:

```
percentile_rank = (count_below + 0.5 * count_equal) / N * 100
```

**Severity thresholds (percentile rank):**

| Range                    | Severity   |
|--------------------------|------------|
| 5 ≤ p ≤ 95              | Normal     |
| 1 ≤ p < 5 or 95 < p ≤ 99| Warning    |
| p < 1 or p > 99         | Critical   |

### 3. Combined Severity

The final severity for each unit is the **maximum** of z-score severity and percentile severity. This ensures units flagged by either method are surfaced.

```
final_severity = max(z_score_severity, percentile_rank_severity)
```

## Output Types

### `AnomalyReport`

Per-unit report containing:

| Field              | Type             | Description                             |
|--------------------|------------------|-----------------------------------------|
| `metric_id`        | String           | KPI metric identifier                   |
| `unit_id`          | String           | Unit identifier                         |
| `observed_value`   | f64              | Raw KPI value for this unit             |
| `population_mean`  | f64              | Population mean                         |
| `population_stddev`| f64              | Population standard deviation           |
| `z_score`          | f64              | Number of standard deviations from mean |
| `percentile_rank`  | f64              | Percentile rank (0..100)                |
| `severity`         | AnomalySeverity  | Normal / Warning / Critical             |
| `explanation`      | String           | Human-readable explanation              |
| `fiscal_year`      | i32              | Fiscal year context                     |
| `benchmark_id`     | String           | Benchmark identifier                    |

### `AnomalyBatch`

Collection of per-unit reports with summary counters:

| Field             | Type    | Description                |
|-------------------|---------|----------------------------|
| `fiscal_year`     | i32     | Fiscal year context        |
| `benchmark_id`    | String  | Benchmark identifier       |
| `total_units`     | usize   | Number of units in population |
| `anomaly_count`   | usize   | Total non-normal reports   |
| `warning_count`   | usize   | Warning severity count     |
| `critical_count`  | usize   | Critical severity count    |
| `reports`         | Vec<AnomalyReport> | Per-unit reports |

## Reproducibility Guarantees

1. Given the same `BenchmarkDistribution`, all detectors produce identical output.
2. No random initialization, no wall-clock dependency, no external state.
3. Severity thresholds are hard-coded constants — not configurable at runtime.
4. All floating-point operations use deterministic `f64` arithmetic in distribution order.

## Edge Cases

| Population              | Behavior                                        |
|-------------------------|-------------------------------------------------|
| Empty (0 units)         | Empty batch, no reports                         |
| Single unit             | Stddev = 0, z = 0, percentile = 50, Normal      |
| All identical values    | Stddev ≈ 0, z = 0, Normal for all               |
| MetricValue::None       | Skipped in both detectors                       |
