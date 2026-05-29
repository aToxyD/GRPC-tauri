use crate::application::oversight::context::ReportsContext;
use crate::application::oversight::types::{Dimension, MetricValue, OversightMetric, UnitType};

/// StockCoverageDays = ending_inventory_value / (total_consumption_value / 365)
pub struct StockCoverageDays;

impl OversightMetric for StockCoverageDays {
    fn id(&self) -> &'static str {
        "stock-coverage-days"
    }
    fn name(&self) -> &'static str {
        "Stock Coverage Days"
    }
    fn formula(&self) -> &'static str {
        "ending_inventory_value / (total_consumption_value / 365)"
    }
    fn unit(&self) -> UnitType {
        UnitType::Days
    }
    fn dimension(&self) -> Dimension {
        Dimension::Unit
    }

    fn compute(&self, ctx: &ReportsContext<'_>) -> Result<MetricValue, String> {
        let summary = match ctx.fiscal_year_summary()? {
            Some(s) => s,
            None => return Ok(MetricValue::None),
        };
        if summary.total_consumption_value <= 0.0 {
            return Ok(MetricValue::None);
        }
        let avg_daily_consumption = summary.total_consumption_value / 365.0;
        let days = summary.ending_inventory_value / avg_daily_consumption;
        Ok(MetricValue::Days((days.round()) as i64))
    }
}
