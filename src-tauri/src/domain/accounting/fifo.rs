use serde::{Deserialize, Serialize};

/// One layer row for FIFO simulation: (layer_id, unit_cost, qty_remaining).
pub type FifoLayerRow = (String, f64, f64);

/// Portion consumed from one FIFO layer during consumption.
///
/// Every consumed item traces to exactly one FIFO layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumedLayerPortion {
    pub layer_id: String,
    pub quantity: f64,
    pub unit_cost: f64,
    pub total_cost: f64,
}

/// Source type classification for a FIFO layer.
///
/// - `Order`: created on supplier order receipt
/// - `Opening`: reclassified from `Order` during fiscal year close, or created as opening balance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum FifoLayerSource {
    Order,
    Opening,
}
