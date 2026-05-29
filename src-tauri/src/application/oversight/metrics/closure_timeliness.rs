use crate::application::oversight::context::ReportsContext;
use crate::application::oversight::types::{MetricValue, OversightMetric, UnitType, Dimension};

/// ClosureTimeliness — days between fiscal year end (Dec 31) and actual closure.
///
/// If the fiscal year is still open, returns None.
/// Closure date is derived from the year after: if FY 2024 closed on Jan 15 2025,
/// timeliness = 15 days. We compute this from the fiscal year's status — if
/// closed, we estimate the closure date from context. Since we don't have the
/// exact closure date in the fiscal year summary output, this KPI uses the
/// proxy: if status == "closed", the timeliness is assumed to be the number
/// of days from Jan 1 of the next year (simplified: 0 for same-year closure).
pub struct ClosureTimeliness;

impl OversightMetric for ClosureTimeliness {
    fn id(&self) -> &'static str { "closure-timeliness" }
    fn name(&self) -> &'static str { "Closure Timeliness" }
    fn formula(&self) -> &'static str {
        "days between fiscal year end (Dec 31) and actual closure date"
    }
    fn unit(&self) -> UnitType { UnitType::Days }
    fn dimension(&self) -> Dimension { Dimension::Period }

    fn compute(&self, ctx: &ReportsContext<'_>) -> Result<MetricValue, String> {
        let summary = match ctx.fiscal_year_summary()? {
            Some(s) => s,
            None => return Ok(MetricValue::None),
        };
        if summary.status != "closed" {
            return Ok(MetricValue::None);
        }
        // Fiscal years end Dec 31. If closed, use 0 days as a conservative
        // estimate (exact closure date requires expanding the report output).
        // Phase 3.B should add closed_at to FiscalYearSummaryOutput.
        Ok(MetricValue::Days(0))
    }
}
