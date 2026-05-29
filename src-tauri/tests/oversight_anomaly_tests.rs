use grpc_lib::application::oversight::anomalies::{
    compute_mean, compute_stddev, compute_z_score, max_severity, percentile_severity,
    z_score_severity, AnomalyBatch, AnomalyDetector, AnomalyReport, AnomalySeverity,
    PercentileOutlierDetector, ZScoreDetector,
};
use grpc_lib::application::oversight::types::{
    BenchmarkDistribution, Dimension, MetricValue, UnitType,
};

// ---------------------------------------------------------------------------
// Helper: build a distribution from (unit_id, value) pairs
// ---------------------------------------------------------------------------

fn make_distribution(values: Vec<(String, f64)>) -> BenchmarkDistribution {
    BenchmarkDistribution::compute(
        "test-benchmark",
        "Test Benchmark",
        UnitType::Ratio,
        Dimension::Unit,
        2025,
        values,
        MetricValue::Ratio,
    )
}

// ---------------------------------------------------------------------------
// Z-Score determinism tests
// ---------------------------------------------------------------------------

#[test]
fn z_score_determinism_identical_inputs_produce_identical_outputs() {
    let dist = make_distribution(vec![
        ("a".into(), 10.0),
        ("b".into(), 20.0),
        ("c".into(), 30.0),
    ]);
    let detector = ZScoreDetector;

    let batch1 = detector.detect(&dist);
    let batch2 = detector.detect(&dist);

    assert_eq!(batch1.reports.len(), batch2.reports.len());
    for (r1, r2) in batch1.reports.iter().zip(batch2.reports.iter()) {
        assert_eq!(r1.z_score, r2.z_score);
        assert_eq!(r1.severity, r2.severity);
        assert_eq!(r1.percentile_rank, r2.percentile_rank);
    }
}

#[test]
fn z_score_computation_known_values() {
    // values: 10, 20, 30  →  mean = 20, stddev = √( (100+0+100)/3 ) ≈ 8.164966
    let dist = make_distribution(vec![
        ("a".into(), 10.0),
        ("b".into(), 20.0),
        ("c".into(), 30.0),
    ]);
    let detector = ZScoreDetector;
    let batch = detector.detect(&dist);

    // unit-a: z = (10 - 20) / 8.164966 = -1.2247
    let a = batch.reports.iter().find(|r| r.unit_id == "a").unwrap();
    assert!((a.z_score - (-1.2247)).abs() < 0.001);
    assert!(a.observed_value == 10.0);

    // unit-b has z = 0
    let b = batch.reports.iter().find(|r| r.unit_id == "b").unwrap();
    assert!((b.z_score).abs() < 0.001);

    // unit-c: z = (30 - 20) / 8.164966 = 1.2247
    let c = batch.reports.iter().find(|r| r.unit_id == "c").unwrap();
    assert!((c.z_score - 1.2247).abs() < 0.001);
}

// ---------------------------------------------------------------------------
// Percentile boundary tests
// ---------------------------------------------------------------------------

#[test]
fn percentile_outlier_detector_identifies_extremes() {
    // 10 values: 1..10
    let values: Vec<(String, f64)> = (1..=10)
        .map(|i| (format!("unit-{}", i), i as f64))
        .collect();
    let dist = make_distribution(values);
    let detector = PercentileOutlierDetector;
    let batch = detector.detect(&dist);

    // unit-1 (value=1, p=5.0) and unit-10 (value=10, p=95.0) are at the boundary
    // For n=10, the percentile computation gives:
    //   unit-1: count_below=0, count_equal=1 → (0 + 0.5)/10*100 = 5.0
    //   unit-10: count_below=9, count_equal=1 → (9 + 0.5)/10*100 = 95.0
    // These are at the Warning boundary (p < 5.0 || p > 95.0) — so they should NOT be flagged
    // since 5.0 is NOT < 5.0, and 95.0 is NOT > 95.0
    let unit_1 = batch.reports.iter().find(|r| r.unit_id == "unit-1");
    let unit_10 = batch.reports.iter().find(|r| r.unit_id == "unit-10");
    assert!(unit_1.is_none(), "percentile rank 5.0 should not be flagged");
    assert!(unit_10.is_none(), "percentile rank 95.0 should not be flagged");
}

