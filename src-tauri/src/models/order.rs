//! Order Models
//!
//! Supplier orders and order items

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Supplier order header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierOrder {
    pub id: String,
    pub order_date: NaiveDate,
    pub supplier_name: String,
    pub reference_number: Option<String>,
    pub total_amount: Option<f64>,
    pub status: OrderStatus,
    pub created_at: DateTime<Utc>,
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
    pub quantity: f64,
    pub unit_price: f64,
    pub total_cost: f64,
}

/// Order status lifecycle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderStatus {
    Draft,
    Confirmed,
    Received,
    Cancelled,
}

impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderStatus::Draft => write!(f, "Draft"),
            OrderStatus::Confirmed => write!(f, "Confirmed"),
            OrderStatus::Received => write!(f, "Received"),
            OrderStatus::Cancelled => write!(f, "Cancelled"),
        }
    }
}

impl From<String> for OrderStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "Confirmed" => OrderStatus::Confirmed,
            "Received" => OrderStatus::Received,
            "Cancelled" => OrderStatus::Cancelled,
            _ => OrderStatus::Draft,
        }
    }
}

impl OrderStatus {
    /// Check if order can be confirmed
    pub fn can_confirm(&self) -> bool {
        matches!(self, OrderStatus::Draft)
    }

    /// Check if order can be cancelled
    pub fn can_cancel(&self) -> bool {
        matches!(self, OrderStatus::Draft | OrderStatus::Confirmed)
    }

    /// Check if order can receive stock
    pub fn can_receive(&self) -> bool {
        matches!(self, OrderStatus::Confirmed)
    }

    /// Get Arabic display name
    pub fn display_arabic(&self) -> &'static str {
        match self {
            OrderStatus::Draft => "مسودة",
            OrderStatus::Confirmed => "مؤكد",
            OrderStatus::Received => "مستلم",
            OrderStatus::Cancelled => "ملغي",
        }
    }
}

/// Request to create a new order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrderRequest {
    pub supplier_name: String,
    pub reference_number: Option<String>,
    pub items: Vec<OrderItemInput>,
}

/// Request to update a draft order (before confirmation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOrderRequest {
    pub id: String,
    pub supplier_name: String,
    pub reference_number: Option<String>,
    pub items: Vec<OrderItemInput>,
}

/// Input for a single order item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItemInput {
    pub product_id: String,
    pub quantity: f64,
    pub unit_price: f64,
}

impl OrderItemInput {
    /// Calculate total cost for this item
    pub fn total_cost(&self) -> f64 {
        self.quantity * self.unit_price
    }

    /// Validate item has positive values
    pub fn is_valid(&self) -> bool {
        !self.product_id.is_empty() && self.quantity > 0.0 && self.unit_price >= 0.0
    }
}

/// Calculate total order amount from items
pub fn calculate_order_total(items: &[OrderItemInput]) -> f64 {
    items.iter().map(|item| item.total_cost()).sum()
}

/// Validate all order items
pub fn validate_order_items(items: &[OrderItemInput]) -> Result<(), String> {
    if items.is_empty() {
        return Err("Order must have at least one item".to_string());
    }

    for (i, item) in items.iter().enumerate() {
        if !item.is_valid() {
            return Err(format!(
                "Invalid item at position {}: product_id={}, qty={}, price={}",
                i, item.product_id, item.quantity, item.unit_price
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
        assert!(OrderStatus::Confirmed.can_receive());
        assert!(!OrderStatus::Draft.can_receive());
    }

    #[test]
    fn test_order_item_total() {
        let item = OrderItemInput {
            product_id: "P001".to_string(),
            quantity: 10.0,
            unit_price: 5.5,
        };
        assert_eq!(item.total_cost(), 55.0);
    }

    #[test]
    fn test_validate_order_items() {
        let valid_items = vec![OrderItemInput {
            product_id: "P001".to_string(),
            quantity: 1.0,
            unit_price: 10.0,
        }];
        assert!(validate_order_items(&valid_items).is_ok());

        let empty: Vec<OrderItemInput> = vec![];
        assert!(validate_order_items(&empty).is_err());
    }
}
