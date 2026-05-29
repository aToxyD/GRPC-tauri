use grpc_lib::infrastructure::sqlite_observability::diagnostics::*;
use grpc_lib::infrastructure::sqlite_observability::integrity::*;
use grpc_lib::infrastructure::sqlite_observability::metrics::PageGrowthMetrics;
use grpc_lib::infrastructure::sqlite_observability::query_plan::*;
use grpc_lib::infrastructure::sqlite_observability::slow_query::*;
use grpc_lib::infrastructure::sqlite_observability::wal::*;

// ─────────────────────────────────────────────────────────────
// 1. Query Plan Parsing Determinism
// ─────────────────────────────────────────────────────────────

#[test]
fn query_plan_parsing_determinism() {
    let rows = vec![
        (3, 0, 0, "SCAN TABLE products".into()),
        (5, 0, 0, "SEARCH TABLE inventory_stocks USING INDEX idx_stock_product (product_id=?)".into()),
    ];

    let plan1 = QueryPlan::from_explain_output("SELECT * FROM products", rows.clone());
    let plan2 = QueryPlan::from_explain_output("SELECT * FROM products", rows);

    assert_eq!(plan1, plan2);
    assert_eq!(plan1.sql_hash, plan2.sql_hash);
}

// ─────────────────────────────────────────────────────────────
// 2. Full Table Scan Detection
// ─────────────────────────────────────────────────────────────

#[test]
fn detects_full_table_scans() {
    let rows = vec![
        (3, 0, 0, "SCAN TABLE products".into()),
    ];
    let plan = QueryPlan::from_explain_output("SELECT * FROM products", rows);
    assert!(plan.has_full_table_scan);
    assert_eq!(plan.scan_count, 1);
    assert_ne!(plan.severity, ScanSeverity::None);
}

#[test]
fn no_full_table_scan_when_using_index() {
    let rows = vec![
        (2, 0, 0, "SEARCH TABLE products USING INDEX idx_products_id (id=?)".into()),
        (4, 0, 0, "SEARCH TABLE inventory_stocks USING COVERING INDEX idx_stock_product (product_id=?)".into()),
    ];
    let plan = QueryPlan::from_explain_output("SELECT * FROM products WHERE id = ?", rows);
    assert!(!plan.has_full_table_scan);
    assert_eq!(plan.severity, ScanSeverity::None);
}

// ─────────────────────────────────────────────────────────────
// 3. Index Recommendation Stability
// ─────────────────────────────────────────────────────────────

#[test]
fn index_recommendation_stability() {
    let rows = vec![
        (3, 0, 0, "SCAN TABLE products".into()),
    ];
    let recs1 = QueryPlan::from_explain_output("SELECT * FROM products", rows.clone()).recommendations();
    let recs2 = QueryPlan::from_explain_output("SELECT * FROM products", rows).recommendations();

    assert_eq!(recs1, recs2);
    assert_eq!(recs1.len(), 1);
    assert_eq!(recs1[0].reason, IndexReason::FullTableScan);
}

// ─────────────────────────────────────────────────────────────
// 4. Slow Query Categorization
// ─────────────────────────────────────────────────────────────

#[test]
fn slow_query_categorization() {
    let mut record = SlowQueryRecord::new(100);
    record.record("SELECT * FROM products", 200, 50);
    record.record("INSERT INTO products (id) VALUES (1)", 150, 1);
    record.record("DELETE FROM products WHERE id = 1", 300, 1);

    assert_eq!(record.by_category(QueryCategory::Select).len(), 1);
    assert_eq!(record.by_category(QueryCategory::Insert).len(), 1);
    assert_eq!(record.by_category(QueryCategory::Delete).len(), 1);
}

// ─────────────────────────────────────────────────────────────
// 5. WAL Metrics Serialization
// ─────────────────────────────────────────────────────────────

