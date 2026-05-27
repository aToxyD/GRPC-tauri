//! Daily Reports and Calculation Commands
//!
//! Daily consumption reports and meal rate calculations
//! Strictly follows Clean Architecture: Commands -> Services -> Repositories -> DB

use crate::application::authz::{Action, ResourceContext};
use crate::application::services::MaintenanceBlockedOperation;
use crate::application::usecases::reports::types::{DailyReportFilters, ReportScope, UnitId};
use crate::commands::common::{
    db_mut_or_command_error, db_ref_or_command_error, user_ctx_from_session,
};
use crate::commands::guards::{authorize_command, require_maintenance_allows};
use crate::commands::types::AppState;
use crate::domain::audit::AuditAction;
use crate::errors::{into_command_error, AppError};
use crate::models::{
    DailyConsumptionView, DailyFifoConsumptionPreview, DailyReport, DailyReportInput,
    DailyReportResult, MonthlySummary, WilayaReportList,
};

use crate::application::services::{
    AuditTxService, DailyReportService, FifoPreviewService, ReportCalculationService,
    SettingsService,
};
use chrono::NaiveDate;
use tauri::State;

pub(crate) fn build_report_scope(
    settings_svc: &SettingsService<'_>,
) -> Result<(ReportScope, ResourceContext), AppError> {
    let settings = settings_svc.get_settings()?;
    match settings.node_type {
        crate::models::NodeType::Wilaya => Ok((ReportScope::Global, ResourceContext::WilayaNode)),
        crate::models::NodeType::Unit => {
            let unit_id = settings_svc.get_current_unit_id()?.ok_or_else(|| {
                AppError::Authorization(crate::errors::AuthorizationError::UnitScopeMismatch)
            })?;
            Ok((
                ReportScope::Unit(UnitId::new(unit_id.clone())),
                ResourceContext::UnitNode { unit_id },
            ))
        }
    }
}

/// Calculate meal cost for a given day
#[tauri::command]
pub fn calculate_meal_cost(items: Vec<(f64, f64)>) -> Result<f64, String> {
    Ok(ReportCalculationService::calculate_meal_cost(items))
}

/// Calculate meal rate from total cost and beneficiary counts
#[tauri::command]
pub fn calculate_meal_rate(
    total_cost: f64,
    staff_24h_count: i32,
    staff_8h_count: i32,
    reservation_count: i32,
    mission_count: i32,
    guest_count: i32,
) -> Result<f64, String> {
    Ok(ReportCalculationService::calculate_meal_rate(
        total_cost,
        staff_24h_count,
        staff_8h_count,
        reservation_count,
        mission_count,
        guest_count,
    ))
}

/// Calculate product price with TVA
#[tauri::command]
pub fn calculate_product_price_with_tva(base_price: f64, tva: f64) -> Result<f64, String> {
    Ok(ReportCalculationService::calculate_product_price_with_tva(
        base_price, tva,
    ))
}

/// Get monthly summary (or full fiscal-year aggregate when month is None).
///
/// When `month` is `None`, the summary covers the entire fiscal year (Jan 1 – Dec 31).
#[tauri::command]
pub fn get_monthly_summary(
    state: State<AppState>,
    year: i32,
    month: Option<i32>,
) -> Result<MonthlySummary, String> {
    let (_session, _) =
        authorize_command(&state, Action::ReadMonthlySummary, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    let executor = db.executor();
    let settings_svc = SettingsService::new(executor);

    // Get effective unit ID
    let effective_unit_id = settings_svc
        .get_current_unit_id()
        .map_err(into_command_error)?;

    crate::application::usecases::reports::get_monthly_summary::execute(
        executor,
        year,
        month,
        effective_unit_id.as_deref(),
    )
    .map_err(into_command_error)
}

/// FIFO dry-run preview for daily consumption (same engine as execution).
#[tauri::command]
pub fn preview_daily_consumption_fifo(
    state: State<AppState>,
    input: DailyReportInput,
) -> Result<DailyFifoConsumptionPreview, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadDailyReports, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    let executor = db.executor();
    let settings_svc = SettingsService::new(executor);
    let unit_id = settings_svc
        .get_current_unit_id()
        .map_err(into_command_error)?;

    let unit_id = unit_id.ok_or_else(|| {
        into_command_error(AppError::BusinessLogic(
            crate::errors::BusinessLogicError::OperationNotPermitted {
                message: "unit_id is required for FIFO preview".to_string(),
            },
        ))
    })?;

    FifoPreviewService::new(executor)
        .preview_daily_report(&input, &unit_id)
        .map_err(into_command_error)
}

