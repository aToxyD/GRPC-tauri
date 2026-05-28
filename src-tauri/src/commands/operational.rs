//! Operational Intelligence Commands
//!
//! Tauri commands that expose the Controlled Operational Intelligence Phase
//! services to the UI. Every command is a THIN adapter:
//!   - authorize
//!   - touch session
//!   - call the service
//!   - return the DTO
//!
//! No business logic lives here.

use crate::application::authz::Action;
use crate::application::services::{
    CriticalOperation, FiscalOperationalSnapshot, FiscalOperationalSnapshotService,
    FiscalClosingService, FiscalTimelineEvent, FiscalTimelineQuery, FiscalTimelineService,
    GuardedOperation, IntegrityAttemptRecorder, OperationExecutionGuard, OperationalAnalysisReport,
    OperationalAnomalyService, OperationalRecommendation, OperationalRecommendationService,
    OperatorSafetyService, VerificationOutcome, VerificationType,
};
use crate::application::services::{
    DeploymentReadinessReport, DeploymentReadinessService, MaintenanceBlockedOperation,
    OperationalConsistencyReport, OperationalConsistencyVerifier, OperationalSessionRecord,
    OperationalSessionService, SessionCounterKind, SystemMaintenanceState,
};
use crate::commands::common::{db_mut_or_command_error, db_ref_or_command_error};
use crate::commands::guards::{authorize_command, require_maintenance_allows};
use crate::commands::types::AppState;
use crate::errors::{into_command_error, AppError};
use tauri::State;

// ─── PRIORITY 1: Anomaly Detection ───────────────────────────────────────────

#[tauri::command]
pub fn run_operational_analysis(
    state: State<AppState>,
) -> Result<OperationalAnalysisReport, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    // The service performs append-only inserts into operational_findings_log,
    // so we wrap it in a transaction for atomic persistence.
    let report = db
        .with_transaction(|tx_executor| {
            OperationalAnomalyService::new(tx_executor).run_operational_analysis()
        })
        .map_err(into_command_error)?;

    drop(guard);

    if !report.findings.is_empty() {
        state.increment_session_counter(SessionCounterKind::AnomalySurfaced);
    }

    Ok(report)
}

// ─── PRIORITY 2: Manual Operational Snapshots ────────────────────────────────

#[tauri::command]
pub fn create_fiscal_operational_snapshot(
    state: State<AppState>,
    fiscal_year: i32,
) -> Result<i64, String> {
    let (session, _settings) = authorize_command(&state, Action::CloseFiscalYearAuthority, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let user = session.username.clone();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    db.with_transaction(|tx| {
        FiscalOperationalSnapshotService::new(tx).create_snapshot(fiscal_year, &user)
    })
    .map_err(into_command_error)
}

#[tauri::command]
pub fn list_fiscal_operational_snapshots(
    state: State<AppState>,
    fiscal_year: Option<i32>,
    limit: Option<i64>,
) -> Result<Vec<FiscalOperationalSnapshot>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    FiscalOperationalSnapshotService::new(db.executor())
        .list_snapshots(fiscal_year, limit.unwrap_or(100))
        .map_err(into_command_error)
}

// ─── PRIORITY 3: Guided Recommendations ──────────────────────────────────────

#[tauri::command]
pub fn get_operational_recommendations(
    state: State<AppState>,
) -> Result<Vec<OperationalRecommendation>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    OperationalRecommendationService::new(db.executor())
        .build_recommendations()
        .map_err(into_command_error)
}

// ─── PRIORITY 4: Fiscal Timeline Reconstruction ──────────────────────────────

#[tauri::command]
pub fn build_fiscal_timeline(
    state: State<AppState>,
    fiscal_year: Option<i32>,
    from_timestamp: Option<String>,
    to_timestamp: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<FiscalTimelineEvent>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    let q = FiscalTimelineQuery {
        fiscal_year,
        from_timestamp,
        to_timestamp,
        limit,
    };

    FiscalTimelineService::new(db.executor())
        .build_timeline(&q)
        .map_err(into_command_error)
}

// ─── PRIORITY 6: Confirmation-Guarded Critical Operations ────────────────────
//
// These commands wrap the existing critical commands (close_fiscal_year,
// restore_backup, archive_year, import_historical_package) with typed
// confirmation safeguards.

