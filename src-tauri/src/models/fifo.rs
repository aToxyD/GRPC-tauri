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
    pub unit_cost: f64,
    pub qty_original: f64,
    pub qty_remaining: f64,
    pub received_at: String,
    pub created_by: String,
    pub origin_fiscal_year: i32,
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
    pub predicted_consumption_layers: Vec<ConsumedLayerPortion>,
}

/// Meal-level preview totals derived from the same aggregation as daily report execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MealFifoPreview {
    pub meal_type: String,
    pub predicted_fifo_cost: f64,
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
}

/// Full daily consumption FIFO preview (matches execution aggregation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyFifoConsumptionPreview {
    pub predicted_fifo_cost: f64,
    pub predicted_consumption_layers: Vec<ProductFifoPreview>,
    pub predicted_remaining_inventory_value: f64,
    pub meal_previews: Vec<MealFifoPreview>,
}
