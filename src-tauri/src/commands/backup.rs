//! Backup Management Commands
//!
//! Database backup and restore operations

use crate::application::authz::Action;
use crate::application::services::{
    BackupIntegrityService, CriticalOperation, FiscalHistoricalGuard, GuardedOperation,
    OperationExecutionGuard, OperatorSafetyService, SystemMaintenanceState,
};
use crate::commands::common::{db_mut_or_command_error, db_ref_or_command_error};
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::domain::audit::AuditAction;
use crate::domain::ports::backup::{BackupInfo, BackupPort};
use crate::errors::{into_command_error, AppError};
use crate::infrastructure::backup::SqliteBackupAdapter;
use std::path::PathBuf;
use tauri::{Manager, State};

#[derive(serde::Serialize)]
pub struct RestoreResult {
    pub success: bool,
}

/// Immediate result of WAL checkpoint, captured inside a single-guard scope.
/// No audit or telemetry writes happen inside the guard scope to avoid deadlock.
enum CheckpointOutcome {
    Success {
        db_path: PathBuf,
        wal_frames_before: i32,
        checkpointed_frames: i32,
        wal_frames_after: i32,
    },
    Failed {
        error_msg: String,
        log_line: String,
    },
    Incomplete {
        error_msg: String,
        log_line: String,
    },
}

