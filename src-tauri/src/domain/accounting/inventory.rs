use std::fmt;

use crate::domain::numeric::Money;

/// Inventory valuation from remaining FIFO layers only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InventoryValue(Money);

impl InventoryValue {
    pub fn new(value: Money) -> Self {
        Self(value)
    }

    pub fn value(self) -> Money {
        self.0
    }

    pub fn zero() -> Self {
        Self(Money::zero())
    }
}

impl fmt::Display for InventoryValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