#[test]
fn percentile_detector_catches_outliers() {
    // 11 values: 1..10 (normal), then 0.5 and 100 (extreme)
    let mut values: Vec<(String, f64)> = (1..=10)
        .map(|i| (format!("normal-{}", i), i as f64))
        .collect();
    values.push(("low-outlier".into(), 0.5));
    values.push(("high-outlier".into(), 100.0));
    let dist = make_distribution(values);
    let detector = PercentileOutlierDetector;
    let batch = detector.detect(&dist);

    let low = batch.reports.iter().find(|r| r.unit_id == "low-outlier");
    let high = batch.reports.iter().find(|r| r.unit_id == "high-outlier");

    assert!(low.is_some(), "low outlier should be flagged");
    assert!(high.is_some(), "high outlier should be flagged");
    if let Some(r) = low {
        assert!(r.percentile_rank < 5.0);
    }
    if let Some(r) = high {
        assert!(r.percentile_rank > 95.0);
    }
}

// ---------------------------------------------------------------------------
// Severity model tests
// ---------------------------------------------------------------------------

#[test]
fn z_score_severity_classification() {
    assert_eq!(z_score_severity(0.0), AnomalySeverity::Normal);
    assert_eq!(z_score_severity(1.5), AnomalySeverity::Normal);
    assert_eq!(z_score_severity(2.0), AnomalySeverity::Warning);
    assert_eq!(z_score_severity(2.5), AnomalySeverity::Warning);
    assert_eq!(z_score_severity(3.0), AnomalySeverity::Critical);
    assert_eq!(z_score_severity(5.0), AnomalySeverity::Critical);
    assert_eq!(z_score_severity(-2.0), AnomalySeverity::Warning);
    assert_eq!(z_score_severity(-3.0), AnomalySeverity::Critical);
}

#[test]
fn percentile_severity_classification() {
    // Normal range
    assert_eq!(percentile_severity(5.0), AnomalySeverity::Normal);
    assert_eq!(percentile_severity(50.0), AnomalySeverity::Normal);
    assert_eq!(percentile_severity(95.0), AnomalySeverity::Normal);
    // Warning range
    assert_eq!(percentile_severity(4.9), AnomalySeverity::Warning);
    assert_eq!(percentile_severity(95.1), AnomalySeverity::Warning);
    // Critical range
    assert_eq!(percentile_severity(0.9), AnomalySeverity::Critical);
    assert_eq!(percentile_severity(99.1), AnomalySeverity::Critical);
}

#[test]
fn max_severity_combines_correctly() {
    assert_eq!(max_severity(AnomalySeverity::Normal, AnomalySeverity::Normal), AnomalySeverity::Normal);
    assert_eq!(max_severity(AnomalySeverity::Warning, AnomalySeverity::Normal), AnomalySeverity::Warning);
    assert_eq!(max_severity(AnomalySeverity::Critical, AnomalySeverity::Normal), AnomalySeverity::Critical);
    assert_eq!(max_severity(AnomalySeverity::Warning, AnomalySeverity::Critical), AnomalySeverity::Critical);
}

// ---------------------------------------------------------------------------
// Edge case tests
// ---------------------------------------------------------------------------

#[test]
fn empty_population_returns_empty_batch() {
    let dist = make_distribution(vec![]);
    let z_detector = ZScoreDetector;
    let p_detector = PercentileOutlierDetector;

    let z_batch = z_detector.detect(&dist);
    let p_batch = p_detector.detect(&dist);

    assert!(z_batch.is_empty());
    assert!(p_batch.is_empty());
    assert_eq!(z_batch.total_units, 0);
    assert_eq!(p_batch.total_units, 0);
}

