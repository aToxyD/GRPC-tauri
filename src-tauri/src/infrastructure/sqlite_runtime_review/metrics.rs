use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use super::evaluation::ConnectionReviewEvaluation;
use super::recommendations::RecommendationSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewMetricsSnapshot {
    pub evaluations: VecDeque<ConnectionReviewEvaluation>,
    pub recommendations: VecDeque<RecommendationSet>,
    pub max_entries: usize,
}

impl ReviewMetricsSnapshot {
    pub fn new(max_entries: usize) -> Self {
        Self {
            evaluations: VecDeque::with_capacity(max_entries),
            recommendations: VecDeque::with_capacity(max_entries),
            max_entries,
        }
    }

    pub fn record_evaluation(&mut self, evaluation: ConnectionReviewEvaluation) {
        if self.evaluations.len() >= self.max_entries {
            self.evaluations.pop_front();
        }
        self.evaluations.push_back(evaluation);
    }

    pub fn record_recommendations(&mut self, recommendations: RecommendationSet) {
        if self.recommendations.len() >= self.max_entries {
            self.recommendations.pop_front();
        }
        self.recommendations.push_back(recommendations);
    }

    pub fn last_evaluation(&self) -> Option<&ConnectionReviewEvaluation> {
        self.evaluations.back()
    }

    pub fn last_recommendations(&self) -> Option<&RecommendationSet> {
        self.recommendations.back()
    }

    pub fn evaluation_count(&self) -> usize {
        self.evaluations.len()
    }

    pub fn recommendation_count(&self) -> usize {
        self.recommendations.len()
    }

    pub fn all_evaluations_ordered(&self) -> Vec<&ConnectionReviewEvaluation> {
        let mut v: Vec<_> = self.evaluations.iter().collect();
        v.sort_by_key(|e| e.context.review_order);
        v
    }

    pub fn active_recommendations(&self) -> Vec<&RecommendationSet> {
        let mut v: Vec<_> = self.recommendations.iter().collect();
        v.sort_by_key(|r| r.generated_order);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite_runtime_review::evaluation::ReviewContext;
    use crate::infrastructure::sqlite_runtime_review::evaluation::ReviewEvaluator;
    use crate::infrastructure::sqlite_runtime_review::recommendations::RecommendationEngine;
    use crate::infrastructure::sqlite_runtime_review::runtime_orchestration::OrchestrationPolicy;

    fn sample_context(order: u64) -> ReviewContext {
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
            review_order: order,
        }
    }

    #[test]
    fn metrics_bounded_evaluation_history() {
        let mut metrics = ReviewMetricsSnapshot::new(5);
        for i in 0..20 {
            let ctx = sample_context(i);
            let eval = ReviewEvaluator::evaluate(ctx);
            metrics.record_evaluation(eval);
        }
        assert_eq!(metrics.evaluation_count(), 5);
    }

    #[test]
    fn metrics_bounded_recommendation_history() {
        let mut metrics = ReviewMetricsSnapshot::new(3);
        for i in 0..10 {
            let ctx = sample_context(i);
            let recs = RecommendationEngine::generate(&ctx, i);
            metrics.record_recommendations(recs);
        }
        assert_eq!(metrics.recommendation_count(), 3);
    }

    #[test]
    fn last_evaluation_returns_most_recent() {
        let mut metrics = ReviewMetricsSnapshot::new(10);
        for i in 0..5 {
            let ctx = sample_context(i);
            let eval = ReviewEvaluator::evaluate(ctx);
            metrics.record_evaluation(eval);
        }
        let last = metrics.last_evaluation().unwrap();
        assert_eq!(last.context.review_order, 4);
    }

    #[test]
    fn last_recommendations_returns_most_recent() {
        let mut metrics = ReviewMetricsSnapshot::new(10);
        for i in 0..5 {
            let ctx = sample_context(i);
            let recs = RecommendationEngine::generate(&ctx, i);
            metrics.record_recommendations(recs);
        }
        let last = metrics.last_recommendations().unwrap();
        assert_eq!(last.generated_order, 4);
    }

    #[test]
    fn evaluations_ordered_by_review_order() {
        let mut metrics = ReviewMetricsSnapshot::new(10);
        let ctx3 = sample_context(3);
        let ctx1 = sample_context(1);
        let ctx2 = sample_context(2);
        metrics.record_evaluation(ReviewEvaluator::evaluate(ctx3));
        metrics.record_evaluation(ReviewEvaluator::evaluate(ctx1));
        metrics.record_evaluation(ReviewEvaluator::evaluate(ctx2));

        let ordered = metrics.all_evaluations_ordered();
        assert_eq!(ordered[0].context.review_order, 1);
        assert_eq!(ordered[1].context.review_order, 2);
        assert_eq!(ordered[2].context.review_order, 3);
    }

    #[test]
    fn empty_metrics_snapshots() {
        let metrics = ReviewMetricsSnapshot::new(10);
        assert_eq!(metrics.evaluation_count(), 0);
        assert_eq!(metrics.recommendation_count(), 0);
        assert!(metrics.last_evaluation().is_none());
        assert!(metrics.last_recommendations().is_none());
    }

    #[test]
    fn serde_round_trip_metrics() {
        let mut metrics = ReviewMetricsSnapshot::new(5);
        let ctx = sample_context(1);
        metrics.record_evaluation(ReviewEvaluator::evaluate(ctx));
        let recs = RecommendationEngine::generate(&sample_context(1), 1);
        metrics.record_recommendations(recs);

        let json = serde_json::to_string(&metrics).unwrap();
        let deserialized: ReviewMetricsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(metrics.evaluation_count(), deserialized.evaluation_count());
        assert_eq!(
            metrics.recommendation_count(),
            deserialized.recommendation_count()
        );
    }

    #[test]
    fn active_recommendations_ordered() {
        let mut metrics = ReviewMetricsSnapshot::new(10);
        let ctx1 = sample_context(1);
        let ctx2 = sample_context(2);
        metrics.record_recommendations(RecommendationEngine::generate(&ctx2, 2));
        metrics.record_recommendations(RecommendationEngine::generate(&ctx1, 1));

        let active = metrics.active_recommendations();
        assert_eq!(active[0].generated_order, 1);
        assert_eq!(active[1].generated_order, 2);
    }
}
