use serde::{Deserialize, Serialize};
use std::fmt;

/// Measurement unit for a metric value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnitType {
    Count,
    Amount,
    Ratio,
    Percentage,
    Days,
}

impl fmt::Display for UnitType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnitType::Count => write!(f, "count"),
            UnitType::Amount => write!(f, "amount"),
            UnitType::Ratio => write!(f, "ratio"),
            UnitType::Percentage => write!(f, "percentage"),
            UnitType::Days => write!(f, "days"),
        }
    }
}

/// The dimension across which a metric is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dimension {
    Product,
    Unit,
    Wilaya,
    Period,
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Dimension::Product => write!(f, "product"),
            Dimension::Unit => write!(f, "unit"),
            Dimension::Wilaya => write!(f, "wilaya"),
            Dimension::Period => write!(f, "period"),
        }
    }
}

/// A computed metric value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MetricValue {
    Count(i64),
    Amount(f64),
    Ratio(f64),
    Percentage(f64),
    Days(i64),
    None,
}

impl MetricValue {
    /// Extract the numeric value as f64 (for statistical computation).
    /// Returns None for MetricValue::None.
    pub fn numeric_value(&self) -> Option<f64> {
        match self {
            MetricValue::Count(v) => Some(*v as f64),
            MetricValue::Amount(v) => Some(*v),
            MetricValue::Ratio(v) => Some(*v),
            MetricValue::Percentage(v) => Some(*v),
            MetricValue::Days(v) => Some(*v as f64),
            MetricValue::None => None,
        }
    }

    pub fn unit_type(&self) -> Option<UnitType> {
        match self {
            MetricValue::Count(_) => Some(UnitType::Count),
            MetricValue::Amount(_) => Some(UnitType::Amount),
            MetricValue::Ratio(_) => Some(UnitType::Ratio),
            MetricValue::Percentage(_) => Some(UnitType::Percentage),
            MetricValue::Days(_) => Some(UnitType::Days),
            MetricValue::None => None,
        }
    }

    pub fn is_some(&self) -> bool {
        !matches!(self, MetricValue::None)
    }
}

/// OversightMetric trait — all KPIs implement this.
///
/// Metrics are pure deterministic functions of report outputs.
/// They must not execute SQL, mutate state, or emit events.
pub trait OversightMetric {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn formula(&self) -> &'static str;
    fn unit(&self) -> UnitType;
    fn dimension(&self) -> Dimension;
    fn compute(&self, ctx: &super::ReportsContext<'_>) -> Result<MetricValue, String>;
}

/// Result wrapper for metric computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricResult {
    pub id: String,
    pub name: String,
    pub formula: String,
    pub unit: UnitType,
    pub dimension: Dimension,
    pub value: MetricValue,
}

impl MetricResult {
    pub fn from_metric<M: OversightMetric>(metric: &M, value: MetricValue) -> Self {
        Self {
            id: metric.id().to_string(),
            name: metric.name().to_string(),
            formula: metric.formula().to_string(),
            unit: metric.unit(),
            dimension: metric.dimension(),
            value,
        }
    }
}

/// Percentile rank (0.0–100.0).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PercentileRank(pub f64);

impl PercentileRank {
    /// Compute the percentile rank of `value` within `sorted_values`.
    /// sorted_values must be sorted in ascending order.
    pub fn compute(value: f64, sorted_values: &[f64]) -> Self {
        if sorted_values.is_empty() {
            return PercentileRank(0.0);
        }
        let count_below = sorted_values.iter().filter(|&&v| v < value).count();
        let count_equal = sorted_values.iter().filter(|&&v| (v - value).abs() < f64::EPSILON).count();
        let total = sorted_values.len();
        let rank = if total == 1 {
            50.0
        } else {
            (count_below as f64 + 0.5 * count_equal as f64) / (total as f64) * 100.0
        };
        PercentileRank(rank)
    }

