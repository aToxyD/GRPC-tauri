use crate::application::oversight::anomalies::{
    compute_mean, compute_stddev, compute_z_score, extract_numeric_values, max_severity,
    percentile_severity, z_score_severity, AnomalyBatch, AnomalyDetector, AnomalyReport,
    AnomalySeverity,
};
use crate::application::oversight::types::BenchmarkDistribution;

/// Percentile-based outlier detector.
///
/// Flags units whose percentile rank falls outside configurable bands:
/// - Below p5 or above p95 → Warning
/// - Below p1 or above p99 → Critical
///
/// Includes z-score and mean/stddev in the report for governance context.
pub struct PercentileOutlierDetector;

impl AnomalyDetector for PercentileOutlierDetector {
    fn id(&self) -> &'static str {
        "percentile-outlier"
    }

    fn name(&self) -> &'static str {
        "Percentile Outlier Detector"
    }

    fn description(&self) -> &'static str {
        "Flags units whose KPI value falls below the 5th percentile or above the 95th percentile of the population."
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

        for rv in &distribution.values {
            let val = match rv.value.numeric_value() {
                Some(v) if v.is_finite() => v,
                _ => continue,
            };

            let p_rank = rv.percentile_rank;
            let p_sev = percentile_severity(p_rank);

            let z = compute_z_score(val, mean, stddev);
            let z_sev = z_score_severity(z);
            let combined = max_severity(p_sev, z_sev);

            let dir = if val > mean { "above" } else { "below" };
            let boundary = if p_rank < 1.0 {
                "below the 1st percentile (extreme outlier)"
            } else if p_rank > 99.0 {
                "above the 99th percentile (extreme outlier)"
            } else if p_rank < 5.0 {
                "below the 5th percentile"
            } else if p_rank > 95.0 {
                "above the 95th percentile"
            } else {
                "within the 5th–95th percentile band"
            };
            let explanation = format!(
                "Percentile rank {:.1}: value {:.2} is {} of the population. Z-score {:.2} ({:.2}σ {} mean {:.2}).",
                p_rank, val, boundary, z, z.abs(), dir, mean
            );

            if p_sev != AnomalySeverity::Normal {
                batch.push(AnomalyReport {
                    metric_id: distribution.benchmark_id.clone(),
                    unit_id: rv.unit_id.clone(),
                    observed_value: val,
                    population_mean: mean,
                    population_stddev: stddev,
                    z_score: z,
                    percentile_rank: p_rank,
                    severity: combined,
                    explanation,
                    fiscal_year: distribution.fiscal_year,
                    benchmark_id: distribution.benchmark_id.clone(),
                });
            }
        }

        batch
    }
}
