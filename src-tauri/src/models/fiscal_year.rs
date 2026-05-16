//! Fiscal year persistence models (schema foundation only).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FiscalYearStatus {
    pub year: i32,
    pub status: String,
    pub opened_at: Option<String>,
    pub closed_at: Option<String>,
    pub closed_by: Option<String>,
    pub archived: bool,
}
