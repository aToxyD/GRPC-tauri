//! Fiscal lifecycle commands — Admin + Wilaya only.

use crate::application::authz::Action;
use crate::application::services::FiscalReportingService;
use crate::commands::common::{db_mut_or_app_error, db_mut_or_command_error};
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::domain::events::DomainEvent;
use crate::errors::{into_command_error, AppError};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct CloseFiscalYearRequest {
    pub year: i32,
    pub next_year: i32,
}

#[derive(Debug, Serialize)]
pub struct CloseFiscalYearResponse {
    pub closed_year: i32,
    pub opened_year: i32,
    pub snapshot_count: usize,
}

/// Close the current fiscal year and open the next one.
///
/// Requires: Admin role + Wilaya node.
/// Fully atomic — wrapped in `db.with_transaction`.
#[tauri::command]
pub fn close_fiscal_year(
    state: State<AppState>,
    request: CloseFiscalYearRequest,
) -> Result<CloseFiscalYearResponse, String> {
    let (session, _settings) = authorize_command(&state, Action::CloseFiscalYearAuthority, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let year = request.year;
    let next_year = request.next_year;

    if next_year != year + 1 {
        return Err(into_command_error(crate::errors::AppError::Validation(
            crate::errors::ValidationError::InvalidFormat {
                field: "next_year".to_string(),
                message: format!(
                    "السنة التالية ({}) يجب أن تكون بالتحديد السنة الحالية + 1 ({})",
                    next_year,
                    year + 1
                ),
            },
        )));
    }

    let user_id = session.user_id.clone();
    let username = session.username.clone();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let (snapshot_count, _event_buffer) = db
        .with_event_persistence(|ctx| {
            let executor = ctx.executor();
            let count = crate::application::services::FiscalClosingService::new(executor)
                .close_year(year, next_year, &user_id, &username, None)?;

            // NEW: Automatically register the package for export via service (Architectural integrity)
            crate::application::services::FiscalClosurePackageService::new(executor)
                .register_pending_package(year, next_year, &username)?;

            ctx.emit(DomainEvent::FiscalYearClosed { year });
            ctx.emit(DomainEvent::FiscalTransitionApplied {
                from_year: year,
                to_year: next_year,
            });

            Ok(count)
        })
        .map_err(|e| {
            log::error!(
                target: "grpc::fiscal",
                "[FISCAL_CLOSE_FAILED] year={} next_year={} user_id={} err={:?}",
                year, next_year, user_id, e
            );
            into_command_error(e)
        })?;

    Ok(CloseFiscalYearResponse {
        closed_year: year,
        opened_year: next_year,
        snapshot_count,
    })
}

/// Get the status of a fiscal year.
#[tauri::command]
pub fn get_fiscal_year_status(
    state: State<AppState>,
    year: i32,
) -> Result<Option<crate::models::FiscalYearStatus>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::AuthenticatedOnly, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = guard.as_ref().ok_or_else(|| {
        into_command_error(crate::errors::AppError::Internal("DB unavailable".into()))
    })?;

    FiscalReportingService::new(db.executor())
        .get_status(year)
        .map_err(into_command_error)
}

#[tauri::command]
pub fn run_fiscal_integrity_scan(
    state: State<AppState>,
) -> Result<crate::application::services::fiscal_integrity_service::FiscalIntegrityReport, String> {
    run_fiscal_integrity_scan_impl(&state).map_err(into_command_error)
}

/// Command-boundary implementation of `run_fiscal_integrity_scan`.
///
/// SEC-005-A: the integrity-scan command performs a DB write
/// (`integrity_attempts`) and MUST be gated at the command boundary —
/// `ViewSystemHealth` (the same authorization used by `verify_integrity` and
/// `get_advanced_diagnostics_bundle`). Extracted for command-boundary testing.
pub fn run_fiscal_integrity_scan_impl(
    state: &AppState,
) -> Result<crate::application::services::fiscal_integrity_service::FiscalIntegrityReport, AppError> {
    let (_session, _settings) = authorize_command(state, Action::ViewSystemHealth, None)?;
    state.touch_session();

    let mut guard = state.get_db()?;
    let db = db_mut_or_app_error(guard.as_mut())?;

    db.with_transaction(|tx| {
        let report =
            crate::application::services::fiscal_integrity_service::FiscalIntegrityService::new(tx)
                .run_full_integrity_scan()?;

        // Append-only attempt log (feeds Anomaly E).
        let recorder = crate::application::services::IntegrityAttemptRecorder::new(tx);
        if report.ok {
            recorder.record(
                crate::application::services::VerificationType::FiscalDrift,
                crate::application::services::VerificationOutcome::Pass,
                None,
            );
        } else {
            let details = serde_json::to_string(&report.warnings).unwrap_or_default();
            recorder.record(
                crate::application::services::VerificationType::FiscalDrift,
                crate::application::services::VerificationOutcome::Fail,
                Some(&details),
            );
        }
        Ok(report)
    })
}

