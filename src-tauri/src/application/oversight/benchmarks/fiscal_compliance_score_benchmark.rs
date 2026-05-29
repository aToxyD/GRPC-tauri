use crate::application::oversight::benchmarks::Benchmark;
use crate::application::oversight::context::ReportsContext;
use crate::application::oversight::metrics::FiscalComplianceScore;
use crate::application::oversight::types::{
    BenchmarkDistribution, Dimension, MetricValue, OversightMetric, UnitType,
};

/// Cross-unit benchmark for FiscalComplianceScore.
pub struct FiscalComplianceScoreBenchmark;

impl Benchmark for FiscalComplianceScoreBenchmark {
    type Metric = FiscalComplianceScore;

    fn id(&self) -> &'static str {
        "benchmark-fiscal-compliance-score"
    }

    fn name(&self) -> &'static str {
        "Fiscal Compliance Score (Cross-Unit)"
    }

    fn metric_id(&self) -> &'static str {
        FiscalComplianceScore.id()
    }

    fn compute(&self, ctx: &ReportsContext<'_>) -> Result<BenchmarkDistribution, String> {
        let unit_ids = ctx.unit_ids()?;
        let metric = FiscalComplianceScore;

        let mut raw = Vec::with_capacity(unit_ids.len());
        for uid in &unit_ids {
            let unit_ctx = ctx.with_unit_id(uid);
            match metric.compute(&unit_ctx)? {
                MetricValue::Percentage(v) => {
                    raw.push((uid.clone(), v));
                }
                MetricValue::None => { /* skip units with no data */ }
                _ => {
                    return Err(format!(
                        "FiscalComplianceScore returned unexpected MetricValue for unit {}",
                        uid
                    ));
                }
            }
        }

        Ok(BenchmarkDistribution::compute(
            self.id(),
            self.name(),
            UnitType::Percentage,
            Dimension::Period,
            ctx.fiscal_year(),
            raw,
            MetricValue::Percentage,
        ))
    }
}
