use serde::{Deserialize, Serialize};
use std::fmt;

/// Inventory valuation from remaining FIFO layers only.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct InventoryValue(f64);

impl InventoryValue {
    pub fn new(value: f64) -> Self {
        Self(value)
    }

    pub fn value(self) -> f64 {
        self.0
    }

    pub fn zero() -> Self {
        Self(0.0)
    }
}

impl fmt::Display for InventoryValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}
