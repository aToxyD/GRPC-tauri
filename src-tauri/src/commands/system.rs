//! System Diagnostic Commands
//! Operational tools for database health and sync forensics.

use crate::application::authz::Action;
use crate::application::services::{SystemDiagnosticsService, SystemStatsService};
use crate::commands::common::db_ref_or_command_error;
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::errors::into_command_error;
use crate::models::{
    ImportAuditEvent, LoginMetrics, SyncPreflightCheck, SyncSecurityDiagnostics, SystemMetrics,
};
use tauri::State;

/// Check database integrity using PRAGMA integrity_check
#[tauri::command]
pub fn check_database_integrity(state: State<AppState>) -> Result<Vec<String>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::AdminOnly, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    SystemDiagnosticsService::new(db.executor())
        .check_database_integrity()
        .map_err(into_command_error)
}

/// Get recent import audit events for diagnostics
#[tauri::command]
pub fn get_import_diagnostics(
    state: State<AppState>,
    limit: usize,
) -> Result<Vec<ImportAuditEvent>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::AdminOnly, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    SystemDiagnosticsService::new(db.executor())
        .get_import_diagnostics(limit)
        .map_err(into_command_error)
}

/// Get login metrics (failed attempts, etc.)
#[tauri::command]
pub fn get_login_metrics(state: State<AppState>) -> Result<LoginMetrics, String> {
    let rate_limiter = state.rate_limiter.lock().map_err(|e| e.to_string())?;
    Ok(SystemStatsService::get_login_metrics(&rate_limiter))
}

/// Get system metrics (counts of products, reports, etc.)
#[tauri::command]
pub fn get_system_metrics(state: State<AppState>) -> Result<SystemMetrics, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadSystemMetrics, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    SystemStatsService::get_system_metrics(db, state.process_start_time).map_err(into_command_error)
}

/// Get sync security diagnostics (key status, env, etc.)
#[tauri::command]
pub fn get_sync_security_diagnostics(
    state: State<AppState>,
) -> Result<SyncSecurityDiagnostics, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::AdminOnly, None).map_err(into_command_error)?;
    state.touch_session();

    crate::infrastructure::security::get_sync_security_diagnostics().map_err(into_command_error)
}

/// Sync preflight check (validate local state for sync)
#[tauri::command]
pub fn sync_preflight_check(state: State<AppState>) -> Result<SyncPreflightCheck, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::AdminOnly, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    crate::application::services::SettingsService::new(db.executor())
        .get_sync_preflight_check()
        .map_err(into_command_error)
}
