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
