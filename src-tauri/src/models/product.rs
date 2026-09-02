//! Product Models
//!
//! Products, inventory stock, and product-related types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Product definition with reference pricing only (ADR-0055 / SEC-087-F).
/// `base_price` is `ReferencePrice`: informational/default, NEVER authoritative
/// order pricing and NEVER a fallback when no contract agreed price exists.
/// Product-level TVA and supplier ownership are removed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub base_price: f64,
    pub year: i32,
    pub created_at: DateTime<Utc>,
}

/// Current inventory stock level for a product
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryStock {
    pub id: String,
    pub product_id: String,
    pub product_name: String,
    pub quantity: f64,
    pub unit: String,
    pub last_updated: DateTime<Utc>,
}

/// Request to create a new product
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProductRequest {
    pub name: String,
    pub base_price: f64,
}

/// Request to update an existing product
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProductRequest {
    pub id: String,
    pub name: String,
    pub base_price: f64,
}

/// Stock availability check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockCheckResult {
    pub available: bool,
    pub product_id: String,
    pub product_name: String,
    pub requested: f64,
    pub available_stock: f64,
    pub deficit: f64,
}

impl StockCheckResult {
    /// Create a result indicating sufficient stock
    pub fn sufficient(
        product_id: String,
        product_name: String,
        requested: f64,
        available: f64,
    ) -> Self {
        Self {
            available: true,
            product_id,
            product_name,
            requested,
            available_stock: available,
            deficit: 0.0,
        }
    }

    /// Create a result indicating insufficient stock
    pub fn insufficient(
        product_id: String,
        product_name: String,
        requested: f64,
        available: f64,
    ) -> Self {
        Self {
            available: false,
            product_id,
            product_name,
            requested,
            available_stock: available,
            deficit: requested - available,
        }
    }

    /// Calculate percentage of available stock
    pub fn availability_percentage(&self) -> f64 {
        if self.requested > 0.0 {
            (self.available_stock / self.requested) * 100.0
        } else {
            100.0
        }
    }
}

/// Helper for product export containing sync information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductExportRow {
    pub product: Product,
    pub updated_at: String,
    pub node_id: String,
    pub deleted: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stock_check_sufficient() {
        let result =
            StockCheckResult::sufficient("P001".to_string(), "Product".to_string(), 10.0, 15.0);
        assert!(result.available);
        assert_eq!(result.deficit, 0.0);
        assert_eq!(result.availability_percentage(), 150.0);
    }

    #[test]
    fn test_stock_check_insufficient() {
        let result =
            StockCheckResult::insufficient("P001".to_string(), "Product".to_string(), 20.0, 10.0);
        assert!(!result.available);
        assert_eq!(result.deficit, 10.0);
        assert_eq!(result.availability_percentage(), 50.0);
    }
}
