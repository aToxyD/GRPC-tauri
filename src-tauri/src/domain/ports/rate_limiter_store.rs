/// Port trait for rate limiter storage backend.
///
/// ARCHITECTURE: Domain port — implementations live in infrastructure/repositories.
use std::sync::Mutex;

/// Serializable attempt info for storage backends.
#[derive(Debug, Clone)]
pub struct PersistedAttemptInfo {
    pub count: u32,
    pub successful_count: u32,
    pub total_failed_count: u32,
    /// Unix epoch seconds of first failed attempt in the current window.
    pub first_attempt_at: i64,
    /// Unix epoch seconds of most recent attempt.
    pub last_attempt_at: i64,
}

pub trait RateLimiterStore: Send + Sync {
    fn get_attempt(&self, key: &str) -> Result<Option<PersistedAttemptInfo>, String>;
    fn upsert_attempt(&self, key: &str, info: &PersistedAttemptInfo) -> Result<(), String>;
    fn delete_key(&self, key: &str) -> Result<(), String>;
    fn get_all_attempts(&self) -> Result<Vec<(String, PersistedAttemptInfo)>, String>;
    fn cleanup_old_entries(&self, window_secs: u64) -> Result<(), String>;
    /// Checkpoint WAL before shutdown to ensure data integrity.
    fn checkpoint(&self) -> Result<(), String> {
        Ok(())
    }
}

/// In-memory implementation for tests (preserves existing unit test behaviour).
pub struct InMemoryRateLimiterStore {
    map: Mutex<std::collections::HashMap<String, PersistedAttemptInfo>>,
}

impl InMemoryRateLimiterStore {
    pub fn new() -> Self {
        Self {
            map: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for InMemoryRateLimiterStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiterStore for InMemoryRateLimiterStore {
    fn get_attempt(&self, key: &str) -> Result<Option<PersistedAttemptInfo>, String> {
        let map = self.map.lock().map_err(|e| e.to_string())?;
        Ok(map.get(key).cloned())
    }

    fn upsert_attempt(&self, key: &str, info: &PersistedAttemptInfo) -> Result<(), String> {
        let mut map = self.map.lock().map_err(|e| e.to_string())?;
        map.insert(key.to_string(), info.clone());
        Ok(())
    }

    fn delete_key(&self, key: &str) -> Result<(), String> {
        let mut map = self.map.lock().map_err(|e| e.to_string())?;
        map.remove(key);
        Ok(())
    }

    fn get_all_attempts(&self) -> Result<Vec<(String, PersistedAttemptInfo)>, String> {
        let map = self.map.lock().map_err(|e| e.to_string())?;
        let mut items: Vec<_> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(items)
    }

    fn cleanup_old_entries(&self, window_secs: u64) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let mut map = self.map.lock().map_err(|e| e.to_string())?;
        map.retain(|_, info| {
            let elapsed = now.saturating_sub(info.first_attempt_at) as u64;
            elapsed <= window_secs
        });
        Ok(())
    }
}
