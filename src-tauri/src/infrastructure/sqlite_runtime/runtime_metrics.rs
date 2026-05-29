use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use super::checkpoint::CheckpointResult;
use super::integrity_runner::IntegrityExecutionResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalGrowthTrend {
    pub samples: VecDeque<WalGrowthSample>,
    pub max_samples: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WalGrowthSample {
    pub page_count: u64,
    pub wal_size_bytes: u64,
    pub checkpoint_seqno: u64,
    pub recorded_at_order: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityExecutionHistory {
    pub executions: VecDeque<IntegrityExecutionResult>,
    pub max_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointExecutionHistory {
    pub checkpoints: VecDeque<CheckpointResult>,
    pub max_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeHealthSnapshot {
    pub checkpoint_count: usize,
    pub integrity_check_count: usize,
    pub last_integrity_severity:
        Option<crate::infrastructure::sqlite_observability::integrity::IntegritySeverity>,
    pub last_checkpoint_result: Option<CheckpointResult>,
    pub wal_growth_trend: WalGrowthTrend,
    pub snapshot_order: u64,
}

impl WalGrowthTrend {
    pub fn new(max_samples: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
        }
    }

    pub fn record(&mut self, sample: WalGrowthSample) {
        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn samples_ordered(&self) -> Vec<WalGrowthSample> {
        let mut v: Vec<_> = self.samples.iter().copied().collect();
        v.sort_by_key(|s| s.recorded_at_order);
        v
    }

    pub fn growth_rate(&self) -> f64 {
        let samples = self.samples_ordered();
        if samples.len() < 2 {
            return 0.0;
        }
        let first = samples.first().unwrap();
        let last = samples.last().unwrap();
        if first.page_count == 0 {
            return 0.0;
        }
        (last.page_count as f64 - first.page_count as f64) / first.page_count as f64
    }
}

impl IntegrityExecutionHistory {
    pub fn new(max_entries: usize) -> Self {
        Self {
            executions: VecDeque::with_capacity(max_entries),
            max_entries,
        }
    }

    pub fn record(&mut self, result: IntegrityExecutionResult) {
        if self.executions.len() >= self.max_entries {
            self.executions.pop_front();
        }
        self.executions.push_back(result);
    }

    pub fn len(&self) -> usize {
        self.executions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.executions.is_empty()
    }

    pub fn executions_ordered(&self) -> Vec<&IntegrityExecutionResult> {
        let mut v: Vec<_> = self.executions.iter().collect();
        v.sort_by_key(|e| e.execution_order);
        v
    }

    pub fn last_execution(&self) -> Option<&IntegrityExecutionResult> {
        self.executions.back()
    }
}

impl CheckpointExecutionHistory {
    pub fn new(max_entries: usize) -> Self {
        Self {
            checkpoints: VecDeque::with_capacity(max_entries),
            max_entries,
        }
    }

    pub fn record(&mut self, result: CheckpointResult) {
        if self.checkpoints.len() >= self.max_entries {
            self.checkpoints.pop_front();
        }
        self.checkpoints.push_back(result);
    }

    pub fn len(&self) -> usize {
        self.checkpoints.len()
    }

    pub fn is_empty(&self) -> bool {
        self.checkpoints.is_empty()
    }

    pub fn checkpoints_ordered(&self) -> Vec<&CheckpointResult> {
        let mut v: Vec<_> = self.checkpoints.iter().collect();
        v.sort_by_key(|c| c.checkpoint_seqno_before);
        v
    }

    pub fn last_checkpoint(&self) -> Option<&CheckpointResult> {
        self.checkpoints.back()
    }
}

impl RuntimeHealthSnapshot {
    pub fn new(
        integrity_history: &IntegrityExecutionHistory,
        checkpoint_history: &CheckpointExecutionHistory,
        wal_trend: &WalGrowthTrend,
        snapshot_order: u64,
    ) -> Self {
        let checkpoint_count = checkpoint_history.len();
        let integrity_check_count = integrity_history.len();
        let last_integrity_severity = integrity_history.last_execution().map(|e| e.severity);
        let last_checkpoint_result = checkpoint_history.last_checkpoint().cloned();
        Self {
            checkpoint_count,
            integrity_check_count,
            last_integrity_severity,
            last_checkpoint_result,
            wal_growth_trend: wal_trend.clone(),
            snapshot_order,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite_observability::integrity::{
        IntegrityCheckResult, IntegritySeverity, QuickCheckResult,
    };
    use crate::infrastructure::sqlite_runtime::checkpoint::CheckpointMode;

    fn sample_integrity_result(order: u64) -> IntegrityExecutionResult {
        IntegrityExecutionResult {
            check_result: IntegrityCheckResult::parse("ok"),
            quick_check_result: QuickCheckResult::parse("ok"),
            severity: IntegritySeverity::Ok,
            execution_order: order,
        }
    }

    fn sample_checkpoint_result(seqno: u64) -> CheckpointResult {
        CheckpointResult {
            pages_before: 100,
            pages_after: 90,
            pages_moved: 10,
            pages_before_checkpoint: 5,
            mode: CheckpointMode::Truncate,
            wal_size_before_bytes: 409600,
            wal_size_after_bytes: 0,
            checkpoint_seqno_before: seqno,
            checkpoint_seqno_after: 0,
        }
    }

    fn sample_wal_sample(order: u64) -> WalGrowthSample {
        WalGrowthSample {
            page_count: 100,
            wal_size_bytes: 409600,
            checkpoint_seqno: order,
            recorded_at_order: order,
        }
    }

    #[test]
    fn wal_growth_trend_bounded() {
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
    fn wal_growth_trend_ordering() {
        let mut trend = WalGrowthTrend::new(10);
        trend.record(sample_wal_sample(3));
        trend.record(sample_wal_sample(1));
        trend.record(sample_wal_sample(2));
        let ordered = trend.samples_ordered();
        assert_eq!(ordered[0].recorded_at_order, 1);
        assert_eq!(ordered[1].recorded_at_order, 2);
        assert_eq!(ordered[2].recorded_at_order, 3);
    }

    #[test]
    fn integrity_history_bounded() {
        let mut history = IntegrityExecutionHistory::new(3);
        for i in 0..10 {
            history.record(sample_integrity_result(i));
        }
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn checkpoint_history_bounded() {
        let mut history = CheckpointExecutionHistory::new(3);
        for i in 0..10 {
            history.record(sample_checkpoint_result(i));
        }
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn runtime_health_snapshot_construction() {
        let mut integrity = IntegrityExecutionHistory::new(10);
        integrity.record(sample_integrity_result(1));
        let mut checkpoint = CheckpointExecutionHistory::new(10);
        checkpoint.record(sample_checkpoint_result(1));
        let mut trend = WalGrowthTrend::new(10);
        trend.record(sample_wal_sample(1));

        let snap = RuntimeHealthSnapshot::new(&integrity, &checkpoint, &trend, 1);
        assert_eq!(snap.checkpoint_count, 1);
        assert_eq!(snap.integrity_check_count, 1);
        assert_eq!(snap.last_integrity_severity, Some(IntegritySeverity::Ok));
        assert!(snap.last_checkpoint_result.is_some());
    }

    #[test]
    fn empty_health_snapshot() {
        let integrity = IntegrityExecutionHistory::new(10);
        let checkpoint = CheckpointExecutionHistory::new(10);
        let trend = WalGrowthTrend::new(10);

        let snap = RuntimeHealthSnapshot::new(&integrity, &checkpoint, &trend, 0);
        assert_eq!(snap.checkpoint_count, 0);
        assert_eq!(snap.integrity_check_count, 0);
        assert_eq!(snap.last_integrity_severity, None);
        assert!(snap.last_checkpoint_result.is_none());
    }

    #[test]
    fn serde_round_trip_wal_growth_trend() {
        let mut trend = WalGrowthTrend::new(5);
        trend.record(sample_wal_sample(1));
        trend.record(sample_wal_sample(2));
        let json = serde_json::to_string(&trend).unwrap();
        let deserialized: WalGrowthTrend = serde_json::from_str(&json).unwrap();
        assert_eq!(trend.len(), deserialized.len());
    }

    #[test]
    fn serde_round_trip_integrity_history() {
        let mut history = IntegrityExecutionHistory::new(5);
        history.record(sample_integrity_result(1));
        let json = serde_json::to_string(&history).unwrap();
        let deserialized: IntegrityExecutionHistory = serde_json::from_str(&json).unwrap();
        assert_eq!(history.len(), deserialized.len());
    }

    #[test]
    fn serde_round_trip_checkpoint_history() {
        let mut history = CheckpointExecutionHistory::new(5);
        history.record(sample_checkpoint_result(1));
        let json = serde_json::to_string(&history).unwrap();
        let deserialized: CheckpointExecutionHistory = serde_json::from_str(&json).unwrap();
        assert_eq!(history.len(), deserialized.len());
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
    }

    #[test]
    fn deterministic_same_input() {
        fn make() -> RuntimeHealthSnapshot {
            let integrity = IntegrityExecutionHistory::new(10);
            let checkpoint = CheckpointExecutionHistory::new(10);
            let trend = WalGrowthTrend::new(10);
            RuntimeHealthSnapshot::new(&integrity, &checkpoint, &trend, 1)
        }
        let snap1 = make();
        let snap2 = make();
        assert_eq!(snap1.snapshot_order, snap2.snapshot_order);
        assert_eq!(snap1.checkpoint_count, snap2.checkpoint_count);
    }

    #[test]
    fn growth_rate_calculation() {
        let mut trend = WalGrowthTrend::new(10);
        trend.record(WalGrowthSample {
            page_count: 100,
            wal_size_bytes: 409600,
            checkpoint_seqno: 0,
            recorded_at_order: 1,
        });
        trend.record(WalGrowthSample {
            page_count: 150,
            wal_size_bytes: 614400,
            checkpoint_seqno: 1,
            recorded_at_order: 2,
        });
        let rate = trend.growth_rate();
        assert!((rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn growth_rate_zero_with_single_sample() {
        let trend = WalGrowthTrend::new(10);
        assert_eq!(trend.growth_rate(), 0.0);
    }

    #[test]
    fn growth_rate_zero_with_no_samples() {
        let trend = WalGrowthTrend::new(10);
        assert_eq!(trend.growth_rate(), 0.0);
    }
}
