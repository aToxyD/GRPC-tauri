//! Pure FIFO consumption engine — shared by persistence and preview paths.
//!
//! Ordering: `received_at ASC`, then `id ASC` (stable FIFO).
//!
//! ADR-0048: all accounting arithmetic here is **exact `Decimal`** via the
//! typed core (`Money`/`Quantity`). The `f64` facade functions below are wire
//! entry points (REAL columns + serde DTOs): they convert at the boundary with
//! the legacy adapter and delegate to the exact core. No float arithmetic
//! exists in the migration path.

use crate::domain::accounting::fifo::ConsumedLayerPortion;
use crate::domain::numeric::{legacy_float, Money, NumericError, Quantity};
use crate::errors::{AppError, BusinessLogicError};

/// One layer row used for simulation: (layer_id, unit_cost, qty_remaining).
pub type FifoLayerRow = (String, f64, f64);

/// Exact-typed FIFO layer (ADR-0048).
#[derive(Debug, Clone)]
pub struct TypedFifoLayer {
    pub layer_id: String,
    pub unit_cost: Money,
    pub qty_remaining: Quantity,
}

/// Exact-typed consumed portion (ADR-0048). `total_cost` is exactly
/// `unit_cost × quantity`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedConsumedPortion {
    pub layer_id: String,
    pub quantity: Quantity,
    pub unit_cost: Money,
    pub total_cost: Money,
}

/// Exact, deterministic FIFO simulation. Rules mirror the legacy semantics but
/// on exact decimals:
/// * zero requested ⇒ empty (no-op), never an error;
/// * insufficiency is an **exact** comparison (requested > available);
/// * per-layer consumption is `min` via `Ord` (no float residue);
/// * consumed quantity is subtracted exactly, so `1.000 − 0.333×3 = 0.001`.
pub(crate) fn simulate_fifo_consumption_typed(
    product_id: &str,
    layers: &[TypedFifoLayer],
    quantity: Quantity,
) -> Result<Vec<TypedConsumedPortion>, AppError> {
    if quantity.is_zero() {
        return Ok(Vec::new());
    }

    let mut total_available = Quantity::zero();
    for layer in layers {
        total_available = total_available.checked_add(layer.qty_remaining)?;
    }
    if quantity > total_available {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::InsufficientStock(format!(
                "Insufficient stock for product {}. Requested: {}, Available: {}",
                product_id, quantity, total_available
            )),
        ));
    }

    let mut remaining_to_consume = quantity;
    let mut consumed_portions = Vec::new();

    for layer in layers {
        if remaining_to_consume.is_zero() {
            break;
        }

        let consumed_qty = layer.qty_remaining.min(remaining_to_consume);
        let total_cost = layer.unit_cost.checked_mul_quantity(&consumed_qty)?;

        consumed_portions.push(TypedConsumedPortion {
            layer_id: layer.layer_id.clone(),
            quantity: consumed_qty,
            unit_cost: layer.unit_cost,
            total_cost,
        });

        remaining_to_consume = remaining_to_consume.checked_sub(consumed_qty)?;
    }

    Ok(consumed_portions)
}

/// Exact remaining layers after a typed consumption (preview display).
pub(crate) fn simulate_remaining_layers_typed(
    layers: &[TypedFifoLayer],
    portions: &[TypedConsumedPortion],
) -> Result<Vec<TypedFifoLayer>, NumericError> {
    use std::collections::HashMap;

    let mut consumed_by_layer: HashMap<&str, Quantity> = HashMap::new();
    for p in portions {
        let entry = consumed_by_layer
            .entry(p.layer_id.as_str())
            .or_insert(Quantity::zero());
        *entry = entry.checked_add(p.quantity)?;
    }

    let mut remaining = Vec::new();
    for layer in layers {
        let left = match consumed_by_layer.get(layer.layer_id.as_str()) {
            Some(consumed) => layer.qty_remaining.checked_sub(*consumed)?,
            None => layer.qty_remaining,
        };
        if left.is_positive() {
            remaining.push(TypedFifoLayer {
                layer_id: layer.layer_id.clone(),
                unit_cost: layer.unit_cost,
                qty_remaining: left,
            });
        }
    }
    Ok(remaining)
}