#[derive(serde::Deserialize)]
pub struct ConfirmedCloseFiscalYearRequest {
    pub year: i32,
    pub next_year: i32,
    pub confirmation: String,
    pub execution_token: String,
}

#[derive(serde::Deserialize)]
pub struct IssueExecutionTokenRequest {
    pub operation: String,
    pub year: Option<i32>,
    pub next_year: Option<i32>,
}

#[derive(serde::Serialize)]
pub struct IssueExecutionTokenResponse {
    pub token: String,
    pub operation: String,
}

#[derive(serde::Deserialize)]
pub struct ConfirmedArchiveFiscalYearRequest {
    pub year: i32,
    pub confirmation: String,
    pub execution_token: String,
}

#[derive(serde::Serialize)]
pub struct ConfirmedCloseFiscalYearResponse {
    pub closed_year: i32,
    pub opened_year: i32,
    pub snapshot_count: usize,
}

/// Issue a state-bound execution token for a guarded critical operation.
#[tauri::command]
pub fn issue_operation_execution_token(
    state: State<AppState>,
    request: IssueExecutionTokenRequest,
) -> Result<IssueExecutionTokenResponse, String> {
    let op = parse_guarded_operation(&request).map_err(into_command_error)?;
    match op {
        GuardedOperation::RestoreBackup => {
            authorize_command(&state, Action::ManageBackups, None).map_err(into_command_error)?;
        }
        GuardedOperation::ImportHistorical => {
            authorize_command(&state, Action::ImportProductsPackage, None)
                .map_err(into_command_error)?;
        }
        _ => {
            authorize_command(&state, Action::CloseFiscalYearAuthority, None)
                .map_err(into_command_error)?;
        }
    }
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    let token = OperationExecutionGuard::issue_execution_token(db.executor(), op)
        .map_err(into_command_error)?;

    Ok(IssueExecutionTokenResponse {
        token,
        operation: request.operation,
    })
}

fn parse_guarded_operation(
    request: &IssueExecutionTokenRequest,
) -> Result<GuardedOperation, AppError> {
    match request.operation.as_str() {
        "fiscal_close" => {
            let year = request.year.ok_or_else(|| {
                AppError::Validation(crate::errors::ValidationError::Required {
                    field: "year".into(),
                })
            })?;
            let next_year = request.next_year.ok_or_else(|| {
                AppError::Validation(crate::errors::ValidationError::Required {
                    field: "next_year".into(),
                })
            })?;
            Ok(GuardedOperation::FiscalClose { year, next_year })
        }
        "archive" => {
            let year = request.year.ok_or_else(|| {
                AppError::Validation(crate::errors::ValidationError::Required {
                    field: "year".into(),
                })
            })?;
            Ok(GuardedOperation::ArchiveYear { year })
        }
        "restore" => Ok(GuardedOperation::RestoreBackup),
        "import_historical" => Ok(GuardedOperation::ImportHistorical),
        _ => Err(AppError::Validation(
            crate::errors::ValidationError::InvalidFormat {
                field: "operation".into(),
                message: format!("عملية محمية غير معروفة: {}", request.operation),
            },
        )),
    }
}

