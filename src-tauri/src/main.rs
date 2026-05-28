#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use grpc_lib::commands::{self, AppState};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::rate_limiter::RateLimiter;
use std::sync::{Arc, Mutex};
use tauri::Manager;

fn main() {
    if let Err(e) = grpc_lib::infrastructure::logging::init_file_logging() {
        eprintln!("file logging init failed: {}", e);
    }

    if let Err(msg) = grpc_lib::infrastructure::security::validate_production_security_environment()
    {
        log::error!(target: "grpc::runtime", "security validation failed: {}", msg);
        eprintln!("{}", msg);
        std::process::exit(1);
    }
    log::info!(target: "grpc::runtime", "security environment validated");

    let db_path = match grpc_lib::db::get_db_path() {
        Ok(p) => {
            log::info!(target: "grpc::runtime", "Using database file: {}", p.display());
            eprintln!("Using database file: {}", p.display());
            p
        }
        Err(e) => {
            eprintln!("Failed to resolve database path: {}", e);
            std::process::exit(1);
        }
    };
    let crypto_probe =
        grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider::new();
    if let Err(e) = grpc_lib::infrastructure::backup::recover_interrupted_restore_and_orphans(
        &db_path,
        &crypto_probe,
    ) {
        eprintln!("Restore recovery / orphan cleanup failed: {}", e);
        std::process::exit(1);
    }

    // Initialize database and create default admin if needed
    let db = match ConnectionFactory::new() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Failed to initialize database: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize rate limiter for login protection
    let rate_limiter = RateLimiter::new();

    // 🚫 Settings cache removed - authorization always reads from DB directly

    let db_path = match db.get_connection_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to resolve database path: {}", e);
            std::process::exit(1);
        }
    };

    // ── Deployment readiness (read-only, fail-closed on blocking checks) ────
    match grpc_lib::application::services::DeploymentReadinessService::new(
        db.executor(),
        db_path.clone(),
    )
    .verify()
    {
        Ok(report) => {
            for w in &report.warnings {
                log::warn!(target: "grpc::deployment", "readiness warning: {}", w);
            }
            if report.status == grpc_lib::application::services::DeploymentReadinessStatus::NotReady
            {
                for f in &report.blocking_failures {
                    log::error!(target: "grpc::deployment", "readiness blocking: {}", f);
                    eprintln!("DEPLOYMENT NOT READY: {}", f);
                }
                std::process::exit(1);
            }
            log::info!(target: "grpc::deployment", "deployment readiness: READY");
        }
        Err(e) => {
            eprintln!("DEPLOYMENT READINESS CHECK FAILED: {}", e);
            std::process::exit(1);
        }
    }

    // ── Recover abandoned operator sessions ─────────────────────────────────
    if let Err(e) = grpc_lib::application::services::OperationalSessionService::new(db.executor())
        .recover_abandoned_sessions()
    {
        log::warn!(target: "grpc::session", "session recovery failed: {}", e);
    }

    // ── Fiscal state validation (startup guard) ─────────────────────────────
    if let Err(msg) =
        grpc_lib::application::services::fiscal_validation_service::validate_fiscal_state(&db)
    {
        log::error!(target: "grpc::fiscal", "STARTUP ABORTED — {}", msg);
        eprintln!("STARTUP ABORTED: {}", msg);
        std::process::exit(1);
    } else {
        let _ = grpc_lib::application::services::SystemHealthService::get_health_report(&db); // triggers initial integrity check + telemetry

        // Record startup event
        let _ = grpc_lib::application::services::TelemetryService::new(db.executor()).record_event(
            grpc_lib::application::services::TelemetryEventType::RuntimeStartup,
            grpc_lib::application::services::TelemetryOutcome::Success,
            None,
            None,
            None,
        );

        log::info!(target: "grpc::runtime", "System boot verification completed successfully");
    }

    // Clean up old audit logs (older than 1 year)
    let _ = grpc_lib::application::services::AuditService::new(db.executor())
        .cleanup_old_audit_logs(None);

    let state = AppState {
        db: Arc::new(Mutex::new(Some(db))),
        rate_limiter: Arc::new(Mutex::new(rate_limiter)),
        operation_guard: Arc::new(grpc_lib::application::services::OperationExecutionGuard::new()),
        maintenance: grpc_lib::application::services::SystemMaintenanceHandle::new(
            grpc_lib::application::services::SystemMaintenanceState::Normal,
        ),

        current_session: Arc::new(Mutex::new(None)),
        crypto_port:
            grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider::new(),
        password_port: Arc::new(grpc_lib::infrastructure::security::Argon2PasswordHashProvider),
        process_start_time: std::time::Instant::now(),
    };

    let run_result = tauri::Builder::default()
        .manage(state)
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            log::info!(target: "grpc::runtime", "single instance triggered: args={:?}, cwd={}", argv, cwd);

            // Focus existing window
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
                let _ = window.unminimize();
            }

            // Show Arabic notification as requested
            use tauri_plugin_dialog::DialogExt;
            app.dialog()
                .message("التطبيق يعمل بالفعل.")
                .title("تنبيه")
                .kind(tauri_plugin_dialog::MessageDialogKind::Info)
                .show(|_| {});
        }))
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                // Close database connection when window is destroyed
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    if let Ok(mut db_guard) = state.db.lock() {
                        // Force a checkpoint before closing to ensure WAL data is committed
                        if let Some(db) = db_guard.as_ref() {
                            let _ = db.get_connection().query_row(
                                "PRAGMA wal_checkpoint(TRUNCATE)",
                                [],
                                |_row| Ok(()),
                            );
                        }
                        // Take the database out of the Option to properly close it
                        let _ = db_guard.take();
                    }
                }
            }
        })
        .invoke_handler(commands::registry::get_invoke_handler())
        .run(tauri::generate_context!())
        .map_err(|e| {
            eprintln!("Error while running tauri application: {}", e);
            e
        });

    if let Err(e) = run_result {
        eprintln!("Failed to finalize app run result: {}", e);
    }
}
