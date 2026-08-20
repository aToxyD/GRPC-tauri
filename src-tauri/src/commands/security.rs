//! Application-key provisioning commands (ADR-0041).
//!
//! Pre-auth lifecycle handlers scoped to Security Setup / Unlock. Thin IPC
//! handlers — the passphrase never leaves the backend; the raw identity only
//! ever crosses the boundary on an explicit opt-in backup export.
//!
//! - `get_security_status` → live provisioning projection
//! - `initialize_app_key(passphrase, export_backup?)` → first-run key generation
//! - `import_app_key(passphrase, artifact_path)` → fleet artifact import (APPKEY-003)
//! - `unlock_app_key(passphrase)` → store unlock + deferred DB bootstrap
//! - `export_app_key_backup()` → guarded re-export (requires unlocked store)

use crate::application::authz::Action;
use crate::application::services::{TelemetryEventType, TelemetryOutcome, TelemetryService};
use crate::commands::common::appkey_store;
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::errors::{into_command_error, AppError, AppResult};
use crate::infrastructure::security::appkey_store::{
    AppKeyFile, AppKeyStore, APPKEY_ALGORITHM_VERSION, APPKEY_FORMAT_VERSION,
};
use crate::infrastructure::security::{
    app_key_status, cache_app_key, cached_app_key, clear_app_key_cache,
    forget_remembered_app_key as remove_remembered_app_key_from_keyring,
    remember_app_key_best_effort,
};
use crate::models::{AppKeyImportResult, AppKeyInitializeResult, AppKeyStatus, AppKeyUnlockResult};
use tauri::State;

/// Minimum app-key passphrase length (strong-passphrase boundary, ADR-0041 §6).
pub const MIN_APP_KEY_PASSPHRASE_LEN: usize = 8;

/// Allowed extensions for an opt-in offline backup export.
const BACKUP_EXPORT_EXTENSIONS: &[&str] = &["age", "key", "txt"];

/// Allowed extensions for the portable provisioning artifact import
/// (APPKEY-003 §6).
const ARTIFACT_IMPORT_EXTENSIONS: &[&str] = &["age", "key", "txt"];

/// Strict maximum size of the portable provisioning artifact (a valid age
/// identity is ~120 bytes; this cap is defense-in-depth against oversized or
/// hostile files, APPKEY-003 §6).
const MAX_APP_KEY_ARTIFACT_BYTES: u64 = 64 * 1024;

/// Normalize raw artifact bytes into exactly one App Key value: trim harmless
/// surrounding whitespace (including a trailing newline), then reject empty
/// content, multiple lines/keys, invalid UTF-8, and unsupported prefixes.
/// Fail-closed on anything ambiguous. Errors never contain secret material.
fn normalize_imported_artifact(content: &[u8]) -> AppResult<String> {
    let text = std::str::from_utf8(content)
        .map_err(|_| AppError::FileFormat("App Key artifact is not valid UTF-8 text".into()))?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(AppError::FileFormat("App Key artifact is empty".into()));
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(AppError::FileFormat(
            "App Key artifact must contain exactly one key line".into(),
        ));
    }
    if !trimmed.starts_with("AGE-SECRET-KEY-1") {
        return Err(AppError::FileFormat(
            "App Key artifact does not contain a valid age x25519 identity (expected AGE-SECRET-KEY-1...)"
                .into(),
        ));
    }
    Ok(trimmed.to_string())
}

/// Core import logic (APPKEY-003, Design B): read + validate + normalize the
/// WILAYA-sourced portable artifact, wrap the App Key with the operator
/// passphrase into the existing `appkey.age` format, and persist atomically
/// with `0600`. Refuses when the local store already exists — existing stores
/// are never mutated. Returns the normalized App Key to the caller
/// (backend-internal only, never part of any IPC result) so the command can
/// prime the runtime cache.
///
/// Public as a test seam so the integration suite (`src-tauri/tests/`) can
/// exercise the real import path against an isolated store dir (same pattern
/// as `import_identity_access_package_impl`).
pub fn import_app_key_into_store(
    store: &AppKeyStore,
    passphrase: &str,
    artifact_path: &str,
) -> AppResult<(AppKeyImportResult, String)> {
    if store.exists() {
        return Err(AppError::Configuration(
            "appkey.age already exists — refusing to overwrite; unlock the existing store"
                .into(),
        ));
    }

    crate::domain::validation::validate_file_path(artifact_path, ARTIFACT_IMPORT_EXTENSIONS)?;

    let metadata = std::fs::metadata(artifact_path).map_err(AppError::Io)?;
    if metadata.len() > MAX_APP_KEY_ARTIFACT_BYTES {
        return Err(AppError::Internal(
            "App Key artifact exceeds the maximum accepted size (64 KiB)".into(),
        ));
    }
    let content = std::fs::read(artifact_path).map_err(AppError::Io)?;
    let identity = normalize_imported_artifact(&content)?;

    let encrypted = store.encrypt_identity(&identity, passphrase)?;
    store.write(&AppKeyFile {
        format_version: APPKEY_FORMAT_VERSION,
        algorithm_version: APPKEY_ALGORITHM_VERSION,
        encrypted_identity: encrypted,
    })?;

    Ok((
        AppKeyImportResult {
            provisioned: true,
            unlocked: true,
            store_path: store.file_path().display().to_string(),
        },
        identity,
    ))
}