/// Fiscal close with typed confirmation: operator must type the year being closed.
#[tauri::command]
pub fn close_fiscal_year_confirmed(
    state: State<AppState>,
    request: ConfirmedCloseFiscalYearRequest,
) -> Result<ConfirmedCloseFiscalYearResponse, String> {
    let (session, _settings) = authorize_command(&state, Action::CloseFiscalYearAuthority, None)
        .map_err(into_command_error)?;
    require_maintenance_allows(&state, MaintenanceBlockedOperation::FiscalClose)
        .map_err(into_command_error)?;
    state.touch_session();

    // Validate typed confirmation BEFORE doing anything else.
    OperatorSafetyService::require_confirmation(
        CriticalOperation::FiscalClose { year: request.year },
        &request.confirmation,
    )
    .map_err(into_command_error)?;

    state
        .operation_guard
        .assert_not_throttled(GuardedOperation::FiscalClose {
            year: request.year,
            next_year: request.next_year,
        })
        .map_err(into_command_error)?;

    if request.next_year != request.year + 1 {
        return Err(into_command_error(AppError::Validation(
            crate::errors::ValidationError::InvalidFormat {
                field: "next_year".to_string(),
                message: format!(
                    "السنة التالية ({}) يجب أن تكون بالتحديد السنة الحالية + 1 ({})",
                    request.next_year,
                    request.year + 1
                ),
            },
        )));
    }

    let user_id = session.user_id.clone();
    let username = session.username.clone();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let close_op = GuardedOperation::FiscalClose {
        year: request.year,
        next_year: request.next_year,
    };
    OperationExecutionGuard::validate_execution_token(
        db.executor(),
        close_op,
        &request.execution_token,
    )
    .map_err(into_command_error)?;

    state
        .maintenance
        .set(SystemMaintenanceState::MaintenanceLocked)
        .map_err(into_command_error)?;

    let snapshot_count = match db.with_transaction(|tx| {
        FiscalClosingService::new(tx).close_year(
            request.year,
            request.next_year,
            &user_id,
            &username,
            None,
        )
    }) {
        Ok(v) => v,
        Err(e) => {
            let _ = state.maintenance.set(SystemMaintenanceState::Normal);
            return Err(into_command_error(e));
        }
    };

    drop(guard);

    let _ = state.maintenance.set(SystemMaintenanceState::Normal);
    state.operation_guard.record_execution(close_op);
    state.increment_session_counter(SessionCounterKind::CriticalOperation);

    Ok(ConfirmedCloseFiscalYearResponse {
        closed_year: request.year,
        opened_year: request.next_year,
        snapshot_count,
    })
}

#[derive(serde::Deserialize)]
pub struct ConfirmedSimpleRequest {
    pub confirmation: String,
}

/// Validate "ARCHIVE" confirmation token. Does NOT mutate state by itself —
/// it is intended to be called from the UI immediately before invoking the
/// actual archive command, providing a deterministic safeguard.
#[tauri::command]
pub fn validate_archive_confirmation(
    state: State<AppState>,
    request: ConfirmedSimpleRequest,
) -> Result<(), String> {
    let (_session, _settings) = authorize_command(&state, Action::CloseFiscalYearAuthority, None)
        .map_err(into_command_error)?;
    state.touch_session();

    OperatorSafetyService::require_confirmation(
        CriticalOperation::ArchiveYear,
        &request.confirmation,
    )
    .map_err(into_command_error)
}

#[tauri::command]
pub fn validate_restore_confirmation(
    state: State<AppState>,
    request: ConfirmedSimpleRequest,
) -> Result<(), String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ManageBackups, None).map_err(into_command_error)?;
    state.touch_session();

    OperatorSafetyService::require_confirmation(
        CriticalOperation::RestoreBackup,
        &request.confirmation,
    )
    .map_err(into_command_error)
}

/// Archive a closed fiscal year (immutable). Requires confirmation + execution token.
#[tauri::command]
pub fn archive_fiscal_year_confirmed(
    state: State<AppState>,
    request: ConfirmedArchiveFiscalYearRequest,
) -> Result<(), String> {
    let (session, _settings) = authorize_command(&state, Action::CloseFiscalYearAuthority, None)
        .map_err(into_command_error)?;
    require_maintenance_allows(&state, MaintenanceBlockedOperation::Archive)
        .map_err(into_command_error)?;
    state.touch_session();

    OperatorSafetyService::require_confirmation(
        CriticalOperation::ArchiveYear,
        &request.confirmation,
    )
    .map_err(into_command_error)?;

    let archive_op = GuardedOperation::ArchiveYear { year: request.year };
    state
        .operation_guard
        .assert_not_throttled(archive_op)
        .map_err(into_command_error)?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    OperationExecutionGuard::assert_integrity_allows_restore_or_archive(db.executor())
        .map_err(into_command_error)?;
    OperationExecutionGuard::validate_execution_token(
        db.executor(),
        archive_op,
        &request.execution_token,
    )
    .map_err(into_command_error)?;
    OperationExecutionGuard::assert_archive_no_critical_anomalies(db.executor(), request.year)
        .map_err(into_command_error)?;

    let user_id = session.user_id.clone();
    let username = session.username.clone();

    state
        .maintenance
        .set(SystemMaintenanceState::MaintenanceLocked)
        .map_err(into_command_error)?;

    if let Err(e) = db.with_transaction(|tx| {
        FiscalClosingService::new(tx).archive_year(request.year, &user_id, &username)
    }) {
        let _ = state.maintenance.set(SystemMaintenanceState::Normal);
        return Err(into_command_error(e));
    }

    drop(guard);

    let _ = state.maintenance.set(SystemMaintenanceState::Normal);
    state.operation_guard.record_execution(archive_op);
    state.increment_session_counter(SessionCounterKind::CriticalOperation);
    Ok(())
}

