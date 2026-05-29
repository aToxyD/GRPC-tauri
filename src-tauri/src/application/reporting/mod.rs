use crate::repositories::DbExecutor;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub mod cache_key;
pub mod fiscal_year_summary;
pub mod inventory_valuation;
pub mod stock_movement_ledger;

pub use cache_key::CacheKey;
pub use fiscal_year_summary::FiscalYearSummaryReport;
pub use inventory_valuation::InventoryValuationReport;
pub use stock_movement_ledger::StockMovementLedgerReport;

/// All reports in this module must be read-only projections.
/// No mutations, no transaction ownership, no side effects.
pub trait Report {
    type Input: Serialize;
    type Output: Serialize;
    type Error: std::error::Error;

    fn slug() -> &'static str;
    fn version() -> u32;
    fn compute(executor: DbExecutor<'_>, input: Self::Input) -> Result<ReportEnvelope<Self::Output>, Self::Error>;
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

/// Round a monetary value to 2 decimal places (half-to-even).
pub fn round_money(value: f64) -> f64 {
    let scaled = (value * 100.0).round();
    scaled / 100.0
}
