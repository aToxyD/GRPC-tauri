//! Phase 5.A — Reporting Cache Runtime
//!
//! Deterministic, synchronous, in-process report cache with
//! DomainEvent-based semantic invalidation.
//!
//! Architectural constraints:
//! - No SQL (delegated to repositories via report compute)
//! - No repository imports (cache knows only CacheKey + DomainEvent)
//! - No mutation outside CacheStore
//! - No chrono::Utc::now() (uses SystemTime for access tracking only)
//! - No TTL-based eviction (only semantic invalidation)
//! - No filesystem persistence (in-memory only)

pub mod invalidation;
pub mod key;
pub mod runtime;
pub mod statistics;
pub mod store;

pub use invalidation::InvalidationKind;
pub use runtime::ReportCacheRuntime;
pub use statistics::CacheStatistics;
pub use store::{CacheStore, CachedReport};


