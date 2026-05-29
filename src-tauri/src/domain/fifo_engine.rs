//! Pure FIFO consumption engine — shared by persistence and preview paths.
//!
//! Ordering: `received_at ASC`, then `id ASC` (stable FIFO).

use crate::domain::accounting::fifo::ConsumedLayerPortion;
use crate::errors::{AppError, BusinessLogicError};

/// One layer row used for simulation: (layer_id, unit_cost, qty_remaining).
pub type FifoLayerRow = (String, f64, f64);

/// Simulate consuming `quantity` from ordered layers without mutating state.
pub fn simulate_fifo_consumption(
    product_id: &str,
    layers: &[FifoLayerRow],
    quantity: f64,
) -> Result<Vec<ConsumedLayerPortion>, AppError> {
    if quantity <= 0.0 {
        return Ok(Vec::new());
    }

    let total_available: f64 = layers.iter().map(|(_, _, q)| q).sum();
    if total_available < quantity {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::InsufficientStock(format!(
                "Insufficient stock for product {}. Requested: {}, Available: {}",
                product_id, quantity, total_available
            )),
        ));
    }

    let mut remaining_to_consume = quantity;
    let mut consumed_portions = Vec::new();

    for (id, unit_cost, qty_remaining) in layers {
        if remaining_to_consume <= 0.0 {
            break;
        }

        let consumed_qty = qty_remaining.min(remaining_to_consume);
        let total_cost = consumed_qty * unit_cost;

        consumed_portions.push(ConsumedLayerPortion {
            layer_id: id.clone(),
            quantity: consumed_qty,
            unit_cost: *unit_cost,
            total_cost,
        });

        remaining_to_consume -= consumed_qty;
    }

    Ok(consumed_portions)
}

/// Remaining layers after a simulated consumption (for preview display).
pub fn simulate_remaining_layers(
    layers: &[FifoLayerRow],
    portions: &[ConsumedLayerPortion],
) -> Vec<FifoLayerRow> {
    use std::collections::HashMap;

    let mut consumed_by_layer: HashMap<&str, f64> = HashMap::new();
    for p in portions {
        *consumed_by_layer.entry(p.layer_id.as_str()).or_insert(0.0) += p.quantity;
    }

    let mut remaining = Vec::new();
    for (id, unit_cost, qty_remaining) in layers {
        let consumed = consumed_by_layer.get(id.as_str()).copied().unwrap_or(0.0);
        let left = qty_remaining - consumed;
        if left > 0.0 {
            remaining.push((id.clone(), *unit_cost, left));
        }
    }
    remaining
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(id: &str, cost: f64, qty: f64) -> FifoLayerRow {
        (id.to_string(), cost, qty)
    }

    #[test]
    fn consumes_oldest_layer_first() {
        let layers = vec![layer("a", 10.0, 5.0), layer("b", 20.0, 10.0)];
        let portions = simulate_fifo_consumption("p1", &layers, 3.0).unwrap();
        assert_eq!(portions.len(), 1);
        assert_eq!(portions[0].layer_id, "a");
        assert!((portions[0].unit_cost - 10.0).abs() < 1e-9);
    }

    #[test]
    fn spans_multiple_layers() {
        let layers = vec![layer("a", 10.0, 5.0), layer("b", 20.0, 10.0)];
        let portions = simulate_fifo_consumption("p1", &layers, 8.0).unwrap();
        assert_eq!(portions.len(), 2);
        assert!((portions[0].quantity - 5.0).abs() < 1e-9);
        assert!((portions[1].quantity - 3.0).abs() < 1e-9);
    }

    #[test]
    fn insufficient_stock_errors() {
        let layers = vec![layer("a", 10.0, 2.0)];
        assert!(simulate_fifo_consumption("p1", &layers, 5.0).is_err());
    }

    #[test]
    fn zero_quantity_returns_empty() {
        let layers = vec![layer("a", 10.0, 5.0)];
        assert!(simulate_fifo_consumption("p1", &layers, 0.0)
            .unwrap()
            .is_empty());
    }
}
