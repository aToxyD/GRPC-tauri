use serde::{Deserialize, Serialize};

use super::connection_model::{ConnectionModelEvaluator, ConnectionTopologyEvaluation};
use super::contention::{ContentionAssessment, ContentionClassifier};
use super::read_connection::{
    ReadConnectionAssessment, ReadConnectionSuitability, ReadConnectionSuitabilityEvaluator,
};
use super::runtime_orchestration::{
    OrchestrationAssessment, OrchestrationEvaluator, OrchestrationPolicy,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewContext {
    pub has_read_connection: bool,
    pub reporting_query_count: u64,
    pub concurrent_readers: u64,
    pub wal_size_bytes: u64,
    pub wal_checkpoint_seqno: u64,
    pub write_frequency: u64,
    pub reporting_frequency: u64,
    pub total_operations: u64,
    pub last_integrity_check_at: u64,
    pub orchestration_policy: OrchestrationPolicy,
    pub review_order: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionReviewEvaluation {
    pub context: ReviewContext,
    pub topology: ConnectionTopologyEvaluation,
    pub contention: ContentionAssessment,
    pub read_connection: ReadConnectionAssessment,
    pub orchestration: OrchestrationAssessment,
    pub review_order: u64,
}

impl ConnectionReviewEvaluation {
    pub fn multi_connection_recommended(&self) -> bool {
        matches!(
            self.read_connection.suitability,
            ReadConnectionSuitability::Recommended
        )
    }

    pub fn has_high_contention(&self) -> bool {
        self.contention.contention_score > 0.5
    }

    pub fn requires_orchestration_action(&self) -> bool {
        self.orchestration.checkpoint_required
            || self.orchestration.integrity_check_required
            || self.orchestration.connection_review_required
    }
}

pub struct ReviewEvaluator;

impl ReviewEvaluator {
    pub fn evaluate(ctx: ReviewContext) -> ConnectionReviewEvaluation {
        let wal_pressure = ContentionClassifier::classify_wal_pressure(
            ctx.wal_size_bytes,
            ctx.wal_checkpoint_seqno,
        );

        let topology = ConnectionModelEvaluator::evaluate(
            ctx.has_read_connection,
            ctx.reporting_query_count,
            wal_pressure,
            ctx.concurrent_readers,
            ctx.review_order,
        );

        let contention = ContentionClassifier::classify(
            ctx.wal_size_bytes,
            ctx.wal_checkpoint_seqno,
            ctx.reporting_frequency,
            ctx.write_frequency,
            ctx.review_order,
        );

        let read_connection = ReadConnectionSuitabilityEvaluator::evaluate(
            ctx.reporting_query_count,
            wal_pressure,
            contention.read_contention,
            ctx.concurrent_readers,
            ctx.review_order,
        );

        let orchestration = OrchestrationEvaluator::evaluate(
            ctx.orchestration_policy,
            ctx.wal_size_bytes,
            ctx.wal_checkpoint_seqno,
            ctx.total_operations,
            ctx.last_integrity_check_at,
            wal_pressure,
            ctx.review_order,
        );

        ConnectionReviewEvaluation {
            context: ctx,
            topology,
            contention,
            read_connection,
            orchestration,
            review_order: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_context() -> ReviewContext {
        ReviewContext {
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
        }
    }

    #[test]
    fn full_evaluation_produces_all_assessments() {
        let eval = ReviewEvaluator::evaluate(default_context());
        assert_eq!(
            eval.topology.current_topology,
            crate::infrastructure::sqlite_runtime_review::connection_model::ConnectionTopology::SingleWriter
        );
        assert_eq!(
            eval.contention.wal_pressure,
            crate::infrastructure::sqlite_runtime_review::contention::WalPressureLevel::Medium
        );
        assert!(eval.read_connection.suitability != ReadConnectionSuitability::NotNeeded);
    }

    #[test]
    fn determinism_same_input() {
        let e1 = ReviewEvaluator::evaluate(default_context());
        let e2 = ReviewEvaluator::evaluate(default_context());
        assert_eq!(e1, e2);
    }

    #[test]
    fn multi_connection_recommended_high_load() {
        let ctx = ReviewContext {
            reporting_query_count: 100,
            concurrent_readers: 5,
            wal_size_bytes: 200 * 1024 * 1024,
            wal_checkpoint_seqno: 50,
            write_frequency: 200,
            reporting_frequency: 100,
            ..default_context()
        };
        let eval = ReviewEvaluator::evaluate(ctx);
        assert!(eval.multi_connection_recommended());
    }

    #[test]
    fn multi_connection_not_recommended_low_load() {
        let ctx = ReviewContext {
            reporting_query_count: 5,
            concurrent_readers: 0,
            wal_size_bytes: 1024,
            wal_checkpoint_seqno: 0,
            write_frequency: 10,
            reporting_frequency: 5,
            ..default_context()
        };
        let eval = ReviewEvaluator::evaluate(ctx);
        assert!(!eval.multi_connection_recommended());
    }

    #[test]
    fn high_contention_detected() {
        let ctx = ReviewContext {
            write_frequency: 1000,
            reporting_frequency: 500,
            wal_size_bytes: 200 * 1024 * 1024,
            ..default_context()
        };
        let eval = ReviewEvaluator::evaluate(ctx);
        assert!(eval.has_high_contention());
    }

    #[test]
    fn low_contention_detected() {
        let ctx = ReviewContext {
            write_frequency: 5,
            reporting_frequency: 3,
            ..default_context()
        };
        let eval = ReviewEvaluator::evaluate(ctx);
        assert!(!eval.has_high_contention());
    }

    #[test]
    fn orchestration_action_required() {
        let ctx = ReviewContext {
            wal_size_bytes: 200 * 1024 * 1024,
            total_operations: 500,
            ..default_context()
        };
        let eval = ReviewEvaluator::evaluate(ctx);
        assert!(eval.requires_orchestration_action());
    }

    #[test]
    fn orchestration_action_not_required_low_load() {
        let ctx = ReviewContext {
            wal_size_bytes: 1024,
            wal_checkpoint_seqno: 1,
            total_operations: 50,
            last_integrity_check_at: 10,
            orchestration_policy: OrchestrationPolicy::ManualOnly,
            ..default_context()
        };
        let eval = ReviewEvaluator::evaluate(ctx);
        assert!(!eval.requires_orchestration_action());
    }

    #[test]
    fn serde_round_trip_context() {
        let ctx = default_context();
        let json = serde_json::to_string(&ctx).unwrap();
        let deserialized: ReviewContext = serde_json::from_str(&json).unwrap();
        assert_eq!(ctx.review_order, deserialized.review_order);
        assert_eq!(ctx.has_read_connection, deserialized.has_read_connection);
    }

    #[test]
    fn serde_round_trip_evaluation() {
        let eval = ReviewEvaluator::evaluate(default_context());
        let json = serde_json::to_string(&eval).unwrap();
        let deserialized: ConnectionReviewEvaluation = serde_json::from_str(&json).unwrap();
        assert_eq!(eval.review_order, deserialized.review_order);
        assert_eq!(eval.topology, deserialized.topology);
    }

    #[test]
    fn review_order_propagation() {
        let ctx = ReviewContext {
            review_order: 42,
            ..default_context()
        };
        let eval = ReviewEvaluator::evaluate(ctx);
        assert_eq!(eval.context.review_order, 42);
    }
}
