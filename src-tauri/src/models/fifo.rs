use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FifoStockLayer {
    pub id: String,
    pub unit_id: String,
    pub product_id: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub unit_cost: f64,
    pub qty_original: f64,
    pub qty_remaining: f64,
    pub received_at: String,
    pub created_by: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumedLayerPortion {
    pub layer_id: String,
    pub quantity: f64,
    pub unit_cost: f64,
    pub total_cost: f64,
}

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

/// Full daily consumption FIFO preview (matches execution aggregation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyFifoConsumptionPreview {
    pub predicted_fifo_cost: f64,
    pub predicted_consumption_layers: Vec<ProductFifoPreview>,
    pub predicted_remaining_inventory_value: f64,
    pub meal_previews: Vec<MealFifoPreview>,
}
