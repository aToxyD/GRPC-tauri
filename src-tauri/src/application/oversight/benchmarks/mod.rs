/// Cross-unit benchmark computation layer.
///
/// Benchmarks are pure deterministic functions that compute statistical
/// distributions of KPI values across a population of units.
/// No SQL, no mutations, no side effects.
pub mod consumption_per_beneficiary_benchmark;
pub mod fiscal_compliance_score_benchmark;
pub mod stock_coverage_days_benchmark;

pub use consumption_per_beneficiary_benchmark::ConsumptionPerBeneficiaryBenchmark;
pub use fiscal_compliance_score_benchmark::FiscalComplianceScoreBenchmark;
pub use stock_coverage_days_benchmark::StockCoverageDaysBenchmark;

use crate::application::oversight::context::ReportsContext;
use crate::application::oversight::types::{BenchmarkDistribution, OversightMetric};

/// A benchmark computes a cross-unit distribution for a single KPI.
///
/// Each benchmark:
/// - Identifies all units via the context
/// - Computes the KPI per unit
/// - Ranks results with deterministic tie-breaking
/// - Returns a BenchmarkDistribution with min, max, median, p95
pub trait Benchmark {
    type Metric: OversightMetric;
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn metric_id(&self) -> &'static str;
    fn compute(&self, ctx: &ReportsContext<'_>) -> Result<BenchmarkDistribution, String>;
}