/// Simulate consuming `quantity` from ordered layers without mutating state.
///
/// Wire facade (ADR-0048): converts `f64` → exact values at the boundary and
/// delegates to the typed core; results are converted back exactly once.
pub fn simulate_fifo_consumption(
    product_id: &str,
    layers: &[FifoLayerRow],
    quantity: f64,
) -> Result<Vec<ConsumedLayerPortion>, AppError> {
    let requested = legacy_float::quantity_from_f64(quantity)?;
    let typed_layers = layers
        .iter()
        .map(|(id, cost, qty)| {
            Ok(TypedFifoLayer {
                layer_id: id.clone(),
                unit_cost: legacy_float::money_from_f64(*cost)?,
                qty_remaining: legacy_float::quantity_from_f64(*qty)?,
            })
        })
        .collect::<Result<Vec<TypedFifoLayer>, AppError>>()?;

    let portions = simulate_fifo_consumption_typed(product_id, &typed_layers, requested)?;
    portions
        .into_iter()
        .map(|p| {
            Ok(ConsumedLayerPortion {
                layer_id: p.layer_id,
                quantity: legacy_float::quantity_to_f64(&p.quantity)?,
                unit_cost: legacy_float::money_to_f64(&p.unit_cost)?,
                total_cost: legacy_float::money_to_f64(&p.total_cost)?,
            })
        })
        .collect()
}

