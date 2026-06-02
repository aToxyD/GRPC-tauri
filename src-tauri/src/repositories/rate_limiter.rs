use crate::domain::ports::rate_limiter_store::{PersistedAttemptInfo, RateLimiterStore};
use rusqlite::{params, Connection};
use std::sync::Mutex;

/// SQLite-backed rate limiter store.
///
/// ARCHITECTURE: Owns its own `Connection` (not via `DbExecutor`) because
/// the rate limiter lives for the entire application lifetime and cannot
/// borrow from the main database handle. Uses `Mutex<Connection>` to provide
/// interior mutability (Connection is !Sync).
pub struct RateLimiterRepository {
    conn: Mutex<Connection>,
}

impl RateLimiterRepository {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }
}

impl RateLimiterStore for RateLimiterRepository {
    fn get_attempt(&self, key: &str) -> Result<Option<PersistedAttemptInfo>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT count, successful_count, total_failed_count, first_attempt_at, last_attempt_at \
                 FROM rate_limiter_attempts WHERE key = ?1",
            )
            .map_err(|e| e.to_string())?;

        let mut rows = stmt.query(params![key]).map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => Ok(Some(PersistedAttemptInfo {
                count: row.get(0).map_err(|e| e.to_string())?,
                successful_count: row.get(1).map_err(|e| e.to_string())?,
                total_failed_count: row.get(2).map_err(|e| e.to_string())?,
                first_attempt_at: row.get(3).map_err(|e| e.to_string())?,
                last_attempt_at: row.get(4).map_err(|e| e.to_string())?,
            })),
            None => Ok(None),
        }
    }

    fn upsert_attempt(&self, key: &str, info: &PersistedAttemptInfo) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO rate_limiter_attempts (key, count, successful_count, total_failed_count, first_attempt_at, last_attempt_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(key) DO UPDATE SET \
                 count = excluded.count, \
                 successful_count = excluded.successful_count, \
                 total_failed_count = excluded.total_failed_count, \
                 first_attempt_at = excluded.first_attempt_at, \
                 last_attempt_at = excluded.last_attempt_at",
            params![
                key,
                info.count,
                info.successful_count,
                info.total_failed_count,
                info.first_attempt_at,
                info.last_attempt_at,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn delete_key(&self, key: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM rate_limiter_attempts WHERE key = ?1",
            params![key],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn get_all_attempts(&self) -> Result<Vec<(String, PersistedAttemptInfo)>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT key, count, successful_count, total_failed_count, first_attempt_at, last_attempt_at \
                 FROM rate_limiter_attempts ORDER BY key",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                let key: String = row.get(0)?;
                let info = PersistedAttemptInfo {
                    count: row.get(1)?,
                    successful_count: row.get(2)?,
                    total_failed_count: row.get(3)?,
                    first_attempt_at: row.get(4)?,
                    last_attempt_at: row.get(5)?,
                };
                Ok((key, info))
            })
            .map_err(|e| e.to_string())?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
        Ok(result)
    }

    fn cleanup_old_entries(&self, window_secs: u64) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let cutoff = now.saturating_sub(window_secs as i64);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM rate_limiter_attempts WHERE first_attempt_at < ?1",
            params![cutoff],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
