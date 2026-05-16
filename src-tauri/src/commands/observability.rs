//! Observability Commands
//!
//! Tauri commands for audit integrity, sync conflict management, and system health.
//! All business logic lives in application/services — commands are thin adapters only.

use crate::application::authz::Action;
use crate::application::services::MaintenanceBlockedOperation;
use crate::application::services::{
    AuditObservabilityService, SyncConflictService, SystemHealthService,
};
use crate::commands::common::{db_mut_or_command_error, db_ref_or_command_error};
use crate::commands::guards::{authorize_command, require_maintenance_allows};
use crate::commands::types::AppState;
use crate::errors::{into_command_error, AppError};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Clone, Serialize)]
pub struct BuildInfo {
    pub app_version: &'static str,
    pub git_commit: &'static str,
    pub build_timestamp: &'static str,
    pub schema_version: u16,
}

// ─── Audit Integrity ─────────────────────────────────────────────────────────

/// Get detailed audit chain status (chain verification report).
#[tauri::command]
pub fn get_audit_chain_status(
    state: State<AppState>,
) -> Result<crate::application::services::audit_observability_service::AuditChainStatus, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::AdminOnly, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    AuditObservabilityService::new(db.executor())
        .get_chain_status()
        .map_err(into_command_error)
}

/// Get the full audit health report including anomalies.
#[tauri::command]
pub fn get_audit_health(
    state: State<AppState>,
) -> Result<crate::application::services::audit_observability_service::AuditHealthReport, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::AdminOnly, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    AuditObservabilityService::new(db.executor())
        .get_health_report()
        .map_err(into_command_error)
}

// ─── System Health ────────────────────────────────────────────────────────────

/// Get the full system health report.
#[tauri::command]
pub fn get_system_health(
    state: State<AppState>,
) -> Result<crate::application::services::system_health_service::SystemHealthReport, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    SystemHealthService::get_health_report(db).map_err(into_command_error)
}

/// Get per-node sync health.
#[tauri::command]
pub fn get_sync_health(
    state: State<AppState>,
) -> Result<Vec<crate::application::services::system_health_service::SyncNodeHealth>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    SystemHealthService::get_sync_node_health(db).map_err(into_command_error)
}

// ─── Sync Conflict Management ────────────────────────────────────────────────

/// Get summary of sync conflicts.
#[tauri::command]
pub fn get_conflict_summary(
    state: State<AppState>,
) -> Result<crate::application::services::sync_conflict_service::ConflictSummary, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    SyncConflictService::new(db.executor())
        .get_conflict_summary()
        .map_err(into_command_error)
}

/// List sync conflicts (optionally only unresolved).
#[tauri::command]
pub fn list_sync_conflicts(
    state: State<AppState>,
    unresolved_only: bool,
) -> Result<Vec<crate::application::services::sync_conflict_service::SyncConflict>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    SyncConflictService::new(db.executor())
        .list_conflicts(unresolved_only)
        .map_err(into_command_error)
}

/// Resolve a sync conflict (admin-only, creates audit trail).
#[tauri::command]
pub fn resolve_sync_conflict(
    state: State<AppState>,
    conflict_id: String,
    note: String,
) -> Result<(), String> {
    let (session, _settings) =
        authorize_command(&state, Action::ManageSyncConflicts, None).map_err(into_command_error)?;
    require_maintenance_allows(&state, MaintenanceBlockedOperation::Sync)
        .map_err(into_command_error)?;
    state.touch_session();

    if conflict_id.trim().is_empty() {
        return Err(into_command_error(AppError::Internal(
            "conflict_id cannot be empty".to_string(),
        )));
    }
    if note.trim().is_empty() {
        return Err(into_command_error(AppError::Internal(
            "resolution note cannot be empty".to_string(),
        )));
    }

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let conflict_id_clone = conflict_id.clone();
    let note_clone = note.clone();
    let user_id = session.user_id.clone();
    let username = session.username.clone();
    let session_id = session.session_id.clone();

    // Resolve in transaction with audit trail
    db.with_transaction(|executor| {
        // Resolve the conflict
        SyncConflictService::new(executor).resolve_conflict(
            &conflict_id_clone,
            &username,
            &note_clone,
        )?;

        // Write audit entry
        crate::application::services::AuditService::new(executor).log_success(
            &user_id,
            &username,
            crate::domain::audit::AuditAction::ResolveConflict,
            crate::domain::audit::EntityType::System,
            Some(&conflict_id_clone),
            Some("SyncConflictResolved"),
            None,
            Some(serde_json::json!({ "note": note_clone, "conflict_id": conflict_id_clone })),
            Some(session_id.as_str()),
            Some(serde_json::json!({ "action": "RESOLVE_CONFLICT" })),
        )
    })
    .map_err(into_command_error)
}

// ─── Build Information ────────────────────────────────────────────────────────

/// Get runtime build and version information.
#[tauri::command]
pub fn get_build_info() -> BuildInfo {
    BuildInfo {
        app_version: env!("APP_VERSION"),
        git_commit: env!("APP_GIT_COMMIT"),
        build_timestamp: env!("BUILD_TIMESTAMP"),
        schema_version: crate::application::sync::SYNC_PACKAGE_SCHEMA_VERSION.as_u16(),
    }
}

/// Get recent telemetry events.
#[tauri::command]
pub fn get_recent_telemetry(
    state: State<AppState>,
    limit: u32,
) -> Result<Vec<crate::application::services::TelemetryEvent>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    crate::application::services::TelemetryService::new(db.executor())
        .get_recent_events(limit)
        .map_err(into_command_error)
}
