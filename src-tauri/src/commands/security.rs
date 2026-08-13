//! Application-key provisioning commands (ADR-0041).
//!
//! Pre-auth lifecycle handlers scoped to Security Setup / Unlock. Thin IPC
//! handlers — the passphrase never leaves the backend; the raw identity only
//! ever crosses the boundary on an explicit opt-in backup export.
//!
//! - `get_security_status` → live provisioning projection
//! - `initialize_app_key(passphrase, export_backup?)` → first-run key generation
//! - `unlock_app_key(passphrase)` → store unlock + deferred DB bootstrap
//! - `export_app_key_backup()` → guarded re-export (requires unlocked store)

use crate::application::authz::Action;
use crate::application::services::{TelemetryEventType, TelemetryOutcome, TelemetryService};
use crate::commands::common::appkey_store;
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::errors::{into_command_error, AppError};
use crate::infrastructure::security::appkey_store::{
    AppKeyFile, APPKEY_ALGORITHM_VERSION, APPKEY_FORMAT_VERSION,
};
use crate::infrastructure::security::{
    app_key_status, cache_app_key, cached_app_key, clear_app_key_cache,
};
use crate::models::{AppKeyInitializeResult, AppKeyStatus, AppKeyUnlockResult};
use tauri::State;

/// Minimum app-key passphrase length (strong-passphrase boundary, ADR-0041 §6).
pub const MIN_APP_KEY_PASSPHRASE_LEN: usize = 8;

/// Allowed extensions for an opt-in offline backup export.
const BACKUP_EXPORT_EXTENSIONS: &[&str] = &["age", "key", "txt"];

/// Live application-key provisioning status (ADR-0041 §9).
#[tauri::command]
pub fn get_security_status() -> Result<AppKeyStatus, String> {
    app_key_status().map_err(into_command_error)
}

/// First-run key generation (Security Setup, ADR-0041 §6): generate a fresh
/// `age::x25519::Identity`, wrap it with the operator passphrase, write
/// `appkey.age` atomically, cache the key, and run the deferred DB bootstrap.
///
/// `export_backup` optionally writes the raw identity once to the operator's
/// chosen destination (explicit warning shown by the UI).
#[tauri::command]
pub fn initialize_app_key(
    state: State<AppState>,
    passphrase: String,
    export_backup: Option<String>,
) -> Result<AppKeyInitializeResult, String> {
    let passphrase = passphrase.trim();
    if passphrase.len() < MIN_APP_KEY_PASSPHRASE_LEN {
        return Err(into_command_error(AppError::Validation(
            crate::errors::ValidationError::InvalidFormat {
                field: "passphrase".into(),
                message: format!(
                    "must be at least {MIN_APP_KEY_PASSPHRASE_LEN} characters (strong passphrase required)"
                ),
            },
        )));
    }

    let store = appkey_store();
    if store.exists() {
        return Err(into_command_error(AppError::Configuration(
            "appkey.age already exists — refusing to overwrite; unlock the existing store".into(),
        )));
    }

    let identity = age::x25519::Identity::generate();
    let raw_identity = {
        use age::secrecy::ExposeSecret;
        identity.to_string().expose_secret().to_string()
    };

    let encrypted = store
        .encrypt_identity(&raw_identity, passphrase)
        .map_err(into_command_error)?;
    store
        .write(&AppKeyFile {
            format_version: APPKEY_FORMAT_VERSION,
            algorithm_version: APPKEY_ALGORITHM_VERSION,
            encrypted_identity: encrypted,
        })
        .map_err(into_command_error)?;
    cache_app_key(&raw_identity).map_err(into_command_error)?;

    let mut exported_backup = false;
    if let Some(path) = export_backup
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        crate::domain::validation::validate_file_path(path, BACKUP_EXPORT_EXTENSIONS)
            .map_err(into_command_error)?;
        std::fs::write(path, &raw_identity).map_err(|e| into_command_error(AppError::Io(e)))?;
        exported_backup = true;
    }

    log::info!(target: "grpc::security", "app key generated and store written (ADR-0041)");

    match finish_unlock(&state) {
        Ok(()) => Ok(AppKeyInitializeResult {
            provisioned: true,
            unlocked: true,
            store_path: store.file_path().display().to_string(),
            exported_backup,
        }),
        Err(e) => {
            clear_app_key_cache();
            Err(e)
        }
    }
}