/// Best-effort provisioning telemetry (DB may be unavailable pre-bootstrap;
/// the import itself is unaffected).
fn record_app_key_import_telemetry(state: &AppState, success: bool) {
    if let Ok(guard) = state.get_db() {
        if let Some(db) = guard.as_ref() {
            let _ = TelemetryService::new(db.executor()).record_event(
                TelemetryEventType::AppKeyImport,
                if success {
                    TelemetryOutcome::Success
                } else {
                    TelemetryOutcome::Failure
                },
                None,
                None,
                None,
            );
        }
    }
}

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
/// chosen destination (explicit warning shown by the UI). `remember` (default
/// `false` — opt-in) persists the VALIDATED App Key to the OS keyring so
/// post-provisioning startup resolves it non-interactively (ADR-0041 §11.4).
#[tauri::command]
pub fn initialize_app_key(
    state: State<AppState>,
    passphrase: String,
    export_backup: Option<String>,
    remember: Option<bool>,
) -> Result<AppKeyInitializeResult, String> {
    initialize_app_key_impl(&state, &passphrase, export_backup.as_deref(), remember)
}

/// Core of `initialize_app_key` (public test seam — same pattern as
/// `import_app_key_into_store`).
pub fn initialize_app_key_impl(
    state: &AppState,
    passphrase: &str,
    export_backup: Option<&str>,
    remember: Option<bool>,
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
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        crate::domain::validation::validate_file_path(path, BACKUP_EXPORT_EXTENSIONS)
            .map_err(into_command_error)?;
        std::fs::write(path, &raw_identity).map_err(|e| into_command_error(AppError::Io(e)))?;
        exported_backup = true;
    }

    log::info!(target: "grpc::security", "app key generated and store written (ADR-0041)");

    match finish_unlock(state) {
        Ok(()) => {
            remember_app_key_best_effort(remember == Some(true), &raw_identity);
            Ok(AppKeyInitializeResult {
                provisioned: true,
                unlocked: true,
                store_path: store.file_path().display().to_string(),
                exported_backup,
            })
        }
        Err(e) => {
            clear_app_key_cache();
            Err(e)
        }
    }
}

/// Import the WILAYA-sourced portable artifact `grpc-app-key.age` into the
/// encrypted local store (APPKEY-003, ratified by ADR-0041 §10.4 amendment).
/// Provisioning convenience only: the App Key remains confidentiality /
/// decryption material — no identity, trust, signing, or B8 semantics are
/// introduced. The artifact is read by the backend (the renderer sends only
/// its path); it is never modified, renamed, or deleted, and its raw value
/// never crosses IPC, the DOM, or logs.
#[tauri::command]
pub fn import_app_key(
    state: State<AppState>,
    passphrase: String,
    artifact_path: String,
    remember: Option<bool>,
) -> Result<AppKeyImportResult, String> {
    import_app_key_impl(&state, &passphrase, &artifact_path, remember)
}

/// Core of `import_app_key` (public test seam — same pattern as
/// `import_app_key_into_store`).
pub fn import_app_key_impl(
    state: &AppState,
    passphrase: &str,
    artifact_path: &str,
    remember: Option<bool>,
) -> Result<AppKeyImportResult, String> {
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
    let (result, identity) =
        import_app_key_into_store(&store, passphrase, artifact_path.trim())
            .map_err(into_command_error)?;

    cache_app_key(&identity).map_err(into_command_error)?;

    match finish_unlock(state) {
        Ok(()) => {
            log::info!(target: "grpc::security", "fleet app key imported into local store (APPKEY-003)");
            remember_app_key_best_effort(remember == Some(true), &identity);
            record_app_key_import_telemetry(state, true);
            Ok(result)
        }
        Err(e) => {
            clear_app_key_cache();
            record_app_key_import_telemetry(state, false);
            Err(e)
        }
    }
}

