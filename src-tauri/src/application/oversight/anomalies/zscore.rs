use crate::application::oversight::anomalies::{
    compute_mean, compute_stddev, compute_z_score, extract_numeric_values, max_severity,
    percentile_severity, z_score_severity, AnomalyBatch, AnomalyDetector, AnomalyReport,
    AnomalySeverity,
};
use crate::application::oversight::types::BenchmarkDistribution;

/// Z-score based anomaly detector.
///
/// For each unit, computes z = (value - mean) / stddev and flags:
/// - |z| >= 2.0 → Warning
/// - |z| >= 3.0 → Critical
///
/// All computations are deterministic. No wall-clock dependency.
pub struct ZScoreDetector;

impl AnomalyDetector for ZScoreDetector {
    fn id(&self) -> &'static str {
        "z-score"
    }

    fn name(&self) -> &'static str {
        "Z-Score Anomaly Detector"
    }

    fn description(&self) -> &'static str {
        "Flags units whose KPI value deviates significantly from the population mean, measured in standard deviations."
    }

    fn detect(&self, distribution: &BenchmarkDistribution) -> AnomalyBatch {
        let numeric = extract_numeric_values(distribution);
        let mut batch = AnomalyBatch::new(
            distribution.fiscal_year,
            &distribution.benchmark_id,
            distribution.unit_count,
        );

        if numeric.is_empty() {
            return batch;
        }

        let mean = compute_mean(&numeric).unwrap_or(0.0);
        let stddev = compute_stddev(&numeric, mean);

        for (idx, rv) in distribution.values.iter().enumerate() {
            let val = match rv.value.numeric_value() {
                Some(v) if v.is_finite() => v,
                _ => continue,
            };

            let z = compute_z_score(val, mean, stddev);
            let z_sev = z_score_severity(z);

            let count_below = numeric.iter().filter(|&&v| v < val).count();
            let count_equal = numeric.iter().filter(|&&v| (v - val).abs() < f64::EPSILON).count();
            let total = numeric.len();
            let percentile_rank = if total <= 1 {
                50.0
            } else {
                (count_below as f64 + 0.5 * count_equal as f64) / total as f64 * 100.0
            };

            let p_sev = percentile_severity(percentile_rank);
            let combined = max_severity(z_sev, p_sev);

            let dir = if val > mean { "above" } else { "below" };
            let explanation = if combined == AnomalySeverity::Normal {
                format!(
                    "Z-score {:.2}: value {:.2} is {:.2}σ {} mean {:.2} (|z| < 2.0: normal range).",
                    z, val, z.abs(), dir, mean
                )
            } else {
                format!(
                    "Z-score {:.2}: value {:.2} is {:.2}σ {} mean {:.2}. Rank {} of {} ({:.1}th percentile).",
                    z, val, z.abs(), dir, mean, idx + 1, total, percentile_rank
                )
            };

            batch.push(AnomalyReport {
                metric_id: distribution.benchmark_id.clone(),
                unit_id: rv.unit_id.clone(),
                observed_value: val,
                population_mean: mean,
                population_stddev: stddev,
                z_score: z,
                percentile_rank,
                severity: combined,
                explanation,
                fiscal_year: distribution.fiscal_year,
                benchmark_id: distribution.benchmark_id.clone(),
            });
        }

        batch
    }
}