// ── Fiscal Closure Package commands ──────────────────────────────────────────

/// [Wilaya only] Export a signed fiscal closure authorization package.
///
/// Must be called **after** a successful `close_fiscal_year`.
/// The package file is written to `file_path` (chosen by operator via file dialog).
#[tauri::command]
pub fn export_fiscal_closure_package(
    state: State<AppState>,
    closed_year: i32,
    opened_year: i32,
    closure_timestamp_utc: String,
    file_path: String,
    transition_id: Option<String>,
) -> Result<String, String> {
    use crate::application::services::FiscalClosurePackageService;

    let (session, settings) = authorize_command(
        &state,
        crate::application::authz::Action::CloseFiscalYearAuthority,
        None,
    )
    .map_err(into_command_error)?;
    state.touch_session();

    // Wilaya-only guard
    if settings.node_type != crate::models::NodeType::Wilaya {
        return Err(into_command_error(crate::errors::AppError::Authorization(
            crate::errors::AuthorizationError::RequiresWilayaNode,
        )));
    }

    let node_id = settings.unit_name.as_deref().unwrap_or("WILAYA");

    let pkg = FiscalClosurePackageService::build_closure_package(
        node_id,
        &session.username,
        closed_year,
        opened_year,
        &closure_timestamp_utc,
        transition_id,
    )
    .map_err(into_command_error)?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = guard.as_mut().ok_or_else(|| "DB unavailable".to_string())?;

    let executor = db.executor();
    FiscalClosurePackageService::new(executor)
        .export_to_file(&pkg, &file_path)
        .map_err(into_command_error)?;

    // Audit the export
    let _ = crate::application::services::AuditService::new(executor).log_success(
        &session.user_id,
        &session.username,
        crate::domain::audit::AuditAction::FiscalClosurePackageExported,
        crate::domain::audit::EntityType::Financial,
        Some(&closed_year.to_string()),
        Some(&format!(
            "Fiscal closure package exported (year {})",
            closed_year
        )),
        None,
        Some(serde_json::json!({
            "fiscal_transition_id": pkg.fiscal_transition_id,
            "closed_year": closed_year,
            "opened_year": opened_year,
            "file_path": file_path,
        })),
        None,
        None,
    );

    log::info!(
        target: "grpc::fiscal",
        "[FISCAL_CLOSURE_PACKAGE_EXPORTED] transition_id={} closed_year={} opened_year={} user={}",
        pkg.fiscal_transition_id, closed_year, opened_year, session.username,
    );

    Ok(pkg.fiscal_transition_id)
}

/// [Unit only] Preview a fiscal closure package — no state mutation.
#[tauri::command]
pub fn preview_fiscal_closure_package(
    state: State<AppState>,
    file_path: String,
) -> Result<crate::application::services::FiscalClosurePreview, String> {
    use crate::application::services::FiscalClosurePackageService;

    let (_session, settings) = authorize_command(
        &state,
        crate::application::authz::Action::ApplyFiscalTransition,
        None,
    )
    .map_err(into_command_error)?;
    state.touch_session();

    // Unit-only guard
    if settings.node_type != crate::models::NodeType::Unit {
        return Err(into_command_error(crate::errors::AppError::Authorization(
            crate::errors::AuthorizationError::RequiresUnitNode,
        )));
    }

    let guard = state.get_db().map_err(into_command_error)?;
    let db = guard.as_ref().ok_or_else(|| "DB unavailable".to_string())?;

    FiscalClosurePackageService::new(db.executor())
        .preview_closure_package(&file_path)
        .map_err(into_command_error)
}

