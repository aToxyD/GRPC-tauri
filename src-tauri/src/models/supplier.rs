//! Supplier Models
//!
//! Supplier is a first-class procurement entity (ADR-0055 / SEC-087-F).
//! UNIT <-> Supplier is M:N via `unit_suppliers`; `units.supplier_id` is
//! deliberately absent.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Supplier entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Supplier {
    pub id: String,
    pub name: String,
    pub contact_info: Option<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

/// Request to create a supplier (WILAYA only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSupplierRequest {
    pub name: String,
    pub contact_info: Option<String>,
}

/// Request to update a supplier (WILAYA only). Renaming a supplier must NOT
/// rewrite historical `supplier_orders.supplier_name` snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSupplierRequest {
    pub id: String,
    pub name: String,
    pub contact_info: Option<String>,
}

/// Associate a UNIT with a supplier (WILAYA only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssociateUnitSupplierRequest {
    pub unit_id: String,
    pub supplier_id: String,
}

/// Disable / re-enable a supplier (WILAYA only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetSupplierActiveRequest {
    pub supplier_id: String,
    pub active: bool,
}
