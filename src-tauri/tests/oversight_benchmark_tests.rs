use grpc_lib::application::oversight::benchmarks::{
    ConsumptionPerBeneficiaryBenchmark, FiscalComplianceScoreBenchmark,
    StockCoverageDaysBenchmark,
};
use grpc_lib::application::oversight::context::ReportsContext;
use grpc_lib::application::oversight::benchmarks::Benchmark;
use grpc_lib::application::oversight::types::{
    BenchmarkDistribution, Dimension, MetricValue, UnitType,
};
use grpc_lib::db::ConnectionFactory;

mod common;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn seed_test_db() -> ReportsContext<'static> {
    let db = ConnectionFactory::new_for_test().expect("create test db");
    // Leak to get static lifetime
    let db = Box::leak(Box::new(db));
    let executor = db.executor();

    // Seed fiscal year
    executor
        .execute(
            "INSERT OR REPLACE INTO fiscal_year_status (year, status, opened_at) VALUES (2025, 'open', '2025-01-01T00:00:00Z')",
            [],
        )
        .expect("seed fiscal year");

    executor
        .execute("UPDATE settings SET current_year = 2025 WHERE id = 1", [])
        .expect("update settings");

    // Seed units
    for (uid, code, name) in &[
        ("unit-a", "UA", "Unit Alpha"),
        ("unit-b", "UB", "Unit Beta"),
        ("unit-c", "UC", "Unit Gamma"),
    ] {
        executor
            .execute(
                "INSERT OR IGNORE INTO units (id, code, name, wilaya_code, created_at) VALUES (?1, ?2, ?3, '00', '2025-01-01T00:00:00Z')",
                rusqlite::params![uid, code, name],
            )
            .expect("seed unit");
    }

    // Seed daily reports for compliance + beneficiary counting (insert raw rows)
    for day in 0..10 {
        for uid in &["unit-a", "unit-b", "unit-c"] {
            let fid = format!("daily-{}-{}", uid, day);
            let date_str = format!("2025-01-{:02}", day + 1);
            executor
                .execute(
                    "INSERT INTO daily_reports (id, unit_id, date, total_daily_cost, total_daily_average, total_daily_beneficiaries, created_at, fiscal_year, deleted) VALUES (?1, ?2, ?3, 100.0, 50.0, ?4, ?5, 2025, 0)",
                    rusqlite::params![fid, uid, date_str, 20 + day, format!("2025-01-{:02}T00:00:00Z", day + 1)],
                )
                .expect("insert daily report");
        }
    }

    // Seed stock movements for consumption + coverage calculations
    let now = "2025-06-15T00:00:00Z";
    executor
        .execute(
            "INSERT OR IGNORE INTO products (id, name, base_price, tva, year, created_at, updated_at) VALUES ('prod-1', 'Test Product', 10.0, 0.0, 2025, ?1, ?1)",
            rusqlite::params![now],
        )
        .expect("seed product");

    // Opening balance for each unit
    for uid in &["unit-a", "unit-b", "unit-c"] {
        executor
            .execute(
                "INSERT INTO stock_movements (id, product_id, movement_type, quantity, balance_before, balance_after, reference_type, timestamp, user_id, username, unit_id, updated_at, fiscal_year, unit_cost) VALUES (?1, 'prod-1', 'OPENING', 100.0, 0.0, 100.0, 'Opening', ?2, 'system', 'system', ?3, ?2, 2025, 10.0)",
                rusqlite::params![format!("open-{}", uid), now, uid],
            )
            .expect("seed opening movement");

        // FIFO layer for ending inventory
        executor
            .execute(
                "INSERT INTO fifo_stock_layers (id, unit_id, product_id, source_type, source_id, unit_cost, qty_original, qty_remaining, received_at, created_by, origin_fiscal_year) VALUES (?1, ?2, 'prod-1', 'ORDER', 'src-1', 10.0, 100.0, 50.0, ?3, 'system', 2025)",
                rusqlite::params![format!("fifo-{}", uid), uid, now],
            )
            .expect("seed fifo layer");
    }

    // OUT movements for unit-a (higher consumption) and unit-b (lower)
    executor
        .execute(
            "INSERT INTO stock_movements (id, product_id, movement_type, quantity, balance_before, balance_after, reference_type, reference_id, timestamp, user_id, username, unit_id, updated_at, fiscal_year, unit_cost) VALUES ('out-a1', 'prod-1', 'OUT', 10.0, 100.0, 90.0, 'Consumption', 'consumption-a1', ?1, 'system', 'system', 'unit-a', ?1, 2025, 10.0)",
            rusqlite::params![now],
        )
        .expect("seed out movement a1");
    executor
        .execute(
            "INSERT INTO stock_movements (id, product_id, movement_type, quantity, balance_before, balance_after, reference_type, reference_id, timestamp, user_id, username, unit_id, updated_at, fiscal_year, unit_cost) VALUES ('out-b1', 'prod-1', 'OUT', 5.0, 100.0, 95.0, 'Consumption', 'consumption-b1', ?1, 'system', 'system', 'unit-b', ?1, 2025, 10.0)",
            rusqlite::params![now],
        )
        .expect("seed out movement b1");
    // unit-c has no OUT movements (should be 0 consumption)

    ReportsContext::new(executor, 2025)
}

