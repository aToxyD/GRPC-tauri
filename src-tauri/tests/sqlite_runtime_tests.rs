use grpc_lib::infrastructure::sqlite_observability::integrity::{
    IntegrityCheckResult, IntegritySeverity, QuickCheckResult,
};
use grpc_lib::infrastructure::sqlite_runtime::backup_validation::*;
use grpc_lib::infrastructure::sqlite_runtime::checkpoint::*;
use grpc_lib::infrastructure::sqlite_runtime::idle_checkpoint::*;
use grpc_lib::infrastructure::sqlite_runtime::integrity_runner::*;
use grpc_lib::infrastructure::sqlite_runtime::policies::CheckpointPolicy;
use grpc_lib::infrastructure::sqlite_runtime::runtime_metrics::*;

// ─────────────────────────────────────────────────────────────
// 1. Checkpoint Eligibility Determinism
// ─────────────────────────────────────────────────────────────

#[test]
fn checkpoint_eligibility_determinism() {
    let policy = CheckpointPolicy::default();
    let e1 = WalCheckpointExecutor::check_eligibility(100, 100, 1, &policy);
    let e2 = WalCheckpointExecutor::check_eligibility(100, 100, 1, &policy);
    assert_eq!(
        matches!(e1, CheckpointEligibility::Eligible),
        matches!(e2, CheckpointEligibility::Eligible)
    );
}

#[test]
fn checkpoint_not_eligible_no_pages() {
    let policy = CheckpointPolicy::default();
    let eligibility = WalCheckpointExecutor::check_eligibility(0, 0, 0, &policy);
    assert!(matches!(
        eligibility,
        CheckpointEligibility::NotEligible { .. }
    ));
}

#[test]
fn checkpoint_not_eligible_empty_wal() {
    let policy = CheckpointPolicy::default();
    let eligibility = WalCheckpointExecutor::check_eligibility(100, 100, 0, &policy);
    assert!(matches!(
        eligibility,
        CheckpointEligibility::NotEligible { .. }
    ));
}

#[test]
fn checkpoint_eligible_with_seqno() {
    let policy = CheckpointPolicy::default();
    let eligibility = WalCheckpointExecutor::check_eligibility(100, 100, 1, &policy);
    assert!(matches!(eligibility, CheckpointEligibility::Eligible));
}

#[test]
fn checkpoint_eligible_with_wal_size_exceeding_threshold() {
    let policy = CheckpointPolicy {
        max_wal_size_bytes: 100 * 4096,
        ..CheckpointPolicy::default()
    };
    let eligibility = WalCheckpointExecutor::check_eligibility(200, 200, 1, &policy);
    assert!(matches!(eligibility, CheckpointEligibility::Eligible));
}

// ─────────────────────────────────────────────────────────────
// 2. WAL Checkpoint Determinism
// ─────────────────────────────────────────────────────────────

#[test]
fn wal_checkpoint_result_determinism() {
    let r1 = CheckpointResult {
        pages_before: 100,
        pages_after: 90,
        pages_moved: 10,
        pages_before_checkpoint: 5,
        mode: CheckpointMode::Truncate,
        wal_size_before_bytes: 409600,
        wal_size_after_bytes: 0,
        checkpoint_seqno_before: 3,
        checkpoint_seqno_after: 0,
    };
    let r2 = CheckpointResult {
        pages_before: 100,
        pages_after: 90,
        pages_moved: 10,
        pages_before_checkpoint: 5,
        mode: CheckpointMode::Truncate,
        wal_size_before_bytes: 409600,
        wal_size_after_bytes: 0,
        checkpoint_seqno_before: 3,
        checkpoint_seqno_after: 0,
    };
    assert_eq!(r1, r2);
}

// ─────────────────────────────────────────────────────────────
// 3. Idle Threshold Logic
// ─────────────────────────────────────────────────────────────