/// Create immediate backup - AUDITED SYSTEM OPERATION
#[tauri::command]
pub async fn create_backup(state: State<'_, AppState>) -> Result<String, String> {
    let (session_user_id, session_username, session_id) = {
        let (session, _) =
            authorize_command(&state, Action::ManageBackups, None).map_err(into_command_error)?;
        (
            session.user_id.clone(),
            session.username.clone(),
            session.session_id.clone(),
        )
    };
    state.touch_session();

    // SAFETY:
    // std::sync::Mutex is non-reentrant.
    // The DB guard acquired below MUST be dropped before any subsequent
    // state.get_db() call on the same thread.
    // Audit and telemetry writes are deferred until after the guard scope
    // has ended.
    let checkpoint_outcome = {
        let mut guard = state.get_db().map_err(into_command_error)?;
        let db = db_mut_or_command_error(guard.as_mut())?;

        // FAIL-CLOSED: Explicit WAL checkpoint verification
        // Backup MUST NOT proceed if checkpoint fails
        let conn = db.get_connection();

        // Execute WAL checkpoint and verify success
        let checkpoint_result: rusqlite::Result<(i32, i32, i32)> =
            conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((
                    row.get(0)?, // wal_frames in log before checkpoint
                    row.get(1)?, // frames checkpointed
                    row.get(2)?, // wal_frames in log after checkpoint
                ))
            });

        match checkpoint_result {
            Ok((wal_frames_before, checkpointed_frames, wal_frames_after)) => {
                if wal_frames_after > 0 {
                    let error_msg = format!(
                        "WAL checkpoint incomplete: {} frames remain after checkpoint (checkpointed {}/{})",
                        wal_frames_after, checkpointed_frames, wal_frames_before
                    );
                    let log_line = format!(
                        "[CHECKPOINT_INCOMPLETE] db_path={} wal_frames_before={} checkpointed_frames={} wal_frames_after={} result_state=failed",
                        db.get_connection_path().unwrap_or_else(|_| PathBuf::from("unknown")).display(),
                        wal_frames_before,
                        checkpointed_frames,
                        wal_frames_after
                    );
                    CheckpointOutcome::Incomplete {
                        error_msg,
                        log_line,
                    }
                } else {
                    CheckpointOutcome::Success {
                        db_path: db
                            .get_connection_path()
                            .map_err(|e| into_command_error(AppError::Internal(e.to_string())))?,
                        wal_frames_before,
                        checkpointed_frames,
                        wal_frames_after,
                    }
                }
            }
            Err(e) => {
                let error_msg = format!("WAL checkpoint execution failed: {}", e);
                let log_line = format!(
                    "[CHECKPOINT_FAILED] db_path={} error={}",
                    db.get_connection_path()
                        .unwrap_or_else(|_| PathBuf::from("unknown"))
                        .display(),
                    error_msg
                );
                CheckpointOutcome::Failed {
                    error_msg,
                    log_line,
                }
            }
        }
        // DB guard drops here — no state.get_db() is alive at this point.
    };

    // Handle checkpoint outcome outside the guard scope.
    let db_path = match checkpoint_outcome {
        CheckpointOutcome::Success {
            db_path,
            wal_frames_before,
            checkpointed_frames,
            wal_frames_after,
        } => {
            log::info!(
                target: "grpc::backup",
                "[CHECKPOINT_SUCCESS] db_path={} wal_frames_before={} checkpointed_frames={} wal_frames_after={} result_state=success",
                db_path.display(),
                wal_frames_before,
                checkpointed_frames,
                wal_frames_after
            );
            let _ = write_checkpoint_telemetry(
                &state,
                &session_user_id,
                wal_frames_before,
                checkpointed_frames,
                wal_frames_after,
            );
            db_path
        }
        CheckpointOutcome::Failed {
            error_msg,
            log_line,
        } => {
            log::error!(target: "grpc::backup", "{}", log_line);
            let _ = write_checkpoint_audit(
                &state,
                &session_user_id,
                &session_username,
                &error_msg,
                &session_id,
            );
            return Err(into_command_error(AppError::Internal(error_msg)));
        }
        CheckpointOutcome::Incomplete {
            error_msg,
            log_line,
        } => {
            log::error!(target: "grpc::backup", "{}", log_line);
            let _ = write_checkpoint_audit(
                &state,
                &session_user_id,
                &session_username,
                &error_msg,
                &session_id,
            );
            return Err(into_command_error(AppError::Internal(error_msg)));
        }
    };

    let backup_manager = SqliteBackupAdapter::new(&db_path, state.crypto_port);

    log::info!(
        target: "grpc::backup",
        "create_backup: starting for db_path={}",
        db_path.display()
    );

    let start_time = std::time::Instant::now();
    let path_str_res = tauri::async_runtime::spawn_blocking(move || backup_manager.create_backup())
        .await
        .map_err(|e| into_command_error(AppError::Internal(format!("Task failed: {}", e))))?
        .map_err(|e| {
            into_command_error(AppError::Internal(format!(
                "فشل إنشاء النسخة الاحتياطية: {}",
                e
            )))
        });

    let duration = start_time.elapsed().as_millis() as i64;
    let path_str = match path_str_res {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            let _ = write_backup_telemetry(&state, &session_user_id, duration, false, Some(&e));
            return Err(e);
        }
    };

    log::info!(
        target: "grpc::backup",
        "create_backup: success path={}",
        path_str
    );

    let _ = write_backup_telemetry(&state, &session_user_id, duration, true, None);
    let _ = write_backup_audit(
        &state,
        &session_user_id,
        &session_username,
        &path_str,
        &session_id,
    );

    Ok(path_str)
}

/// Write WAL checkpoint telemetry — safe to call without a live DB guard.
fn write_checkpoint_telemetry(
    state: &AppState,
    session_user_id: &str,
    wal_frames_before: i32,
    checkpointed_frames: i32,
    wal_frames_after: i32,
) -> Result<(), ()> {
    let guard = state.get_db().map_err(|_| ())?;
    let db = db_ref_or_command_error(guard.as_ref()).map_err(|_| ())?;
    let _ = crate::application::services::TelemetryService::new(db.executor()).record_event(
        crate::application::services::TelemetryEventType::WalCheckpoint,
        crate::application::services::TelemetryOutcome::Success,
        None,
        Some(serde_json::json!({
            "wal_frames_before": wal_frames_before,
            "checkpointed_frames": checkpointed_frames,
            "wal_frames_after": wal_frames_after
        })),
        Some(session_user_id),
    );
    Ok(())
}

