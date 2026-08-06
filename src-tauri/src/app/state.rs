//! Application state for Tauri (`AppState`, session, DB handle).
//!
//! Commands and guards depend on this module; do not duplicate state structs under `commands/`.

use crate::application::services::{
    OperationExecutionGuard, OperationalSessionService, SessionCounterKind,
    SystemMaintenanceHandle, SystemMaintenanceState,
};
use crate::db::Database;
use crate::domain::identity::IdentityChallengeState;
use crate::domain::rate_limiter::RateLimiter;
use crate::domain::security::PasswordHashPort;
use crate::domain::session::CurrentSession;
use crate::errors::AppError;
use crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use crate::models::Settings;
use std::sync::{Arc, Mutex};

/// Application state container for Tauri commands
pub struct AppState {
    pub db: Arc<Mutex<Option<Database>>>,
    pub rate_limiter: Arc<Mutex<RateLimiter>>,
    pub operation_guard: Arc<OperationExecutionGuard>,
    pub maintenance: SystemMaintenanceHandle,

    pub current_session: Arc<Mutex<Option<CurrentSession>>>,
    pub identity_challenge: Arc<Mutex<IdentityChallengeState>>,
    pub crypto_port: AgeFileEncryptionProvider,
    pub password_port: Arc<dyn PasswordHashPort>,
    pub process_start_time: std::time::Instant,
}

impl AppState {
    /// Create AppState for testing with injected database
    pub fn new_for_test(db: Database) -> Self {
        Self {
            db: Arc::new(Mutex::new(Some(db))),
            rate_limiter: Arc::new(Mutex::new(RateLimiter::new())),
            operation_guard: Arc::new(OperationExecutionGuard::new()),
            maintenance: SystemMaintenanceHandle::new(SystemMaintenanceState::Normal),

            current_session: Arc::new(Mutex::new(None)),
            identity_challenge: Arc::new(Mutex::new(IdentityChallengeState::default())),
            crypto_port: AgeFileEncryptionProvider::new(),
            password_port: Arc::new(crate::infrastructure::security::Argon2PasswordHashProvider),
            process_start_time: std::time::Instant::now(),
        }
    }

    pub fn get_db(&self) -> Result<std::sync::MutexGuard<'_, Option<Database>>, AppError> {
        self.db
            .lock()
            .map_err(|e| AppError::Internal(format!("Failed to lock database: {}", e)))
    }

    /// Update session last activity (call this in every command)
    pub fn touch_session(&self) {
        if let Ok(mut session) = self.current_session.lock() {
            if let Some(s) = session.as_mut() {
                s.touch();
            }
        }
    }

    /// Get current session info
    pub fn get_session_info(&self) -> Option<(String, String, String)> {
        self.current_session
            .lock()
            .ok()?
            .as_ref()
            .map(|s| (s.user_id.clone(), s.username.clone(), s.session_id.clone()))
    }

    /// Get current session for authorization checks
    pub fn get_session(&self) -> Result<CurrentSession, AppError> {
        self.current_session
            .lock()
            .map_err(|e| AppError::Internal(format!("Failed to lock session: {}", e)))?
            .clone()
            .ok_or_else(|| {
                AppError::Authentication(crate::errors::AuthenticationError::SessionNotFound)
            })
    }

    /// Get system settings - always from DB directly
    pub fn get_settings(&self) -> Result<Settings, AppError> {
        let db_guard = self.get_db()?;
        let db = db_guard
            .as_ref()
            .ok_or_else(|| AppError::Internal("Database not available".into()))?;
        crate::application::services::SettingsService::new(db.executor()).get_settings()
    }

    pub fn take_db(&self) -> Result<Database, AppError> {
        let mut guard = self
            .db
            .lock()
            .map_err(|e| AppError::Internal(format!("Failed to lock database: {}", e)))?;
        guard
            .take()
            .ok_or_else(|| AppError::Internal("Database already closed".to_string()))
    }

    pub fn set_db(&self, db: Database) -> Result<(), AppError> {
        let mut guard = self
            .db
            .lock()
            .map_err(|e| AppError::Internal(format!("Failed to lock database: {}", e)))?;
        *guard = Some(db);
        Ok(())
    }

    /// Increment an operational session counter for the active session (best-effort).
    pub fn increment_session_counter(&self, kind: SessionCounterKind) {
        let session_id = match self.get_session_info() {
            Some((_, _, sid)) => sid,
            None => return,
        };
        if let Ok(guard) = self.get_db() {
            if let Some(db) = guard.as_ref() {
                let _ = OperationalSessionService::new(db.executor())
                    .increment_counter(&session_id, kind);
            }
        }
    }
}
