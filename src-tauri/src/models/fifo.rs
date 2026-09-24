use crate::models::DailyConsumptionSummary;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FifoStockLayer {
    pub id: String,
    pub unit_id: String,
    pub product_id: String,
    /// Accounting classification of the layer in the current fiscal context.
    ///
    /// Notes:
    /// - New inventory layers are created as `ORDER`.
    /// - When a fiscal year is closed, any remaining quantity from an `ORDER`
    ///   layer is reclassified to `OPENING`.
    /// - Therefore `source_type` represents the current accounting status
    ///   of the layer and not necessarily its historical origin.
    /// - The same FIFO layer record is preserved; no new layer is created.
    pub source_type: String,
    pub source_id: Option<String>,
    /// TTC unit cost in CONSUMPTION units (scale-2), rounded once at creation.
    pub unit_cost: f64,
    /// Quantities are in CONSUMPTION units.
    pub qty_original: f64,
    pub qty_remaining: f64,
    pub received_at: String,
    pub created_by: String,
    pub origin_fiscal_year: i32,
    /// SEC-087 Phase 5: exact historical purchase-side snapshot of the layer
    /// (null for legacy/OPENING layers without a purchase origin).
    #[serde(default)]
    pub purchase_quantity: Option<f64>,
    #[serde(default)]
    pub purchase_unit_cost: Option<f64>,
    #[serde(default)]
    pub purchase_unit: Option<i32>,
    #[serde(default)]
    pub consumption_unit: Option<i32>,
    #[serde(default)]
    pub conversion_factor: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryLayerConsumption {
    pub id: String,
    pub unit_id: String,
    pub movement_id: String,
    pub layer_id: String,
    pub quantity: f64,
    pub unit_cost: f64,
    pub total_cost: f64,
    pub consumed_at: String,
}

pub use crate::domain::accounting::fifo::ConsumedLayerPortion;

/// Predicted consumption for one product line (FIFO dry-run).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductFifoPreview {
    pub product_id: String,
    pub quantity: f64,
    pub predicted_fifo_cost: f64,
    pub unit_cost: f64,
    pub predicted_consumption_layers: Vec<ConsumedLayerPortion>,
}

/// Meal-level preview totals derived from the same aggregation as daily report execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MealFifoPreview {
    pub meal_type: String,
    pub predicted_fifo_cost: f64,
    pub total_beneficiaries: i32,
    pub meal_average: f64,
    pub product_previews: Vec<ProductFifoPreview>,
}

/// One active FIFO layer in the inventory view (for StockPage).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryLayerView {
    pub layer_id: String,
    pub source_type: Option<String>,
    pub received_at: String,
    pub qty_remaining: f64,
    pub unit_cost: f64,
    pub layer_value: f64,
}

/// Advisory snapshot-coverage warning for the UNIT stock overview (SEC-087
/// Phase 5): stock that exists without a persisted purchase snapshot backing
/// it, or vice versa. Informational only — never blocks operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryCoverageWarning {
    /// Distinct phase-5 warning code: `STOCK_WITHOUT_PURCHASE_SNAPSHOT`,
    /// `PURCHASE_SNAPSHOT_WITHOUT_STOCK`.
    pub code: String,
    pub product_id: String,
    pub product_name: String,
    pub message: String,
}

/// Per-product FIFO summary in the inventory view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryProductView {
    pub product_id: String,
    pub product_name: String,
    pub total_quantity: f64,
    pub total_value: f64,
    pub oldest_layer_date: Option<String>,
    pub layer_count: usize,
    pub layers: Vec<InventoryLayerView>,
}

/// Full inventory FIFO page view for UNIT StockPage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryStockPageView {
    pub products: Vec<InventoryProductView>,
    pub total_inventory_value: f64,
    pub total_products: usize,
    pub total_active_layers: usize,
    /// SEC-087 Phase 5 advisory snapshot-coverage warnings (empty normally).
    #[serde(default)]
    pub warnings: Vec<InventoryCoverageWarning>,
}

/// Full daily consumption FIFO preview (matches execution aggregation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyFifoConsumptionPreview {
    pub predicted_fifo_cost: f64,
    pub predicted_consumption_layers: Vec<ProductFifoPreview>,
    pub predicted_remaining_inventory_value: f64,
    pub meal_previews: Vec<MealFifoPreview>,
    pub daily_summary: DailyConsumptionSummary,
}