/// Unlock the passphrase-protected `appkey.age` store (ADR-0041 §4 `Unlocked`):
/// decrypt with the passphrase, cache the key, and run the deferred DB
/// bootstrap. Wrong passphrase / corrupt store stays locked (fail-closed).
///
/// `remember` (default `false` — opt-in) persists the VALIDATED decrypted App
/// Key to the OS keyring so post-provisioning startup resolves it
/// non-interactively (ADR-0041 §11.4). Persistence is best-effort AFTER
/// successful unlock: a keyring failure never rolls back the unlock and never
/// blocks the application for this process.
#[tauri::command]
pub fn unlock_app_key(
    state: State<AppState>,
    passphrase: String,
    remember: Option<bool>,
) -> Result<AppKeyUnlockResult, String> {
    unlock_app_key_impl(&state, &passphrase, remember)
}

/// Core of `unlock_app_key` (public test seam — same pattern as
/// `import_app_key_into_store`).
pub fn unlock_app_key_impl(
    state: &AppState,
    passphrase: &str,
    remember: Option<bool>,
) -> Result<AppKeyUnlockResult, String> {
    let store = appkey_store();
    if !store.exists() {
        return Err(into_command_error(AppError::Configuration(
            "appkey.age not found — initialize the app key first (Security Setup)".into(),
        )));
    }
    let store_path = store.file_path().display().to_string();

    // Idempotent: already unlocked and the DB is open → no-op success.
    if cached_app_key().is_some() && db_is_open(state) {
        // Still honor an explicit opt-in: persist the cached key so the
        // operator can enable remember-on-device on an already-open session.
        if remember == Some(true) {
            if let Some(key) = cached_app_key() {
                remember_app_key_best_effort(true, &key);
            }
        }
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

    match finish_unlock(state) {
        Ok(()) => {
            log::info!(target: "grpc::security", "app key unlocked — DB bootstrap complete (ADR-0041)");
            remember_app_key_best_effort(remember == Some(true), &raw_identity);
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

/// Remove the remembered App Key from the OS keyring (ADR-0041 §11.4).
/// Device-local preference only: `appkey.age`, the database, identities,
/// users, sessions, and provisioning state are untouched. No session is
/// created or destroyed. Returns `true` if a remembered entry was removed,
/// `false` if none existed.
#[tauri::command]
pub fn forget_remembered_app_key() -> Result<bool, String> {
    remove_remembered_app_key_from_keyring().map_err(into_command_error)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use tempfile::TempDir;

    const VALID_ARTIFACT: &str = "AGE-SECRET-KEY-1TESTVALIDAGEKEYMATERIALPLACEHOLDER0000000000";

    fn write_artifact(dir: &TempDir, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    fn test_store(dir: &TempDir) -> AppKeyStore {
        AppKeyStore::new(dir.path().to_path_buf())
    }

    fn fresh_valid_artifact() -> String {
        use age::secrecy::ExposeSecret;
        age::x25519::Identity::generate()
            .to_string()
            .expose_secret()
            .to_string()
    }

    // ------------------------------------------------------------------
    // normalize_imported_artifact
    // ------------------------------------------------------------------

    #[test]
    fn normalize_accepts_single_line_with_trailing_newline() {
        let normalized = normalize_imported_artifact(format!("{VALID_ARTIFACT}\n").as_bytes());
        assert_eq!(normalized.unwrap(), VALID_ARTIFACT);
    }

    #[test]
    fn normalize_accepts_surrounding_whitespace() {
        let normalized = normalize_imported_artifact(format!("  {VALID_ARTIFACT}  \n").as_bytes());
        assert_eq!(normalized.unwrap(), VALID_ARTIFACT);
    }

    #[test]
    fn normalize_rejects_empty() {
        assert!(normalize_imported_artifact(b"").is_err());
    }

    #[test]
    fn normalize_rejects_whitespace_only() {
        assert!(normalize_imported_artifact(b" \n\t ").is_err());
    }

    #[test]
    fn normalize_rejects_multiple_lines() {
        let content = format!("{VALID_ARTIFACT}\n{VALID_ARTIFACT}\n");
        assert!(normalize_imported_artifact(content.as_bytes()).is_err());
    }

    #[test]
    fn normalize_rejects_invalid_prefix() {
        let content = b"-----BEGIN AGE ENCRYPTED FILE-----\n";
        assert!(normalize_imported_artifact(content).is_err());
    }

    #[test]
    fn normalize_rejects_invalid_utf8() {
        assert!(normalize_imported_artifact(&[0xff, 0xfe, 0x00, 0x01]).is_err());
    }

    // ------------------------------------------------------------------
    // import_app_key_into_store — success path
    // ------------------------------------------------------------------

    #[test]
    fn import_creates_encrypted_store_with_0600() {
        let dir = TempDir::new().unwrap();
        let artifact = write_artifact(&dir, "grpc-app-key.age", format!("{VALID_ARTIFACT}\n").as_bytes());
        let store = test_store(&dir);

        let (result, identity) =
            import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
                .unwrap();

        assert!(result.provisioned);
        assert!(result.unlocked);
        assert!(result.store_path.ends_with("appkey.age"));
        assert_eq!(identity, VALID_ARTIFACT);

        assert!(store.exists());
        let perms = std::fs::metadata(store.file_path()).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }

    #[test]
    fn import_store_contains_no_plaintext_identity() {
        let dir = TempDir::new().unwrap();
        let artifact = write_artifact(&dir, "grpc-app-key.age", format!("{VALID_ARTIFACT}\n").as_bytes());
        let store = test_store(&dir);

        import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
            .unwrap();

        let stored = std::fs::read(store.file_path()).unwrap();
        let stored_text = String::from_utf8_lossy(&stored);
        assert!(!stored_text.contains("AGE-SECRET-KEY-1"));
        assert!(stored != b"");
    }

    #[test]
    fn import_roundtrip_decrypt_with_passphrase() {
        let dir = TempDir::new().unwrap();
        let artifact = write_artifact(&dir, "grpc-app-key.age", format!("{VALID_ARTIFACT}\n").as_bytes());
        let store = test_store(&dir);

        let (_, identity) =
            import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
                .unwrap();

        let file = store.read().unwrap();
        let decrypted = store
            .decrypt_identity(&file.encrypted_identity, "correct-horse-import")
            .unwrap();
        assert_eq!(decrypted, identity);
        assert_eq!(decrypted, VALID_ARTIFACT);
    }

    #[test]
    fn import_wrong_passphrase_cannot_decrypt() {
        let dir = TempDir::new().unwrap();
        let artifact = write_artifact(&dir, "grpc-app-key.age", format!("{VALID_ARTIFACT}\n").as_bytes());
        let store = test_store(&dir);

        import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
            .unwrap();

        let file = store.read().unwrap();
        assert!(store
            .decrypt_identity(&file.encrypted_identity, "wrong-passphrase-1")
            .is_err());
    }

    #[test]
    fn import_never_mutates_or_deletes_artifact() {
        let dir = TempDir::new().unwrap();
        let artifact = write_artifact(&dir, "grpc-app-key.age", format!("{VALID_ARTIFACT}\n").as_bytes());
        let store = test_store(&dir);

        import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
            .unwrap();

        let after = std::fs::read(&artifact).unwrap();
        assert_eq!(after, format!("{VALID_ARTIFACT}\n").as_bytes());
        assert!(artifact.exists());
    }

    // ------------------------------------------------------------------
    // import_app_key_into_store — conflict behavior
    // ------------------------------------------------------------------

    #[test]
    fn import_refuses_when_store_already_exists() {
        let dir = TempDir::new().unwrap();
        let artifact = write_artifact(&dir, "grpc-app-key.age", format!("{VALID_ARTIFACT}\n").as_bytes());
        let store = test_store(&dir);
        import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
            .unwrap();

        let before = std::fs::read(store.file_path()).unwrap();
        let err = import_app_key_into_store(&store, "second-passphrase", artifact.to_str().unwrap())
            .unwrap_err();
        assert!(matches!(err, AppError::Configuration(_)));
        let after = std::fs::read(store.file_path()).unwrap();
        assert_eq!(before, after, "existing store must remain byte-identical");
    }

    #[test]
    fn import_second_invocation_refuses() {
        let dir = TempDir::new().unwrap();
        let artifact = write_artifact(&dir, "grpc-app-key.age", format!("{VALID_ARTIFACT}\n").as_bytes());
        let store = test_store(&dir);

        import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
            .unwrap();
        assert!(import_app_key_into_store(&store, "another-passphrase-1", artifact.to_str().unwrap()).is_err());
    }

    // ------------------------------------------------------------------
    // import_app_key_into_store — input validation
    // ------------------------------------------------------------------

    #[test]
    fn import_rejects_disallowed_extension() {
        let dir = TempDir::new().unwrap();
        let artifact = write_artifact(&dir, "grpc-app-key.unit", format!("{VALID_ARTIFACT}\n").as_bytes());
        let store = test_store(&dir);

        let err = import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
        assert!(!store.exists());
    }

    #[test]
    fn import_rejects_missing_extension() {
        let dir = TempDir::new().unwrap();
        let artifact = write_artifact(&dir, "grpc-app-key", format!("{VALID_ARTIFACT}\n").as_bytes());
        let store = test_store(&dir);

        let err = import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
        assert!(!store.exists());
    }

    #[test]
    fn import_rejects_path_traversal() {
        let dir = TempDir::new().unwrap();
        let store = test_store(&dir);

        let err = import_app_key_into_store(
            &store,
            "correct-horse-import",
            "../../../etc/passwd",
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
        assert!(!store.exists());
    }

    #[test]
    fn import_rejects_nonexistent_artifact() {
        let dir = TempDir::new().unwrap();
        let store = test_store(&dir);

        let err = import_app_key_into_store(
            &store,
            "correct-horse-import",
            dir.path().join("missing.age").to_str().unwrap(),
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Io(_)));
        assert!(!store.exists());
    }

    #[test]
    fn import_rejects_directory_as_artifact() {
        let dir = TempDir::new().unwrap();
        let store = test_store(&dir);

        let err = import_app_key_into_store(&store, "correct-horse-import", dir.path().to_str().unwrap())
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
        assert!(!store.exists());
    }

    #[test]
    fn import_rejects_oversized_artifact() {
        let dir = TempDir::new().unwrap();
        let big = vec![b'A'; (MAX_APP_KEY_ARTIFACT_BYTES as usize) + 1];
        let artifact = write_artifact(&dir, "grpc-app-key.age", &big);
        let store = test_store(&dir);

        let err = import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
            .unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
        assert!(!store.exists());
    }

    #[test]
    fn import_rejects_empty_artifact() {
        let dir = TempDir::new().unwrap();
        let artifact = write_artifact(&dir, "grpc-app-key.age", b"");
        let store = test_store(&dir);

        let err = import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
            .unwrap_err();
        assert!(matches!(err, AppError::FileFormat(_)));
        assert!(!store.exists());
    }

    #[test]
    fn import_rejects_multiline_artifact() {
        let dir = TempDir::new().unwrap();
        let content = format!("{VALID_ARTIFACT}\nAGE-SECRET-KEY-1SECONDKEY0000000000000000000000\n");
        let artifact = write_artifact(&dir, "grpc-app-key.age", content.as_bytes());
        let store = test_store(&dir);

        let err = import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
            .unwrap_err();
        assert!(matches!(err, AppError::FileFormat(_)));
        assert!(!store.exists());
    }

    #[test]
    fn import_rejects_wrong_prefix_artifact() {
        let dir = TempDir::new().unwrap();
        let artifact = write_artifact(
            &dir,
            "grpc-app-key.age",
            b"-----BEGIN AGE ENCRYPTED FILE-----\nAAAA",
        );
        let store = test_store(&dir);

        let err = import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
            .unwrap_err();
        assert!(matches!(err, AppError::FileFormat(_)));
        assert!(!store.exists());
    }

    // ------------------------------------------------------------------
    // Runtime integration: cache priming + file-encryption provider
    // ------------------------------------------------------------------

    #[test]
    fn imported_key_primes_cache_and_drives_file_encryption() {
        let dir = TempDir::new().unwrap();
        let identity = fresh_valid_artifact();
        let artifact = write_artifact(&dir, "grpc-app-key.age", format!("{identity}\n").as_bytes());
        let store = test_store(&dir);

        let (_, imported) =
            import_app_key_into_store(&store, "correct-horse-import", artifact.to_str().unwrap())
                .unwrap();
        assert_eq!(imported, identity);

        clear_app_key_cache();
        assert!(cached_app_key().is_none());

        cache_app_key(&imported).unwrap();
        assert_eq!(cached_app_key().unwrap(), identity);

        let provider = AgeFileEncryptionProvider::new();
        let data = b"unit-package-bytes-for-roundtrip";
        let encrypted = provider.encrypt_data(data).unwrap();
        assert_ne!(encrypted, data);
        let decrypted = provider.decrypt_data(&encrypted).unwrap();
        assert_eq!(decrypted, data);

        clear_app_key_cache();
        assert!(cached_app_key().is_none());
    }

    #[test]
    fn command_passphrase_validation_threshold_is_enforced() {
        let short = "x".repeat(MIN_APP_KEY_PASSPHRASE_LEN - 1);
        assert!(short.len() < MIN_APP_KEY_PASSPHRASE_LEN);
        let ok = "x".repeat(MIN_APP_KEY_PASSPHRASE_LEN);
        assert!(ok.len() >= MIN_APP_KEY_PASSPHRASE_LEN);
    }
}
