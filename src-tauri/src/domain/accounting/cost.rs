use serde::{Deserialize, Serialize};
use std::fmt;

use super::fifo::ConsumedLayerPortion;

/// Catalog / reference price — must never drive FIFO, COGS, or valuation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReferencePrice(f64);

impl ReferencePrice {
    pub fn new(value: f64) -> Self {
        Self(value)
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

/// Unit cost from a FIFO layer (historical, immutable once consumed).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FifoUnitCost(f64);

impl FifoUnitCost {
    pub fn new(value: f64) -> Self {
        Self(value)
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

/// Total cost of a consumption event (sum of layer portions).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConsumptionCost(f64);

impl ConsumptionCost {
    pub fn new(value: f64) -> Self {
        Self(value)
    }

    pub fn value(self) -> f64 {
        self.0
    }

    pub fn from_portions(portions: &[ConsumedLayerPortion]) -> Self {
        let total: f64 = portions.iter().map(|p| p.total_cost).sum();
        Self(total)
    }
}

/// Agreed price from a contract product line (ADR-0055 / SEC-087-F).
///
/// Authoritative transaction price: backend-resolved through the contract
/// entitlement chain (ContractAgreedPrice → allocation agreed_price →
/// `supplier_order_items.unit_price`), never `ReferencePrice`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ContractAgreedPrice(f64);

impl ContractAgreedPrice {
    pub fn new(value: f64) -> Self {
        Self(value)
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

/// Immutable historical cost captured at transaction time.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HistoricalCost(f64);

impl HistoricalCost {
    pub fn from_fifo_unit_cost(cost: FifoUnitCost) -> Self {
        Self(cost.value())
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

// Prevent accidental mixing at compile time — no From impls between
// ReferencePrice, ContractAgreedPrice, and FifoUnitCost.

impl fmt::Display for ReferencePrice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2} (reference)", self.0)
    }
}

impl fmt::Display for ContractAgreedPrice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2} (agreed)", self.0)
    }
}

impl fmt::Display for FifoUnitCost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}
