//! Logical kinds of interchange packages between nodes (not transport format).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPackageKind {
    Products,
    DailyReports,
    MonthlySummary,
    IdentityAccess,
}
