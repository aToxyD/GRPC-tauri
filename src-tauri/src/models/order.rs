//! Order Models
//!
//! Supplier orders and order items.
//!
//! ADR-0055 / SEC-087-F: a SupplierOrder is bound to exactly ONE supplier
//! (`supplier_id`), determined by the backend resolver — never by the caller.
//! `supplier_name` is an immutable historical snapshot captured at creation.
//! Per-item `unit_price` is backend-authoritative (contract agreed price);
//! caller-supplied prices are never accepted.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Supplier order header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierOrder {
    pub id: String,
    pub order_date: NaiveDate,
    pub supplier_id: String,
    pub supplier_name: String,
    pub reference_number: Option<String>,
    pub total_amount: Option<f64>,
    pub status: OrderStatus,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub unit_id: Option<String>,
    #[serde(default)]
    pub fiscal_year: Option<i32>,
}

/// Individual item in a supplier order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierOrderItem {
    pub id: String,
    pub order_id: String,
    pub product_id: String,
    pub product_name: String,
    /// Purchase quantity (the agreed unit of the contract).
    pub quantity: f64,
    /// TTC price per purchase unit (backend-authoritative).
    pub unit_price: f64,
    pub total_cost: f64,
    #[serde(default)]
    pub unit_id: Option<String>,
    #[serde(default)]
    pub fiscal_year: Option<i32>,
    /// SEC-087 Phase 5: purchase→consumption unit snapshot captured at order
    /// creation (all-or-nothing; all `None` = legacy pre-Phase-5 item).
    #[serde(default)]
    pub purchase_unit: Option<i32>,
    #[serde(default)]
    pub consumption_unit: Option<i32>,
    #[serde(default)]
    pub conversion_factor: Option<i32>,
    /// Purchase quantity converted to consumption units at creation.
    #[serde(default)]
    pub consumption_quantity: Option<f64>,
}

/// Read-only confirmation input row of ONE supplier order item (SEC-087
/// Phase 5). Carries the persisted unit snapshot so the receipt converts
/// purchase→consumption from the immutable creation-time snapshot only.
#[derive(Debug, Clone)]
pub struct ConfirmationItemRow {
    pub product_id: String,
    /// Purchase quantity (quantity reserved/fulfilled on the allocation).
    pub quantity: f64,
    pub product_name: String,
    /// TTC price per purchase unit.
    pub unit_price: f64,
    pub allocation_id: String,
    pub purchase_unit: Option<i32>,
    pub consumption_unit: Option<i32>,
    pub conversion_factor: Option<i32>,
}

/// Order status lifecycle (phantom `Received`/`Cancelled` states removed:
/// there is no code path producing them).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderStatus {
    Draft,
    Confirmed,
}

impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderStatus::Draft => write!(f, "Draft"),
            OrderStatus::Confirmed => write!(f, "Confirmed"),
        }
    }
}

impl From<String> for OrderStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "Confirmed" => OrderStatus::Confirmed,
            _ => OrderStatus::Draft,
        }
    }
}

impl OrderStatus {
    /// Check if order can be confirmed
    pub fn can_confirm(&self) -> bool {
        matches!(self, OrderStatus::Draft)
    }

    /// Get Arabic display name
    pub fn display_arabic(&self) -> &'static str {
        match self {
            OrderStatus::Draft => "مسودة",
            OrderStatus::Confirmed => "مؤكد",
        }
    }
}

/// Request to create a new order. The caller supplies only products and
/// quantities; the backend resolves supplier + authoritative price.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrderRequest {
    pub reference_number: Option<String>,
    pub items: Vec<OrderItemInput>,
}

/// Request to update a draft order (before confirmation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOrderRequest {
    pub id: String,
    pub reference_number: Option<String>,
    pub items: Vec<OrderItemInput>,
}

/// Input for a single order item (product + quantity only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItemInput {
    pub product_id: String,
    pub quantity: f64,
}

impl OrderItemInput {
    /// Validate item has positive values
    pub fn is_valid(&self) -> bool {
        !self.product_id.is_empty() && self.quantity > 0.0
    }
}

/// Validate all order items
pub fn validate_order_items(items: &[OrderItemInput]) -> Result<(), String> {
    if items.is_empty() {
        return Err("Order must have at least one item".to_string());
    }

    for (i, item) in items.iter().enumerate() {
        if !item.is_valid() {
            return Err(format!(
                "Invalid item at position {}: product_id={}, qty={}",
                i, item.product_id, item.quantity
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_status_lifecycle() {
        assert!(OrderStatus::Draft.can_confirm());
        assert!(!OrderStatus::Confirmed.can_confirm());
    }

    #[test]
    fn test_validate_order_items() {
        let valid_items = vec![OrderItemInput {
            product_id: "P001".to_string(),
            quantity: 10.0,
        }];
        assert!(validate_order_items(&valid_items).is_ok());

        let empty: Vec<OrderItemInput> = vec![];
        assert!(validate_order_items(&empty).is_err());
    }
}
