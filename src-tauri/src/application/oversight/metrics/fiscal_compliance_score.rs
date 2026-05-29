use crate::application::oversight::context::ReportsContext;
use crate::application::oversight::types::{MetricValue, OversightMetric, UnitType, Dimension};

/// FiscalComplianceScore = (actual_daily_reports / expected_daily_reports) × 100
///
/// Expected daily reports = 365 (or 366 for leap year, but we use 365 for determinism).
/// Actual = daily_report_count from FiscalYearSummary.
pub struct FiscalComplianceScore;

impl OversightMetric for FiscalComplianceScore {
    fn id(&self) -> &'static str { "fiscal-compliance-score" }
    fn name(&self) -> &'static str { "Fiscal Compliance Score" }
    fn formula(&self) -> &'static str { "(daily_report_count / 365) × 100" }
    fn unit(&self) -> UnitType { UnitType::Percentage }
    fn dimension(&self) -> Dimension { Dimension::Period }

    fn compute(&self, ctx: &ReportsContext<'_>) -> Result<MetricValue, String> {
        let summary = match ctx.fiscal_year_summary()? {
            Some(s) => s,
            None => return Ok(MetricValue::Percentage(0.0)),
        };
        let expected: f64 = 365.0;
        let score = (summary.daily_report_count as f64 / expected) * 100.0;
        let rounded = (score * 100.0).round() / 100.0;
        Ok(MetricValue::Percentage(rounded))
    }
}
