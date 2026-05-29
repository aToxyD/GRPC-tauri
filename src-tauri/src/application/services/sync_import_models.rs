use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::application::sync_integrity::types::{
    ConflictDetectionOutcome, SyncConflict,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncImportRequest {
    pub package_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub kind: SyncPackageKind,
    pub incoming_sequence: Option<u64>,
    pub last_applied_sequence: Option<u64>,
    pub fiscal_year: Option<i32>,
    pub transition_ids: Vec<String>,
    pub transaction_ids: Vec<String>,
    pub payload: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncPackageKind {
    Fiscal,
    Operational,
    Product,
    StockMovement,
    DailyReport,
    MonthlyReport,
}

impl SyncPackageKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fiscal => "fiscal",
            Self::Operational => "operational",
            Self::Product => "product",
            Self::StockMovement => "stock_movement",
            Self::DailyReport => "daily_report",
            Self::MonthlyReport => "monthly_report",
        }
    }

    pub fn is_fiscal(&self) -> bool {
        matches!(self, Self::Fiscal)
    }

    pub fn is_operational(&self) -> bool {
        matches!(self, Self::Operational)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncImportResult {
    pub success: bool,
    pub execution_summary: Option<ImportExecutionSummary>,
    pub conflict_outcome: Option<ConflictResolutionOutcome>,
    pub replay_protection: Option<ReplayProtectionResult>,
    pub error: Option<ImportExecutionError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportExecutionSummary {
    pub package_id: String,
    pub kind: SyncPackageKind,
    pub transaction_id: String,
    pub events_emitted: u64,
    pub mutations_applied: ImportMutationSummary,
    pub fiscal_year: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportMutationSummary {
    pub products_imported: u64,
    pub products_updated: u64,
    pub products_skipped: u64,
    pub movements_applied: u64,
    pub reports_imported: u64,
    pub fiscal_transitions_applied: u64,
}

impl ImportMutationSummary {
    pub fn none() -> Self {
        Self {
            products_imported: 0,
            products_updated: 0,
            products_skipped: 0,
            movements_applied: 0,
            reports_imported: 0,
            fiscal_transitions_applied: 0,
        }
    }

    pub fn has_mutations(&self) -> bool {
        self.products_imported > 0
            || self.products_updated > 0
            || self.movements_applied > 0
            || self.reports_imported > 0
            || self.fiscal_transitions_applied > 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictResolutionOutcome {
    pub conflict: SyncConflict,
    pub resolution: ResolutionPolicy,
    pub audit_logged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResolutionPolicy {
    RejectWithAudit,
    SkipIdempotent,
    ManualResolutionRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayProtectionResult {
    pub package_id: String,
    pub replay_detected: bool,
    pub conflict: Option<SyncConflict>,
    pub audited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportExecutionError {
    ReplayDetected(String),
    SequenceGap { expected: u64, got: u64 },
    StaleImport { incoming_sequence: u64, last_applied: u64 },
    FiscalYearMismatch { incoming: i32, current: i32 },
    FiscalYearClosed(i32),
    DuplicatePackage(String),
    ValidationFailed(String),
    TransactionFailed(String),
    Internal(String),
}

impl std::fmt::Display for ImportExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReplayDetected(pkg) => write!(f, "Replay detected for package: {}", pkg),
            Self::SequenceGap { expected, got } => {
                write!(f, "Sequence gap: expected {} but got {}", expected, got)
            }
            Self::StaleImport {
                incoming_sequence,
                last_applied,
            } => write!(
                f,
                "Stale import: incoming={} last_applied={}",
                incoming_sequence, last_applied
            ),
            Self::FiscalYearMismatch { incoming, current } => {
                write!(f, "Fiscal year mismatch: incoming={} current={}", incoming, current)
            }
            Self::FiscalYearClosed(y) => write!(f, "Fiscal year {} is closed", y),
            Self::DuplicatePackage(pkg) => write!(f, "Duplicate package: {}", pkg),
            Self::ValidationFailed(msg) => write!(f, "Validation failed: {}", msg),
            Self::TransactionFailed(msg) => write!(f, "Transaction failed: {}", msg),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationSnapshot {
    pub package_id: String,
    pub replay_check: ConflictDetectionOutcome,
    pub sequence_check: Option<ConflictDetectionOutcome>,
    pub fiscal_year_check: Option<ConflictDetectionOutcome>,
    pub fiscal_scope: Option<i32>,
    pub all_checks_passed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_package_kind_as_str() {
        assert_eq!(SyncPackageKind::Fiscal.as_str(), "fiscal");
        assert_eq!(SyncPackageKind::Operational.as_str(), "operational");
        assert_eq!(SyncPackageKind::Product.as_str(), "product");
    }

    #[test]
    fn fiscal_kind_is_fiscal() {
        assert!(SyncPackageKind::Fiscal.is_fiscal());
        assert!(!SyncPackageKind::Operational.is_fiscal());
    }

    #[test]
    fn mutation_summary_none_has_no_mutations() {
        let s = ImportMutationSummary::none();
        assert!(!s.has_mutations());
    }

    #[test]
    fn mutation_summary_with_mutations() {
        let s = ImportMutationSummary {
            products_imported: 1,
            ..ImportMutationSummary::none()
        };
        assert!(s.has_mutations());
    }

    #[test]
    fn import_execution_error_display() {
        let err = ImportExecutionError::ReplayDetected("pkg-1".into());
        assert!(err.to_string().contains("Replay detected"));

        let err = ImportExecutionError::SequenceGap {
            expected: 5,
            got: 7,
        };
        assert!(err.to_string().contains("Sequence gap"));
    }

    #[test]
    fn sync_import_request_serde_round_trip() {
        let req = SyncImportRequest {
            package_id: "pkg-1".into(),
            source_node_id: "node-a".into(),
            target_node_id: "node-b".into(),
            kind: SyncPackageKind::Fiscal,
            incoming_sequence: Some(1),
            last_applied_sequence: None,
            fiscal_year: Some(2025),
            transition_ids: vec![],
            transaction_ids: vec![],
            payload: HashMap::new(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let restored: SyncImportRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.package_id, "pkg-1");
        assert_eq!(restored.kind.as_str(), "fiscal");
    }

    #[test]
    fn import_execution_error_serde_round_trip() {
        let err = ImportExecutionError::SequenceGap {
            expected: 5,
            got: 7,
        };
        let json = serde_json::to_string(&err).unwrap();
        let restored: ImportExecutionError = serde_json::from_str(&json).unwrap();
        match restored {
            ImportExecutionError::SequenceGap { expected, got } => {
                assert_eq!(expected, 5);
                assert_eq!(got, 7);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn resolution_policy_serde() {
        let p = ResolutionPolicy::RejectWithAudit;
        let json = serde_json::to_string(&p).unwrap();
        let restored: ResolutionPolicy = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, ResolutionPolicy::RejectWithAudit));
    }

    #[test]
    fn replay_protection_result_construction() {
        let r = ReplayProtectionResult {
            package_id: "pkg-1".into(),
            replay_detected: false,
            conflict: None,
            audited: true,
        };
        assert!(!r.replay_detected);
        assert!(r.audited);
    }

    #[test]
    fn import_result_defaults() {
        let r = SyncImportResult {
            success: true,
            execution_summary: None,
            conflict_outcome: None,
            replay_protection: None,
            error: None,
        };
        assert!(r.success);
    }
}
