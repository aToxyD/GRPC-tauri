# Report Cache Runtime

## Phase 5.A — In-Process Deterministic Cache

### Architecture

The report cache runtime is a synchronous, in-process write-through cache
implemented as `Arc<RwLock<HashMap<CacheKey, CachedReport>>>`. It stores
serialized `ReportEnvelope<T>` values keyed by a deterministic SHA-256 hash.

### Cache Identity

Every cache entry is identified by a `CacheKey` computed as:

```
SHA256(slug | version | canonical_input_json | fiscal_scope)
```

Where:

- `slug` — `Report::slug()` (unique identifier)
- `version` — `Report::version()` (bumped on semantic change)
- `canonical_input_json` — `serde_json::to_string(&input)` (deterministic JSON)
- `fiscal_scope` — `Some(year)` or `"null"` for unfiltered reports

The same input always produces the same key. No randomness, no timestamps,
no non-deterministic components.

### Module Structure

| File | Responsibility |
|------|---------------|
| `key.rs` | Re-exports `CacheKey`, helper `build_cache_key()` |
| `store.rs` | `CachedReport` struct, `CacheStore` (thread-safe HashMap wrapper) |
| `invalidation.rs` | Pure function mapping `DomainEvent` variants to affected report slugs |
| `runtime.rs` | `ReportCacheRuntime` — get, insert, invalidate, stats |
| `statistics.rs` | `CacheStatistics` — aggregated cache metrics |
| `mod.rs` | Module declaration + `compute_or_get_cached()` helper |

### CachedReport

```rust
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
```

`payload_json` stores the serialized `ReportEnvelope<R::Output>`. Serialization
is deterministic: the same `ReportEnvelope` always produces the same JSON Value.

### ReportCacheRuntime

| Method | Behavior |
|--------|----------|
| `get(key)` | Lookup by key, increment access_count, update last_accessed_at |
| `insert(key, slug, version, fiscal_scope, payload)` | Store new entry |
| `invalidate(event, is_year_closed)` | DomainEvent-based semantic invalidation |
| `invalidate_by_slug(slug, is_year_closed)` | Invalidate all entries for a slug |
| `invalidate_by_fiscal_year(year, is_year_closed)` | Invalidate entries scoped to a year |
| `stats()` | Return `CacheStatistics` |
| `clear()` | Remove all entries |

### compute_or_get_cached

Bridge function that integrates the cache with the Report trait:

```
compute_or_get_cached<R: Report>(
    runtime: &ReportCacheRuntime,
    executor: DbExecutor<'_>,
    input: R::Input,
    fiscal_scope: Option<i32>,
) -> Result<ReportEnvelope<R::Output>, R::Error>
```

Behavior:
1. Compute cache key from slug, version, serialized input, fiscal_scope
2. Look up in cache
3. On hit: deserialize `CachedReport.payload_json` → `ReportEnvelope<R::Output>`,
   update access stats, return cached value
4. On miss: call `R::compute(executor, input)`, serialize result, insert into cache,
   return result

### Closed Fiscal Year Immutability

Cache entries scoped to a closed fiscal year are NEVER invalidated. The runtime
accepts an `is_year_closed` predicate that is checked before each invalidation.
If an entry's `fiscal_scope` year is closed, the entry is preserved regardless
of the invalidation event.

This guarantees that historical reports remain reproducible indefinitely.

### Governance Rationale

- **No TTL**: TTL-based eviction introduces non-determinism. A report could
  disappear from cache due to wall-clock time, making behavior timing-dependent.
- **No LRU**: LRU eviction would make cache hit/miss behavior dependent on access
  patterns, violating reproducibility guarantees.
- **No persistence**: Disk-backed cache introduces serialization compatibility
  issues across versions. In-memory only ensures consistent behavior.
- **No distributed cache**: Distributed state introduces consistency and
  coordination complexity that is unnecessary for a single-instance system.

### Thread Safety

The cache uses `Arc<RwLock<HashMap>>` for thread-safe access. Write locks are
held only during mutation (insert, remove, access_count increment). Reads
(contains, len, keys) use read locks for concurrent access.
