//! Unit Models
//!
//! Wilaya units and unit management

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::identity::IdentityCertificate;
use crate::models::user::UserExport;

/// Unit (military unit) under a Wilaya
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unit {
    pub id: String,
    pub code: String,
    pub name: String,
    pub wilaya_code: String,
    pub user_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Unit {
    /// Create display name with code
    pub fn display_name(&self) -> String {
        format!("{} - {}", self.code, self.name)
    }

    /// Check if unit has an associated user
    pub fn has_user(&self) -> bool {
        self.user_id.is_some()
    }
}

/// Request to create a new unit.
///
/// ADR-0052: the operator username is NOT caller-supplied — the backend
/// derives the canonical `user` server-side (node-scoped to the unit code).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUnitRequest {
    pub code: String,
    pub name: String,
    pub password: String,
}

/// Unit export package for node synchronization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitNodePackage {
    pub unit: Unit,
    pub user: UserExport,
    /// WILAYA-signed UNIT identity certificate embedded at export time
    /// (ADR-0044 packaged-identity bootstrap). Absent on legacy packages and
    /// on legacy imports — serde-compatible (`None` = old shape).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_certificate: Option<IdentityCertificate>,
    /// UNIT Ed25519 private key embedded at export time (ADR-0044 packaged
    /// identity bootstrap). Transported ONLY inside the encrypted `.unit`;
    /// never written to the WILAYA NodeKeyStore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_private_key: Option<Vec<u8>>,
}

/// Monthly inventory snapshot for a unit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitMonthlySnapshot {
    pub id: String,
    pub unit_id: String,
    pub unit_name: String,
    pub report_year: i32,
    pub report_month: u32,
    pub product_id: String,
    pub product_name: String,

    // Balance equation: Opening + IN - OUT
    pub opening_stock: f64,
    pub total_in: f64,
    pub total_out: f64,
    pub computed_closing: f64,
    pub reported_closing: f64,

    // Anomaly detection
    pub variance: f64,
    pub has_balance_anomaly: bool,
    pub avg_consumption_3months: Option<f64>,
    pub has_consumption_anomaly: bool,

    // Data freshness
    pub is_stale: bool,

    pub computed_at: String,
}

impl UnitMonthlySnapshot {
    /// Calculate variance percentage
    pub fn variance_percentage(&self) -> f64 {
        if self.reported_closing.abs() > f64::EPSILON {
            (self.variance / self.reported_closing) * 100.0
        } else {
            0.0
        }
    }

    /// Check if snapshot has any anomalies
    pub fn has_any_anomaly(&self) -> bool {
        self.has_balance_anomaly || self.has_consumption_anomaly || self.is_stale
    }
}

/// Unit inventory view for a specific month
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitInventoryView {
    pub unit_id: String,
    pub unit_name: String,
    pub report_year: i32,
    pub report_month: u32,
    pub items: Vec<UnitMonthlySnapshot>,
    pub total_products: usize,
    pub balance_anomaly_count: usize,
    pub consumption_anomaly_count: usize,
    pub stale_count: usize,
    pub has_any_anomaly: bool,
    pub computed_at: String,
}

/// Result of computing a snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeSnapshotResult {
    pub unit_id: String,
    pub unit_name: String,
    pub year: i32,
    pub month: u32,
    pub products_computed: usize,
    pub balance_anomalies: usize,
    pub consumption_anomalies: usize,
    pub already_existed: bool,
}

impl Default for UnitMonthlySnapshot {
    fn default() -> Self {
        Self {
            id: String::new(),
            unit_id: String::new(),
            unit_name: String::new(),
            report_year: 0,
            report_month: 0,
            product_id: String::new(),
            product_name: String::new(),
            opening_stock: 0.0,
            total_in: 0.0,
            total_out: 0.0,
            computed_closing: 0.0,
            reported_closing: 0.0,
            variance: 0.0,
            has_balance_anomaly: false,
            avg_consumption_3months: None,
            has_consumption_anomaly: false,
            is_stale: false,
            computed_at: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_display_name() {
        let unit = Unit {
            id: "U001".to_string(),
            code: "123456".to_string(),
            name: "Test Unit".to_string(),
            wilaya_code: "01".to_string(),
            user_id: None,
            created_at: Utc::now(),
        };
        assert_eq!(unit.display_name(), "123456 - Test Unit");
    }

    #[test]
    fn test_snapshot_variance_percentage() {
        let snapshot = UnitMonthlySnapshot {
            variance: 10.0,
            reported_closing: 100.0,
            ..Default::default()
        };
        assert_eq!(snapshot.variance_percentage(), 10.0);
    }

    #[test]
    fn test_snapshot_anomaly_detection() {
        let snapshot = UnitMonthlySnapshot {
            has_balance_anomaly: true,
            has_consumption_anomaly: false,
            is_stale: false,
            ..Default::default()
        };
        assert!(snapshot.has_any_anomaly());
    }
}