/// Write checkpoint audit failure — safe to call without a live DB guard.
fn write_checkpoint_audit(
    state: &AppState,
    session_user_id: &str,
    session_username: &str,
    error_msg: &str,
    session_id: &str,
) -> Result<(), ()> {
    let guard = state.get_db().map_err(|_| ())?;
    let db = db_ref_or_command_error(guard.as_ref()).map_err(|_| ())?;
    let _ = crate::application::services::AuditService::new(db.executor()).log_failure(
        session_user_id,
        session_username,
        AuditAction::BackupCheckpointFailed,
        crate::domain::audit::EntityType::System,
        None,
        error_msg,
        Some(session_id),
    );
    Ok(())
}

/// Write backup telemetry — safe to call without a live DB guard.
fn write_backup_telemetry(
    state: &AppState,
    session_user_id: &str,
    duration: i64,
    success: bool,
    error: Option<&str>,
) -> Result<(), ()> {
    let guard = state.get_db().map_err(|_| ())?;
    let db = db_ref_or_command_error(guard.as_ref()).map_err(|_| ())?;
    let outcome = if success {
        crate::application::services::TelemetryOutcome::Success
    } else {
        crate::application::services::TelemetryOutcome::Failure
    };
    let details = if let Some(err) = error {
        serde_json::json!({ "error": err })
    } else {
        serde_json::json!({})
    };
    let _ = crate::application::services::TelemetryService::new(db.executor()).record_event(
        crate::application::services::TelemetryEventType::Backup,
        outcome,
        Some(duration),
        Some(details),
        Some(session_user_id),
    );
    Ok(())
}

/// Write backup audit — safe to call without a live DB guard.
fn write_backup_audit(
    state: &AppState,
    session_user_id: &str,
    session_username: &str,
    path_str: &str,
    session_id: &str,
) -> Result<(), ()> {
    let guard = state.get_db().map_err(|_| ())?;
    let db = db_ref_or_command_error(guard.as_ref()).map_err(|_| ())?;
    let _ = crate::application::services::AuditService::new(db.executor()).log_success(
        session_user_id,
        session_username,
        AuditAction::CreateBackup,
        crate::domain::audit::EntityType::System,
        Some(path_str),
        Some("Database Backup"),
        None,
        None,
        Some(session_id),
        None,
    );
    Ok(())
}

/// List available backups
#[tauri::command]
pub fn list_backups(state: State<'_, AppState>) -> Result<Vec<BackupInfo>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ManageBackups, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    let db_path = db
        .get_connection_path()
        .map_err(|e| into_command_error(AppError::Internal(e.to_string())))?;
    let backup_manager = SqliteBackupAdapter::new(&db_path, state.crypto_port);

    match backup_manager.get_backup_info() {
        Ok(backups) => Ok(backups),
        Err(e) => Err(into_command_error(AppError::Internal(format!(
            "فشل قراءة قائمة النسخ: {}",
            e
        )))),
    }
}

