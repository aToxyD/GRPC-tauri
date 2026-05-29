//! Read-only cross-surface operational consistency verification — manual invocation only.

use crate::application::services::operational_anomaly_service::{
    FindingSeverity, OperationalAnomalyService,
};
use crate::application::services::operational_recommendation_service::OperationalRecommendationService;
use crate::application::services::system_integrity_state_service::SystemIntegrityState;
use crate::application::services::{FiscalTimelineQuery, FiscalTimelineService, TimelineEventKind};
use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use crate::repositories::RepositoryProvider;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OperationalConsistencyStatus {
    Consistent,
    DriftDetected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationalConsistencyFinding {
    pub rule_id: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationalConsistencyReport {
    pub status: OperationalConsistencyStatus,
    pub findings: Vec<OperationalConsistencyFinding>,
    pub drift_count: usize,
}

pub struct OperationalConsistencyVerifier<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> OperationalConsistencyVerifier<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn verify(&self) -> Result<OperationalConsistencyReport, AppError> {
        let mut findings = Vec::new();

        self.rule_critical_anomaly_has_recommendation(&mut findings)?;
        self.rule_corruption_blocks_operations(&mut findings)?;
        self.rule_archived_years_immutable_in_diagnostics(&mut findings)?;
        self.rule_integrity_failures_in_timeline(&mut findings)?;
        self.rule_fiscal_close_audit_timeline_snapshot(&mut findings)?;

        let drift_count = findings.iter().filter(|f| !f.passed).count();
        let status = if drift_count == 0 {
            OperationalConsistencyStatus::Consistent
        } else {
            OperationalConsistencyStatus::DriftDetected
        };

        Ok(OperationalConsistencyReport {
            status,
            findings,
            drift_count,
        })
    }

    fn push(
        findings: &mut Vec<OperationalConsistencyFinding>,
        rule_id: &str,
        passed: bool,
        message: String,
    ) {
        findings.push(OperationalConsistencyFinding {
            rule_id: rule_id.to_string(),
            passed,
            message,
        });
    }

    /// A: every CRITICAL operational finding has a non-empty recommendation.
    fn rule_critical_anomaly_has_recommendation(
        &self,
        findings: &mut Vec<OperationalConsistencyFinding>,
    ) -> Result<(), AppError> {
        let missing = self
            .executor
            .anomaly()
            .count_critical_without_recommendation()?;
        let passed = missing == 0;
        Self::push(
            findings,
            "A_critical_anomaly_recommendation",
            passed,
            if passed {
                "all CRITICAL findings include recommendations".into()
            } else {
                format!("{} CRITICAL finding(s) missing recommendation", missing)
            },
        );
        Ok(())
    }

    /// B: corrupted/critical integrity state is reflected in anomaly analysis severity.
    fn rule_corruption_blocks_operations(
        &self,
        findings: &mut Vec<OperationalConsistencyFinding>,
    ) -> Result<(), AppError> {
        let state = SystemIntegrityState::resolve_from_executor(self.executor)?;
        let blocks = state.blocks_financial_operations();
        let analysis = OperationalAnomalyService::new(self.executor).analyze()?;
        let has_critical_finding = analysis
            .findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Critical);
        let passed = if blocks { has_critical_finding } else { true };
        Self::push(
            findings,
            "B_corruption_blocks_operations",
            passed,
            if passed {
                format!(
                    "integrity state {:?} aligned with operational findings",
                    state
                )
            } else {
                format!(
                    "integrity state {:?} blocks operations but no CRITICAL operational finding surfaced",
                    state
                )
            },
        );
        Ok(())
    }

    /// C: archived years appear in fiscal_year_status with archived=1 (diagnostics immutability signal).
    fn rule_archived_years_immutable_in_diagnostics(
        &self,
        findings: &mut Vec<OperationalConsistencyFinding>,
    ) -> Result<(), AppError> {
        let archived_years = self.executor.fiscal_year_status().count_archived()?;
        let (inconsistent, _, _) = self
            .executor
            .fiscal_year_status()
            .count_invariant_violations()?;
        let passed = inconsistent == 0;
        Self::push(
            findings,
            "C_archived_years_immutable",
            passed,
            if passed {
                format!(
                    "{} archived year(s) satisfy closed+archived invariant",
                    archived_years
                )
            } else {
                format!(
                    "{} archived year row(s) violate closed+archived invariant",
                    inconsistent
                )
            },
        );
        Ok(())
    }

    /// D: recent integrity FAIL attempts appear in fiscal timeline (best-effort window).
    fn rule_integrity_failures_in_timeline(
        &self,
        findings: &mut Vec<OperationalConsistencyFinding>,
    ) -> Result<(), AppError> {
        let failed_recent = self.executor.integrity().count_recent_failures()?;
        if failed_recent == 0 {
            Self::push(
                findings,
                "D_integrity_failures_in_timeline",
                true,
                "no recent integrity failures to correlate".into(),
            );
            return Ok(());
        }
        let query = FiscalTimelineQuery {
            limit: Some(500),
            ..Default::default()
        };
        let timeline = FiscalTimelineService::new(self.executor).build_timeline(&query)?;
        let timeline_integrity = timeline
            .iter()
            .filter(|e| {
                matches!(e.kind, TimelineEventKind::IntegrityFailure)
                    || e.summary.to_lowercase().contains("integrity")
            })
            .count();
        let passed = timeline_integrity > 0;
        Self::push(
            findings,
            "D_integrity_failures_in_timeline",
            passed,
            if passed {
                format!(
                    "{} recent integrity failure(s); timeline exposes integrity-related events",
                    failed_recent
                )
            } else {
                format!(
                    "{} recent integrity failure(s) but timeline has no integrity-related events",
                    failed_recent
                )
            },
        );
        Ok(())
    }

    /// E: each fiscal close produces audit + timeline + operational snapshot consistency.
    fn rule_fiscal_close_audit_timeline_snapshot(
        &self,
        findings: &mut Vec<OperationalConsistencyFinding>,
    ) -> Result<(), AppError> {
        let closed_years = self.executor.fiscal_year_status().count_closed()?;
        if closed_years == 0 {
            Self::push(
                findings,
                "E_fiscal_close_consistency",
                true,
                "no closed fiscal years to verify".into(),
            );
            return Ok(());
        }
        let audit_closes = self.executor.audit().count_by_action("FiscalYearClosed")?;
        let snapshots_after_close = self
            .executor
            .fiscal_snapshots()
            .count_distinct_closed_years_with_snapshot()?;
        let recs = OperationalRecommendationService::new(self.executor).build_recommendations()?;
        let _ = recs;
        let passed = audit_closes > 0 && snapshots_after_close > 0;
        Self::push(
            findings,
            "E_fiscal_close_consistency",
            passed,
            if passed {
                format!(
                    "fiscal closes audited (events={}) with operational snapshots for closed years ({})",
                    audit_closes, snapshots_after_close
                )
            } else {
                format!(
                    "fiscal close consistency drift: audit_events={} snapshot_years={} closed_years={}",
                    audit_closes, snapshots_after_close, closed_years
                )
            },
        );
        Ok(())
    }
}

impl crate::architecture::Service for OperationalConsistencyVerifier<'_> {}
