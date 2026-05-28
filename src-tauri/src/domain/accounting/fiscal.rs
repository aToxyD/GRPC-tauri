use serde::{Deserialize, Serialize};

/// Lifecycle status of a fiscal year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FiscalStatus {
    Open,
    Closed,
    Archived,
}
