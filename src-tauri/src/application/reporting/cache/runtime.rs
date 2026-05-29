use serde_json::Value;

use crate::application::reporting::CacheKey;
use crate::domain::events::DomainEvent;

use super::invalidation::{self, InvalidationKind};
use super::statistics::CacheStatistics;
use super::store::{CacheStore, CachedReport};

#[derive(Debug, Clone)]
pub struct InvalidationResult {
    pub removed_keys: Vec<CacheKey>,
}

impl InvalidationResult {
    pub fn is_empty(&self) -> bool {
        self.removed_keys.is_empty()
    }

    pub fn len(&self) -> usize {
        self.removed_keys.len()
    }
}

#[derive(Debug, Clone)]
pub struct ReportCacheRuntime {
    store: CacheStore,
}

impl ReportCacheRuntime {
    pub fn new() -> Self {
        Self {
            store: CacheStore::new(),
        }
    }

    pub fn store(&self) -> &CacheStore {
        &self.store
    }

    pub fn get(&self, key: &CacheKey) -> Option<CachedReport> {
        self.store.get(key)
    }

    pub fn insert(
        &self,
        key: CacheKey,
        report_slug: &str,
        report_version: u32,
        fiscal_scope: Option<i32>,
        payload_json: Value,
    ) {
        let entry = CachedReport::new(
            key,
            report_slug.to_string(),
            report_version,
            fiscal_scope,
            payload_json,
        );
        self.store.insert(entry);
    }

    pub fn invalidate(
        &self,
        event: &DomainEvent,
        is_year_closed: &dyn Fn(i32) -> bool,
    ) -> InvalidationResult {
        let kind = invalidation::invalidation_kind(event);
        let entries = self.store.entries();
        let mut removed_keys = Vec::new();

        for entry in entries {
            if entry.fiscal_scope.is_some_and(is_year_closed) {
                continue;
            }

            let should_remove = match &kind {
                InvalidationKind::Slugs(slugs) => slugs.contains(&entry.report_slug.as_str()),
                InvalidationKind::AllOpenFiscalYear => {
                    entry.fiscal_scope.is_none_or(|y| !is_year_closed(y))
                }
            };

            if should_remove {
                self.store.remove(&entry.key);
                removed_keys.push(entry.key.clone());
            }
        }

        InvalidationResult { removed_keys }
    }

    pub fn invalidate_by_slug(
        &self,
        slug: &str,
        is_year_closed: &dyn Fn(i32) -> bool,
    ) -> InvalidationResult {
        let entries = self.store.entries();
        let mut removed_keys = Vec::new();

        for entry in entries {
            if entry.report_slug != slug {
                continue;
            }

            if entry.fiscal_scope.is_some_and(is_year_closed) {
                continue;
            }

            self.store.remove(&entry.key);
            removed_keys.push(entry.key.clone());
        }

        InvalidationResult { removed_keys }
    }

    pub fn invalidate_by_fiscal_year(
        &self,
        year: i32,
        is_year_closed: &dyn Fn(i32) -> bool,
    ) -> InvalidationResult {
        let entries = self.store.entries();
        let mut removed_keys = Vec::new();

        for entry in entries {
            if entry.fiscal_scope != Some(year) {
                continue;
            }

            if is_year_closed(year) {
                continue;
            }

            self.store.remove(&entry.key);
            removed_keys.push(entry.key.clone());
        }

        InvalidationResult { removed_keys }
    }

    pub fn stats(&self) -> CacheStatistics {
        CacheStatistics::compute(&self.store)
    }

    pub fn len(&self) -> usize {
        self.store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    pub fn clear(&self) {
        self.store.clear();
    }
}

impl Default for ReportCacheRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::reporting::CacheKey;
    use crate::domain::events::DomainEvent;
    use serde_json::json;