#[test]
fn idle_below_threshold_no_checkpoint() {
    let policy = CheckpointPolicy::default();
    let window = IdleWindow::new(10);
    let eligibility = CheckpointEligibility::Eligible;
    let decision = IdleCheckpointEvaluator::evaluate(&window, &eligibility, &policy);
    assert!(matches!(
        decision,
        IdleCheckpointDecision::ShouldNotCheckpoint { .. }
    ));
}

#[test]
fn idle_at_threshold_checkpoint() {
    let policy = CheckpointPolicy::default();
    let window = IdleWindow::new(30);
    let eligibility = CheckpointEligibility::Eligible;
    let decision = IdleCheckpointEvaluator::evaluate(&window, &eligibility, &policy);
    assert!(matches!(decision, IdleCheckpointDecision::ShouldCheckpoint));
}

#[test]
fn idle_not_eligible_skips_checkpoint() {
    let policy = CheckpointPolicy::default();
    let window = IdleWindow::new(30);
    let eligibility = CheckpointEligibility::NotEligible {
        reason: "test".into(),
    };
    let decision = IdleCheckpointEvaluator::evaluate(&window, &eligibility, &policy);
    assert!(matches!(
        decision,
        IdleCheckpointDecision::ShouldNotCheckpoint { .. }
    ));
}

#[test]
fn idle_threshold_determinism() {
    let policy = CheckpointPolicy::default();
    let window = IdleWindow::new(45);
    let eligibility = CheckpointEligibility::Eligible;
    let d1 = IdleCheckpointEvaluator::evaluate(&window, &eligibility, &policy);
    let d2 = IdleCheckpointEvaluator::evaluate(&window, &eligibility, &policy);
    assert_eq!(
        matches!(d1, IdleCheckpointDecision::ShouldCheckpoint),
        matches!(d2, IdleCheckpointDecision::ShouldCheckpoint)
    );
}

#[test]
fn idle_checkpoint_disabled_by_policy() {
    let policy = CheckpointPolicy {
        idle_checkpoint_enabled: false,
        ..CheckpointPolicy::default()
    };
    let window = IdleWindow::new(30);
    let eligibility = CheckpointEligibility::Eligible;
    let decision = IdleCheckpointEvaluator::evaluate(&window, &eligibility, &policy);
    assert!(matches!(
        decision,
        IdleCheckpointDecision::ShouldNotCheckpoint { .. }
    ));
}

// ─────────────────────────────────────────────────────────────
// 4. Integrity Execution Parsing
// ─────────────────────────────────────────────────────────────

#[test]
fn integrity_execution_result_ok() {
    let result = IntegrityExecutionResult {
        check_result: IntegrityCheckResult::parse("ok"),
        quick_check_result: QuickCheckResult::parse("ok"),
        severity: IntegritySeverity::Ok,
        execution_order: 1,
    };
    assert_eq!(result.severity, IntegritySeverity::Ok);
    assert!(result.check_result.passed);
    assert!(result.quick_check_result.passed);
}

#[test]
fn integrity_execution_parses_integrity_check_errors() {
    let raw = "row 5 missing from index idx_products\nwrong page 42 in table products";
    let check = IntegrityCheckResult::parse(raw);
    let quick = QuickCheckResult::parse("ok");
    let result = IntegrityExecutionResult {
        check_result: check,
        quick_check_result: quick,
        severity: IntegritySeverity::Warning,
        execution_order: 1,
    };
    assert!(!result.check_result.passed);
    assert_eq!(result.check_result.issues.len(), 2);
}

#[test]
fn integrity_execution_parses_quick_check_errors() {
    let check = IntegrityCheckResult::parse("ok");
    let quick = QuickCheckResult::parse("wrong page 42 in table products");
    let result = IntegrityExecutionResult {
        check_result: check,
        quick_check_result: quick,
        severity: IntegritySeverity::Error,
        execution_order: 1,
    };
    assert!(!result.quick_check_result.passed);
    assert!(result.quick_check_result.issue.is_some());
}

