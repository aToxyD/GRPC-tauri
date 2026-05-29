use grpc_lib::infrastructure::sqlite_runtime_review::connection_model::*;
use grpc_lib::infrastructure::sqlite_runtime_review::contention::*;
use grpc_lib::infrastructure::sqlite_runtime_review::evaluation::*;
use grpc_lib::infrastructure::sqlite_runtime_review::metrics::*;
use grpc_lib::infrastructure::sqlite_runtime_review::read_connection::*;
use grpc_lib::infrastructure::sqlite_runtime_review::recommendations::*;
use grpc_lib::infrastructure::sqlite_runtime_review::runtime_orchestration::*;

// ─────────────────────────────────────────────────────────────
// 1. Determinism Tests
// ─────────────────────────────────────────────────────────────

#[test]
fn connection_topology_evaluation_determinism() {
    let e1 = ConnectionModelEvaluator::evaluate(false, 50, WalPressureLevel::Medium, 2, 1);
    let e2 = ConnectionModelEvaluator::evaluate(false, 50, WalPressureLevel::Medium, 2, 1);
    assert_eq!(e1, e2);
}

#[test]
fn contention_classification_determinism() {
    let a1 = ContentionClassifier::classify(50 * 1024 * 1024, 20, 40, 60, 1);
    let a2 = ContentionClassifier::classify(50 * 1024 * 1024, 20, 40, 60, 1);
    assert_eq!(a1, a2);
}

#[test]
fn read_connection_assessment_determinism() {
    let a1 = ReadConnectionSuitabilityEvaluator::evaluate(
        50,
        WalPressureLevel::Medium,
        ReadContentionLevel::Medium,
        2,
        1,
    );
    let a2 = ReadConnectionSuitabilityEvaluator::evaluate(
        50,
        WalPressureLevel::Medium,
        ReadContentionLevel::Medium,
        2,
        1,
    );
    assert_eq!(a1, a2);
}

#[test]
fn orchestration_assessment_determinism() {
    let a1 = OrchestrationEvaluator::evaluate(
        OrchestrationPolicy::PolicyDriven,
        50 * 1024 * 1024,
        15,
        150,
        100,
        WalPressureLevel::Medium,
        1,
    );
    let a2 = OrchestrationEvaluator::evaluate(
        OrchestrationPolicy::PolicyDriven,
        50 * 1024 * 1024,
        15,
        150,
        100,
        WalPressureLevel::Medium,
        1,
    );
    assert_eq!(a1, a2);
}

#[test]
fn full_evaluation_determinism() {
    let ctx = ReviewContext {
        has_read_connection: false,
        reporting_query_count: 50,
        concurrent_readers: 2,
        wal_size_bytes: 50 * 1024 * 1024,
        wal_checkpoint_seqno: 15,
        write_frequency: 50,
        reporting_frequency: 30,
        total_operations: 200,
        last_integrity_check_at: 100,
        orchestration_policy: OrchestrationPolicy::PolicyDriven,
        review_order: 1,
    };
    let e1 = ReviewEvaluator::evaluate(ctx.clone());
    let e2 = ReviewEvaluator::evaluate(ctx);
    assert_eq!(e1, e2);
}

// ─────────────────────────────────────────────────────────────
// 2. Recommendation Stability
// ─────────────────────────────────────────────────────────────

#[test]
fn recommendation_stability_same_input() {
    let ctx = ReviewContext {
        has_read_connection: false,
        reporting_query_count: 50,
        concurrent_readers: 2,
        wal_size_bytes: 50 * 1024 * 1024,
        wal_checkpoint_seqno: 15,
        write_frequency: 50,
        reporting_frequency: 30,
        total_operations: 200,
        last_integrity_check_at: 100,
        orchestration_policy: OrchestrationPolicy::PolicyDriven,
        review_order: 1,
    };
    let s1 = RecommendationEngine::generate(&ctx, 1);
    let s2 = RecommendationEngine::generate(&ctx, 1);
    assert_eq!(s1, s2);
}

#[test]
fn recommendation_stability_identical_output() {
    let set1 = RecommendationEngine::generate_with_context(
        false,
        50,
        2,
        50 * 1024 * 1024,
        15,
        50,
        30,
        200,
        100,
        1,
    );
    let set2 = RecommendationEngine::generate_with_context(
        false,
        50,
        2,
        50 * 1024 * 1024,
        15,
        50,
        30,
        200,
        100,
        1,
    );
    assert_eq!(set1, set2);
}

