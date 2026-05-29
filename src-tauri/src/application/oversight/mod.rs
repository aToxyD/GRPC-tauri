/// Oversight Engine — deterministic KPI computation layer.
///
/// All metrics are pure deterministic projections over report outputs.
/// No SQL, no mutations, no events, no side effects.
pub mod anomalies;
pub mod benchmarks;
pub mod context;
pub mod metrics;
pub mod types;

pub use anomalies::*;
pub use benchmarks::*;
pub use context::ReportsContext;
pub use metrics::*;
pub use types::*;
