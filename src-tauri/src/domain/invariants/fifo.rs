use std::cmp::Ordering;
use std::fmt;

use crate::domain::invariants::Invariant;

/// Violation raised when FIFO consumption order is violated.
#[derive(Debug, Clone, PartialEq)]
pub enum FifoOrderStableViolation {
    LayerConsumedOutOfOrder {
        layer_id: String,
        received_at: String,
        older_layer_with_stock_id: String,
        older_layer_received_at: String,
    },
    ConsumptionSkippedLayer {
        layer_id: String,
        received_at: String,
        quantity_remaining_before_consumption: f64,
    },
}

impl fmt::Display for FifoOrderStableViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FifoOrderStableViolation::LayerConsumedOutOfOrder {
                layer_id,
                received_at,
                older_layer_with_stock_id,
                older_layer_received_at,
            } => {
                write!(
                    f,
                    "layer {} (received {}) consumed before older layer {} (received {})",
                    layer_id, received_at, older_layer_with_stock_id, older_layer_received_at
                )
            }
            FifoOrderStableViolation::ConsumptionSkippedLayer {
                layer_id,
                received_at,
                quantity_remaining_before_consumption,
            } => {
                write!(
                    f,
                    "layer {} (received {}) had {:.2} remaining but was skipped during consumption",
                    layer_id, received_at, quantity_remaining_before_consumption
                )
            }
        }
    }
}

/// A FIFO layer snapshot at the time of consumption.
#[derive(Debug, Clone)]
pub struct LayerAtConsumption {
    pub layer_id: String,
    pub received_at: String,
    pub quantity_consumed: f64,
    pub quantity_remaining_before: f64,
    pub fifo_order_index: usize,
}

/// A layer state before any consumption occurred.
#[derive(Debug, Clone)]
pub struct PreConsumptionLayer {
    pub layer_id: String,
    pub received_at: String,
    pub quantity_remaining: f64,
}

/// Input context for the FifoOrderStable invariant.
#[derive(Debug, Clone)]
pub struct FifoConsumptionContext {
    pub applied_consumptions: Vec<LayerAtConsumption>,
    pub pre_consumption_layers: Vec<PreConsumptionLayer>,
}

/// Compare two RFC3339 timestamps lexicographically (valid for ISO-8601
/// when both timestamps share the same timezone — the system always uses UTC).
fn timestamp_cmp(a: &str, b: &str) -> Ordering {
    a.cmp(b)
}

/// Invariant: FIFO consumption order must respect `received_at ASC, id ASC`.
///
/// A newer layer must not be consumed while an older layer with remaining
/// stock is available.
pub struct FifoOrderStable;

impl Invariant for FifoOrderStable {
    type Context = FifoConsumptionContext;
    type Violation = FifoOrderStableViolation;

    fn check(ctx: &FifoConsumptionContext) -> Vec<FifoOrderStableViolation> {
        let mut violations = Vec::new();

        for c in &ctx.applied_consumptions {
            for older in &ctx.pre_consumption_layers {
                if older.layer_id == c.layer_id {
                    break;
                }

                let older_received_earlier =
                    timestamp_cmp(&older.received_at, &c.received_at) == Ordering::Less;
                let same_received =
                    timestamp_cmp(&older.received_at, &c.received_at) == Ordering::Equal;

                if older_received_earlier && older.quantity_remaining > 0.0 {
                    let was_consumed = ctx
                        .applied_consumptions
                        .iter()
                        .any(|ac| ac.layer_id == older.layer_id);
                    if !was_consumed {
                        violations.push(FifoOrderStableViolation::ConsumptionSkippedLayer {
                            layer_id: older.layer_id.clone(),
                            received_at: older.received_at.clone(),
                            quantity_remaining_before_consumption: older.quantity_remaining,
                        });
                    }
                }

                if !older_received_earlier && !same_received {
                    break;
                }
            }
        }

        violations
    }
}

/// Convenience: check that a single consumption operation respected FIFO order.
pub fn check_fifo_consumption_order(
    applied_consumptions: Vec<LayerAtConsumption>,
    pre_consumption_layers: Vec<PreConsumptionLayer>,
) -> Vec<FifoOrderStableViolation> {
    let ctx = FifoConsumptionContext {
        applied_consumptions,
        pre_consumption_layers,
    };
    FifoOrderStable::check(&ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(id: &str, received: &str, remaining: f64) -> PreConsumptionLayer {
        PreConsumptionLayer {
            layer_id: id.to_string(),
            received_at: received.to_string(),
            quantity_remaining: remaining,
        }
    }

    fn consumption(
        layer_id: &str,
        received: &str,
        consumed: f64,
        before: f64,
        order: usize,
    ) -> LayerAtConsumption {
        LayerAtConsumption {
            layer_id: layer_id.to_string(),
            received_at: received.to_string(),
            quantity_consumed: consumed,
            quantity_remaining_before: before,
            fifo_order_index: order,
        }
    }

    #[test]
    fn oldest_layer_consumed_first_is_valid() {
        let layers = vec![
            layer("l1", "2025-01-01T00:00:00Z", 50.0),
            layer("l2", "2025-01-10T00:00:00Z", 100.0),
        ];
        let consumptions = vec![consumption("l1", "2025-01-01T00:00:00Z", 30.0, 50.0, 0)];

        assert!(check_fifo_consumption_order(consumptions, layers).is_empty());
    }

    #[test]
    fn skipping_older_layer_triggers_violation() {
        let layers = vec![
            layer("l1", "2025-01-01T00:00:00Z", 50.0),
            layer("l2", "2025-01-10T00:00:00Z", 100.0),
        ];
        let consumptions = vec![consumption("l2", "2025-01-10T00:00:00Z", 20.0, 100.0, 1)];

        let v = check_fifo_consumption_order(consumptions, layers);
        assert!(!v.is_empty());
    }

    #[test]
    fn older_layer_exhausted_allows_newer() {
        let layers = vec![
            layer("l1", "2025-01-01T00:00:00Z", 0.0),
            layer("l2", "2025-01-10T00:00:00Z", 100.0),
        ];
        let consumptions = vec![consumption("l2", "2025-01-10T00:00:00Z", 20.0, 100.0, 1)];

        assert!(check_fifo_consumption_order(consumptions, layers).is_empty());
    }

    #[test]
    fn consuming_from_multiple_layers_in_order_is_valid() {
        let layers = vec![
            layer("l1", "2025-01-01T00:00:00Z", 50.0),
            layer("l2", "2025-01-10T00:00:00Z", 100.0),
        ];
        let consumptions = vec![
            consumption("l1", "2025-01-01T00:00:00Z", 50.0, 50.0, 0),
            consumption("l2", "2025-01-10T00:00:00Z", 30.0, 100.0, 1),
        ];

        assert!(check_fifo_consumption_order(consumptions, layers).is_empty());
    }

    #[test]
    fn empty_inputs_returns_empty() {
        assert!(check_fifo_consumption_order(vec![], vec![]).is_empty());
    }
}
