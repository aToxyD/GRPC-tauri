use grpc_lib::application::oversight::context::ReportsContext;
use grpc_lib::application::oversight::metrics::{
    ClosureTimeliness, ConsumptionPerBeneficiary, FiscalComplianceScore, PercentileRanking,
    SlowMovingStock, StockCoverageDays,
};
use grpc_lib::application::oversight::types::{MetricValue, OversightMetric, UnitType, Dimension};
use grpc_lib::db::ConnectionFactory;

mod common;

// ---------------------------------------------------------------------------
// Trait metadata tests
// ---------------------------------------------------------------------------

#[test]
fn consumption_per_beneficiary_metadata() {
    let m = ConsumptionPerBeneficiary;
    assert_eq!(m.id(), "consumption-per-beneficiary");
    assert_eq!(m.unit(), UnitType::Amount);
    assert_eq!(m.dimension(), Dimension::Unit);
    assert!(m.formula().contains("consumption_value"));
}

#[test]
fn stock_coverage_days_metadata() {
    let m = StockCoverageDays;
    assert_eq!(m.id(), "stock-coverage-days");
    assert_eq!(m.unit(), UnitType::Days);
    assert_eq!(m.dimension(), Dimension::Unit);
}

#[test]
fn slow_moving_stock_metadata() {
    let m = SlowMovingStock;
    assert_eq!(m.id(), "slow-moving-stock");
    assert_eq!(m.unit(), UnitType::Count);
    assert_eq!(m.dimension(), Dimension::Product);
}

#[test]
fn fiscal_compliance_score_metadata() {
    let m = FiscalComplianceScore;
    assert_eq!(m.id(), "fiscal-compliance-score");
    assert_eq!(m.unit(), UnitType::Percentage);
    assert_eq!(m.dimension(), Dimension::Period);
}

#[test]
fn closure_timeliness_metadata() {
    let m = ClosureTimeliness;
    assert_eq!(m.id(), "closure-timeliness");
    assert_eq!(m.unit(), UnitType::Days);
    assert_eq!(m.dimension(), Dimension::Period);
}

#[test]
fn percentile_ranking_metadata() {
    let m = PercentileRanking;
    assert_eq!(m.id(), "percentile-ranking");
    assert_eq!(m.unit(), UnitType::Ratio);
    assert_eq!(m.dimension(), Dimension::Wilaya);
}

#[test]
fn all_kpi_ids_unique() {
    let metrics: Vec<&dyn OversightMetric> = vec![
        &ConsumptionPerBeneficiary,
        &StockCoverageDays,
        &SlowMovingStock,
        &FiscalComplianceScore,
        &ClosureTimeliness,
        &PercentileRanking,
    ];
    let mut ids: Vec<&str> = metrics.iter().map(|m| m.id()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), metrics.len(), "all KPI ids must be unique");
}

// ---------------------------------------------------------------------------
// Empty-state behavior (fresh DB)
// ---------------------------------------------------------------------------

fn empty_context(fiscal_year: i32) -> ReportsContext<'static> {
    let db = ConnectionFactory::new_for_test().expect("create test db");
    // Leak the Database to get a static lifetime for testing
    let db = Box::leak(Box::new(db));
    let executor = db.executor();
    ReportsContext::new(executor, fiscal_year)
}

#[test]
fn consumption_per_beneficiary_empty_db_returns_none() {
    let ctx = empty_context(2024);
    let result = ConsumptionPerBeneficiary.compute(&ctx).unwrap();
    assert_eq!(result, MetricValue::None);
}

#[test]
fn stock_coverage_days_empty_db_returns_none() {
    let ctx = empty_context(2024);
    let result = StockCoverageDays.compute(&ctx).unwrap();
    assert_eq!(result, MetricValue::None);
}

#[test]
fn slow_moving_stock_empty_db_returns_zero() {
    let ctx = empty_context(2024);
    let result = SlowMovingStock.compute(&ctx).unwrap();
    assert_eq!(result, MetricValue::Count(0));
}

#[test]
fn fiscal_compliance_score_empty_db_returns_zero() {
    let ctx = empty_context(2024);
    let result = FiscalComplianceScore.compute(&ctx).unwrap();
    // 0/365 * 100 = 0
    assert_eq!(result, MetricValue::Percentage(0.0));
}

#[test]
fn closure_timeliness_open_year_returns_none() {
    let ctx = empty_context(2024);
    let result = ClosureTimeliness.compute(&ctx).unwrap();
    assert_eq!(result, MetricValue::None);
}

#[test]
fn percentile_ranking_returns_none_for_now() {
    let ctx = empty_context(2024);
    let result = PercentileRanking.compute(&ctx).unwrap();
    assert_eq!(result, MetricValue::None);
}

// ---------------------------------------------------------------------------
// Reproducibility — identical empty DB produces identical outputs
// ---------------------------------------------------------------------------

#[test]
fn kpi_outputs_are_deterministic_across_empty_dbs() {
    let ctx1 = empty_context(2024);
    let ctx2 = empty_context(2024);

    let metrics: Vec<&dyn OversightMetric> = vec![
        &ConsumptionPerBeneficiary,
        &StockCoverageDays,
        &SlowMovingStock,
        &FiscalComplianceScore,
        &ClosureTimeliness,
        &PercentileRanking,
    ];

    for metric in &metrics {
        let r1 = metric.compute(&ctx1).unwrap();
        let r2 = metric.compute(&ctx2).unwrap();
        assert_eq!(
            r1, r2,
            "KPI {} produced different outputs on identical empty DB state",
            metric.id()
        );
    }
}

// ---------------------------------------------------------------------------
// MetricValue semantics
// ---------------------------------------------------------------------------

#[test]
fn metric_value_unit_type_mapping() {
    assert_eq!(MetricValue::Count(5).unit_type(), Some(UnitType::Count));
    assert_eq!(MetricValue::Amount(1.0).unit_type(), Some(UnitType::Amount));
    assert_eq!(MetricValue::Ratio(0.5).unit_type(), Some(UnitType::Ratio));
    assert_eq!(
        MetricValue::Percentage(95.0).unit_type(),
        Some(UnitType::Percentage)
    );
    assert_eq!(MetricValue::Days(30).unit_type(), Some(UnitType::Days));
    assert_eq!(MetricValue::None.unit_type(), None);
}

#[test]
fn metric_value_is_some() {
    assert!(MetricValue::Count(0).is_some());
    assert!(!MetricValue::None.is_some());
}
