use crate::application::oversight::benchmarks::Benchmark;
use crate::application::oversight::context::ReportsContext;
use crate::application::oversight::metrics::ConsumptionPerBeneficiary;
use crate::application::oversight::types::{
    BenchmarkDistribution, Dimension, MetricValue, OversightMetric, UnitType,
};

/// Cross-unit benchmark for ConsumptionPerBeneficiary.
pub struct ConsumptionPerBeneficiaryBenchmark;

impl Benchmark for ConsumptionPerBeneficiaryBenchmark {
    type Metric = ConsumptionPerBeneficiary;

    fn id(&self) -> &'static str {
        "benchmark-consumption-per-beneficiary"
    }

    fn name(&self) -> &'static str {
        "Consumption Per Beneficiary (Cross-Unit)"
    }

    fn metric_id(&self) -> &'static str {
        ConsumptionPerBeneficiary.id()
    }

    fn compute(&self, ctx: &ReportsContext<'_>) -> Result<BenchmarkDistribution, String> {
        let unit_ids = ctx.unit_ids()?;
        let metric = ConsumptionPerBeneficiary;

        let mut raw = Vec::with_capacity(unit_ids.len());
        for uid in &unit_ids {
            let unit_ctx = ctx.with_unit_id(uid);
            match metric.compute(&unit_ctx)? {
                MetricValue::Amount(v) => {
                    raw.push((uid.clone(), v));
                }
                MetricValue::None => { /* skip units with no data */ }
                _ => {
                    return Err(format!(
                        "ConsumptionPerBeneficiary returned unexpected MetricValue for unit {}",
                        uid
                    ));
                }
            }
        }

        Ok(BenchmarkDistribution::compute(
            self.id(),
            self.name(),
            UnitType::Amount,
            Dimension::Unit,
            ctx.fiscal_year(),
            raw,
            MetricValue::Amount,
        ))
    }
}
