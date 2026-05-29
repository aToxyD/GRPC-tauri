use crate::application::oversight::benchmarks::Benchmark;
use crate::application::oversight::context::ReportsContext;
use crate::application::oversight::metrics::StockCoverageDays;
use crate::application::oversight::types::{
    BenchmarkDistribution, Dimension, MetricValue, OversightMetric, UnitType,
};

/// Cross-unit benchmark for StockCoverageDays.
pub struct StockCoverageDaysBenchmark;

impl Benchmark for StockCoverageDaysBenchmark {
    type Metric = StockCoverageDays;

    fn id(&self) -> &'static str {
        "benchmark-stock-coverage-days"
    }

    fn name(&self) -> &'static str {
        "Stock Coverage Days (Cross-Unit)"
    }

    fn metric_id(&self) -> &'static str {
        StockCoverageDays.id()
    }

    fn compute(&self, ctx: &ReportsContext<'_>) -> Result<BenchmarkDistribution, String> {
        let unit_ids = ctx.unit_ids()?;
        let metric = StockCoverageDays;

        let mut raw = Vec::with_capacity(unit_ids.len());
        for uid in &unit_ids {
            let unit_ctx = ctx.with_unit_id(uid);
            match metric.compute(&unit_ctx)? {
                MetricValue::Days(v) => {
                    raw.push((uid.clone(), v as f64));
                }
                MetricValue::None => { /* skip units with no data */ }
                _ => {
                    return Err(format!(
                        "StockCoverageDays returned unexpected MetricValue for unit {}",
                        uid
                    ));
                }
            }
        }

        Ok(BenchmarkDistribution::compute(
            self.id(),
            self.name(),
            UnitType::Days,
            Dimension::Unit,
            ctx.fiscal_year(),
            raw,
            |v| MetricValue::Days(v as i64),
        ))
    }
}