#[test]
fn wal_metrics_serialization() {
    let metrics = WalMetrics::new(65536, 200, 10, 1, WalState::CheckpointRequired, 1000);
    let json = serde_json::to_string(&metrics).unwrap();
    let deserialized: WalMetrics = serde_json::from_str(&json).unwrap();
    assert_eq!(metrics, deserialized);
    assert!(json.contains("checkpoint_seqno"));
    assert!(json.contains("is_eligible_for_checkpoint"));
}

// ─────────────────────────────────────────────────────────────
// 6. Integrity Parsing
// ─────────────────────────────────────────────────────────────

#[test]
fn integrity_check_parsing_ok() {
    let result = IntegrityCheckResult::parse("ok");
    assert!(result.passed);
    assert!(result.issues.is_empty());
}

#[test]
fn integrity_check_parsing_errors() {
    let raw = "row 5 missing from index idx_products\nwrong page 42 in table products";
    let result = IntegrityCheckResult::parse(raw);
    assert!(!result.passed);
    assert_eq!(result.issues.len(), 2);
}

#[test]
fn quick_check_parsing_ok() {
    let result = QuickCheckResult::parse("ok");
    assert!(result.passed);
    assert!(result.issue.is_none());
}

#[test]
fn quick_check_parsing_failure() {
    let result = QuickCheckResult::parse("wrong page 42 in table products");
    assert!(!result.passed);
    assert!(result.issue.is_some());
}

// ─────────────────────────────────────────────────────────────
// 7. Diagnostics Snapshot Reproducibility
// ─────────────────────────────────────────────────────────────

#[test]
fn diagnostics_snapshot_reproducibility() {
    let wal = WalMetrics::new(4096, 100, 5, 0, WalState::Ok, 1000);
    let check = IntegrityCheckResult::parse("ok");
    let quick = QuickCheckResult::parse("ok");
    let integrity = IntegritySnapshot::new(check, quick);

    let mut slow = SlowQueryRecord::new(10);
    slow.record("SELECT * FROM t", 200, 10);

    let rows = vec![(3, 0, 0, "SCAN TABLE products".into())];
    let diag = QueryDiagnostics::new("SELECT * FROM products", rows);
    let growth = PageGrowthMetrics::new(200, 100, 10, 5);

    let snap1 = SqliteDiagnosticsSnapshot::new(
        Some(wal.clone()),
        Some(integrity.clone()),
        Some(&slow),
        vec![diag.clone()],
        Some(growth.clone()),
        1,
    );

    let snap2 = SqliteDiagnosticsSnapshot::new(
        Some(wal),
        Some(integrity),
        Some(&slow),
        vec![diag],
        Some(growth),
        1,
    );

    assert_eq!(snap1, snap2);
    assert_eq!(snap1.snapshot_id, snap2.snapshot_id);
}

// ─────────────────────────────────────────────────────────────
// 8. Bounded History Eviction
// ─────────────────────────────────────────────────────────────

#[test]
fn bounded_history_eviction() {
    let mut record = SlowQueryRecord::new(5);
    for i in 0..20 {
        record.record(&format!("SELECT {}", i), 200, 0);
    }
    assert_eq!(record.len(), 5);
}

// ─────────────────────────────────────────────────────────────
// 9. Deterministic Ordering
// ─────────────────────────────────────────────────────────────

#[test]
fn deterministic_ordering_in_slow_queries() {
    let mut record = SlowQueryRecord::new(10);
    record.record("SELECT c", 200, 0);
    record.record("SELECT a", 200, 0);
    record.record("SELECT b", 200, 0);

    let samples = record.samples();
    assert_eq!(samples[0].sql, "SELECT c");
    assert_eq!(samples[1].sql, "SELECT a");
    assert_eq!(samples[2].sql, "SELECT b");
}

#[test]
fn deterministic_ordering_in_integrity_issues() {
    let raw = "row 5 missing from index idx_b\nrow 3 missing from index idx_a\nwrong page 1";
    let result1 = IntegrityCheckResult::parse(raw);
    let result2 = IntegrityCheckResult::parse(raw);
    assert_eq!(result1, result2);
}