/// Unlock the passphrase-protected `appkey.age` store (ADR-0041 §4 `Unlocked`):
/// decrypt with the passphrase, cache the key, and run the deferred DB
/// bootstrap. Wrong passphrase / corrupt store stays locked (fail-closed).
#[tauri::command]
pub fn unlock_app_key(
    state: State<AppState>,
    passphrase: String,
) -> Result<AppKeyUnlockResult, String> {
    let store = appkey_store();
    if !store.exists() {
        return Err(into_command_error(AppError::Configuration(
            "appkey.age not found — initialize the app key first (Security Setup)".into(),
        )));
    }
    let store_path = store.file_path().display().to_string();

    // Idempotent: already unlocked and the DB is open → no-op success.
    if cached_app_key().is_some() && db_is_open(&state) {
        return Ok(AppKeyUnlockResult {
            provisioned: true,
            unlocked: true,
            store_path,
        });
    }

    let file = store.read().map_err(into_command_error)?;
    let raw_identity = store
        .decrypt_identity(&file.encrypted_identity, passphrase.trim())
        .map_err(|e| {
            log::warn!(target: "grpc::security", "unlock_app_key failed — store stays locked: {e}");
            into_command_error(e)
        })?;
    cache_app_key(&raw_identity).map_err(into_command_error)?;

    match finish_unlock(&state) {
        Ok(()) => {
            log::info!(target: "grpc::security", "app key unlocked — DB bootstrap complete (ADR-0041)");
            Ok(AppKeyUnlockResult {
                provisioned: true,
                unlocked: true,
                store_path,
            })
        }
        Err(e) => {
            clear_app_key_cache();
            Err(e)
        }
    }
}

/// Guarded re-export of the raw application identity (ADR-0041 §7). Requires
/// an authenticated Admin session plus unlocked state (store cache); used for
/// offline backup after first setup.
///
/// SEC-003-03: the raw fleet AGE identity must not be reachable without an
/// authenticated Admin. The setup-time backup remains available through
/// `initialize_app_key(..., export_backup = Some(path))`, which is unchanged.
#[tauri::command]
pub fn export_app_key_backup(state: State<AppState>) -> Result<String, String> {
    let (session, _settings) =
        authorize_command(&state, Action::AdminOnly, None).map_err(into_command_error)?;
    state.touch_session();

    let identity = cached_app_key().ok_or_else(|| {
        into_command_error(AppError::Configuration(
            "app key is locked — unlock the store before exporting a backup".into(),
        ))
    })?;

    // Best-effort telemetry (mirrors existing command patterns); DB may be
    // unavailable in edge states — the export itself is unaffected.
    if let Ok(guard) = state.get_db() {
        if let Some(db) = guard.as_ref() {
            let _ = TelemetryService::new(db.executor()).record_event(
                TelemetryEventType::Backup,
                TelemetryOutcome::Success,
                None,
                Some(serde_json::json!({ "export": "app_key_backup" })),
                Some(&session.user_id),
            );
        }
    }

    Ok(identity)
}

/// Run the deferred DB bootstrap (ADR-0041 §5) and publish the database + the
/// persisted rate limiter into `AppState`.
fn finish_unlock(state: &AppState) -> Result<(), String> {
    let runtime = crate::application::services::bootstrap_runtime().map_err(into_command_error)?;
    state.set_db(runtime.db).map_err(into_command_error)?;
    if let Ok(mut rl_guard) = state.rate_limiter.lock() {
        rl_guard.shutdown();
        *rl_guard = runtime.rate_limiter;
    }
    Ok(())
}

fn db_is_open(state: &AppState) -> bool {
    match state.get_db() {
        Ok(guard) => guard.is_some(),
        Err(_) => false,
    }
}