    fn closed_years(years: &[i32]) -> impl Fn(i32) -> bool + '_ {
        |y| years.contains(&y)
    }

    fn runtime_with_entry(
        runtime: &ReportCacheRuntime,
        slug: &str,
        year: Option<i32>,
    ) -> CacheKey {
        let key = CacheKey::new(slug, 1, "{}", year);
        runtime.insert(key.clone(), slug, 1, year, json!("data"));
        key
    }

    #[test]
    fn runtime_insert_and_get() {
        let runtime = ReportCacheRuntime::new();
        let key = CacheKey::new("test", 1, "{}", Some(2024));
        runtime.insert(key.clone(), "test", 1, Some(2024), json!("data"));

        let cached = runtime.get(&key);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().report_slug, "test");
    }

    #[test]
    fn runtime_get_miss_returns_none() {
        let runtime = ReportCacheRuntime::new();
        let key = CacheKey::new("test", 1, "{}", None);
        assert!(runtime.get(&key).is_none());
    }

    #[test]
    fn runtime_invalidate_stock_event() {
        let runtime = ReportCacheRuntime::new();
        let key = runtime_with_entry(&runtime, "inventory-valuation", Some(2024));

        let event = DomainEvent::StockMovementRecorded {
            movement_id: "m1".into(),
            account: "OUT".into(),
            actor_user_id: "u1".into(),
        };
        let result = runtime.invalidate(&event, &closed_years(&[]));
        assert!(!result.is_empty());
        assert!(runtime.get(&key).is_none());
    }

    #[test]
    fn runtime_invalidate_by_slug() {
        let runtime = ReportCacheRuntime::new();
        let key = runtime_with_entry(&runtime, "stock-movement-ledger", None);

        let result = runtime.invalidate_by_slug("stock-movement-ledger", &closed_years(&[]));
        assert_eq!(result.len(), 1);
        assert!(runtime.get(&key).is_none());
    }

    #[test]
    fn runtime_invalidate_by_fiscal_year() {
        let runtime = ReportCacheRuntime::new();
        let key_24 = runtime_with_entry(&runtime, "report", Some(2024));
        let key_25 = runtime_with_entry(&runtime, "report", Some(2025));

        let result = runtime.invalidate_by_fiscal_year(2024, &closed_years(&[]));
        assert_eq!(result.len(), 1);
        assert!(runtime.get(&key_24).is_none());
        assert!(runtime.get(&key_25).is_some());
    }

    #[test]
    fn closed_fiscal_year_entries_never_invalidated() {
        let runtime = ReportCacheRuntime::new();
        let key = runtime_with_entry(&runtime, "inventory-valuation", Some(2024));

        let event = DomainEvent::StockMovementRecorded {
            movement_id: "m1".into(),
            account: "OUT".into(),
            actor_user_id: "u1".into(),
        };
        let result = runtime.invalidate(&event, &closed_years(&[2024]));
        assert!(result.is_empty());
        assert!(runtime.get(&key).is_some());
    }

    #[test]
    fn closed_year_entries_survive_slug_invalidation() {
        let runtime = ReportCacheRuntime::new();
        let key = runtime_with_entry(&runtime, "inventory-valuation", Some(2023));

        let result = runtime.invalidate_by_slug("inventory-valuation", &closed_years(&[2023]));
        assert!(result.is_empty());
        assert!(runtime.get(&key).is_some());
    }

    #[test]
    fn closed_year_entries_survive_fiscal_year_invalidation() {
        let runtime = ReportCacheRuntime::new();
        let key = runtime_with_entry(&runtime, "report", Some(2023));

        let result = runtime.invalidate_by_fiscal_year(2023, &closed_years(&[2023]));
        assert!(result.is_empty());
        assert!(runtime.get(&key).is_some());
    }

    #[test]
    fn sync_package_import_invalidates_open_years_only() {
        let runtime = ReportCacheRuntime::new();
        let open_key = runtime_with_entry(&runtime, "inventory-valuation", Some(2024));
        let closed_key = runtime_with_entry(&runtime, "fiscal-year-summary", Some(2023));

        let event = DomainEvent::SyncPackageImported {
            package_id: "pkg1".into(),
            kind: "full".into(),
        };
        let result = runtime.invalidate(&event, &closed_years(&[2023]));
        assert_eq!(result.len(), 1);
        assert!(runtime.get(&open_key).is_none());
        assert!(runtime.get(&closed_key).is_some());
    }

    #[test]
    fn sync_package_invalidates_unscoped_entries() {
        let runtime = ReportCacheRuntime::new();
        let unscoped = runtime_with_entry(&runtime, "stock-movement-ledger", None);
        let scoped_closed = runtime_with_entry(&runtime, "fiscal-year-summary", Some(2023));

        let event = DomainEvent::SyncPackageImported {
            package_id: "pkg1".into(),
            kind: "full".into(),
        };
        let result = runtime.invalidate(&event, &closed_years(&[2023]));
        assert_eq!(result.len(), 1);
        assert!(runtime.get(&unscoped).is_none());
        assert!(runtime.get(&scoped_closed).is_some());
    }

    #[test]
    fn unknown_event_does_not_panic() {
        let runtime = ReportCacheRuntime::new();
        runtime_with_entry(&runtime, "report", None);

        let event = DomainEvent::AuditIntegrityBreach {
            details: "test".into(),
        };
        let result = runtime.invalidate(&event, &closed_years(&[]));
        assert!(result.is_empty());
    }

    #[test]
    fn stats_method_returns_correct_values() {
        let runtime = ReportCacheRuntime::new();
        runtime_with_entry(&runtime, "slug-a", Some(2024));
        runtime_with_entry(&runtime, "slug-a", Some(2025));
        runtime_with_entry(&runtime, "slug-b", None);

        let stats = runtime.stats();
        assert_eq!(stats.total_entries, 3);
        assert_eq!(*stats.entries_per_slug.get("slug-a").unwrap(), 2);
        assert_eq!(*stats.entries_per_slug.get("slug-b").unwrap(), 1);
    }

    #[test]
    fn access_count_increments_on_get() {
        let runtime = ReportCacheRuntime::new();
        let key = runtime_with_entry(&runtime, "report", None);

        let _ = runtime.get(&key);
        let _ = runtime.get(&key);
        let cached = runtime.get(&key).unwrap();
        assert_eq!(cached.access_count, 3);
    }

    #[test]
    fn payload_json_round_trip() {
        let runtime = ReportCacheRuntime::new();
        let key = CacheKey::new("test", 1, "{}", None);
        let payload = json!({"answer": 42, "nested": {"key": "value"}});

        runtime.insert(key.clone(), "test", 1, None, payload.clone());
        let cached = runtime.get(&key).unwrap();
        assert_eq!(cached.payload_json, payload);
    }

    #[test]
    fn runtime_len_and_is_empty() {
        let runtime = ReportCacheRuntime::new();
        assert!(runtime.is_empty());
        assert_eq!(runtime.len(), 0);

        runtime_with_entry(&runtime, "report", None);
        assert!(!runtime.is_empty());
        assert_eq!(runtime.len(), 1);
    }

    #[test]
    fn runtime_clear() {
        let runtime = ReportCacheRuntime::new();
        runtime_with_entry(&runtime, "a", None);
        runtime_with_entry(&runtime, "b", None);
        assert_eq!(runtime.len(), 2);

        runtime.clear();
        assert!(runtime.is_empty());
    }

    #[test]
    fn invalidation_result_is_empty() {
        let runtime = ReportCacheRuntime::new();
        let event = DomainEvent::AuditEventWritten { audit_event_id: 1 };
        let result = runtime.invalidate(&event, &closed_years(&[]));
        assert!(result.is_empty());
    }

    #[test]
    fn deterministic_invalidation_depends_only_on_event() {
        let event = DomainEvent::StockMovementRecorded {
            movement_id: "m1".into(),
            account: "OUT".into(),
            actor_user_id: "u1".into(),
        };

        let runtime1 = ReportCacheRuntime::new();
        runtime_with_entry(&runtime1, "inventory-valuation", Some(2024));
        let r1 = runtime1.invalidate(&event, &closed_years(&[]));

        let runtime2 = ReportCacheRuntime::new();
        runtime_with_entry(&runtime2, "inventory-valuation", Some(2024));
        let r2 = runtime2.invalidate(&event, &closed_years(&[]));

        assert_eq!(r1.len(), r2.len());
    }
}