    pub fn value(&self) -> f64 {
        self.0
    }
}

/// A single unit's metric value with rank and percentile within a population.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedMetricValue {
    pub unit_id: String,
    pub value: MetricValue,
    pub rank: usize,
    pub percentile_rank: f64,
}

/// Statistical distribution of a metric across a population of units.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkDistribution {
    pub benchmark_id: String,
    pub benchmark_name: String,
    pub metric_unit: UnitType,
    pub metric_dimension: Dimension,
    pub fiscal_year: i32,
    pub unit_count: usize,
    pub values: Vec<RankedMetricValue>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub median: Option<f64>,
    pub p95: Option<f64>,
}

/// Summary statistics for a cohort of units.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CohortSummary {
    pub cohort_id: String,
    pub name: String,
    pub unit_count: usize,
    pub distribution: Option<BenchmarkDistribution>,
}

/// Scope definition for a benchmark computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkWindow {
    pub fiscal_year: i32,
    pub unit_ids: Vec<String>,
    pub benchmark_ids: Vec<String>,
}

impl BenchmarkDistribution {
    /// Build a distribution from raw per-unit (unit_id, numeric_value) pairs.
    /// `to_metric` converts a raw numeric value to the appropriate MetricValue variant.
    /// Values are sorted deterministically (descending by numeric value, ascending by unit_id for ties).
    /// Ranks are 1-based (highest value = rank 1). Ties share the same rank.
    pub fn compute<F: Fn(f64) -> MetricValue>(
        benchmark_id: &str,
        benchmark_name: &str,
        metric_unit: UnitType,
        metric_dimension: Dimension,
        fiscal_year: i32,
        raw: Vec<(String, f64)>,
        to_metric: F,
    ) -> Self {
        if raw.is_empty() {
            return Self {
                benchmark_id: benchmark_id.to_string(),
                benchmark_name: benchmark_name.to_string(),
                metric_unit,
                metric_dimension,
                fiscal_year,
                unit_count: 0,
                values: Vec::new(),
                min: None,
                max: None,
                median: None,
                p95: None,
            };
        }

        // Sort by value descending, then by unit_id ascending for stable ties
        let mut sorted = raw;
        sorted.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        let total = sorted.len();
        let numeric_values: Vec<f64> = sorted.iter().map(|(_, v)| *v).collect();

        // Assign ranks (1-based, ties share same rank)
        let values: Vec<RankedMetricValue> = sorted
            .iter()
            .enumerate()
            .map(|(idx, (unit_id, val))| {
                let rank = {
                    let mut r = idx + 1;
                    for j in (0..idx).rev() {
                        if (sorted[j].1 - *val).abs() < f64::EPSILON {
                            r = j + 1;
                        } else {
                            break;
                        }
                    }
                    r
                };
                let pct = PercentileRank::compute(*val, &numeric_values);
                RankedMetricValue {
                    unit_id: unit_id.clone(),
                    value: to_metric(*val),
                    rank,
                    percentile_rank: pct.value(),
                }
            })
            .collect();

        let min = numeric_values.iter().cloned().fold(f64::NAN, f64::min);
        let max = numeric_values.iter().cloned().fold(f64::NAN, f64::max);
        let median = percentile_value(&numeric_values, 50.0);
        let p95 = percentile_value(&numeric_values, 95.0);

        Self {
            benchmark_id: benchmark_id.to_string(),
            benchmark_name: benchmark_name.to_string(),
            metric_unit,
            metric_dimension,
            fiscal_year,
            unit_count: total,
            values,
            min: Some(min),
            max: Some(max),
            median,
            p95,
        }
    }
}

/// Compute the value at a given percentile using the nearest-rank method.
pub(crate) fn percentile_value(sorted: &[f64], percentile: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = ((percentile / 100.0) * sorted.len() as f64).ceil() as usize;
    let idx = rank.max(1).min(sorted.len()) - 1;
    Some(sorted[idx])
}