// ─────────────────────────────────────────────────────────────
// 10. Serde Round-trips
// ─────────────────────────────────────────────────────────────

#[test]
fn serde_round_trip_query_plan() {
    let rows = vec![(3, 0, 0, "SCAN TABLE products".into())];
    let plan = QueryPlan::from_explain_output("SELECT * FROM products", rows);
    let json = serde_json::to_string(&plan).unwrap();
    let deserialized: QueryPlan = serde_json::from_str(&json).unwrap();
    assert_eq!(plan, deserialized);
}

#[test]
fn serde_round_trip_slow_query_record() {
    let mut record = SlowQueryRecord::new(10);
    record.record("SELECT * FROM t", 200, 5);
    let json = serde_json::to_string(&record).unwrap();
    let deserialized: SlowQueryRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(record.len(), deserialized.len());
    assert_eq!(record.samples(), deserialized.samples());
}

#[test]
fn serde_round_trip_integrity_result() {
    let result = IntegrityCheckResult::parse("row 5 missing from index idx_products");
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: IntegrityCheckResult = serde_json::from_str(&json).unwrap();
    assert_eq!(result, deserialized);
}

#[test]
fn serde_round_trip_diagnostics_snapshot() {
    let wal = WalMetrics::new(4096, 100, 5, 0, WalState::Ok, 1000);
    let check = IntegrityCheckResult::parse("ok");
    let quick = QuickCheckResult::parse("ok");
    let integrity = IntegritySnapshot::new(check, quick);
    let growth = PageGrowthMetrics::new(200, 100, 10, 5);

    let snap = SqliteDiagnosticsSnapshot::new(
        Some(wal),
        Some(integrity),
        None,
        vec![],
        Some(growth),
        42,
    );
    let json = serde_json::to_string(&snap).unwrap();
    let deserialized: SqliteDiagnosticsSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(snap, deserialized);
}

// ─────────────────────────────────────────────────────────────
// 11. Same Input => Same Diagnostics Snapshot
// ─────────────────────────────────────────────────────────────

#[test]
fn same_input_produces_same_snapshot() {
    fn make_snapshot(order: u64) -> SqliteDiagnosticsSnapshot {
        let wal = WalMetrics::new(4096, 100, 5, 0, WalState::Ok, 1000);
        let check = IntegrityCheckResult::parse("ok");
        let quick = QuickCheckResult::parse("ok");
        let integrity = IntegritySnapshot::new(check, quick);
        let growth = PageGrowthMetrics::new(200, 100, 10, 5);
        SqliteDiagnosticsSnapshot::new(
            Some(wal),
            Some(integrity),
            None,
            vec![],
            Some(growth),
            order,
        )
    }

    let snap1 = make_snapshot(1);
    let snap2 = make_snapshot(1);
    assert_eq!(snap1, snap2);
}

// ─────────────────────────────────────────────────────────────
// 12. Scan Severity Classification
// ─────────────────────────────────────────────────────────────

#[test]
fn scan_severity_none() {
    let rows = vec![
        (2, 0, 0, "SEARCH TABLE products USING INDEX idx_id (id=?)".into()),
    ];
    let plan = QueryPlan::from_explain_output("SELECT * FROM products WHERE id = ?", rows);
    assert_eq!(plan.severity, ScanSeverity::None);
}

#[test]
fn scan_severity_high() {
    // 3 out of 4 are scans = ratio 0.75 → "High" (since > 0.5)
    let rows = vec![
        (1, 0, 0, "SCAN TABLE a".into()),
        (2, 0, 0, "SCAN TABLE b".into()),
        (3, 0, 0, "SCAN TABLE c".into()),
        (4, 0, 0, "SEARCH TABLE d USING INDEX idx_d (id=?)".into()),
    ];
    let plan = QueryPlan::from_explain_output("SELECT * FROM a,b,c,d", rows);
    assert_eq!(plan.severity, ScanSeverity::High);
}