/// Restore database from backup - requires confirmation + execution token + integrity preflight.
#[tauri::command]
pub async fn restore_backup(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    backup_path: String,
    confirmation: String,
    execution_token: String,
) -> Result<RestoreResult, String> {
    let (session, _settings) =
        authorize_command(&state, Action::AdminOnly, None).map_err(into_command_error)?;
    let session_user_id = session.user_id.clone();
    state.touch_session();

    OperatorSafetyService::require_confirmation(CriticalOperation::RestoreBackup, &confirmation)
        .map_err(into_command_error)?;

    let restore_op = GuardedOperation::RestoreBackup;
    state
        .operation_guard
        .assert_not_throttled(restore_op)
        .map_err(into_command_error)?;

    let db_path = {
        let guard = state.get_db().map_err(into_command_error)?;
        let db = db_ref_or_command_error(guard.as_ref())?;

        OperationExecutionGuard::assert_integrity_allows_restore_or_archive(db.executor())
            .map_err(into_command_error)?;
        OperationExecutionGuard::validate_execution_token(
            db.executor(),
            restore_op,
            &execution_token,
        )
        .map_err(into_command_error)?;

        db.get_connection_path()
            .map_err(|e| into_command_error(AppError::Internal(e.to_string())))?
    };

    let backup_path_p = std::path::PathBuf::from(&backup_path);
    let backup_manager = SqliteBackupAdapter::new(&db_path, state.crypto_port);

    {
        let guard = state.get_db().map_err(into_command_error)?;
        let db = db_ref_or_command_error(guard.as_ref())?;

        FiscalHistoricalGuard::new(db.executor())
            .assert_restore_would_not_regress_archived_state(&backup_path_p, &backup_manager)
            .map_err(into_command_error)?;
    }

    let integrity = BackupIntegrityService::verify_backup_file(&backup_path_p, &backup_manager);
    match integrity {
        Ok(r) if r.valid => {}
        Ok(r) => {
            log::error!(
                target: "grpc::backup",
                "[BACKUP_INTEGRITY_FAILED] path={} failures={:?}",
                backup_path_p.display(),
                r.failures
            );
            return Err(into_command_error(AppError::BusinessLogic(
                crate::errors::BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "فشل التحقق من سلامة النسخة الاحتياطية: {}",
                        r.failures.join("; ")
                    ),
                },
            )));
        }
        Err(e) => return Err(into_command_error(e)),
    }

    log::info!(
        target: "grpc::backup",
        "restore_backup: starting from backup_path={}",
        backup_path_p.display()
    );

    state
        .maintenance
        .set(SystemMaintenanceState::RestoreInProgress)
        .map_err(into_command_error)?;

    // Close current connection
    let old_db = state.take_db().map_err(into_command_error)?;
    drop(old_db); // ensure connection is closed

    // Delete WAL and SHM files to ensure a clean start with the new database
    let wal_path = db_path.with_extension("db-wal");
    let shm_path = db_path.with_extension("db-shm");
    if wal_path.exists() {
        let _ = std::fs::remove_file(wal_path);
    }
    if shm_path.exists() {
        let _ = std::fs::remove_file(shm_path);
    }

    // Perform atomic restore (file swap) in a blocking thread
    let start_time = std::time::Instant::now();
    let restore_res = tauri::async_runtime::spawn_blocking(move || {
        backup_manager.restore_backup_atomic(&backup_path_p)
    })
    .await
    .map_err(|e| into_command_error(AppError::Internal(format!("Task failed: {}", e))))?
    .map_err(|e| {
        into_command_error(AppError::Internal(format!(
            "فشل استعادة النسخة الذرية: {}",
            e
        )))
    });
    let duration = start_time.elapsed().as_millis() as i64;

    restore_res?;

    log::info!(
        target: "grpc::backup",
        "restore_backup: atomic restore succeeded, restarting app"
    );

    // Record success in the NEW database (since restore_backup_atomic already swapped the files)
    // We need to open a temporary connection to the new database file.
    if let Ok(new_db) = crate::db::ConnectionFactory::new_with_path(&db_path) {
        let _ = crate::application::services::TelemetryService::new(new_db.executor())
            .record_event(
                crate::application::services::TelemetryEventType::Restore,
                crate::application::services::TelemetryOutcome::Success,
                Some(duration),
                Some(serde_json::json!({ "path": backup_path })),
                Some(&session_user_id),
            );
    }

    state.operation_guard.record_execution(restore_op);

    // Force restart - use the low-level restart to preserve environment (crucial for dev mode)
    tauri::process::restart(&app_handle.env());

    #[allow(unreachable_code)]
    Ok(RestoreResult { success: true })
}