#[test]
fn integrity_issue_ordering_deterministic() {
    let raw = "ok\nrow 5 missing from index idx_b\nrow 3 missing from index idx_a\nwrong page 1";
    let r1 = IntegrityExecutionResult {
        check_result: IntegrityCheckResult::parse(raw),
        quick_check_result: QuickCheckResult::parse("ok"),
        severity: IntegritySeverity::Warning,
        execution_order: 1,
    };
    let r2 = IntegrityExecutionResult {
        check_result: IntegrityCheckResult::parse(raw),
        quick_check_result: QuickCheckResult::parse("ok"),
        severity: IntegritySeverity::Warning,
        execution_order: 1,
    };
    assert_eq!(r1, r2);
    assert_eq!(r1.check_result.issues[0].line_number, 2);
    assert_eq!(r2.check_result.issues[0].line_number, 2);
}

// ─────────────────────────────────────────────────────────────
// 5. Backup Validation Behavior
// ─────────────────────────────────────────────────────────────

#[test]
fn backup_validation_valid_db_passes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute("CREATE TABLE t (id INTEGER PRIMARY KEY)", [])
            .unwrap();
        conn.execute("INSERT INTO t VALUES (1)", []).unwrap();
    }
    let result = BackupValidationRunner::validate(&path).unwrap();
    assert!(result.passed);
    assert!(result.issues.is_empty());
    assert!(result.page_count > 0);
    assert!(result.page_size > 0);
}

#[test]
fn backup_validation_invalid_file_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid.bak");
    std::fs::write(&path, b"not a database").unwrap();
    let result = BackupValidationRunner::validate(&path);
    assert!(result.is_err());
}

#[test]
fn backup_validation_non_existent_file_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nonexistent.db");
    let result = BackupValidationRunner::validate(&path);
    assert!(result.is_err());
}

// ─────────────────────────────────────────────────────────────
// 6. Bounded History Eviction
// ─────────────────────────────────────────────────────────────

#[test]
fn wal_growth_trend_bounded_history() {
    let mut trend = WalGrowthTrend::new(5);
    for i in 0..20 {
        trend.record(WalGrowthSample {
            page_count: i,
            wal_size_bytes: i * 4096,
            checkpoint_seqno: i,
            recorded_at_order: i,
        });
    }
    assert_eq!(trend.len(), 5);
}

#[test]
fn integrity_history_bounded_history() {
    let mut history = IntegrityExecutionHistory::new(3);
    for i in 0..10 {
        history.record(IntegrityExecutionResult {
            check_result: IntegrityCheckResult::parse("ok"),
            quick_check_result: QuickCheckResult::parse("ok"),
            severity: IntegritySeverity::Ok,
            execution_order: i as u64,
        });
    }
    assert_eq!(history.len(), 3);
}

#[test]
fn checkpoint_history_bounded_history() {
    let mut history = CheckpointExecutionHistory::new(3);
    for i in 0..10 {
        history.record(CheckpointResult {
            pages_before: 100,
            pages_after: 90,
            pages_moved: 10,
            pages_before_checkpoint: 5,
            mode: CheckpointMode::Truncate,
            wal_size_before_bytes: 409600,
            wal_size_after_bytes: 0,
            checkpoint_seqno_before: i,
            checkpoint_seqno_after: 0,
        });
    }
    assert_eq!(history.len(), 3);
}

// ─────────────────────────────────────────────────────────────
// 7. Deterministic Ordering in History
// ─────────────────────────────────────────────────────────────

#[test]
fn wal_growth_samples_ordered_by_recorded_at() {
    let mut trend = WalGrowthTrend::new(10);
    trend.record(WalGrowthSample {
        page_count: 3,
        wal_size_bytes: 100,
        checkpoint_seqno: 0,
        recorded_at_order: 3,
    });
    trend.record(WalGrowthSample {
        page_count: 1,
        wal_size_bytes: 50,
        checkpoint_seqno: 0,
        recorded_at_order: 1,
    });
    trend.record(WalGrowthSample {
        page_count: 2,
        wal_size_bytes: 75,
        checkpoint_seqno: 0,
        recorded_at_order: 2,
    });
    let ordered = trend.samples_ordered();
    assert_eq!(ordered[0].recorded_at_order, 1);
    assert_eq!(ordered[1].recorded_at_order, 2);
    assert_eq!(ordered[2].recorded_at_order, 3);
}

