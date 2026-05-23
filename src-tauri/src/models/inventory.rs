//! Inventory Models
//!
//! Stock movements, inventory tracking, and warehouse management

use serde::{Deserialize, Serialize};

/// نوع حركة المخزون
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum StockMovementType {
    In,      // دخول (من طلبية)
    Out,     // خروج (استهلاك)
    Opening, // رصيد افتتاحي
}

impl StockMovementType {
    pub fn as_str(&self) -> &'static str {
        match self {
            StockMovementType::In => "IN",
            StockMovementType::Out => "OUT",
            StockMovementType::Opening => "OPENING",
        }
    }

    pub fn display_arabic(&self) -> &'static str {
        match self {
            StockMovementType::In => "دخول مخزون",
            StockMovementType::Out => "خروج مخزون",
            StockMovementType::Opening => "رصيد افتتاحي",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "IN" => Some(StockMovementType::In),
            "OUT" => Some(StockMovementType::Out),
            "OPENING" => Some(StockMovementType::Opening),
            _ => None,
        }
    }

    /// Check if movement affects stock positively
    pub fn increases_stock(&self) -> bool {
        matches!(self, StockMovementType::In | StockMovementType::Opening)
    }

    /// Check if movement affects stock negatively
    pub fn decreases_stock(&self) -> bool {
        matches!(self, StockMovementType::Out)
    }
}

/// سجل حركة مخزون
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockMovement {
    pub id: String,
    pub product_id: String,
    pub product_name: Option<String>,
    pub movement_type: StockMovementType,
    pub quantity: f64,
    pub balance_before: f64,
    pub balance_after: f64,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub notes: Option<String>,
    pub timestamp: String,
    pub user_id: String,
    pub username: String,
    #[serde(default)]
    pub unit_id: Option<String>,
    pub fiscal_year: Option<i32>,
    #[serde(default)]
    pub unit_cost: Option<f64>,
}

impl StockMovement {
    /// Calculate the change in stock
    pub fn stock_change(&self) -> f64 {
        self.balance_after - self.balance_before
    }

    /// Check if this is a system-generated movement
    pub fn is_system_generated(&self) -> bool {
        self.reference_type
            .as_ref()
            .map(|t| t.starts_with("SYSTEM"))
            .unwrap_or(false)
    }
}

/// بيانات حركة مخزون جديدة
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewStockMovement {
    pub product_id: String,
    pub movement_type: StockMovementType,
    pub quantity: f64,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub notes: Option<String>,
    pub user_id: String,
    pub username: String,
    pub unit_id: Option<String>,
    #[serde(default)]
    pub unit_cost: Option<f64>,
}

/// فلاتر البحث في حركات المخزون
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StockMovementFilters {
    pub product_id: Option<String>,
    pub movement_type: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub unit_id: Option<String>,
}

/// نتيجة استرجاع حركات المخزون
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockMovementResponse {
    pub movements: Vec<StockMovement>,
    pub total_count: i64,
    pub page: usize,
    pub page_size: usize,
    pub has_more: bool,
}

// =============================================================================
// Clean Architecture boundary structs (DB rows / queries)
// =============================================================================

/// Normalized query parameters for stock movement listing (built by service).
#[derive(Debug, Clone, Default)]
pub struct StockMovementQuery {
    pub product_id: Option<String>,
    pub movement_type: Option<String>,
    pub start_timestamp: Option<String>,
    pub end_timestamp: Option<String>,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub unit_id: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

/// Raw DB row mapping for stock movements (no parsing/defaulting here).
#[derive(Debug, Clone)]
pub struct StockMovementDbRow {
    pub id: String,
    pub product_id: String,
    pub product_name: Option<String>,
    pub movement_type: String,
    pub quantity: f64,
    pub balance_before: f64,
    pub balance_after: f64,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub notes: Option<String>,
    pub timestamp: String,
    pub user_id: String,
    pub username: String,
    pub unit_id: Option<String>,
    pub fiscal_year: Option<i32>,
    pub unit_cost: Option<f64>,
}

/// ملخص مخزون منتج
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockSummary {
    pub product_id: String,
    pub product_name: String,
    pub current_quantity: f64,
    pub total_in: f64,
    pub total_out: f64,
    pub last_movement: Option<String>,
    pub movement_count: i64,
}

/// Result of stock movements import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockMovementsImportResult {
    pub movement_count: i32,
    pub unit_id: String,
    pub file_hash: String,
    pub imported_by: String,
    pub timestamp: String,
}

/// لقطة الرصيد الافتتاحي لسنة مالية
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpeningBalanceSnapshot {
    pub id: String,
    pub product_id: String,
    pub fiscal_year: i32,
    pub opening_quantity: f64,
    pub unit_cost: f64,
    pub total_value: f64,
    pub snapshot_reason: String,
    pub carried_from_year: Option<i32>,
    pub created_at: String,
    pub created_by: String,
}
