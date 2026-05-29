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