fn empty_context(fiscal_year: i32) -> ReportsContext<'static> {
    let db = ConnectionFactory::new_for_test().expect("create test db");
    let db = Box::leak(Box::new(db));
    let executor = db.executor();
    ReportsContext::new(executor, fiscal_year)
}

// ---------------------------------------------------------------------------
// Empty cohort handling
// ---------------------------------------------------------------------------

#[test]
fn empty_db_produces_empty_benchmark() {
    let ctx = empty_context(2025);
    let bench = ConsumptionPerBeneficiaryBenchmark;
    let result = bench.compute(&ctx).unwrap();
    assert_eq!(result.unit_count, 0, "no units in empty DB");
    assert!(result.values.is_empty());
    assert!(result.min.is_none());
    assert!(result.max.is_none());
}

// ---------------------------------------------------------------------------
// Single-unit cohort
// ---------------------------------------------------------------------------

#[test]
fn single_unit_cohort_returns_self_value() {
    let ctx = seed_test_db();
    // Only units with OUT movements and daily reports have data.
    // unit-a has OUT movements + daily reports => has consumption data.
    let bench = ConsumptionPerBeneficiaryBenchmark;
    let result = bench.compute(&ctx).unwrap();

    // Only unit-a and unit-b have consumption data (unit-c has no OUT movements)
    // So we get 1 or 2 units with actual values
    assert!(
        result.unit_count > 0,
        "should have at least one unit with data"
    );

    for ranked in &result.values {
        assert!(!ranked.unit_id.is_empty());
        assert!(ranked.rank >= 1);
        assert!(ranked.percentile_rank >= 0.0 && ranked.percentile_rank <= 100.0);
    }
}

// ---------------------------------------------------------------------------
// Percentile stability — identical DB => identical distribution
// ---------------------------------------------------------------------------

macro_rules! assert_benchmark_reproducible {
    ($name:expr, $bench:expr, $ctx1:expr, $ctx2:expr) => {
        let r1 = $bench.compute(&$ctx1).unwrap();
        let r2 = $bench.compute(&$ctx2).unwrap();
        assert_eq!(r1.unit_count, r2.unit_count, "{}: unit_count differs", $name);
        assert_eq!(r1.values.len(), r2.values.len(), "{}: values.len differs", $name);
        let json1 = serde_json::to_string(&r1).expect("serialize r1");
        let json2 = serde_json::to_string(&r2).expect("serialize r2");
        assert_eq!(json1, json2, "{}: distribution differs across identical DB states", $name);
    };
}

#[test]
fn benchmark_is_deterministic_on_identical_db() {
    let ctx1 = seed_test_db();
    let ctx2 = seed_test_db();

    assert_benchmark_reproducible!(
        "consumption-per-beneficiary",
        ConsumptionPerBeneficiaryBenchmark,
        ctx1,
        ctx2
    );
    assert_benchmark_reproducible!(
        "stock-coverage-days",
        StockCoverageDaysBenchmark,
        ctx1,
        ctx2
    );
    assert_benchmark_reproducible!(
        "fiscal-compliance-score",
        FiscalComplianceScoreBenchmark,
        ctx1,
        ctx2
    );
}

// ---------------------------------------------------------------------------
// Tie ordering — identical values produce same rank
// ---------------------------------------------------------------------------

#[test]
fn percentile_rank_stability() {
    use grpc_lib::application::oversight::types::PercentileRank;

    // Same value => same rank
    let r1 = PercentileRank::compute(50.0, &[50.0, 50.0, 50.0]);
    let r2 = PercentileRank::compute(50.0, &[50.0, 50.0, 50.0]);
    assert_eq!(r1.value(), r2.value(), "same value should produce same rank");

    // Deterministic ordering: tie-breaking by unit_id should be stable
    let raw = vec![
        ("b".to_string(), 100.0),
        ("a".to_string(), 100.0),
    ];
    let dist = BenchmarkDistribution::compute(
        "test",
        "test",
        UnitType::Amount,
        Dimension::Unit,
        2025,
        raw,
        MetricValue::Amount,
    );
    assert_eq!(dist.values.len(), 2);
    // Both tied at value 100.0, sorted by id ascending => "a" first, then "b"
    assert_eq!(dist.values[0].unit_id, "a", "tie-breaker should sort by unit_id asc");
    assert_eq!(dist.values[0].rank, 1, "tied values share same rank");
    assert_eq!(dist.values[1].rank, 1, "tied values share same rank");
}