/// Remaining layers after a simulated consumption (for preview display).
///
/// Wire facade. Conversion failure is only reachable via corrupted inputs —
/// the producing simulation already validated the same values — and fails
/// closed to an empty preview.
pub fn simulate_remaining_layers(
    layers: &[FifoLayerRow],
    portions: &[ConsumedLayerPortion],
) -> Vec<FifoLayerRow> {
    let typed_layers = match layers
        .iter()
        .map(|(id, cost, qty)| {
            Ok(TypedFifoLayer {
                layer_id: id.clone(),
                unit_cost: legacy_float::money_from_f64(*cost)?,
                qty_remaining: legacy_float::quantity_from_f64(*qty)?,
            })
        })
        .collect::<Result<Vec<TypedFifoLayer>, NumericError>>()
    {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let typed_portions = match portions
        .iter()
        .map(|p| {
            Ok(TypedConsumedPortion {
                layer_id: p.layer_id.clone(),
                quantity: legacy_float::quantity_from_f64(p.quantity)?,
                unit_cost: legacy_float::money_from_f64(p.unit_cost)?,
                total_cost: legacy_float::money_from_f64(p.total_cost)?,
            })
        })
        .collect::<Result<Vec<TypedConsumedPortion>, NumericError>>()
    {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    match simulate_remaining_layers_typed(&typed_layers, &typed_portions) {
        Ok(remaining) => remaining
            .into_iter()
            .filter_map(|l| {
                let cost = legacy_float::money_to_f64(&l.unit_cost).ok()?;
                let qty = legacy_float::quantity_to_f64(&l.qty_remaining).ok()?;
                Some((l.layer_id, cost, qty))
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(id: &str, cost: f64, qty: f64) -> FifoLayerRow {
        (id.to_string(), cost, qty)
    }

    fn typed_layer(id: &str, cost: &str, qty: &str) -> TypedFifoLayer {
        TypedFifoLayer {
            layer_id: id.to_string(),
            unit_cost: Money::parse_str(cost).unwrap(),
            qty_remaining: Quantity::parse_str(qty).unwrap(),
        }
    }

    #[test]
    fn consumes_oldest_layer_first() {
        let layers = vec![layer("a", 10.0, 5.0), layer("b", 20.0, 10.0)];
        let portions = simulate_fifo_consumption("p1", &layers, 3.0).unwrap();
        assert_eq!(portions.len(), 1);
        assert_eq!(portions[0].layer_id, "a");
        // Facade results are boundary-converted exactly (Money 10.00 → 10.0).
        assert_eq!(portions[0].unit_cost, 10.0);
    }

    #[test]
    fn spans_multiple_layers() {
        let layers = vec![layer("a", 10.0, 5.0), layer("b", 20.0, 10.0)];
        let portions = simulate_fifo_consumption("p1", &layers, 8.0).unwrap();
        assert_eq!(portions.len(), 2);
        assert_eq!(portions[0].quantity, 5.0);
        assert_eq!(portions[1].quantity, 3.0);
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

    #[test]
    fn typed_three_thirds_leaves_exact_dust() {
        // 1.000 − 0.333 × 3 = 0.001 exactly. The typed engine must preserve this
        // dust so the next 0.001 order still resolves (no EPSILON swallowing).
        let layers = vec![typed_layer("a", "10.00", "1.000")];
        let portions =
            simulate_fifo_consumption_typed("p1", &layers, Quantity::parse_str("0.333").unwrap())
                .unwrap();
        assert_eq!(portions[0].quantity, Quantity::parse_str("0.333").unwrap());
        let left = simulate_remaining_layers_typed(&layers, &portions).unwrap();
        assert_eq!(left[0].qty_remaining, Quantity::parse_str("0.667").unwrap());

        let p2 =
            simulate_fifo_consumption_typed("p1", &left, Quantity::parse_str("0.333").unwrap())
                .unwrap();
        let left2 = simulate_remaining_layers_typed(&left, &p2).unwrap();
        let p3 =
            simulate_fifo_consumption_typed("p1", &left2, Quantity::parse_str("0.333").unwrap())
                .unwrap();
        let left3 = simulate_remaining_layers_typed(&left2, &p3).unwrap();
        assert_eq!(left3[0].qty_remaining.to_scaled_i64().unwrap(), 1);
        assert!(!left3[0].qty_remaining.is_zero());
    }

    #[test]
    fn typed_insufficiency_is_exact() {
        let layers = vec![typed_layer("a", "10.00", "0.333")];
        // Requesting 0.334 > 0.333 must fail exactly.
        let r =
            simulate_fifo_consumption_typed("p1", &layers, Quantity::parse_str("0.334").unwrap());
        assert!(r.is_err());
        // Requesting exactly 0.333 succeeds.
        let r =
            simulate_fifo_consumption_typed("p1", &layers, Quantity::parse_str("0.333").unwrap());
        assert!(r.is_ok());
    }

    #[test]
    fn typed_total_cost_is_exact_money() {
        let layers = vec![typed_layer("a", "150.50", "2.000")];
        let portions =
            simulate_fifo_consumption_typed("p1", &layers, Quantity::parse_str("1.500").unwrap())
                .unwrap();
        assert_eq!(portions[0].total_cost, Money::parse_str("225.750").unwrap());
    }

    #[test]
    fn facade_bounds_precisely() {
        // 1.00 × 0.333 → 0.333 exact internally; the wire total_cost is rounded
        // once to the money boundary (cents): 0.333 → 0.33. This is a single
        // MidpointAwayFromZero boundary conversion, not float dust.
        let layers = vec![layer("a", 1.0, 100.0)];
        let portions = simulate_fifo_consumption("p1", &layers, 0.333).unwrap();
        assert_eq!(portions[0].quantity, 0.333);
        assert_eq!(portions[0].total_cost, 0.33);
        assert_eq!(
            legacy_float::quantity_to_f64(
                &legacy_float::quantity_from_f64(portions[0].quantity).unwrap()
            )
            .unwrap(),
            0.333
        );
    }
}
