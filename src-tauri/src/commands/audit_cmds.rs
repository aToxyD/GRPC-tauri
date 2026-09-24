//! Audit Trail Commands
//!
//! Audit log queries and management
//! Strictly follows Clean Architecture: Commands -> Services -> Repositories -> DB

use crate::application::authz::Action;

use crate::application::services::AuditService;
use crate::commands::common::{db_mut_or_command_error, db_ref_or_command_error};
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::domain::audit::{AuditEntry, AuditFilters, AuditLogResponse, AuditStats};
use crate::domain::validation;
use crate::errors::into_command_error;
use crate::models::XlsxExportResult;
use tauri::State;

/// Get audit log with filtering
#[tauri::command]
pub fn get_audit_log(
    state: State<AppState>,
    filters: AuditFilters,
    page: usize,
    page_size: usize,
) -> Result<AuditLogResponse, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadAuditLog, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    AuditService::new(db.executor())
        .get_audit_entries(&filters, page, page_size)
        .map_err(into_command_error)
}

/// Get audit statistics
#[tauri::command]
pub fn get_audit_stats(
    state: State<AppState>,
    start_date: String,
    end_date: String,
) -> Result<AuditStats, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadAuditLog, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    AuditService::new(db.executor())
        .get_audit_statistics(&start_date, &end_date)
        .map_err(into_command_error)
}

/// Get user activity
#[tauri::command]
pub fn get_user_activity(
    state: State<AppState>,
    user_id: String,
    days: i32,
) -> Result<Vec<AuditEntry>, String> {
    let (_session, _settings) = authorize_command(&state, Action::ReadUserActivity, Some(&user_id))
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    AuditService::new(db.executor())
        .get_user_audit_activity(&user_id, days)
        .map_err(into_command_error)
}

/// Verify the integrity of the audit chain
#[tauri::command]
pub fn verify_audit_chain(state: State<AppState>) -> Result<(), String> {
    let (_session, _settings) =
        authorize_command(&state, Action::AdminOnly, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    AuditService::new(db.executor())
        .verify_audit_hash_chain()
        .map_err(into_command_error)
}

/// Export audit log to Excel
#[tauri::command]
pub fn export_audit_log_excel(
    state: State<AppState>,
    filters: AuditFilters,
    file_path: String,
) -> Result<XlsxExportResult, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadAuditLog, None).map_err(into_command_error)?;
    validation::validate_file_path(&file_path, &["xlsx"]).map_err(into_command_error)?;

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    state.touch_session();
    let count = AuditService::new(db.executor())
        .export_audit_to_excel(&filters, &file_path)
        .map_err(into_command_error)?;

    Ok(XlsxExportResult::success(file_path, count))
}

/// Clean up old audit logs
#[tauri::command]
pub fn cleanup_audit_logs(
    state: State<AppState>,
    before_date: Option<String>,
) -> Result<u64, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::AdminOnly, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    AuditService::new(db.executor())
        .cleanup_old_audit_logs(before_date)
        .map_err(into_command_error)
}