#[test]
fn single_unit_population_has_no_anomalies() {
    let dist = make_distribution(vec![("only".into(), 42.0)]);
    let z_detector = ZScoreDetector;
    let p_detector = PercentileOutlierDetector;

    let z_batch = z_detector.detect(&dist);
    let p_batch = p_detector.detect(&dist);

    // With 1 unit, stddev = 0, z-score = 0, percentile = 50
    // Both should show Normal
    assert!(z_batch.is_empty());
    assert!(p_batch.is_empty());
    assert_eq!(z_batch.total_units, 1);
    assert_eq!(p_batch.total_units, 1);
}

#[test]
fn zero_stddev_population_produces_no_anomalies() {
    let dist = make_distribution(vec![
        ("a".into(), 10.0),
        ("b".into(), 10.0),
        ("c".into(), 10.0),
    ]);
    let z_detector = ZScoreDetector;

    let batch = z_detector.detect(&dist);
    assert!(batch.is_empty());
    // All z-scores should be 0
    for report in &batch.reports {
        assert!((report.z_score).abs() < f64::EPSILON);
    }
}

// ---------------------------------------------------------------------------
// Serialization reproducibility tests
// ---------------------------------------------------------------------------

#[test]
fn anomaly_report_serialization_round_trip() {
    let report = AnomalyReport {
        metric_id: "m1".into(),
        unit_id: "u1".into(),
        observed_value: 42.0,
        population_mean: 30.0,
        population_stddev: 5.0,
        z_score: 2.4,
        percentile_rank: 95.5,
        severity: AnomalySeverity::Warning,
        explanation: "Z-score 2.40: value 42.00 is 2.40σ above mean 30.00.".into(),
        fiscal_year: 2025,
        benchmark_id: "b1".into(),
    };

    let json = serde_json::to_string(&report).expect("serialize");
    let deserialized: AnomalyReport = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(report.metric_id, deserialized.metric_id);
    assert_eq!(report.unit_id, deserialized.unit_id);
    assert_eq!(report.z_score, deserialized.z_score);
    assert_eq!(report.severity, deserialized.severity);
    assert_eq!(report.explanation, deserialized.explanation);
}

#[test]
fn anomaly_batch_serialization_round_trip() {
    let mut batch = AnomalyBatch::new(2025, "b1", 2);
    batch.push(AnomalyReport {
        metric_id: "m1".into(),
        unit_id: "u1".into(),
        observed_value: 100.0,
        population_mean: 50.0,
        population_stddev: 10.0,
        z_score: 5.0,
        percentile_rank: 99.9,
        severity: AnomalySeverity::Critical,
        explanation: "test".into(),
        fiscal_year: 2025,
        benchmark_id: "b1".into(),
    });

    let json = serde_json::to_string(&batch).expect("serialize");
    let deserialized: AnomalyBatch = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(batch.fiscal_year, deserialized.fiscal_year);
    assert_eq!(batch.critical_count, deserialized.critical_count);
    assert_eq!(batch.reports.len(), deserialized.reports.len());
}

// ---------------------------------------------------------------------------
// Identical DB state reproducibility (no DB needed — distribution is the state)
// ---------------------------------------------------------------------------

#[test]
fn identical_distribution_produces_identical_anomaly_batch() {
    let values: Vec<(String, f64)> = (1..=10)
        .map(|i| (format!("u-{:02}", i), (i * 10) as f64))
        .collect();
    let dist = make_distribution(values.clone());

    let detector = ZScoreDetector;
    let batch1 = detector.detect(&dist);

    // Rebuild identical distribution
    let dist2 = make_distribution(values);
    let batch2 = detector.detect(&dist2);

    assert_eq!(batch1.reports.len(), batch2.reports.len());
    for (r1, r2) in batch1.reports.iter().zip(batch2.reports.iter()) {
        assert_eq!(r1.z_score, r2.z_score);
        assert_eq!(r1.severity, r2.severity);
        assert_eq!(r1.percentile_rank, r2.percentile_rank);
        assert_eq!(r1.explanation, r2.explanation);
    }
}

