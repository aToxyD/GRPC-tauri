#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use grpc_lib::commands::{self, AppState};
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

    // Eager DB-path resolution (ADR-0041 §5: stays eager for readiness/cleanup;
    // no SQLite connection or migrations start while the app key is locked).
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

    let state = if grpc_lib::infrastructure::security::app_key_unlocked() {
        // Eager startup (ADR-0041 §5 rank 1 / rank 3): env or dev key present.
        let runtime = match grpc_lib::application::services::runtime_bootstrap::bootstrap_runtime()
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to initialize database: {}", e);
                std::process::exit(1);
            }
        };
        AppState {
            db: Arc::new(Mutex::new(Some(runtime.db))),
            rate_limiter: Arc::new(Mutex::new(runtime.rate_limiter)),
            operation_guard: Arc::new(
                grpc_lib::application::services::OperationExecutionGuard::new(),
            ),
            maintenance: grpc_lib::application::services::SystemMaintenanceHandle::new(
                grpc_lib::application::services::SystemMaintenanceState::Normal,
            ),

            current_session: Arc::new(Mutex::new(None)),
            identity_challenge: Arc::new(Mutex::new(
                grpc_lib::domain::identity::IdentityChallengeState::default(),
            )),
            crypto_port:
                grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider::new(
                ),
            password_port: Arc::new(grpc_lib::infrastructure::security::Argon2PasswordHashProvider),
            process_start_time: std::time::Instant::now(),
        }
    } else {
        // Locked / Unprovisioned (ADR-0041 rank 2 / rank 4): the store must be
        // unlocked (or first-run setup completed) before the DB opens. Boot
        // reaches the Security Setup / Unlock UI with the DB deferred.
        log::info!(target: "grpc::runtime", "app key locked or unprovisioned — DB bootstrap deferred until unlock (ADR-0041)");
        AppState {
            db: Arc::new(Mutex::new(None)),
            rate_limiter: Arc::new(Mutex::new(RateLimiter::new())),
            operation_guard: Arc::new(
                grpc_lib::application::services::OperationExecutionGuard::new(),
            ),
            maintenance: grpc_lib::application::services::SystemMaintenanceHandle::new(
                grpc_lib::application::services::SystemMaintenanceState::Normal,
            ),

            current_session: Arc::new(Mutex::new(None)),
            identity_challenge: Arc::new(Mutex::new(
                grpc_lib::domain::identity::IdentityChallengeState::default(),
            )),
            crypto_port:
                grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider::new(
                ),
            password_port: Arc::new(grpc_lib::infrastructure::security::Argon2PasswordHashProvider),
            process_start_time: std::time::Instant::now(),
        }
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
        .on_window_event(move |window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    // Checkpoint and close rate limiter connection
                    if let Ok(mut rl_guard) = state.rate_limiter.lock() {
                        rl_guard.shutdown();
                        // Replace with in-memory rate limiter to drop the persisted connection
                        *rl_guard = grpc_lib::domain::rate_limiter::RateLimiter::new();
                    }

                    // Close database connection when window is destroyed
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

                    // Clean up WAL and SHM files after both connections are closed
                    let wal_path = db_path.with_extension("db-wal");
                    let shm_path = db_path.with_extension("db-shm");
                    let _ = std::fs::remove_file(&wal_path);
                    let _ = std::fs::remove_file(&shm_path);
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
