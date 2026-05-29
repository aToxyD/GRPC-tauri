//! Deterministic anomaly detection over benchmark distributions.
//!
//! All detectors are pure functions consuming BenchmarkDistribution data.
//! No SQL, no mutations, no side effects, no wall-clock dependency.

mod percentile;
mod zscore;

pub use percentile::PercentileOutlierDetector;
pub use zscore::ZScoreDetector;

use crate::application::oversight::types::BenchmarkDistribution;
use serde::{Deserialize, Serialize};

/// Anomaly severity classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalySeverity {
    Normal,
    Warning,
    Critical,
}

/// A single anomaly report for one unit within a benchmark distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyReport {
    pub metric_id: String,
    pub unit_id: String,
    pub observed_value: f64,
    pub population_mean: f64,
    pub population_stddev: f64,
    pub z_score: f64,
    pub percentile_rank: f64,
    pub severity: AnomalySeverity,
    pub explanation: String,
    pub fiscal_year: i32,
    pub benchmark_id: String,
}

/// Batch of anomaly reports for a single distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyBatch {
    pub fiscal_year: i32,
    pub benchmark_id: String,
    pub total_units: usize,
    pub anomaly_count: usize,
    pub warning_count: usize,
    pub critical_count: usize,
    pub reports: Vec<AnomalyReport>,
}

impl AnomalyBatch {
    pub fn new(fiscal_year: i32, benchmark_id: &str, total_units: usize) -> Self {
        Self {
            fiscal_year,
            benchmark_id: benchmark_id.to_string(),
            total_units,
            anomaly_count: 0,
            warning_count: 0,
            critical_count: 0,
            reports: Vec::new(),
        }
    }

    pub fn push(&mut self, report: AnomalyReport) {
        match report.severity {
            AnomalySeverity::Warning => self.warning_count += 1,
            AnomalySeverity::Critical => self.critical_count += 1,
            AnomalySeverity::Normal => {}
        }
        if report.severity != AnomalySeverity::Normal {
            self.anomaly_count += 1;
        }
        self.reports.push(report);
    }

    pub fn is_empty(&self) -> bool {
        self.anomaly_count == 0
    }
}

/// Deterministic anomaly detector over a benchmark distribution.
pub trait AnomalyDetector {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// Detect anomalies in the given distribution.
    /// Returns an AnomalyBatch with per-unit reports.
    fn detect(&self, distribution: &BenchmarkDistribution) -> AnomalyBatch;
}

/// Deterministic mean computation over a slice of f64 values.
pub fn compute_mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let sum: f64 = values.iter().sum();
    Some(sum / values.len() as f64)
}

/// Deterministic population standard deviation (two-pass for stability).
/// Divisor is N (population), not N-1 (sample).
/// Returns 0.0 for populations of size 0 or 1.
pub fn compute_stddev(values: &[f64], mean: f64) -> f64 {
    if values.len() <= 1 {
        return 0.0;
    }
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

/// Z-score: (x - mean) / stddev. Returns 0.0 when stddev is ~0.
pub fn compute_z_score(value: f64, mean: f64, stddev: f64) -> f64 {
    if stddev.abs() < f64::EPSILON {
        return 0.0;
    }
    (value - mean) / stddev
}

/// Classify severity based on absolute z-score.
pub fn z_score_severity(z: f64) -> AnomalySeverity {
    let abs_z = z.abs();
    if abs_z >= 3.0 {
        AnomalySeverity::Critical
    } else if abs_z >= 2.0 {
        AnomalySeverity::Warning
    } else {
        AnomalySeverity::Normal
    }
}

/// Classify severity based on percentile rank (0..100).
pub fn percentile_severity(p: f64) -> AnomalySeverity {
    if !(1.0..=99.0).contains(&p) {
        AnomalySeverity::Critical
    } else if !(5.0..=95.0).contains(&p) {
        AnomalySeverity::Warning
    } else {
        AnomalySeverity::Normal
    }
}

/// Combine two severities, taking the most severe.
pub fn max_severity(a: AnomalySeverity, b: AnomalySeverity) -> AnomalySeverity {
    use AnomalySeverity::*;
    match (a, b) {
        (Critical, _) | (_, Critical) => Critical,
        (Warning, _) | (_, Warning) => Warning,
        _ => Normal,
    }
}

/// Extract numeric values from a BenchmarkDistribution (in distribution order).
/// Skips MetricValue::None entries and non-finite values (NaN, Inf).
pub(crate) fn extract_numeric_values(dist: &BenchmarkDistribution) -> Vec<f64> {
    dist.values
        .iter()
        .filter_map(|rv| rv.value.numeric_value())
        .filter(|v| v.is_finite())
        .collect()
}