/// [Unit only] Apply a fiscal closure package — atomic, fail-closed.
///
/// Operator must confirm with "APPLY-FISCAL-CLOSURE" typed in the UI.
#[tauri::command]
pub fn apply_fiscal_closure_package(
    state: State<AppState>,
    file_path: String,
    confirmation: String,
) -> Result<crate::application::services::FiscalClosureApplyResult, String> {
    use crate::application::services::{FiscalClosurePackageService, MaintenanceBlockedOperation};
    use crate::commands::guards::require_maintenance_allows;

    let (session, settings) = authorize_command(
        &state,
        crate::application::authz::Action::ApplyFiscalTransition,
        None,
    )
    .map_err(into_command_error)?;
    state.touch_session();

    // Unit-only guard
    if settings.node_type != crate::models::NodeType::Unit {
        return Err(into_command_error(crate::errors::AppError::Authorization(
            crate::errors::AuthorizationError::RequiresUnitNode,
        )));
    }

    // Operator typed confirmation
    if confirmation != "APPLY-FISCAL-TRANSITION" {
        return Err(into_command_error(crate::errors::AppError::BusinessLogic(
            crate::errors::BusinessLogicError::OperationNotPermitted {
                message: "رمز تأكيد غير صالح. اكتب 'APPLY-FISCAL-TRANSITION' للمتابعة.".to_string(),
            },
        )));
    }

    // F: maintenance mode — checked at command layer before entering transaction
    require_maintenance_allows(&state, MaintenanceBlockedOperation::FiscalClose)
        .map_err(into_command_error)?;

    let user_id = session.user_id.clone();
    let username = session.username.clone();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let (result, _event_buffer) = db
        .with_event_persistence(|ctx| {
            let executor = ctx.executor();
            let res = FiscalClosurePackageService::new(executor)
                .apply_closure_package(&file_path, &user_id, &username)?;

            ctx.emit(DomainEvent::FiscalTransitionApplied {
                from_year: res.closed_year,
                to_year: res.opened_year,
            });

            Ok(res)
        })
        .map_err(|e| {
            log::error!(
                target: "grpc::fiscal",
                "[FISCAL_CLOSURE_PACKAGE_REJECTED] file_path={} user={} err={:?}",
                file_path, username, e
            );
            into_command_error(e)
        })?;

    log::info!(
        target: "grpc::fiscal",
        "[FISCAL_CLOSURE_APPLIED] transition_id={} closed_year={} opened_year={} applied_by={}",
        result.fiscal_transition_id, result.closed_year, result.opened_year, username,
    );

    Ok(result)
}

/// Get the timeline of fiscal transitions.
#[tauri::command]
pub fn get_fiscal_transition_history(
    state: State<AppState>,
) -> Result<Vec<crate::application::services::FiscalTransitionHistoryEntry>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::AuthenticatedOnly, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = guard.as_ref().ok_or_else(|| "DB unavailable".to_string())?;

    crate::application::services::FiscalTransitionHistoryService::new(db.executor())
        .build_transition_history()
        .map_err(into_command_error)
}

/// List all entries in the fiscal package registry.
#[tauri::command]
pub fn list_fiscal_package_registry(
    state: State<AppState>,
) -> Result<Vec<crate::repositories::FiscalPackageRegistryEntry>, String> {
    use crate::application::services::FiscalClosurePackageService;
    let (_session, _settings) =
        authorize_command(&state, Action::AuthenticatedOnly, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = guard.as_ref().ok_or_else(|| "DB unavailable".to_string())?;

    FiscalClosurePackageService::new(db.executor())
        .list_package_registry()
        .map_err(into_command_error)
}

/// Update the retention status of a fiscal package.
#[tauri::command]
pub fn update_fiscal_package_retention_status(
    state: State<AppState>,
    transition_id: String,
    status: String,
    confirmation: String,
) -> Result<(), String> {
    use crate::application::services::FiscalClosurePackageService;
    let (_session, _settings) = authorize_command(
        &state,
        crate::application::authz::Action::CloseFiscalYearAuthority, // Wilaya Admin only
        None,
    )
    .map_err(into_command_error)?;
    state.touch_session();

    // Typed confirmation requirement
    let expected = format!("{}-PACKAGE", status.to_uppercase());
    if confirmation != expected {
        return Err(format!("تأكيد غير صالح. اكتب '{}' للمتابعة.", expected));
    }

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    db.with_transaction(|tx| {
        FiscalClosurePackageService::new(tx).update_retention_status(
            &transition_id,
            &status,
            &_session.user_id,
            &_session.username,
        )
    })
    .map_err(into_command_error)?;

    log::info!(
        target: "grpc::fiscal",
        "[FISCAL_PACKAGE_RETENTION_UPDATED] transition_id={} status={}",
        transition_id, status
    );

    Ok(())
}
