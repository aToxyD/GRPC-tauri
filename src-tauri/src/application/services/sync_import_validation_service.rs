use crate::application::sync_integrity::replay::ReplayDetector;
use crate::application::sync_integrity::types::ConflictDetectionOutcome;
use crate::application::sync_integrity::validation::ValidationGate;
use crate::db::Database;
use crate::errors::AppError;
use crate::repositories::DbExecutor;

use super::sync_import_models::{SyncImportRequest, ValidationSnapshot};

pub struct SyncImportValidationService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> SyncImportValidationService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Validate fiscal constraints for an incoming package import.
    /// Checks: fiscal year is open, year is within allowed import horizon,
    /// and non-historical packages match the current year.
    pub fn validate_import_fiscal_year(
        &self,
        incoming_year: i32,
        package_type: &str,
        source_node: &str,
        package_id: &str,
    ) -> Result<(), AppError> {
        let current_year: i32 = self
            .executor
            // [arch:allow-sql] pre-existing legacy method
            .query_row("SELECT current_year FROM settings WHERE id=1", [], |r| {
                r.get(0)
            })?;
        crate::application::services::FiscalHistoricalGuard::new(self.executor)
            .assert_import_year_allowed(incoming_year)?;
        crate::application::services::FiscalValidationService::new(self.executor)
            .assert_fiscal_year_open(incoming_year)?;
        if package_type != "historical_import" && incoming_year != current_year {
            log::warn!(target:"grpc::sync","[FISCAL_IMPORT_REJECTED] source_node={} package_id={} incoming_year={} current_year={}",source_node,package_id,incoming_year,current_year);
            return Err(AppError::Internal(
                "Fiscal year mismatch during import".into(),
            ));
        }
        Ok(())
    }

    pub fn get_current_node_id(db: &Database) -> String {
        match crate::application::services::SettingsService::new(db.executor()).get_settings() {
            Ok(s) => s.unit_name.unwrap_or_else(|| "WILAYA".to_string()),
            Err(_) => "unknown".to_string(),
        }
    }

    /// Phase 5.C: Validate a sync import request without mutating state.
    pub fn validate(
        request: &SyncImportRequest,
        replay_detector: &ReplayDetector,
        current_fiscal_year: i32,
        max_historical_years: i32,
    ) -> ValidationSnapshot {
        let package_id = &request.package_id;

        let replay_check = replay_detector.check_all(
            package_id,
            &request.transition_ids,
            &request.transaction_ids,
        );

        if !matches!(replay_check, ConflictDetectionOutcome::NoConflict) {
            return ValidationSnapshot {
                package_id: package_id.clone(),
                replay_check,
                sequence_check: None,
                fiscal_year_check: None,
                fiscal_scope: request.fiscal_year,
                all_checks_passed: false,
            };
        }

        let sequence_check = if request.kind.is_operational() {
            let result = ValidationGate::validate_import(
                package_id,
                replay_detector,
                request.incoming_sequence,
                request.last_applied_sequence,
                request.fiscal_year,
            );
            if let Some(ConflictDetectionOutcome::ConflictDetected(_)) =
                result.replay_check.as_ref()
            {
                return ValidationSnapshot {
                    package_id: package_id.clone(),
                    replay_check,
                    sequence_check: result.replay_check,
                    fiscal_year_check: None,
                    fiscal_scope: request.fiscal_year,
                    all_checks_passed: false,
                };
            }
            result.duplicate_check.or(result.replay_check)
        } else {
            None
        };

        let fiscal_year_check = if let Some(year) = request.fiscal_year {
            match ValidationGate::validate_fiscal_year(
                year,
                current_fiscal_year,
                max_historical_years,
            ) {
                Ok(_) => None,
                Err(_) => {
                    return ValidationSnapshot {
                        package_id: package_id.clone(),
                        replay_check,
                        sequence_check: None,
                        fiscal_year_check: Some(ConflictDetectionOutcome::ConflictDetected(
                            crate::application::sync_integrity::types::SyncConflict::StaleImport(
                                crate::application::sync_integrity::types::ConflictMetadata {
                                    conflict_id: crate::application::sync_integrity::types::ConflictId(
                                        format!("fy-mismatch-{}-{}", year, current_fiscal_year)
                                    ),
                                    package_id: package_id.clone(),
                                    source_node_id: request.source_node_id.clone(),
                                    target_node_id: request.target_node_id.clone(),
                                    explanation: crate::application::sync_integrity::types::ConflictExplanation {
                                        label: "FiscalYearMismatch".into(),
                                        description: format!("fiscal year {} is not valid (current: {})", year, current_fiscal_year),
                                    },
                                    fiscal_scope: crate::application::sync_integrity::types::FiscalScope::single_year(year),
                                    evidence: vec![format!("incoming_year:{}", year), format!("current_year:{}", current_fiscal_year)],
                                },
                            ),
                        )),
                        fiscal_scope: request.fiscal_year,
                        all_checks_passed: false,
                    };
                }
            }
        } else {
            None
        };

        let all_checks_passed = sequence_check
            .as_ref()
            .is_none_or(|c| matches!(c, ConflictDetectionOutcome::NoConflict))
            && fiscal_year_check
                .as_ref()
                .is_none_or(|c| matches!(c, ConflictDetectionOutcome::NoConflict));

        ValidationSnapshot {
            package_id: package_id.clone(),
            replay_check,
            sequence_check,
            fiscal_year_check,
            fiscal_scope: request.fiscal_year,
            all_checks_passed,
        }
    }

    pub fn validate_immutable_snapshot(
        request: &SyncImportRequest,
        applied_packages: std::collections::BTreeSet<String>,
        applied_transitions: std::collections::BTreeSet<String>,
        seen_transactions: std::collections::BTreeSet<String>,
        current_fiscal_year: i32,
        max_historical_years: i32,
    ) -> ValidationSnapshot {
        let detector = ReplayDetector::new(
            crate::application::sync_integrity::replay::AppliedPackages::new(applied_packages),
            crate::application::sync_integrity::replay::AppliedTransitions::new(
                applied_transitions,
            ),
            crate::application::sync_integrity::replay::SeenTransactions::new(seen_transactions),
            request.source_node_id.clone(),
            request.target_node_id.clone(),
        );

        Self::validate(
            request,
            &detector,
            current_fiscal_year,
            max_historical_years,
        )
    }
}

