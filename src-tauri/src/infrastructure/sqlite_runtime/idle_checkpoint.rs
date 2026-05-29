use serde::{Deserialize, Serialize};

use super::checkpoint::CheckpointEligibility;
use super::policies::CheckpointPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdleWindow {
    pub idle_duration_seconds: u64,
}

impl IdleWindow {
    pub fn new(idle_duration_seconds: u64) -> Self {
        Self {
            idle_duration_seconds,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdleCheckpointDecision {
    ShouldCheckpoint,
    ShouldNotCheckpoint { reason: String },
}

pub struct IdleCheckpointEvaluator;

impl IdleCheckpointEvaluator {
    pub const DEFAULT_IDLE_THRESHOLD_SECONDS: u64 = 30;

    pub fn evaluate(
        idle_window: &IdleWindow,
        eligibility: &CheckpointEligibility,
        policy: &CheckpointPolicy,
    ) -> IdleCheckpointDecision {
        if idle_window.idle_duration_seconds < Self::DEFAULT_IDLE_THRESHOLD_SECONDS {
            return IdleCheckpointDecision::ShouldNotCheckpoint {
                reason: format!(
                    "idle duration {}s below threshold {}s",
                    idle_window.idle_duration_seconds,
                    Self::DEFAULT_IDLE_THRESHOLD_SECONDS
                ),
            };
        }

        if !matches!(eligibility, CheckpointEligibility::Eligible) {
            return IdleCheckpointDecision::ShouldNotCheckpoint {
                reason: "checkpoint eligibility not met".into(),
            };
        }

        if policy.idle_checkpoint_enabled && policy.max_wal_size_bytes > 0 {
            IdleCheckpointDecision::ShouldCheckpoint
        } else if !policy.idle_checkpoint_enabled {
            IdleCheckpointDecision::ShouldNotCheckpoint {
                reason: "idle checkpoint disabled by policy".into(),
            }
        } else {
            IdleCheckpointDecision::ShouldCheckpoint
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite_runtime::policies::CheckpointPolicy;

    #[test]
    fn below_idle_threshold_no_checkpoint() {
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
    fn at_threshold_and_eligible_checkpoint() {
        let policy = CheckpointPolicy::default();
        let window = IdleWindow::new(30);
        let eligibility = CheckpointEligibility::Eligible;
        let decision = IdleCheckpointEvaluator::evaluate(&window, &eligibility, &policy);
        assert!(matches!(decision, IdleCheckpointDecision::ShouldCheckpoint));
    }

    #[test]
    fn above_threshold_and_eligible_checkpoint() {
        let policy = CheckpointPolicy::default();
        let window = IdleWindow::new(60);
        let eligibility = CheckpointEligibility::Eligible;
        let decision = IdleCheckpointEvaluator::evaluate(&window, &eligibility, &policy);
        assert!(matches!(decision, IdleCheckpointDecision::ShouldCheckpoint));
    }

    #[test]
    fn not_eligible_no_checkpoint() {
        let policy = CheckpointPolicy::default();
        let window = IdleWindow::new(30);
        let eligibility = CheckpointEligibility::NotEligible {
            reason: "no pages".into(),
        };
        let decision = IdleCheckpointEvaluator::evaluate(&window, &eligibility, &policy);
        assert!(matches!(
            decision,
            IdleCheckpointDecision::ShouldNotCheckpoint { .. }
        ));
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

    #[test]
    fn deterministic_same_input() {
        let policy = CheckpointPolicy::default();
        let window = IdleWindow::new(30);
        let eligibility = CheckpointEligibility::Eligible;
        let d1 = IdleCheckpointEvaluator::evaluate(&window, &eligibility, &policy);
        let d2 = IdleCheckpointEvaluator::evaluate(&window, &eligibility, &policy);
        assert_eq!(
            matches!(d1, IdleCheckpointDecision::ShouldCheckpoint),
            matches!(d2, IdleCheckpointDecision::ShouldCheckpoint)
        );
    }

    #[test]
    fn serde_round_trip_decision() {
        let should = IdleCheckpointDecision::ShouldCheckpoint;
        let should_not = IdleCheckpointDecision::ShouldNotCheckpoint {
            reason: "test".into(),
        };
        for val in [should.clone(), should_not.clone()] {
            let json = serde_json::to_string(&val).unwrap();
            let deserialized: IdleCheckpointDecision = serde_json::from_str(&json).unwrap();
            assert_eq!(val, deserialized);
        }
    }

    #[test]
    fn serde_round_trip_idle_window() {
        let window = IdleWindow::new(30);
        let json = serde_json::to_string(&window).unwrap();
        let deserialized: IdleWindow = serde_json::from_str(&json).unwrap();
        assert_eq!(window, deserialized);
    }
}
