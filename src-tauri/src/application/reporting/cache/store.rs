use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::key::CacheKey;

fn timestamp_secs() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedReport {
    pub key: CacheKey,
    pub report_slug: String,
    pub report_version: u32,
    pub fiscal_scope: Option<i32>,
    pub created_at: String,
    pub last_accessed_at: String,
    pub access_count: u64,
    pub payload_json: serde_json::Value,
}

impl CachedReport {
    pub fn new(
        key: CacheKey,
        report_slug: String,
        report_version: u32,
        fiscal_scope: Option<i32>,
        payload_json: serde_json::Value,
    ) -> Self {
        let now = timestamp_secs();
        Self {
            key,
            report_slug,
            report_version,
            fiscal_scope,
            created_at: now.clone(),
            last_accessed_at: now,
            access_count: 0,
            payload_json,
        }
    }

    pub fn touch(&mut self) {
        self.last_accessed_at = timestamp_secs();
        self.access_count += 1;
    }
}

type CacheMap = HashMap<CacheKey, CachedReport>;

#[derive(Debug, Clone)]
pub struct CacheStore {
    inner: Arc<RwLock<CacheMap>>,
}

impl CacheStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(CacheMap::new())),
        }
    }

    pub fn get(&self, key: &CacheKey) -> Option<CachedReport> {
        let mut map = self.inner.write().ok()?;
        if let Some(entry) = map.get_mut(key) {
            entry.touch();
            Some(entry.clone())
        } else {
            None
        }
    }

    pub fn insert(&self, entry: CachedReport) {
        if let Ok(mut map) = self.inner.write() {
            map.insert(entry.key.clone(), entry);
        }
    }

    pub fn remove(&self, key: &CacheKey) -> Option<CachedReport> {
        self.inner.write().ok().and_then(|mut map| map.remove(key))
    }

    pub fn remove_by_slug(&self, slug: &str) -> Vec<CacheKey> {
        let mut map = match self.inner.write() {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        let mut removed = Vec::new();
        map.retain(|k, v| {
            if v.report_slug == slug {
                removed.push(k.clone());
                false
            } else {
                true
            }
        });
        removed
    }

    pub fn remove_by_fiscal_year(&self, year: i32) -> Vec<CacheKey> {
        let mut map = match self.inner.write() {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        let mut removed = Vec::new();
        map.retain(|k, v| {
            if v.fiscal_scope == Some(year) {
                removed.push(k.clone());
                false
            } else {
                true
            }
        });
        removed
    }

    pub fn contains(&self, key: &CacheKey) -> bool {
        self.inner
            .read()
            .ok()
            .map(|m| m.contains_key(key))
            .unwrap_or(false)
    }

    pub fn len(&self) -> usize {
        self.inner.read().ok().map(|m| m.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn keys(&self) -> Vec<CacheKey> {
        self.inner
            .read()
            .ok()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn entries(&self) -> Vec<CachedReport> {
        self.inner
            .read()
            .ok()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut map) = self.inner.write() {
            map.clear();
        }
    }
}

impl Default for CacheStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_entry(key: &CacheKey) -> CachedReport {
        CachedReport::new(
            key.clone(),
            "test-report".into(),
            1,
            Some(2024),
            json!({"data": "value"}),
        )
    }

    #[test]
    fn store_insert_and_get() {
        let store = CacheStore::new();
        let key = CacheKey::new("test", 1, "{}", Some(2024));
        store.insert(sample_entry(&key));

        let retrieved = store.get(&key);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().report_slug, "test-report");
    }

    #[test]
    fn store_get_miss_returns_none() {
        let store = CacheStore::new();
        let key = CacheKey::new("test", 1, "{}", Some(2024));
        assert!(store.get(&key).is_none());
    }

    #[test]
    fn store_remove_evicts_entry() {
        let store = CacheStore::new();
        let key = CacheKey::new("test", 1, "{}", Some(2024));
        store.insert(sample_entry(&key));
        assert!(store.contains(&key));

        let removed = store.remove(&key);
        assert!(removed.is_some());
        assert!(!store.contains(&key));
    }

    #[test]
    fn store_remove_by_slug() {
        let store = CacheStore::new();
        let key1 = CacheKey::new("a", 1, "{}", Some(2024));
        let key2 = CacheKey::new("b", 1, "{}", Some(2024));
        store.insert(CachedReport::new(
            key1.clone(),
            "slug-a".into(),
            1,
            Some(2024),
            json!(1),
        ));
        store.insert(CachedReport::new(
            key2.clone(),
            "slug-b".into(),
            1,
            Some(2024),
            json!(2),
        ));

        let removed = store.remove_by_slug("slug-a");
        assert_eq!(removed.len(), 1);
        assert!(!store.contains(&key1));
        assert!(store.contains(&key2));
    }

    #[test]
    fn store_remove_by_fiscal_year() {
        let store = CacheStore::new();
        let key1 = CacheKey::new("r", 1, "{}", Some(2024));
        let key2 = CacheKey::new("r", 1, "{}", Some(2025));
        store.insert(sample_entry(&key1));
        store.insert(CachedReport::new(
            key2.clone(),
            "test-report".into(),
            1,
            Some(2025),
            json!({"y": 2025}),
        ));

        let removed = store.remove_by_fiscal_year(2024);
        assert_eq!(removed.len(), 1);
        assert!(!store.contains(&key1));
        assert!(store.contains(&key2));
    }

    #[test]
    fn store_access_count_increments() {
        let store = CacheStore::new();
        let key = CacheKey::new("test", 1, "{}", Some(2024));
        store.insert(sample_entry(&key));

        let _ = store.get(&key);
        let _ = store.get(&key);
        let cached = store.get(&key).unwrap();
        assert_eq!(cached.access_count, 3);
    }

    #[test]
    fn store_len_and_is_empty() {
        let store = CacheStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        let key = CacheKey::new("test", 1, "{}", Some(2024));
        store.insert(sample_entry(&key));
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn store_clear_removes_all() {
        let store = CacheStore::new();
        store.insert(sample_entry(&CacheKey::new("a", 1, "{}", None)));
        store.insert(sample_entry(&CacheKey::new("b", 1, "{}", None)));
        assert_eq!(store.len(), 2);

        store.clear();
        assert!(store.is_empty());
    }

    #[test]
    fn store_entries_returns_all() {
        let store = CacheStore::new();
        store.insert(sample_entry(&CacheKey::new("a", 1, "{}", None)));
        store.insert(sample_entry(&CacheKey::new("b", 1, "{}", None)));

        let entries = store.entries();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn store_get_then_insert_updates_access_time() {
        let store = CacheStore::new();
        let key = CacheKey::new("t", 1, "{}", None);
        store.insert(sample_entry(&key));

        let before = store.get(&key).unwrap();
        assert_eq!(before.access_count, 1);
        let after = store.get(&key).unwrap();
        assert_eq!(after.access_count, 2);
    }

    #[test]
    fn store_remove_non_existent_key() {
        let store = CacheStore::new();
        let key = CacheKey::new("nonexistent", 1, "{}", None);
        let removed = store.remove(&key);
        assert!(removed.is_none());
    }

    #[test]
    fn store_payload_json_round_trip() {
        let store = CacheStore::new();
        let key = CacheKey::new("rt", 1, "{}", None);
        let payload = json!({"report": "data", "values": [1, 2, 3], "nested": {"a": "b"}});

        store.insert(CachedReport::new(
            key.clone(),
            "round-trip".into(),
            1,
            None,
            payload.clone(),
        ));

        let cached = store.get(&key).unwrap();
        assert_eq!(cached.payload_json, payload);
        assert_eq!(cached.report_slug, "round-trip");
    }
}
