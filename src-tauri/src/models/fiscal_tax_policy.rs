//! Fiscal Tax Policy Models
//!
//! Exactly ONE TVA rate per fiscal year (ADR-0055 / SEC-087-F). WILAYA-
//! controlled; immutable once established for the fiscal year; frozen at
//! fiscal close. Product-level TVA is removed.

use serde::{Deserialize, Serialize};

/// Fiscal-year TVA policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiscalYearTaxPolicy {
    pub fiscal_year: i32,
    pub tva_rate: f64,
    pub frozen: bool,
    pub set_by: String,
    pub created_at: String,
}

/// Request to establish a fiscal-year TVA policy (WILAYA only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTaxPolicyRequest {
    pub fiscal_year: i32,
    pub tva_rate: f64,
}