// ─────────────────────────────────────────────────────────────
// 3. Contention Classification
// ─────────────────────────────────────────────────────────────

#[test]
fn contention_classification_low_all_metrics_low() {
    let a = ContentionClassifier::classify(1024, 0, 5, 5, 1);
    assert_eq!(a.wal_pressure, WalPressureLevel::Low);
    assert_eq!(a.read_contention, ReadContentionLevel::Low);
    assert!(a.contention_score < 0.3);
}

#[test]
fn contention_classification_high_all_metrics_high() {
    let a = ContentionClassifier::classify(200 * 1024 * 1024, 50, 1000, 1000, 1);
    assert_eq!(a.wal_pressure, WalPressureLevel::High);
    assert_eq!(a.read_contention, ReadContentionLevel::High);
    assert!(a.contention_score > 0.3);
}

#[test]
fn contention_score_never_exceeds_one() {
    let a = ContentionClassifier::classify(200 * 1024 * 1024, 100, 2000, 2000, 1);
    assert!(a.contention_score <= 1.0);
}

// ─────────────────────────────────────────────────────────────
// 4. Serialization Round-trips
// ─────────────────────────────────────────────────────────────

#[test]
fn serde_round_trip_connection_topology() {
    let t = ConnectionTopology::SingleWriterWithReadConnection;
    let json = serde_json::to_string(&t).unwrap();
    let back: ConnectionTopology = serde_json::from_str(&json).unwrap();
    assert_eq!(t, back);
}

#[test]
fn serde_round_trip_contention_assessment() {
    let a = ContentionClassifier::classify(1024, 1, 10, 5, 42);
    let json = serde_json::to_string(&a).unwrap();
    let back: ContentionAssessment = serde_json::from_str(&json).unwrap();
    assert_eq!(a, back);
}

#[test]
fn serde_round_trip_read_connection_assessment() {
    let a = ReadConnectionSuitabilityEvaluator::evaluate(
        10,
        WalPressureLevel::Low,
        ReadContentionLevel::Low,
        0,
        42,
    );
    let json = serde_json::to_string(&a).unwrap();
    let back: ReadConnectionAssessment = serde_json::from_str(&json).unwrap();
    assert_eq!(a, back);
}

#[test]
fn serde_round_trip_orchestration_assessment() {
    let a = OrchestrationEvaluator::evaluate(
        OrchestrationPolicy::PolicyDriven,
        1024,
        1,
        100,
        50,
        WalPressureLevel::Low,
        42,
    );
    let json = serde_json::to_string(&a).unwrap();
    let back: OrchestrationAssessment = serde_json::from_str(&json).unwrap();
    assert_eq!(a, back);
}

#[test]
fn serde_round_trip_full_evaluation() {
    let ctx = ReviewContext {
        has_read_connection: false,
        reporting_query_count: 50,
        concurrent_readers: 2,
        wal_size_bytes: 50 * 1024 * 1024,
        wal_checkpoint_seqno: 15,
        write_frequency: 50,
        reporting_frequency: 30,
        total_operations: 200,
        last_integrity_check_at: 100,
        orchestration_policy: OrchestrationPolicy::PolicyDriven,
        review_order: 42,
    };
    let eval = ReviewEvaluator::evaluate(ctx);
    let json = serde_json::to_string(&eval).unwrap();
    let back: ConnectionReviewEvaluation = serde_json::from_str(&json).unwrap();
    assert_eq!(eval, back);
}

#[test]
fn serde_round_trip_recommendation_set() {
    let ctx = ReviewContext {
        has_read_connection: false,
        reporting_query_count: 100,
        concurrent_readers: 5,
        wal_size_bytes: 200 * 1024 * 1024,
        wal_checkpoint_seqno: 50,
        write_frequency: 200,
        reporting_frequency: 100,
        total_operations: 500,
        last_integrity_check_at: 50,
        orchestration_policy: OrchestrationPolicy::ManualOnly,
        review_order: 42,
    };
    let set = RecommendationEngine::generate(&ctx, 42);
    let json = serde_json::to_string(&set).unwrap();
    let back: RecommendationSet = serde_json::from_str(&json).unwrap();
    assert_eq!(set, back);
}

