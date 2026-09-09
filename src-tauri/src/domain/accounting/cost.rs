use std::fmt;

use crate::domain::numeric::legacy_float::money_from_f64;
use crate::domain::numeric::{Money, NumericError};

use super::fifo::ConsumedLayerPortion;

/// Catalog / reference price — must never drive FIFO, COGS, or valuation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReferencePrice(Money);

impl ReferencePrice {
    pub fn new(value: Money) -> Self {
        Self(value)
    }

    pub fn value(self) -> Money {
        self.0
    }
}

/// Unit cost from a FIFO layer (historical, immutable once consumed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FifoUnitCost(Money);

impl FifoUnitCost {
    pub fn new(value: Money) -> Self {
        Self(value)
    }

    pub fn value(self) -> Money {
        self.0
    }
}

/// Total cost of a consumption event (sum of layer portions).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConsumptionCost(Money);

impl ConsumptionCost {
    pub fn new(value: Money) -> Self {
        Self(value)
    }

    pub fn value(self) -> Money {
        self.0
    }

    /// Exact Money sum of layer portion totals (ADR-0048). `ConsumedLayerPortion`
    /// is the wire DTO, so its `f64` total_cost is materialized through the
    /// legacy boundary adapter before exact accumulation.
    pub fn from_portions(portions: &[ConsumedLayerPortion]) -> Result<Self, NumericError> {
        let mut total = Money::zero();
        for portion in portions {
            total = total.checked_add(money_from_f64(portion.total_cost)?)?;
        }
        Ok(Self(total))
    }
}

/// Agreed TTC price from a contract product line (ADR-0055 / SEC-087).
///
/// Authoritative transaction price: backend-resolved through the contract
/// entitlement chain (`contract_products.price_ttc` snapshot →
/// `supplier_order_items.unit_price`), never `ReferencePrice` and never an
/// HT projection (`agreed_price_ht`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContractAgreedPrice(Money);

impl ContractAgreedPrice {
    pub fn new(value: Money) -> Self {
        Self(value)
    }

    pub fn value(self) -> Money {
        self.0
    }
}

/// Immutable historical cost captured at transaction time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistoricalCost(Money);

impl HistoricalCost {
    pub fn from_fifo_unit_cost(cost: FifoUnitCost) -> Self {
        Self(cost.value())
    }

    pub fn value(self) -> Money {
        self.0
    }
}

// Prevent accidental mixing at compile time — no From impls between
// ReferencePrice, ContractAgreedPrice, and FifoUnitCost.

impl fmt::Display for ReferencePrice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (reference)", self.0)
    }
}

impl fmt::Display for ContractAgreedPrice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (agreed)", self.0)
    }
}

impl fmt::Display for FifoUnitCost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::accounting::fifo::ConsumedLayerPortion;

    fn portion(total_cost: f64) -> ConsumedLayerPortion {
        ConsumedLayerPortion {
            layer_id: "l".into(),
            quantity: 1.0,
            unit_cost: 1.0,
            total_cost,
        }
    }

    #[test]
    fn consumption_cost_from_portions_is_exact_money_sum() {
        // 150.50 + 225.75 + 0.00 = 37625 centimes — exact, no epsilon.
        let portions = [portion(150.50), portion(225.75), portion(0.0)];
        let cost = ConsumptionCost::from_portions(&portions).unwrap();
        assert_eq!(cost.value(), Money::from_centimes(37_625).unwrap());
    }

    #[test]
    fn consumption_cost_from_empty_portions_is_zero() {
        let cost = ConsumptionCost::from_portions(&[]).unwrap();
        assert_eq!(cost.value(), Money::zero());
    }

    #[test]
    fn reference_and_agreed_prices_roundtrip_money() {
        let ref_price = ReferencePrice::new(Money::from_centimes(4250).unwrap());
        assert_eq!(ref_price.value(), Money::from_centimes(4250).unwrap());

        let agreed = ContractAgreedPrice::new(Money::from_centimes(4250).unwrap());
        assert_eq!(agreed.value(), Money::from_centimes(4250).unwrap());
    }
}
