use std::fmt;

use crate::domain::invariants::Invariant;

/// Violation raised when a stock quantity is negative.
#[derive(Debug, Clone, PartialEq)]
pub enum StockNonNegativeViolation {
    NegativeLayerQuantity {
        layer_id: String,
        quantity: f64,
    },
    NegativeBalance {
        product_label: String,
        balance: f64,
    },
}

impl fmt::Display for StockNonNegativeViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StockNonNegativeViolation::NegativeLayerQuantity { layer_id, quantity } => {
                write!(f, "layer {} has negative remaining quantity: {}", layer_id, quantity)
            }
            StockNonNegativeViolation::NegativeBalance { product_label, balance } => {
                write!(f, "product {} has negative stock balance: {}", product_label, balance)
            }
        }
    }
}

/// A layer with a quantity to check.
#[derive(Debug, Clone)]
pub struct LayerQuantity {
    pub layer_id: String,
    pub quantity: f64,
}

/// A product-level balance to check.
#[derive(Debug, Clone)]
pub struct ProductBalance {
    pub product_label: String,
    pub balance: f64,
}

/// Input context for the StockNonNegative invariant.
#[derive(Debug, Clone)]
pub struct StockContext {
    pub layers: Vec<LayerQuantity>,
    pub product_balances: Vec<ProductBalance>,
}

/// Invariant: Stock quantities and balances must never be negative.
pub struct StockNonNegative;

impl Invariant for StockNonNegative {
    type Context = StockContext;
    type Violation = StockNonNegativeViolation;

    fn check(ctx: &StockContext) -> Vec<StockNonNegativeViolation> {
        let mut violations = Vec::new();

        for layer in &ctx.layers {
            if layer.quantity < 0.0 {
                violations.push(StockNonNegativeViolation::NegativeLayerQuantity {
                    layer_id: layer.layer_id.clone(),
                    quantity: layer.quantity,
                });
            }
        }

        for pb in &ctx.product_balances {
            if pb.balance < 0.0 {
                violations.push(StockNonNegativeViolation::NegativeBalance {
                    product_label: pb.product_label.clone(),
                    balance: pb.balance,
                });
            }
        }

        violations
    }
}

/// Convenience: check that a single layer quantity is non-negative.
pub fn check_layer_quantity(layer_id: &str, quantity: f64) -> Vec<StockNonNegativeViolation> {
    if quantity < 0.0 {
        vec![StockNonNegativeViolation::NegativeLayerQuantity {
            layer_id: layer_id.to_string(),
            quantity,
        }]
    } else {
        vec![]
    }
}

/// Convenience: check that a product balance is non-negative.
pub fn check_product_balance(product_label: &str, balance: f64) -> Vec<StockNonNegativeViolation> {
    if balance < 0.0 {
        vec![StockNonNegativeViolation::NegativeBalance {
            product_label: product_label.to_string(),
            balance,
        }]
    } else {
        vec![]
    }
}

/// Convenience: check all layer quantities and product balances.
pub fn check_stock_non_negative(
    layers: Vec<LayerQuantity>,
    product_balances: Vec<ProductBalance>,
) -> Vec<StockNonNegativeViolation> {
    let ctx = StockContext {
        layers,
        product_balances,
    };
    StockNonNegative::check(&ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_quantity_is_valid() {
        assert!(check_layer_quantity("layer-1", 0.0).is_empty());
    }

    #[test]
    fn positive_quantity_is_valid() {
        assert!(check_layer_quantity("layer-1", 42.5).is_empty());
    }

    #[test]
    fn negative_quantity_triggers_violation() {
        let v = check_layer_quantity("layer-1", -5.0);
        assert_eq!(v.len(), 1);
        match &v[0] {
            StockNonNegativeViolation::NegativeLayerQuantity { layer_id, quantity } => {
                assert_eq!(layer_id, "layer-1");
                assert!((*quantity - -5.0).abs() < 1e-9);
            }
            _ => panic!("wrong violation type"),
        }
    }

    #[test]
    fn negative_balance_triggers_violation() {
        let v = check_product_balance("product-xyz", -1.0);
        assert_eq!(v.len(), 1);
        match &v[0] {
            StockNonNegativeViolation::NegativeBalance { product_label, balance } => {
                assert_eq!(product_label, "product-xyz");
                assert!((*balance - -1.0).abs() < 1e-9);
            }
            _ => panic!("wrong violation type"),
        }
    }

    #[test]
    fn multiple_violations_collected() {
        let layers = vec![
            LayerQuantity { layer_id: "a".into(), quantity: 10.0 },
            LayerQuantity { layer_id: "b".into(), quantity: -3.0 },
            LayerQuantity { layer_id: "c".into(), quantity: -1.5 },
        ];
        let balances = vec![
            ProductBalance { product_label: "p1".into(), balance: 5.0 },
            ProductBalance { product_label: "p2".into(), balance: -7.0 },
        ];

        let v = check_stock_non_negative(layers, balances);
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn all_valid_returns_empty() {
        let layers = vec![
            LayerQuantity { layer_id: "a".into(), quantity: 10.0 },
            LayerQuantity { layer_id: "b".into(), quantity: 0.0 },
        ];
        let balances = vec![
            ProductBalance { product_label: "p1".into(), balance: 5.0 },
        ];

        assert!(check_stock_non_negative(layers, balances).is_empty());
    }

    #[test]
    fn empty_inputs_returns_empty() {
        assert!(check_stock_non_negative(vec![], vec![]).is_empty());
    }
}
