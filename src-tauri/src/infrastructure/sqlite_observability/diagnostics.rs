use serde::{Deserialize, Serialize};

use super::integrity::{IntegritySeverity, IntegritySnapshot};
use super::metrics::PageGrowthMetrics;
use super::query_plan::{QueryDiagnostics, ScanSeverity};
use super::slow_query::{QueryExecutionSample, SlowQueryRecord};
use super::wal::WalMetrics;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlowQuerySummary {
    pub total_slow_queries: usize,
    pub slowest: Option<QueryExecutionSample>,
    pub average_duration_ms: f64,
    pub by_category: std::collections::HashMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryPlanWarningSummary {
    pub query_count: usize,
    pub total_warnings: usize,
    pub highest_severity: ScanSeverity,
    pub slowest_scan_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SqliteDiagnosticsSnapshot {
    pub wal_metrics: Option<WalMetrics>,
    pub integrity_snapshot: Option<IntegritySnapshot>,
    pub slow_query_summary: Option<SlowQuerySummary>,
    pub query_plan_warnings: Option<QueryPlanWarningSummary>,
    pub page_growth: Option<PageGrowthMetrics>,
    pub snapshot_order: u64,
    pub snapshot_id: String,
}

impl SlowQuerySummary {
    pub fn from_record(record: &SlowQueryRecord) -> Self {
        let samples = record.samples();
        let total_slow_queries = samples.len();
        let slowest = record.slowest();
        let average_duration_ms = record.average_duration_ms();
        let mut by_category = std::collections::HashMap::new();
        for sample in &samples {
            let label = format!("{:?}", sample.category);
            *by_category.entry(label).or_insert(0) += 1;
        }
        Self {
            total_slow_queries,
            slowest,
            average_duration_ms,
            by_category,
        }
    }
}

impl QueryPlanWarningSummary {
    pub fn from_diagnostics(diagnostics: &[QueryDiagnostics]) -> Self {
        let query_count = diagnostics.len();
        let total_warnings: usize = diagnostics.iter().map(|d| d.warnings.len()).sum();
        let highest_severity = diagnostics
            .iter()
            .map(|d| d.severity())
            .max()
            .unwrap_or(ScanSeverity::None);
        let slowest_scan_count = diagnostics
            .iter()
            .map(|d| d.plan.scan_count)
            .max()
            .unwrap_or(0);
        Self {
            query_count,
            total_warnings,
            highest_severity,
            slowest_scan_count,
        }
    }
}

impl SqliteDiagnosticsSnapshot {
    pub fn new(
        wal_metrics: Option<WalMetrics>,
        integrity_snapshot: Option<IntegritySnapshot>,
        slow_query_record: Option<&SlowQueryRecord>,
        query_diagnostics: Vec<QueryDiagnostics>,
        page_growth: Option<PageGrowthMetrics>,
        snapshot_order: u64,
    ) -> Self {
        let slow_query_summary = slow_query_record.map(SlowQuerySummary::from_record);
        let query_plan_warnings = QueryPlanWarningSummary::from_diagnostics(&query_diagnostics);
        let snapshot_id = format!("diag-{:016x}", snapshot_order);
        Self {
            wal_metrics,
            integrity_snapshot,
            slow_query_summary,
            query_plan_warnings: Some(query_plan_warnings),
            page_growth,
            snapshot_order,
            snapshot_id,
        }
    }

    pub fn has_integrity_issues(&self) -> bool {
        self.integrity_snapshot
            .as_ref()
            .map(|s| s.severity != IntegritySeverity::Ok)
            .unwrap_or(false)
    }

    pub fn has_query_warnings(&self) -> bool {
        self.query_plan_warnings
            .as_ref()
            .map(|s| s.total_warnings > 0)
            .unwrap_or(false)
    }

    pub fn has_slow_queries(&self) -> bool {
        self.slow_query_summary
            .as_ref()
            .map(|s| s.total_slow_queries > 0)
            .unwrap_or(false)
    }

    pub fn is_healthy(&self) -> bool {
        !self.has_integrity_issues() && !self.has_slow_queries() && !self.has_query_warnings()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite_observability::integrity::*;
    use crate::infrastructure::sqlite_observability::query_plan::*;
    use crate::infrastructure::sqlite_observability::slow_query::*;

    fn sample_integrity_snapshot() -> IntegritySnapshot {
        IntegritySnapshot::new(
            IntegrityCheckResult::parse("ok"),
            QuickCheckResult::parse("ok"),
        )
    }

    fn sample_wal_metrics() -> WalMetrics {
        WalMetrics::new(4096, 100, 5, 0, super::super::wal::WalState::Ok, 1000)
    }

    fn sample_query_diagnostics() -> Vec<QueryDiagnostics> {
        let rows = vec![(3, 0, 0, "SCAN TABLE products".into())];
        vec![QueryDiagnostics::new("SELECT * FROM products", rows)]
    }

    fn sample_slow_record() -> SlowQueryRecord {
        let mut record = SlowQueryRecord::new(10);
        record.record("SELECT * FROM products", 200, 100);
        record
    }

    #[test]
    fn snapshot_construction() {
        let snap = SqliteDiagnosticsSnapshot::new(
            Some(sample_wal_metrics()),
            Some(sample_integrity_snapshot()),
            Some(&sample_slow_record()),
            sample_query_diagnostics(),
            None,
            1,
        );
        assert!(snap.wal_metrics.is_some());
        assert!(snap.integrity_snapshot.is_some());
        assert!(snap.slow_query_summary.is_some());
        assert!(snap.query_plan_warnings.is_some());
    }

    #[test]
    fn snapshot_is_healthy() {
        let snap = SqliteDiagnosticsSnapshot::new(
            Some(sample_wal_metrics()),
            Some(sample_integrity_snapshot()),
            None,
            vec![],
            None,
            1,
        );
        assert!(snap.is_healthy());
    }

    #[test]
    fn snapshot_with_integrity_issues_is_unhealthy() {
        let check = IntegrityCheckResult::parse("row 5 missing from index idx_products");
        let quick = QuickCheckResult::parse("ok");
        let snap = SqliteDiagnosticsSnapshot::new(
            None,
            Some(IntegritySnapshot::new(check, quick)),
            None,
            vec![],
            None,
            1,
        );
        assert!(snap.has_integrity_issues());
        assert!(!snap.is_healthy());
    }

    #[test]
    fn slow_query_summary_from_empty_record() {
        let record = SlowQueryRecord::new(10);
        let summary = SlowQuerySummary::from_record(&record);
        assert_eq!(summary.total_slow_queries, 0);
        assert!(summary.slowest.is_none());
    }

    #[test]
    fn slow_query_summary_from_record() {
        let record = sample_slow_record();
        let summary = SlowQuerySummary::from_record(&record);
        assert_eq!(summary.total_slow_queries, 1);
        assert!(summary.slowest.is_some());
    }

    #[test]
    fn query_plan_warning_summary_empty() {
        let summary = QueryPlanWarningSummary::from_diagnostics(&[]);
        assert_eq!(summary.query_count, 0);
        assert_eq!(summary.total_warnings, 0);
    }

    #[test]
    fn serde_round_trip_snapshot() {
        let snap = SqliteDiagnosticsSnapshot::new(
            Some(sample_wal_metrics()),
            Some(sample_integrity_snapshot()),
            Some(&sample_slow_record()),
            sample_query_diagnostics(),
            None,
            1,
        );
        let json = serde_json::to_string(&snap).unwrap();
        let deserialized: SqliteDiagnosticsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap, deserialized);
    }

    #[test]
    fn same_input_same_snapshot() {
        let snap1 = SqliteDiagnosticsSnapshot::new(
            Some(WalMetrics::new(
                4096,
                100,
                5,
                0,
                super::super::wal::WalState::Ok,
                1000,
            )),
            Some(sample_integrity_snapshot()),
            Some(&sample_slow_record()),
            sample_query_diagnostics(),
            None,
            1,
        );
        let snap2 = SqliteDiagnosticsSnapshot::new(
            Some(WalMetrics::new(
                4096,
                100,
                5,
                0,
                super::super::wal::WalState::Ok,
                1000,
            )),
            Some(sample_integrity_snapshot()),
            Some(&sample_slow_record()),
            sample_query_diagnostics(),
            None,
            1,
        );
        assert_eq!(snap1, snap2);
    }

    #[test]
    fn snapshot_id_is_deterministic_given_order() {
        let snap1 = SqliteDiagnosticsSnapshot::new(None, None, None, vec![], None, 42);
        let snap2 = SqliteDiagnosticsSnapshot::new(None, None, None, vec![], None, 42);
        assert_eq!(snap1.snapshot_id, snap2.snapshot_id);
    }
}
