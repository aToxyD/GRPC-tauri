use serde::{Deserialize, Serialize};

use super::contention::{ContentionClassifier, WalPressureLevel};
use super::evaluation::ReviewContext;
use super::read_connection::{ReadConnectionSuitability, ReadConnectionSuitabilityEvaluator};
use super::runtime_orchestration::OrchestrationEvaluator;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewRecommendation {
    NoActionNeeded,
    AddReadConnection,
    ScheduleCheckpoint,
    ScheduleIntegrityCheck,
    ScheduleReview,
    UpgradeOrchestrationPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationPriority {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecommendationEntry {
    pub recommendation: ReviewRecommendation,
    pub priority: RecommendationPriority,
    pub rationale: String,
    pub recommendation_order: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecommendationSet {
    pub entries: Vec<RecommendationEntry>,
    pub generated_order: u64,
}

pub struct RecommendationEngine;

impl RecommendationEngine {
    pub fn generate(ctx: &ReviewContext, generated_order: u64) -> RecommendationSet {
        let mut entries = Vec::new();

        let wal_pressure = ContentionClassifier::classify_wal_pressure(
            ctx.wal_size_bytes,
            ctx.wal_checkpoint_seqno,
        );

        let connection_suitability = ReadConnectionSuitabilityEvaluator::evaluate(
            ctx.reporting_query_count,
            wal_pressure,
            ContentionClassifier::classify_read_contention(
                ctx.write_frequency,
                ctx.reporting_frequency,
            ),
            ctx.concurrent_readers,
            0,
        );

        if matches!(
            connection_suitability.suitability,
            ReadConnectionSuitability::Recommended
        ) && !ctx.has_read_connection
        {
            entries.push(RecommendationEntry {
                recommendation: ReviewRecommendation::AddReadConnection,
                priority: RecommendationPriority::High,
                rationale: format!(
                    "reporting load ({queries} queries, {readers} readers) and {wp:?} WAL pressure justify a dedicated read connection",
                    queries = ctx.reporting_query_count,
                    readers = ctx.concurrent_readers,
                    wp = wal_pressure,
                ),
                recommendation_order: entries.len() as u64,
            });
        }

        if OrchestrationEvaluator::is_checkpoint_recommended(
            ctx.wal_size_bytes,
            ctx.wal_checkpoint_seqno,
            wal_pressure,
        ) {
            let priority = if matches!(wal_pressure, WalPressureLevel::High) {
                RecommendationPriority::High
            } else {
                RecommendationPriority::Medium
            };
            entries.push(RecommendationEntry {
                recommendation: ReviewRecommendation::ScheduleCheckpoint,
                priority,
                rationale: format!(
                    "WAL pressure ({wp:?}) with {size} bytes and seqno {seqno} suggests checkpoint needed",
                    wp = wal_pressure,
                    size = ctx.wal_size_bytes,
                    seqno = ctx.wal_checkpoint_seqno,
                ),
                recommendation_order: entries.len() as u64,
            });
        }

        if OrchestrationEvaluator::is_integrity_check_needed(
            ctx.total_operations,
            ctx.last_integrity_check_at,
        ) {
            entries.push(RecommendationEntry {
                recommendation: ReviewRecommendation::ScheduleIntegrityCheck,
                priority: RecommendationPriority::Medium,
                rationale: format!(
                    "{ops} operations since last integrity check at {last} exceeds interval {interval}",
                    ops = ctx.total_operations - ctx.last_integrity_check_at,
                    last = ctx.last_integrity_check_at,
                    interval = OrchestrationEvaluator::INTEGRITY_CHECK_INTERVAL_EXECUTIONS,
                ),
                recommendation_order: entries.len() as u64,
            });
        }

        if matches!(wal_pressure, WalPressureLevel::High)
            && !matches!(
                connection_suitability.suitability,
                ReadConnectionSuitability::Recommended
            )
        {
            entries.push(RecommendationEntry {
                recommendation: ReviewRecommendation::ScheduleReview,
                priority: RecommendationPriority::Low,
                rationale: "high WAL pressure observed but read connection not yet recommended — monitor and re-evaluate".into(),
                recommendation_order: entries.len() as u64,
            });
        }

        if ctx.orchestration_policy
            != super::runtime_orchestration::OrchestrationPolicy::PolicyDriven
        {
            let has_need = OrchestrationEvaluator::is_checkpoint_recommended(
                ctx.wal_size_bytes,
                ctx.wal_checkpoint_seqno,
                wal_pressure,
            ) || OrchestrationEvaluator::is_integrity_check_needed(
                ctx.total_operations,
                ctx.last_integrity_check_at,
            );
            if has_need {
                entries.push(RecommendationEntry {
                    recommendation: ReviewRecommendation::UpgradeOrchestrationPolicy,
                    priority: RecommendationPriority::Medium,
                    rationale: "manual policy constrains automated runtime decisions — PolicyDriven enables proactive checkpoint and integrity scheduling".into(),
                    recommendation_order: entries.len() as u64,
                });
            }
        }

        if entries.is_empty() {
            entries.push(RecommendationEntry {
                recommendation: ReviewRecommendation::NoActionNeeded,
                priority: RecommendationPriority::Low,
                rationale: "all runtime metrics within acceptable thresholds".into(),
                recommendation_order: 0,
            });
        }

        RecommendationSet {
            entries,
            generated_order,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn generate_with_context(
        has_read_connection: bool,
        reporting_query_count: u64,
        concurrent_readers: u64,
        wal_size_bytes: u64,
        wal_checkpoint_seqno: u64,
        write_frequency: u64,
        reporting_frequency: u64,
        total_operations: u64,
        last_integrity_check_at: u64,
        generated_order: u64,
    ) -> RecommendationSet {
        let ctx = ReviewContext {
            has_read_connection,
            reporting_query_count,
            concurrent_readers,
            wal_size_bytes,
            wal_checkpoint_seqno,
            write_frequency,
            reporting_frequency,
            total_operations,
            last_integrity_check_at,
            orchestration_policy: super::runtime_orchestration::OrchestrationPolicy::ManualOnly,
            review_order: generated_order,
        };
        Self::generate(&ctx, generated_order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite_runtime_review::runtime_orchestration::OrchestrationPolicy;

    fn default_context() -> ReviewContext {
        ReviewContext {
            has_read_connection: false,
            reporting_query_count: 10,
            concurrent_readers: 0,
            wal_size_bytes: 1024,
            wal_checkpoint_seqno: 1,
            write_frequency: 10,
            reporting_frequency: 5,
            total_operations: 50,
            last_integrity_check_at: 10,
            orchestration_policy: OrchestrationPolicy::PolicyDriven,
            review_order: 1,
        }
    }

    #[test]
    fn no_action_when_all_healthy() {
        let ctx = default_context();
        let set = RecommendationEngine::generate(&ctx, 1);
        assert_eq!(set.entries.len(), 1);
        assert_eq!(
            set.entries[0].recommendation,
            ReviewRecommendation::NoActionNeeded
        );
    }

    #[test]
    fn add_read_connection_recommended() {
        let ctx = ReviewContext {
            reporting_query_count: 100,
            concurrent_readers: 5,
            wal_size_bytes: 200 * 1024 * 1024,
            wal_checkpoint_seqno: 50,
            write_frequency: 200,
            reporting_frequency: 100,
            total_operations: 200,
            last_integrity_check_at: 100,
            ..default_context()
        };
        let set = RecommendationEngine::generate(&ctx, 1);
        let has_add_read = set
            .entries
            .iter()
            .any(|e| matches!(e.recommendation, ReviewRecommendation::AddReadConnection));
        assert!(has_add_read);
    }

    #[test]
    fn checkpoint_recommended_high_wal() {
        let ctx = ReviewContext {
            wal_size_bytes: 200 * 1024 * 1024,
            ..default_context()
        };
        let set = RecommendationEngine::generate(&ctx, 1);
        let has_checkpoint = set
            .entries
            .iter()
            .any(|e| matches!(e.recommendation, ReviewRecommendation::ScheduleCheckpoint));
        assert!(has_checkpoint);
    }

    #[test]
    fn integrity_check_recommended() {
        let ctx = ReviewContext {
            total_operations: 500,
            last_integrity_check_at: 50,
            ..default_context()
        };
        let set = RecommendationEngine::generate(&ctx, 1);
        let has_integrity = set.entries.iter().any(|e| {
            matches!(
                e.recommendation,
                ReviewRecommendation::ScheduleIntegrityCheck
            )
        });
        assert!(has_integrity);
    }

    #[test]
    fn upgrade_policy_recommended_with_manual() {
        let ctx = ReviewContext {
            wal_size_bytes: 200 * 1024 * 1024,
            orchestration_policy: OrchestrationPolicy::ManualOnly,
            ..default_context()
        };
        let set = RecommendationEngine::generate(&ctx, 1);
        let has_upgrade = set.entries.iter().any(|e| {
            matches!(
                e.recommendation,
                ReviewRecommendation::UpgradeOrchestrationPolicy
            )
        });
        assert!(has_upgrade);
    }

    #[test]
    fn determinism_same_input() {
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
    fn serde_round_trip_recommendation() {
        let recs = [
            ReviewRecommendation::NoActionNeeded,
            ReviewRecommendation::AddReadConnection,
            ReviewRecommendation::ScheduleCheckpoint,
            ReviewRecommendation::ScheduleIntegrityCheck,
            ReviewRecommendation::ScheduleReview,
            ReviewRecommendation::UpgradeOrchestrationPolicy,
        ];
        for r in &recs {
            let json = serde_json::to_string(r).unwrap();
            let deserialized: ReviewRecommendation = serde_json::from_str(&json).unwrap();
            assert_eq!(*r, deserialized);
        }
    }

    #[test]
    fn serde_round_trip_priority() {
        let priorities = [
            RecommendationPriority::Low,
            RecommendationPriority::Medium,
            RecommendationPriority::High,
        ];
        for p in &priorities {
            let json = serde_json::to_string(p).unwrap();
            let deserialized: RecommendationPriority = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, deserialized);
        }
    }

    #[test]
    fn serde_round_trip_entry() {
        let entry = RecommendationEntry {
            recommendation: ReviewRecommendation::AddReadConnection,
            priority: RecommendationPriority::High,
            rationale: "test".into(),
            recommendation_order: 1,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: RecommendationEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn serde_round_trip_set() {
        let set = RecommendationSet {
            entries: vec![RecommendationEntry {
                recommendation: ReviewRecommendation::NoActionNeeded,
                priority: RecommendationPriority::Low,
                rationale: "all good".into(),
                recommendation_order: 0,
            }],
            generated_order: 42,
        };
        let json = serde_json::to_string(&set).unwrap();
        let deserialized: RecommendationSet = serde_json::from_str(&json).unwrap();
        assert_eq!(set, deserialized);
    }

    #[test]
    fn generate_with_context_convenience() {
        let set =
            RecommendationEngine::generate_with_context(false, 10, 0, 1024, 1, 10, 5, 50, 10, 1);
        assert_eq!(set.generated_order, 1);
    }

    #[test]
    fn recommendation_order_sequential() {
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
        let set = RecommendationEngine::generate(&ctx, 1);
        for (i, entry) in set.entries.iter().enumerate() {
            assert_eq!(
                entry.recommendation_order as usize, i,
                "entry {} should have order {}",
                i, i
            );
        }
    }
}