// ─────────────────────────────────────────────────────────────
// 13. Integrity Severity Classification
// ─────────────────────────────────────────────────────────────

#[test]
fn integrity_snapshot_ok() {
    let check = IntegrityCheckResult::parse("ok");
    let quick = QuickCheckResult::parse("ok");
    let snapshot = IntegritySnapshot::new(check, quick);
    assert_eq!(snapshot.severity, IntegritySeverity::Ok);
}

#[test]
fn integrity_snapshot_warning() {
    let check = IntegrityCheckResult::parse("row 5 missing from index idx_products");
    let quick = QuickCheckResult::parse("ok");
    let snapshot = IntegritySnapshot::new(check, quick);
    assert_eq!(snapshot.severity, IntegritySeverity::Warning);
}

#[test]
fn integrity_snapshot_error() {
    let check = IntegrityCheckResult::parse("row 5 missing from index idx_products");
    let quick = QuickCheckResult::parse("wrong page 42");
    let snapshot = IntegritySnapshot::new(check, quick);
    assert_eq!(snapshot.severity, IntegritySeverity::Error);
}

// ─────────────────────────────────────────────────────────────
// 14. Page Growth Metrics
// ─────────────────────────────────────────────────────────────

#[test]
fn page_growth_metrics_positive() {
    let growth = PageGrowthMetrics::new(300, 100, 15, 5);
    assert_eq!(growth.growth, 200);
    assert!((growth.growth_pct - 200.0).abs() < f64::EPSILON);
    assert_eq!(growth.freelist_growth, 10);
}

#[test]
fn page_growth_metrics_negative() {
    let growth = PageGrowthMetrics::new(50, 100, 2, 5);
    assert_eq!(growth.growth, -50);
    assert_eq!(growth.freelist_growth, -3);
}

// ─────────────────────────────────────────────────────────────
// 15. Diagnostics Health Checks
// ─────────────────────────────────────────────────────────────

#[test]
fn diagnostics_healthy_when_all_ok() {
    let wal = WalMetrics::new(4096, 100, 5, 0, WalState::Ok, 1000);
    let check = IntegrityCheckResult::parse("ok");
    let quick = QuickCheckResult::parse("ok");
    let integrity = IntegritySnapshot::new(check, quick);

    let snap = SqliteDiagnosticsSnapshot::new(
        Some(wal),
        Some(integrity),
        None,
        vec![],
        None,
        1,
    );
    assert!(snap.is_healthy());
    assert!(!snap.has_integrity_issues());
    assert!(!snap.has_slow_queries());
    assert!(!snap.has_query_warnings());
}

// ─────────────────────────────────────────────────────────────
// 16. WAL State Transitions
// ─────────────────────────────────────────────────────────────

#[test]
fn wal_state_ok_not_eligible() {
    let metrics = WalMetrics::from_pragma_values(4096, 100, 5, 0, 1, 1000);
    assert!(!metrics.is_eligible_for_checkpoint);
}

#[test]
fn wal_state_checkpoint_required_eligible() {
    let metrics = WalMetrics::from_pragma_values(65536, 100, 5, 1, 1, 1000);
    assert!(metrics.is_eligible_for_checkpoint);
}

#[test]
fn wal_state_overflow_eligible() {
    let metrics = WalMetrics::from_pragma_values(131072, 100, 5, 3, 1, 1000);
    assert!(metrics.is_eligible_for_checkpoint);
}

// ─────────────────────────────────────────────────────────────
// 17. Bounded Slow Query Record
// ─────────────────────────────────────────────────────────────

#[test]
fn slow_query_default_threshold() {
    let record = SlowQueryRecord::new(10);
    assert_eq!(record.threshold().as_millis(), 100);
}

#[test]
fn slow_query_custom_threshold() {
    let record = SlowQueryRecord::with_threshold(10, SlowQueryThreshold::Milliseconds(500));
    assert_eq!(record.threshold().as_millis(), 500);
}
