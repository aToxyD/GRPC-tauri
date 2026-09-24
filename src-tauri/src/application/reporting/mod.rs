use crate::repositories::DbExecutor;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub mod cache;
pub mod cache_key;
pub mod fiscal_year_summary;
pub mod inventory_valuation;
pub mod stock_movement_ledger;

pub use cache::ReportCacheRuntime;
pub use cache_key::CacheKey;
pub use fiscal_year_summary::FiscalYearSummaryReport;
pub use inventory_valuation::InventoryValuationReport;
pub use stock_movement_ledger::StockMovementLedgerReport;

/// All reports in this module must be read-only projections.
/// No mutations, no transaction ownership, no side effects.
pub trait Report {
    type Input: Serialize;
    type Output: Serialize + DeserializeOwned;
    type Error: std::error::Error;

    fn slug() -> &'static str;
    fn version() -> u32;
    fn compute(
        executor: DbExecutor<'_>,
        input: Self::Input,
    ) -> Result<ReportEnvelope<Self::Output>, Self::Error>;
    fn is_reproducible() -> bool {
        true
    }
}

/// Context metadata attached to every report output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportMetadata {
    pub report_slug: String,
    pub report_version: u32,
    pub computed_at: String,
    pub fiscal_scope: Option<i32>,
    pub database_state_hash: Option<String>,
    pub snapshot_source: Option<String>,
}

impl ReportMetadata {
    pub fn new(slug: &str, version: u32, fiscal_scope: Option<i32>) -> Self {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            report_slug: slug.to_string(),
            report_version: version,
            computed_at: now_secs.to_string(),
            fiscal_scope,
            database_state_hash: None,
            snapshot_source: None,
        }
    }

    pub fn with_state_hash(mut self, hash: String) -> Self {
        self.database_state_hash = Some(hash);
        self
    }

    pub fn with_snapshot_source(mut self, source: String) -> Self {
        self.snapshot_source = Some(source);
        self
    }
}

/// Wrapper produced by every report compute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportEnvelope<T: Serialize> {
    pub metadata: ReportMetadata,
    pub data: T,
}

/// Compute a report or return it from cache.
///
/// 1. Compute cache key from slug, version, serialized input, fiscal_scope
/// 2. Look up in cache — on hit, deserialize and return
/// 3. On miss, compute the report, serialize, insert into cache, return
pub fn compute_or_get_cached<R: Report>(
    runtime: &ReportCacheRuntime,
    executor: DbExecutor<'_>,
    input: R::Input,
    fiscal_scope: Option<i32>,
) -> Result<ReportEnvelope<R::Output>, R::Error> {
    let input_json =
        serde_json::to_string(&input).expect("Report input serialization must not fail");
    let key = CacheKey::new(R::slug(), R::version(), &input_json, fiscal_scope);

    if let Some(cached) = runtime.get(&key) {
        let envelope: ReportEnvelope<R::Output> = serde_json::from_value(cached.payload_json)
            .expect("Cached report deserialization must not fail; version mismatch = key change");
        return Ok(envelope);
    }

    let envelope = R::compute(executor, input)?;

    let payload =
        serde_json::to_value(&envelope).expect("Report envelope serialization must not fail");
    runtime.insert(key, R::slug(), R::version(), fiscal_scope, payload);

    Ok(envelope)
}
