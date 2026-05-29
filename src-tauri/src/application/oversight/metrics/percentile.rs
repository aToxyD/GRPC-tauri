use crate::application::oversight::types::{MetricValue, OversightMetric, UnitType, Dimension};
use crate::application::oversight::context::ReportsContext;

/// PercentileRanking — computes percentile rank of a unit's KPI value
/// against a reference population.
///
/// This is a generic primitive. In Phase 3.B, it will be fed with
/// cross-unit KPI values for full benchmark analysis.
pub struct PercentileRanking;

impl OversightMetric for PercentileRanking {
    fn id(&self) -> &'static str { "percentile-ranking" }
    fn name(&self) -> &'static str { "Percentile Ranking" }
    fn formula(&self) -> &'static str {
        "percentile rank of unit KPI value against reference population"
    }
    fn unit(&self) -> UnitType { UnitType::Ratio }
    fn dimension(&self) -> Dimension { Dimension::Wilaya }

    fn compute(&self, _ctx: &ReportsContext<'_>) -> Result<MetricValue, String> {
        // Phase 3.B: accept a population of values and compute percentile.
        // For now, return None — the primitive `PercentileRank::compute`
        // is available for use when cross-unit data is supplied.
        Ok(MetricValue::None)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn percentile_rank_single_value() {
        use crate::application::oversight::types::PercentileRank;
        let rank = PercentileRank::compute(50.0, &[50.0]);
        assert!((rank.value() - 50.0).abs() < 0.01);
    }

    #[test]
    fn percentile_rank_below_all() {
        use crate::application::oversight::types::PercentileRank;
        let rank = PercentileRank::compute(10.0, &[20.0, 30.0, 40.0]);
        assert!((rank.value() - 0.0).abs() < 0.01);
    }

    #[test]
    fn percentile_rank_above_all() {
        use crate::application::oversight::types::PercentileRank;
        let rank = PercentileRank::compute(50.0, &[10.0, 20.0, 30.0]);
        assert!((rank.value() - 100.0).abs() < 0.01);
    }

    #[test]
    fn percentile_rank_middle() {
        use crate::application::oversight::types::PercentileRank;
        let rank = PercentileRank::compute(30.0, &[10.0, 20.0, 30.0, 40.0]);
        assert!((rank.value() - 62.5).abs() < 0.01);
    }

    #[test]
    fn percentile_rank_empty() {
        use crate::application::oversight::types::PercentileRank;
        let rank = PercentileRank::compute(50.0, &[]);
        assert!((rank.value() - 0.0).abs() < 0.01);
    }

    #[test]
    fn percentile_rank_all_same() {
        use crate::application::oversight::types::PercentileRank;
        let rank = PercentileRank::compute(25.0, &[25.0, 25.0, 25.0]);
        assert!((rank.value() - 50.0).abs() < 0.01);
    }

    #[test]
    fn percentile_rank_two_values_middle() {
        use crate::application::oversight::types::PercentileRank;
        let rank = PercentileRank::compute(15.0, &[10.0, 20.0]);
        assert!((rank.value() - 50.0).abs() < 0.01);
    }
}