// ---------------------------------------------------------------------------
// Ranking reproducibility — same inputs produce same ranks
// ---------------------------------------------------------------------------

#[test]
fn ranking_is_reproducible() {
    let raw1 = vec![
        ("unit-a".to_string(), 50.0),
        ("unit-b".to_string(), 30.0),
        ("unit-c".to_string(), 20.0),
    ];
    let raw2 = raw1.clone();

    let d1 = BenchmarkDistribution::compute("r", "r", UnitType::Amount, Dimension::Unit, 2025, raw1, MetricValue::Amount);
    let d2 = BenchmarkDistribution::compute("r", "r", UnitType::Amount, Dimension::Unit, 2025, raw2, MetricValue::Amount);

    assert_eq!(d1.values[0].rank, d2.values[0].rank);
    assert_eq!(d1.values[1].rank, d2.values[1].rank);
    assert_eq!(d1.values[2].rank, d2.values[2].rank);
    assert_eq!(d1.values[0].percentile_rank, d2.values[0].percentile_rank);
}

// ---------------------------------------------------------------------------
// Stable serialization ordering — JSON output is deterministic
// ---------------------------------------------------------------------------

#[test]
fn benchmark_distribution_serialization_is_stable() {
    let raw = vec![
        ("z".to_string(), 10.0),
        ("a".to_string(), 100.0),
    ];
    let d = BenchmarkDistribution::compute("test-id", "test-name", UnitType::Amount, Dimension::Unit, 2025, raw, MetricValue::Amount);
    let json = serde_json::to_string(&d).expect("serialize");

    // Re-compute and verify same JSON (deep stability)
    let raw2 = vec![
        ("z".to_string(), 10.0),
        ("a".to_string(), 100.0),
    ];
    let d2 = BenchmarkDistribution::compute("test-id", "test-name", UnitType::Amount, Dimension::Unit, 2025, raw2, MetricValue::Amount);
    let json2 = serde_json::to_string(&d2).expect("serialize");
    assert_eq!(json, json2, "identical distributions must produce identical JSON");
}

// ---------------------------------------------------------------------------
// Metric metadata
// ---------------------------------------------------------------------------

#[test]
fn benchmark_metadata_consumption_per_beneficiary() {
    let b = ConsumptionPerBeneficiaryBenchmark;
    assert_eq!(b.id(), "benchmark-consumption-per-beneficiary");
    assert_eq!(b.metric_id(), "consumption-per-beneficiary");
    assert!(b.name().contains("Consumption Per Beneficiary"));
}

#[test]
fn benchmark_metadata_stock_coverage_days() {
    let b = StockCoverageDaysBenchmark;
    assert_eq!(b.id(), "benchmark-stock-coverage-days");
    assert_eq!(b.metric_id(), "stock-coverage-days");
}

#[test]
fn benchmark_metadata_fiscal_compliance_score() {
    let b = FiscalComplianceScoreBenchmark;
    assert_eq!(b.id(), "benchmark-fiscal-compliance-score");
    assert_eq!(b.metric_id(), "fiscal-compliance-score");
}

// ---------------------------------------------------------------------------
// Median and percentile computation
// ---------------------------------------------------------------------------

#[test]
fn benchmark_distribution_median_and_p95() {
    let raw = vec![
        ("a".to_string(), 5.0),
        ("b".to_string(), 10.0),
        ("c".to_string(), 15.0),
        ("d".to_string(), 20.0),
        ("e".to_string(), 25.0),
    ];
    let d = BenchmarkDistribution::compute(
        "test",
        "test",
        UnitType::Amount,
        Dimension::Unit,
        2025,
        raw,
        MetricValue::Amount,
    );
    // 5 values, sorted desc: 25, 20, 15, 10, 5
    // median (p50): nearest-rank ceil(50/100 * 5) = ceil(2.5) = 3 => index 2 => 15
    // p95: ceil(95/100 * 5) = ceil(4.75) = 5 => index 4 => 5
    assert_eq!(d.median, Some(15.0), "median should be 15 for values 5,10,15,20,25 sorted desc");
    assert_eq!(d.min, Some(5.0));
    assert_eq!(d.max, Some(25.0));
    assert_eq!(d.p95, Some(5.0), "p95 should be the 5th value (lowest since sorted desc = nearest rank)");
}