#[test]
fn serde_round_trip_review_metrics() {
    let mut metrics = ReviewMetricsSnapshot::new(5);
    let ctx = ReviewContext {
        has_read_connection: false,
        reporting_query_count: 50,
        concurrent_readers: 2,
        wal_size_bytes: 50 * 1024 * 1024,
        wal_checkpoint_seqno: 15,
        write_frequency: 50,
        reporting_frequency: 30,
        total_operations: 200,
        last_integrity_check_at: 100,
        orchestration_policy: OrchestrationPolicy::PolicyDriven,
        review_order: 1,
    };
    metrics.record_evaluation(ReviewEvaluator::evaluate(ctx));
    let json = serde_json::to_string(&metrics).unwrap();
    let back: ReviewMetricsSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(metrics.evaluation_count(), back.evaluation_count());
}

// ─────────────────────────────────────────────────────────────
// 5. Orchestration Reproducibility
// ─────────────────────────────────────────────────────────────

#[test]
fn orchestration_reproducibility_same_input() {
    fn make_assessment() -> OrchestrationAssessment {
        OrchestrationEvaluator::evaluate(
            OrchestrationPolicy::PolicyDriven,
            50 * 1024 * 1024,
            15,
            150,
            100,
            WalPressureLevel::Medium,
            1,
        )
    }
    let a1 = make_assessment();
    let a2 = make_assessment();
    assert_eq!(a1, a2);
}

#[test]
fn orchestration_reproducibility_checkpoint_decision() {
    fn needs_checkpoint() -> bool {
        OrchestrationEvaluator::is_checkpoint_recommended(
            50 * 1024 * 1024,
            15,
            WalPressureLevel::Medium,
        )
    }
    let r1 = needs_checkpoint();
    let r2 = needs_checkpoint();
    assert_eq!(r1, r2);
}

#[test]
fn orchestration_reproducibility_integrity_decision() {
    fn needs_integrity() -> bool {
        OrchestrationEvaluator::is_integrity_check_needed(200, 100)
    }
    let r1 = needs_integrity();
    let r2 = needs_integrity();
    assert_eq!(r1, r2);
}

// ─────────────────────────────────────────────────────────────
// 6. WAL Pressure Evaluation
// ─────────────────────────────────────────────────────────────

#[test]
fn wal_pressure_low_small_wal_no_seqno() {
    let p = ContentionClassifier::classify_wal_pressure(1024, 0);
    assert_eq!(p, WalPressureLevel::Low);
}

#[test]
fn wal_pressure_medium_moderate_wal() {
    let p = ContentionClassifier::classify_wal_pressure(50 * 1024 * 1024, 5);
    assert_eq!(p, WalPressureLevel::Medium);
}

#[test]
fn wal_pressure_medium_high_seqno() {
    let p = ContentionClassifier::classify_wal_pressure(1024, 15);
    assert_eq!(p, WalPressureLevel::Medium);
}

#[test]
fn wal_pressure_high_large_wal() {
    let p = ContentionClassifier::classify_wal_pressure(200 * 1024 * 1024, 1);
    assert_eq!(p, WalPressureLevel::High);
}

#[test]
fn wal_pressure_high_large_wal_and_high_seqno() {
    let p = ContentionClassifier::classify_wal_pressure(300 * 1024 * 1024, 100);
    assert_eq!(p, WalPressureLevel::High);
}

// ─────────────────────────────────────────────────────────────
// 7. Reporting Pressure Evaluation
// ─────────────────────────────────────────────────────────────

#[test]
fn read_connection_not_needed_low_load() {
    let suit = ReadConnectionSuitabilityEvaluator::evaluate(
        10,
        WalPressureLevel::Low,
        ReadContentionLevel::Low,
        0,
        1,
    );
    assert_eq!(suit.suitability, ReadConnectionSuitability::NotNeeded);
}

#[test]
fn read_connection_beneficial_moderate_load() {
    let suit = ReadConnectionSuitabilityEvaluator::evaluate(
        150,
        WalPressureLevel::Low,
        ReadContentionLevel::Low,
        3,
        1,
    );
    assert_eq!(suit.suitability, ReadConnectionSuitability::Beneficial);
}