// ---------------------------------------------------------------------------
// Helper function determinism tests
// ---------------------------------------------------------------------------

#[test]
fn compute_mean_and_stddev_known_values() {
    let values = vec![10.0, 20.0, 30.0];
    let mean = compute_mean(&values).unwrap();
    assert!((mean - 20.0).abs() < f64::EPSILON);

    let stddev = compute_stddev(&values, mean);
    let variance: f64 = (100.0 + 0.0 + 100.0) / 3.0;
    let expected = variance.sqrt();
    assert!((stddev - expected).abs() < 1e-10);
}

#[test]
fn compute_z_score_zero_stddev_returns_zero() {
    let z = compute_z_score(42.0, 50.0, 0.0);
    assert!((z).abs() < f64::EPSILON);
}

// ---------------------------------------------------------------------------
// NaN / Inf edge case tests
// ---------------------------------------------------------------------------

#[test]
fn nan_values_do_not_panic() {
    let values: Vec<(String, f64)> = vec![
        ("a".into(), f64::NAN),
        ("b".into(), 10.0),
        ("c".into(), 20.0),
    ];
    let dist = make_distribution(values);
    let z_batch = ZScoreDetector.detect(&dist);
    let p_batch = PercentileOutlierDetector.detect(&dist);

    // NaN should be filtered out; only 2 usable values remain
    assert_eq!(z_batch.total_units, 3, "total_units reflects raw population");
    assert_eq!(z_batch.reports.len(), 2, "NaN filtered, 2 valid reports");
    assert!(p_batch.is_empty(), "no percentile outliers in [10, 20]");
}

#[test]
fn infinite_values_do_not_panic() {
    let values: Vec<(String, f64)> = vec![
        ("a".into(), f64::INFINITY),
        ("b".into(), f64::NEG_INFINITY),
        ("c".into(), 10.0),
    ];
    let dist = make_distribution(values);
    let z_batch = ZScoreDetector.detect(&dist);
    let p_batch = PercentileOutlierDetector.detect(&dist);

    assert_eq!(z_batch.total_units, 3, "total_units reflects raw population");
    assert_eq!(z_batch.reports.len(), 1, "Inf/-Inf filtered, 1 valid report");
    assert!(p_batch.is_empty(), "single valid value has no outliers");
}

// ---------------------------------------------------------------------------
// AnomalyBatch counter correctness
// ---------------------------------------------------------------------------

#[test]
fn anomaly_batch_counts_correctly() {
    let mut batch = AnomalyBatch::new(2025, "b1", 5);

    // Push 1 normal, 2 warning, 1 critical = 3 anomalies
    let base = AnomalyReport {
        metric_id: "m1".into(),
        unit_id: "u1".into(),
        observed_value: 0.0,
        population_mean: 0.0,
        population_stddev: 1.0,
        z_score: 0.0,
        percentile_rank: 50.0,
        severity: AnomalySeverity::Normal,
        explanation: "normal".into(),
        fiscal_year: 2025,
        benchmark_id: "b1".into(),
    };

    batch.push(AnomalyReport { severity: AnomalySeverity::Normal, ..base.clone() });
    batch.push(AnomalyReport { severity: AnomalySeverity::Warning, unit_id: "u2".into(), ..base.clone() });
    batch.push(AnomalyReport { severity: AnomalySeverity::Warning, unit_id: "u3".into(), ..base.clone() });
    batch.push(AnomalyReport { severity: AnomalySeverity::Critical, unit_id: "u4".into(), ..base.clone() });
    batch.push(AnomalyReport { severity: AnomalySeverity::Normal, unit_id: "u5".into(), ..base.clone() });

    assert_eq!(batch.total_units, 5);
    assert_eq!(batch.anomaly_count, 3);
    assert_eq!(batch.warning_count, 2);
    assert_eq!(batch.critical_count, 1);
    assert!(!batch.is_empty());
}