#[test]
fn checkpoint_history_ordered_by_seqno() {
    let mut history = CheckpointExecutionHistory::new(10);
    history.record(CheckpointResult {
        pages_before: 100,
        pages_after: 90,
        pages_moved: 10,
        pages_before_checkpoint: 5,
        mode: CheckpointMode::Truncate,
        wal_size_before_bytes: 0,
        wal_size_after_bytes: 0,
        checkpoint_seqno_before: 3,
        checkpoint_seqno_after: 0,
    });
    history.record(CheckpointResult {
        pages_before: 100,
        pages_after: 90,
        pages_moved: 10,
        pages_before_checkpoint: 5,
        mode: CheckpointMode::Truncate,
        wal_size_before_bytes: 0,
        wal_size_after_bytes: 0,
        checkpoint_seqno_before: 1,
        checkpoint_seqno_after: 0,
    });
    let ordered = history.checkpoints_ordered();
    assert_eq!(ordered[0].checkpoint_seqno_before, 1);
    assert_eq!(ordered[1].checkpoint_seqno_before, 3);
}

// ─────────────────────────────────────────────────────────────
// 8. Serde Round-trips
// ─────────────────────────────────────────────────────────────

#[test]
fn serde_round_trip_checkpoint_result() {
    let result = CheckpointResult {
        pages_before: 100,
        pages_after: 90,
        pages_moved: 10,
        pages_before_checkpoint: 5,
        mode: CheckpointMode::Full,
        wal_size_before_bytes: 409600,
        wal_size_after_bytes: 0,
        checkpoint_seqno_before: 2,
        checkpoint_seqno_after: 0,
    };
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: CheckpointResult = serde_json::from_str(&json).unwrap();
    assert_eq!(result, deserialized);
}

#[test]
fn serde_round_trip_checkpoint_mode() {
    for mode in &[
        CheckpointMode::Passive,
        CheckpointMode::Full,
        CheckpointMode::Restart,
        CheckpointMode::Truncate,
    ] {
        let json = serde_json::to_string(mode).unwrap();
        let deserialized: CheckpointMode = serde_json::from_str(&json).unwrap();
        assert_eq!(*mode, deserialized);
    }
}

#[test]
fn serde_round_trip_checkpoint_eligibility() {
    let eligible = CheckpointEligibility::Eligible;
    let not_eligible = CheckpointEligibility::NotEligible {
        reason: "test reason".into(),
    };
    for val in &[eligible, not_eligible] {
        let json = serde_json::to_string(val).unwrap();
        let deserialized: CheckpointEligibility = serde_json::from_str(&json).unwrap();
        assert_eq!(*val, deserialized);
    }
}

#[test]
fn serde_round_trip_idle_decision() {
    let should = IdleCheckpointDecision::ShouldCheckpoint;
    let should_not = IdleCheckpointDecision::ShouldNotCheckpoint {
        reason: "too soon".into(),
    };
    for val in &[should, should_not] {
        let json = serde_json::to_string(val).unwrap();
        let deserialized: IdleCheckpointDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(*val, deserialized);
    }
}

#[test]
fn serde_round_trip_integrity_execution_result() {
    let result = IntegrityExecutionResult {
        check_result: IntegrityCheckResult::parse("row 5 missing from index idx_products"),
        quick_check_result: QuickCheckResult::parse("ok"),
        severity: IntegritySeverity::Warning,
        execution_order: 42,
    };
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: IntegrityExecutionResult = serde_json::from_str(&json).unwrap();
    assert_eq!(result, deserialized);
}

