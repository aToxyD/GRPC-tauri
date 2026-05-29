use serde::{Deserialize, Serialize};

use super::contention::WalPressureLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrchestrationPolicy {
    ManualOnly,
    PolicyDriven,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrchestrationAssessment {
    pub current_policy: OrchestrationPolicy,
    pub checkpoint_required: bool,
    pub integrity_check_required: bool,
    pub connection_review_required: bool,
    pub reason: String,
    pub assessment_order: u64,
}

pub struct OrchestrationEvaluator;

impl OrchestrationEvaluator {
    pub const MAX_WAL_SIZE_BEFORE_CHECKPOINT: u64 = 100 * 1024 * 1024;
    pub const MAX_CHECKPOINT_SEQNO_BEFORE_CHECKPOINT: u64 = 20;
    pub const INTEGRITY_CHECK_INTERVAL_EXECUTIONS: u64 = 100;

    pub fn evaluate(
        current_policy: OrchestrationPolicy,
        wal_size_bytes: u64,
        wal_checkpoint_seqno: u64,
        total_operations: u64,
        last_integrity_check_at: u64,
        wal_pressure: WalPressureLevel,
        assessment_order: u64,
    ) -> OrchestrationAssessment {
        let checkpoint_required = match current_policy {
            OrchestrationPolicy::ManualOnly => false,
            OrchestrationPolicy::PolicyDriven => {
                wal_size_bytes > Self::MAX_WAL_SIZE_BEFORE_CHECKPOINT
                    || wal_checkpoint_seqno > Self::MAX_CHECKPOINT_SEQNO_BEFORE_CHECKPOINT
                    || wal_pressure == WalPressureLevel::High
            }
        };

        let integrity_check_required = match current_policy {
            OrchestrationPolicy::ManualOnly => false,
            OrchestrationPolicy::PolicyDriven => {
                total_operations - last_integrity_check_at
                    >= Self::INTEGRITY_CHECK_INTERVAL_EXECUTIONS
            }
        };

        let connection_review_required = match current_policy {
            OrchestrationPolicy::ManualOnly => wal_pressure == WalPressureLevel::High,
            OrchestrationPolicy::PolicyDriven => {
                wal_pressure == WalPressureLevel::High
                    || (wal_size_bytes > Self::MAX_WAL_SIZE_BEFORE_CHECKPOINT / 2)
            }
        };

        let reason = format!(
            "policy={current_policy:?} checkpoint_needed={checkpoint_required} integrity_needed={integrity_check_required} review_needed={connection_review_required}"
        );

        OrchestrationAssessment {
            current_policy,
            checkpoint_required,
            integrity_check_required,
            connection_review_required,
            reason,
            assessment_order,
        }
    }

    pub fn evaluate_policy_only(
        current_policy: OrchestrationPolicy,
        assessment_order: u64,
    ) -> OrchestrationAssessment {
        OrchestrationAssessment {
            current_policy,
            checkpoint_required: false,
            integrity_check_required: false,
            connection_review_required: false,
            reason: format!("policy-only evaluation ({current_policy:?})"),
            assessment_order,
        }
    }

    pub fn is_checkpoint_recommended(
        wal_size_bytes: u64,
        wal_checkpoint_seqno: u64,
        wal_pressure: WalPressureLevel,
    ) -> bool {
        wal_size_bytes > Self::MAX_WAL_SIZE_BEFORE_CHECKPOINT
            || wal_checkpoint_seqno > Self::MAX_CHECKPOINT_SEQNO_BEFORE_CHECKPOINT
            || wal_pressure == WalPressureLevel::High
    }

    pub fn is_integrity_check_needed(total_operations: u64, last_integrity_check_at: u64) -> bool {
        total_operations - last_integrity_check_at >= Self::INTEGRITY_CHECK_INTERVAL_EXECUTIONS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_policy_no_checkpoint_needed() {
        let a = OrchestrationEvaluator::evaluate(
            OrchestrationPolicy::ManualOnly,
            200 * 1024 * 1024,
            50,
            1000,
            800,
            WalPressureLevel::High,
            1,
        );
        assert!(!a.checkpoint_required);
        assert!(!a.integrity_check_required);
    }

    #[test]
    fn policy_driven_checkpoint_needed_high_wal() {
        let a = OrchestrationEvaluator::evaluate(
            OrchestrationPolicy::PolicyDriven,
            200 * 1024 * 1024,
            5,
            100,
            50,
            WalPressureLevel::Low,
            1,
        );
        assert!(a.checkpoint_required);
    }

    #[test]
    fn policy_driven_checkpoint_needed_high_seqno() {
        let a = OrchestrationEvaluator::evaluate(
            OrchestrationPolicy::PolicyDriven,
            1024,
            50,
            100,
            50,
            WalPressureLevel::Low,
            1,
        );
        assert!(a.checkpoint_required);
    }

    #[test]
    fn policy_driven_integrity_check_needed() {
        let a = OrchestrationEvaluator::evaluate(
            OrchestrationPolicy::PolicyDriven,
            1024,
            1,
            200,
            50,
            WalPressureLevel::Low,
            1,
        );
        assert!(a.integrity_check_required);
    }

    #[test]
    fn policy_driven_integrity_check_not_needed() {
        let a = OrchestrationEvaluator::evaluate(
            OrchestrationPolicy::PolicyDriven,
            1024,
            1,
            100,
            50,
            WalPressureLevel::Low,
            1,
        );
        assert!(!a.integrity_check_required);
    }

    #[test]
    fn manual_policy_connection_review_on_high_pressure() {
        let a = OrchestrationEvaluator::evaluate(
            OrchestrationPolicy::ManualOnly,
            1024,
            1,
            100,
            50,
            WalPressureLevel::High,
            1,
        );
        assert!(a.connection_review_required);
    }

    #[test]
    fn determinism_same_input() {
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
    fn serde_round_trip_policy() {
        let policies = [
            OrchestrationPolicy::ManualOnly,
            OrchestrationPolicy::PolicyDriven,
        ];
        for p in &policies {
            let json = serde_json::to_string(p).unwrap();
            let deserialized: OrchestrationPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, deserialized);
        }
    }

    #[test]
    fn serde_round_trip_assessment() {
        let a = OrchestrationEvaluator::evaluate(
            OrchestrationPolicy::PolicyDriven,
            50 * 1024 * 1024,
            15,
            150,
            100,
            WalPressureLevel::Medium,
            42,
        );
        let json = serde_json::to_string(&a).unwrap();
        let deserialized: OrchestrationAssessment = serde_json::from_str(&json).unwrap();
        assert_eq!(a, deserialized);
    }

    #[test]
    fn policy_only_evaluation() {
        let a = OrchestrationEvaluator::evaluate_policy_only(OrchestrationPolicy::ManualOnly, 1);
        assert!(!a.checkpoint_required);
        assert!(!a.integrity_check_required);
    }

    #[test]
    fn assessment_order_preserved() {
        let a = OrchestrationEvaluator::evaluate(
            OrchestrationPolicy::ManualOnly,
            0,
            0,
            0,
            0,
            WalPressureLevel::Low,
            7,
        );
        assert_eq!(a.assessment_order, 7);
    }

    #[test]
    fn static_checkpoint_recommended() {
        assert!(OrchestrationEvaluator::is_checkpoint_recommended(
            200 * 1024 * 1024,
            1,
            WalPressureLevel::Low,
        ));
        assert!(OrchestrationEvaluator::is_checkpoint_recommended(
            1024,
            50,
            WalPressureLevel::Low,
        ));
        assert!(OrchestrationEvaluator::is_checkpoint_recommended(
            1024,
            1,
            WalPressureLevel::High,
        ));
        assert!(!OrchestrationEvaluator::is_checkpoint_recommended(
            1024,
            1,
            WalPressureLevel::Low,
        ));
    }

    #[test]
    fn static_integrity_check_needed() {
        assert!(OrchestrationEvaluator::is_integrity_check_needed(200, 50));
        assert!(!OrchestrationEvaluator::is_integrity_check_needed(100, 50));
    }
}