#[tauri::command]
pub fn validate_historical_import_confirmation(
    state: State<AppState>,
    request: ConfirmedSimpleRequest,
) -> Result<(), String> {
    let (_session, _settings) = authorize_command(&state, Action::ImportProductsPackage, None)
        .map_err(into_command_error)?;
    state.touch_session();

    OperatorSafetyService::require_confirmation(
        CriticalOperation::ImportHistoricalPackage,
        &request.confirmation,
    )
    .map_err(into_command_error)
}

// ─── Aggregated Diagnostics (used by FiscalDiagnosticsPage in one round-trip) ──

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedDiagnosticsBundle {
    pub anomaly_report: OperationalAnalysisReport,
    pub recommendations: Vec<OperationalRecommendation>,
    pub recent_snapshots: Vec<FiscalOperationalSnapshot>,
    pub timeline: Vec<FiscalTimelineEvent>,
    pub fiscal_integrity:
        crate::application::services::fiscal_integrity_service::FiscalIntegrityReport,
}

#[tauri::command]
pub fn get_advanced_diagnostics_bundle(
    state: State<AppState>,
    fiscal_year: Option<i32>,
) -> Result<AdvancedDiagnosticsBundle, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let bundle = db
        .with_transaction(|tx| {
            let anomaly_report = OperationalAnomalyService::new(tx).run_operational_analysis()?;
            let fiscal_integrity =
                crate::application::services::fiscal_integrity_service::FiscalIntegrityService::new(tx)
                    .run_full_integrity_scan()?;

            // Record outcome of fiscal-drift verification attempt (Anomaly E source).
            let recorder = IntegrityAttemptRecorder::new(tx);
            if fiscal_integrity.ok {
                recorder.record(VerificationType::FiscalDrift, VerificationOutcome::Pass, None);
            } else {
                let details = serde_json::to_string(&fiscal_integrity.warnings).unwrap_or_default();
                recorder.record(
                    VerificationType::FiscalDrift,
                    VerificationOutcome::Fail,
                    Some(&details),
                );
            }

            let recommendations =
                OperationalRecommendationService::new(tx).build_recommendations()?;
            let recent_snapshots =
                FiscalOperationalSnapshotService::new(tx).list_snapshots(fiscal_year, 30)?;
            let timeline = FiscalTimelineService::new(tx).build_timeline(&FiscalTimelineQuery {
                fiscal_year,
                from_timestamp: None,
                to_timestamp: None,
                limit: Some(200),
            })?;

            Ok::<_, AppError>(AdvancedDiagnosticsBundle {
                anomaly_report,
                recommendations,
                recent_snapshots,
                timeline,
                fiscal_integrity,
            })
        })
        .map_err(into_command_error)?;

    Ok(bundle)
}

// ─── Operator Reliability (read-only verification + session history) ─────────

#[tauri::command]
pub fn verify_deployment_readiness(
    state: State<AppState>,
) -> Result<DeploymentReadinessReport, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    let db_path = db
        .get_connection_path()
        .map_err(|e| into_command_error(AppError::Internal(e.to_string())))?;

    DeploymentReadinessService::new(db.executor(), db_path)
        .verify()
        .map_err(into_command_error)
}

#[tauri::command]
pub fn verify_operational_consistency(
    state: State<AppState>,
) -> Result<OperationalConsistencyReport, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    OperationalConsistencyVerifier::new(db.executor())
        .verify()
        .map_err(into_command_error)
}

#[tauri::command]
pub fn get_system_maintenance_state(state: State<AppState>) -> Result<String, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();
    state
        .maintenance
        .get()
        .map(|s| s.label().to_string())
        .map_err(into_command_error)
}

#[tauri::command]
pub fn list_operational_sessions(
    state: State<AppState>,
    limit: Option<i64>,
) -> Result<Vec<OperationalSessionRecord>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    OperationalSessionService::new(db.executor())
        .list_recent(limit.unwrap_or(50))
        .map_err(into_command_error)
}
