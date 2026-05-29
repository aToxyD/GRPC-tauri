use crate::application::oversight::context::ReportsContext;
use crate::application::oversight::types::{MetricValue, OversightMetric, UnitType, Dimension};

/// ConsumptionPerBeneficiary = total_consumption_value / total_beneficiaries
pub struct ConsumptionPerBeneficiary;

impl OversightMetric for ConsumptionPerBeneficiary {
    fn id(&self) -> &'static str { "consumption-per-beneficiary" }
    fn name(&self) -> &'static str { "Consumption Per Beneficiary" }
    fn formula(&self) -> &'static str { "total_consumption_value / total_beneficiaries" }
    fn unit(&self) -> UnitType { UnitType::Amount }
    fn dimension(&self) -> Dimension { Dimension::Unit }

    fn compute(&self, ctx: &ReportsContext<'_>) -> Result<MetricValue, String> {
        let summary = match ctx.fiscal_year_summary()? {
            Some(s) => s,
            None => return Ok(MetricValue::None),
        };
        if summary.total_beneficiaries == 0 {
            return Ok(MetricValue::None);
        }
        let value = summary.total_consumption_value / summary.total_beneficiaries as f64;
        Ok(MetricValue::Amount((value * 100.0).round() / 100.0))
    }
}