/// Create daily report with atomic stock update
#[tauri::command]
pub fn create_daily_report(
    state: State<AppState>,
    input: DailyReportInput,
    _unit_id: Option<String>,
) -> Result<DailyReportResult, String> {
    let (session, _) =
        authorize_command(&state, Action::ManageDailyReports, None).map_err(into_command_error)?;
    require_maintenance_allows(&state, MaintenanceBlockedOperation::ReportWrite)
        .map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let executor = db.executor();
    let settings_svc = SettingsService::new(executor);

    // Get effective unit ID
    let effective_unit_id = settings_svc
        .get_current_unit_id()
        .map_err(into_command_error)?;

    let user_ctx = user_ctx_from_session(&session);

    let report_id =
        AuditTxService::execute_with_audit(db, AuditAction::CreateDailyReport, &user_ctx, |tx| {
            DailyReportService::new(tx.executor).create_daily_report(
                &input,
                effective_unit_id.as_deref(),
                &session.user_id,
                &session.username,
            )
        })
        .map_err(into_command_error)?;

    DailyReportService::new(db.executor())
        .load_daily_report_result(&report_id)
        .map_err(into_command_error)?
        .ok_or_else(|| {
            into_command_error(AppError::Internal(
                "Report not found after creation".to_string(),
            ))
        })
}

/// Get daily report by ID
#[tauri::command]
pub fn get_daily_report(
    state: State<AppState>,
    report_id: String,
) -> Result<DailyReportResult, String> {
    let (_session, _) =
        authorize_command(&state, Action::ReadDailyReports, None).map_err(into_command_error)?;

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    let executor = db.executor();
    let settings_svc = SettingsService::new(executor);
    let (scope, _) = build_report_scope(&settings_svc).map_err(into_command_error)?;

    crate::application::usecases::reports::get_daily_report::execute(executor, scope, &report_id)
        .map_err(into_command_error)
}

/// List daily reports, optionally filtered by fiscal_year and month.
///
/// When `fiscal_year` or `month` are supplied, they take priority over
/// `start_date`/`end_date` (the date range is derived from the fiscal year).
#[tauri::command]
pub fn list_daily_reports(
    state: State<AppState>,
    start_date: Option<String>,
    end_date: Option<String>,
    fiscal_year: Option<i32>,
    month: Option<u32>,
) -> Result<Vec<DailyReport>, String> {
    let (_session, _) =
        authorize_command(&state, Action::ReadDailyReports, None).map_err(into_command_error)?;

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    let executor = db.executor();
    let settings_svc = SettingsService::new(executor);
    let (scope, _) = build_report_scope(&settings_svc).map_err(into_command_error)?;

    let start = start_date
        .map(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| {
            into_command_error(AppError::DateParse(format!(
                "Invalid start_date format: {}",
                e
            )))
        })?;
    let end = end_date
        .map(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| {
            into_command_error(AppError::DateParse(format!(
                "Invalid end_date format: {}",
                e
            )))
        })?;
    let filters = DailyReportFilters {
        start_date: start,
        end_date: end,
        fiscal_year,
        month,
    };
    crate::application::usecases::reports::list_daily_reports::execute(executor, scope, filters)
        .map_err(into_command_error)
}

/// Get daily consumption view for a date (meals + daily summary)
#[tauri::command]
pub fn get_daily_consumption(
    state: State<AppState>,
    date: String,
) -> Result<Option<DailyConsumptionView>, String> {
    let (_session, _) =
        authorize_command(&state, Action::ReadDailyReports, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    let executor = db.executor();
    let settings_svc = SettingsService::new(executor);

    // Get effective unit ID
    let effective_unit_id = settings_svc
        .get_current_unit_id()
        .map_err(into_command_error)?;

    DailyReportService::new(executor)
        .get_daily_consumption_view(&date, effective_unit_id.as_deref())
        .map_err(into_command_error)
}

/// List wilaya reports with filtering
#[tauri::command]
pub fn list_wilaya_reports(
    state: State<AppState>,
    unit_id: Option<String>,
    report_type: String,
    year: Option<i32>,
    month: Option<i32>,
) -> Result<WilayaReportList, String> {
    let (_session, _) =
        authorize_command(&state, Action::ReadWilayaReports, None).map_err(into_command_error)?;

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    // Build authz principal + context (WILAYA-only)

    crate::application::services::ReportCalculationService::new(db.executor())
        .build_wilaya_reports(unit_id, report_type, year, month)
        .map_err(into_command_error)
}

/// Get report data (legacy alias)
#[tauri::command]
pub fn get_report_data(
    state: State<AppState>,
    _report_type: String,
    month: i32,
    year: i32,
) -> Result<MonthlySummary, String> {
    get_monthly_summary(state, year, Some(month))
}

/// List all distinct fiscal years available across daily_reports and
/// fiscal_year_status, sorted descending.
#[tauri::command]
pub fn list_fiscal_years(state: State<AppState>) -> Result<Vec<i32>, String> {
    let (_session, _) =
        authorize_command(&state, Action::ReadDailyReports, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    DailyReportService::new(db.executor())
        .list_available_fiscal_years()
        .map_err(into_command_error)
}

/// Generate reports (no-op stub)
#[tauri::command]
pub fn generate_reports(
    _state: State<AppState>,
    _report_type: String,
    _month: i32,
    _year: i32,
) -> Result<(), String> {
    Err("هذه الميزة غير متوفرة بعد".to_string())
}

/// Record consumption (alias for create_daily_report)
#[tauri::command]
pub fn record_consumption(
    state: State<AppState>,
    consumption: DailyReportInput,
    unit_id: Option<String>,
) -> Result<DailyReportResult, String> {
    create_daily_report(state, consumption, unit_id)
}
