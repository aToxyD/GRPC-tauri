//! Persisted rate-limiter adapter (ADR-0029).
//!
//! The rate limiter is the ONLY permitted secondary SQLite connection (besides
//! the primary `Database`). Its wiring lives here so `application/` never touches
//! `rusqlite` or constructs repositories directly; the shared bootstrap sequence
//! (ADR-0041 §5) opens it through this adapter.

use std::path::Path;

use crate::db::apply_pragma_settings;
use crate::domain::rate_limiter::RateLimiter;
use crate::errors::{AppError, AppResult};
use crate::repositories::rate_limiter::RateLimiterRepository;

/// Open the persisted rate limiter against the active database file.
pub fn open_persisted_rate_limiter(db_path: &Path) -> AppResult<RateLimiter> {
    let rl_conn = rusqlite::Connection::open(db_path).map_err(|e| {
        AppError::Internal(format!(
            "Failed to open rate limiter database connection: {e}"
        ))
    })?;
    apply_pragma_settings(&rl_conn)
        .map_err(|e| AppError::Internal(format!("Failed to apply rate limiter pragmas: {e}")))?;
    let store = Box::new(RateLimiterRepository::new(rl_conn));
    Ok(RateLimiter::with_store(store, 300, 5))
}