/// Pure-function timestamp comparison — determines whether an incoming record
/// is newer than the existing one.
///
/// Used by product sync to avoid overwriting with stale data.
pub fn is_incoming_newer(incoming: &str, existing: &str) -> bool {
    match (
        chrono::DateTime::parse_from_rfc3339(incoming),
        chrono::DateTime::parse_from_rfc3339(existing),
    ) {
        (Ok(incoming_dt), Ok(existing_dt)) => {
            incoming_dt.with_timezone(&chrono::Utc) > existing_dt.with_timezone(&chrono::Utc)
        }
        _ => incoming > existing,
    }
}

impl crate::architecture::Service for SyncImportValidationService<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::services::sync_import_models::SyncPackageKind;
    use crate::application::sync_integrity::replay::{
        AppliedPackages, AppliedTransitions, SeenTransactions,
    };
    use std::collections::BTreeSet;

    fn empty_detector() -> ReplayDetector {
        ReplayDetector::new(
            AppliedPackages::new(BTreeSet::new()),
            AppliedTransitions::new(BTreeSet::new()),
            SeenTransactions::new(BTreeSet::new()),
            "node-a".into(),
            "node-b".into(),
        )
    }

    fn make_request(
        package_id: &str,
        kind: crate::application::services::SyncPackageKind,
    ) -> SyncImportRequest {
        SyncImportRequest {
            package_id: package_id.into(),
            source_node_id: "node-a".into(),
            target_node_id: "node-b".into(),
            kind,
            incoming_sequence: None,
            last_applied_sequence: None,
            fiscal_year: Some(2025),
            transition_ids: vec![],
            transaction_ids: vec![],
            payload: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn fresh_package_passes_validation() {
        let req = make_request("pkg-fresh", SyncPackageKind::Fiscal);
        let d = empty_detector();
        let result = SyncImportValidationService::validate(&req, &d, 2025, 5);
        assert!(result.all_checks_passed);
    }

    #[test]
    fn duplicate_package_rejected() {
        let mut pkgs = BTreeSet::new();
        pkgs.insert("pkg-dup".into());
        let req = make_request("pkg-dup", SyncPackageKind::Fiscal);
        let d = ReplayDetector::new(
            AppliedPackages::new(pkgs),
            AppliedTransitions::new(BTreeSet::new()),
            SeenTransactions::new(BTreeSet::new()),
            "node-a".into(),
            "node-b".into(),
        );
        let result = SyncImportValidationService::validate(&req, &d, 2025, 5);
        assert!(!result.all_checks_passed);
        assert!(matches!(
            result.replay_check,
            ConflictDetectionOutcome::ConflictDetected(_)
        ));
    }

    #[test]
    fn sequence_gap_detected_for_operational() {
        let req = SyncImportRequest {
            incoming_sequence: Some(5),
            last_applied_sequence: Some(2),
            ..make_request("pkg-seq", SyncPackageKind::Operational)
        };
        let d = empty_detector();
        let result = SyncImportValidationService::validate(&req, &d, 2025, 5);
        assert!(!result.all_checks_passed);
    }

    #[test]
    fn validation_deterministic_for_same_input() {
        let req = make_request("pkg-det", SyncPackageKind::Fiscal);
        let d = empty_detector();
        let r1 = SyncImportValidationService::validate(&req, &d, 2025, 5);
        let r2 = SyncImportValidationService::validate(&req, &d, 2025, 5);
        assert_eq!(r1.all_checks_passed, r2.all_checks_passed);
        assert_eq!(
            format!("{:?}", r1.replay_check),
            format!("{:?}", r2.replay_check)
        );
    }

    #[test]
    fn immutable_snapshot_produces_same_result() {
        let req = make_request("pkg-snap", SyncPackageKind::Fiscal);
        let pkgs: BTreeSet<String> = BTreeSet::new();
        let transitions: BTreeSet<String> = BTreeSet::new();
        let seen: BTreeSet<String> = BTreeSet::new();
        let result = SyncImportValidationService::validate_immutable_snapshot(
            &req,
            pkgs,
            transitions,
            seen,
            2025,
            5,
        );
        assert!(result.all_checks_passed);
    }

    #[test]
    fn fiscal_year_mismatch_rejected() {
        let req = make_request("pkg-fy", SyncPackageKind::Fiscal);
        let d = empty_detector();
        let result = SyncImportValidationService::validate(&req, &d, 2024, 5);
        assert!(!result.all_checks_passed);
    }

    #[test]
    fn no_mutation_during_validation() {
        let req = make_request("pkg-nomut", SyncPackageKind::Fiscal);
        let d = empty_detector();
        let _result = SyncImportValidationService::validate(&req, &d, 2025, 5);
    }
}
