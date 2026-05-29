use std::collections::HashMap;

use super::store::CacheStore;

#[derive(Debug, Clone, PartialEq)]
pub struct CacheStatistics {
    pub total_entries: usize,
    pub total_access_count: u64,
    pub entries_per_slug: HashMap<String, usize>,
    pub entries_per_fiscal_year: HashMap<Option<i32>, usize>,
    pub slugs: Vec<String>,
}

impl CacheStatistics {
    pub fn compute(store: &CacheStore) -> Self {
        let entries = store.entries();
        let total_entries = entries.len();
        let mut total_access_count: u64 = 0;
        let mut entries_per_slug: HashMap<String, usize> = HashMap::new();
        let mut entries_per_fiscal_year: HashMap<Option<i32>, usize> = HashMap::new();
        let mut slugs: Vec<String> = Vec::new();

        for entry in &entries {
            total_access_count += entry.access_count;
            *entries_per_slug
                .entry(entry.report_slug.clone())
                .or_insert(0) += 1;
            *entries_per_fiscal_year
                .entry(entry.fiscal_scope)
                .or_insert(0) += 1;
            if !slugs.contains(&entry.report_slug) {
                slugs.push(entry.report_slug.clone());
            }
        }

        slugs.sort();

        Self {
            total_entries,
            total_access_count,
            entries_per_slug,
            entries_per_fiscal_year,
            slugs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::reporting::cache::key::CacheKey;
    use crate::application::reporting::cache::store::CachedReport;
    use serde_json::json;

    #[test]
    fn stats_empty_store() {
        let store = CacheStore::new();
        let stats = CacheStatistics::compute(&store);
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.total_access_count, 0);
        assert!(stats.entries_per_slug.is_empty());
        assert!(stats.slugs.is_empty());
    }

    #[test]
    fn stats_tracks_access_count() {
        let store = CacheStore::new();
        let key = CacheKey::new("test", 1, "{}", None);
        store.insert(CachedReport::new(
            key.clone(),
            "slug-a".into(),
            1,
            None,
            json!(1),
        ));

        let _ = store.get(&key);
        let _ = store.get(&key);

        let stats = CacheStatistics::compute(&store);
        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.total_access_count, 2);
    }

    #[test]
    fn stats_entries_per_slug() {
        let store = CacheStore::new();
        store.insert(CachedReport::new(
            CacheKey::new("a", 1, "{}", None),
            "slug-a".into(),
            1,
            None,
            json!(1),
        ));
        store.insert(CachedReport::new(
            CacheKey::new("b", 1, "{}", None),
            "slug-a".into(),
            1,
            None,
            json!(2),
        ));
        store.insert(CachedReport::new(
            CacheKey::new("c", 1, "{}", None),
            "slug-b".into(),
            1,
            None,
            json!(3),
        ));

        let stats = CacheStatistics::compute(&store);
        assert_eq!(stats.total_entries, 3);
        assert_eq!(*stats.entries_per_slug.get("slug-a").unwrap(), 2);
        assert_eq!(*stats.entries_per_slug.get("slug-b").unwrap(), 1);
    }

    #[test]
    fn stats_entries_per_fiscal_year() {
        let store = CacheStore::new();
        store.insert(CachedReport::new(
            CacheKey::new("a", 1, "{}", Some(2024)),
            "r1".into(),
            1,
            Some(2024),
            json!(1),
        ));
        store.insert(CachedReport::new(
            CacheKey::new("b", 1, "{}", Some(2024)),
            "r2".into(),
            1,
            Some(2024),
            json!(2),
        ));
        store.insert(CachedReport::new(
            CacheKey::new("c", 1, "{}", None),
            "r3".into(),
            1,
            None,
            json!(3),
        ));

        let stats = CacheStatistics::compute(&store);
        assert_eq!(*stats.entries_per_fiscal_year.get(&Some(2024)).unwrap(), 2);
        assert_eq!(*stats.entries_per_fiscal_year.get(&None).unwrap(), 1);
    }

    #[test]
    fn stats_deterministic_ordering() {
        let store = CacheStore::new();
        store.insert(CachedReport::new(
            CacheKey::new("z", 1, "{}", None),
            "z-report".into(),
            1,
            None,
            json!(1),
        ));
        store.insert(CachedReport::new(
            CacheKey::new("a", 1, "{}", None),
            "a-report".into(),
            1,
            None,
            json!(2),
        ));

        let stats1 = CacheStatistics::compute(&store);
        let stats2 = CacheStatistics::compute(&store);
        assert_eq!(stats1.slugs, stats2.slugs);
    }
}