#[test]
fn serde_round_trip_backup_validation_result() {
    let result = BackupValidationResult {
        passed: true,
        issues: vec![],
        integrity_raw_output: "ok".into(),
        page_count: 42,
        page_size: 4096,
    };
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: BackupValidationResult = serde_json::from_str(&json).unwrap();
    assert_eq!(result, deserialized);
}

#[test]
fn serde_round_trip_backup_validation_failure() {
    let failure = BackupValidationFailure {
        reason: "cannot open file".into(),
    };
    let json = serde_json::to_string(&failure).unwrap();
    let deserialized: BackupValidationFailure = serde_json::from_str(&json).unwrap();
    assert_eq!(failure, deserialized);
}

#[test]
fn serde_round_trip_policy() {
    let policy = CheckpointPolicy::default();
    let json = serde_json::to_string(&policy).unwrap();
    let deserialized: CheckpointPolicy = serde_json::from_str(&json).unwrap();
    assert_eq!(policy, deserialized);
}

#[test]
fn serde_round_trip_wal_growth_trend() {
    let mut trend = WalGrowthTrend::new(5);
    trend.record(WalGrowthSample {
        page_count: 100,
        wal_size_bytes: 409600,
        checkpoint_seqno: 1,
        recorded_at_order: 1,
    });
    let json = serde_json::to_string(&trend).unwrap();
    let deserialized: WalGrowthTrend = serde_json::from_str(&json).unwrap();
    assert_eq!(trend.len(), deserialized.len());
}

#[test]
fn serde_round_trip_health_snapshot() {
    let integrity = IntegrityExecutionHistory::new(5);
    let checkpoint = CheckpointExecutionHistory::new(5);
    let trend = WalGrowthTrend::new(5);
    let snap = RuntimeHealthSnapshot::new(&integrity, &checkpoint, &trend, 1);
    let json = serde_json::to_string(&snap).unwrap();
    let deserialized: RuntimeHealthSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(snap.snapshot_order, deserialized.snapshot_order);
    assert_eq!(snap.checkpoint_count, deserialized.checkpoint_count);
}

// ─────────────────────────────────────────────────────────────
// 9. Reproducibility
// ─────────────────────────────────────────────────────────────

#[test]
fn same_input_produces_same_checkpoint_decision() {
    fn make_decision() -> CheckpointEligibility {
        WalCheckpointExecutor::check_eligibility(100, 100, 1, &CheckpointPolicy::default())
    }
    let d1 = make_decision();
    let d2 = make_decision();
    assert_eq!(
        matches!(d1, CheckpointEligibility::Eligible),
        matches!(d2, CheckpointEligibility::Eligible)
    );
}

#[test]
fn same_input_produces_same_idle_decision() {
    fn make_decision() -> IdleCheckpointDecision {
        let policy = CheckpointPolicy::default();
        let window = IdleWindow::new(30);
        let eligibility = CheckpointEligibility::Eligible;
        IdleCheckpointEvaluator::evaluate(&window, &eligibility, &policy)
    }
    let d1 = make_decision();
    let d2 = make_decision();
    assert_eq!(
        matches!(d1, IdleCheckpointDecision::ShouldCheckpoint),
        matches!(d2, IdleCheckpointDecision::ShouldCheckpoint)
    );
}

#[test]
fn same_input_produces_same_runtime_decision() {
    fn make_snapshot() -> RuntimeHealthSnapshot {
        let integrity = IntegrityExecutionHistory::new(10);
        let checkpoint = CheckpointExecutionHistory::new(10);
        let trend = WalGrowthTrend::new(10);
        RuntimeHealthSnapshot::new(&integrity, &checkpoint, &trend, 1)
    }
    let s1 = make_snapshot();
    let s2 = make_snapshot();
    assert_eq!(s1.snapshot_order, s2.snapshot_order);
    assert_eq!(s1.checkpoint_count, s2.checkpoint_count);
}