#[test]
fn read_connection_recommended_high_pressure() {
    let suit = ReadConnectionSuitabilityEvaluator::evaluate(
        50,
        WalPressureLevel::Medium,
        ReadContentionLevel::Medium,
        2,
        1,
    );
    assert_eq!(suit.suitability, ReadConnectionSuitability::Recommended);
}

#[test]
fn reporting_pressure_evaluation_integrated() {
    let ctx = ReviewContext {
        has_read_connection: false,
        reporting_query_count: 100,
        concurrent_readers: 5,
        wal_size_bytes: 200 * 1024 * 1024,
        wal_checkpoint_seqno: 50,
        write_frequency: 200,
        reporting_frequency: 100,
        total_operations: 500,
        last_integrity_check_at: 50,
        orchestration_policy: OrchestrationPolicy::ManualOnly,
        review_order: 1,
    };
    let eval = ReviewEvaluator::evaluate(ctx);
    assert!(eval.multi_connection_recommended());
    assert!(eval.requires_orchestration_action());
}

#[test]
fn reporting_pressure_evaluation_with_high_contention() {
    let ctx = ReviewContext {
        has_read_connection: false,
        reporting_query_count: 100,
        concurrent_readers: 5,
        wal_size_bytes: 200 * 1024 * 1024,
        wal_checkpoint_seqno: 50,
        write_frequency: 1000,
        reporting_frequency: 1000,
        total_operations: 500,
        last_integrity_check_at: 50,
        orchestration_policy: OrchestrationPolicy::ManualOnly,
        review_order: 1,
    };
    let eval = ReviewEvaluator::evaluate(ctx);
    assert!(eval.multi_connection_recommended());
    assert!(eval.has_high_contention());
    assert!(eval.requires_orchestration_action());
}

#[test]
fn reporting_pressure_evaluation_contention_threshold_adjusted() {
    let ctx = ReviewContext {
        has_read_connection: false,
        reporting_query_count: 100,
        concurrent_readers: 5,
        wal_size_bytes: 200 * 1024 * 1024,
        wal_checkpoint_seqno: 50,
        write_frequency: 200,
        reporting_frequency: 100,
        total_operations: 500,
        last_integrity_check_at: 50,
        orchestration_policy: OrchestrationPolicy::ManualOnly,
        review_order: 1,
    };
    let eval = ReviewEvaluator::evaluate(ctx);
    assert!(eval.multi_connection_recommended());
    assert!(eval.requires_orchestration_action());
}

// ─────────────────────────────────────────────────────────────
// 8. Metrics Bounded History
// ─────────────────────────────────────────────────────────────

#[test]
fn review_metrics_bounded_evaluations() {
    let mut metrics = ReviewMetricsSnapshot::new(3);
    for i in 0..10 {
        let ctx = ReviewContext {
            review_order: i,
            ..ReviewContext {
                has_read_connection: false,
                reporting_query_count: 50,
                concurrent_readers: 2,
                wal_size_bytes: 50 * 1024 * 1024,
                wal_checkpoint_seqno: 15,
                write_frequency: 50,
                reporting_frequency: 30,
                total_operations: 200,
                last_integrity_check_at: 100,
                orchestration_policy: OrchestrationPolicy::PolicyDriven,
                review_order: 0,
            }
        };
        metrics.record_evaluation(ReviewEvaluator::evaluate(ctx));
    }
    assert_eq!(metrics.evaluation_count(), 3);
}

#[test]
fn review_metrics_bounded_recommendations() {
    let mut metrics = ReviewMetricsSnapshot::new(3);
    let ctx = ReviewContext {
        has_read_connection: false,
        reporting_query_count: 50,
        concurrent_readers: 2,
        wal_size_bytes: 50 * 1024 * 1024,
        wal_checkpoint_seqno: 15,
        write_frequency: 50,
        reporting_frequency: 30,
        total_operations: 200,
        last_integrity_check_at: 100,
        orchestration_policy: OrchestrationPolicy::PolicyDriven,
        review_order: 1,
    };
    for i in 0..10 {
        metrics.record_recommendations(RecommendationEngine::generate(&ctx, i));
    }
    assert_eq!(metrics.recommendation_count(), 3);
}
