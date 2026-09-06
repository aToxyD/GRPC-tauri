//! Legacy `f64` ↔ exact-decimal adapter (ADR-0048, Phase 3A, §16).
//!
//! These conversions exist **only** at persistence/wire boundaries while
//! SQLite REAL columns and serde DTO fields remain `f64` (Phase 4 directly
//! replaces those columns with INTEGER scaled values). Rules:
//!
//! * Never used mid-calculation — accounting arithmetic lives on the typed
//!   side and never passes through `f64`.
//! * NaN / ±Infinity are rejected (`Parse`), so data corruption fails closed.
//! * Boundary conversions normalize exactly once with `MidpointAwayFromZero`
//!   via the type's `to_scaled_i64`; `f64` appears only in the final division.
//! * `Decimal::from_f64` resolves to the shortest round-trip representation
//!   (`0.1f64 → "0.1"`), which is deterministic and lossless for persisted
//!   REAL values carrying ≤ ~15 significant digits.

use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;

use super::{Money, NumericError, Quantity, Rate};

/// `f64` → money, shortest-round-trip decimal. Rejects NaN/Infinity.
pub(crate) fn money_from_f64(value: f64) -> Result<Money, NumericError> {
    Money::from_decimal(decimal_from_f64(value)?)
}

/// `f64` → quantity, shortest-round-trip decimal. Rejects NaN/Infinity.
pub(crate) fn quantity_from_f64(value: f64) -> Result<Quantity, NumericError> {
    Quantity::from_decimal(decimal_from_f64(value)?)
}

/// `f64` percent → rate, shortest-round-trip decimal. Rejects NaN/Infinity;
/// range `[0, 100]` enforcement happens in `Rate::from_decimal`.
pub(crate) fn rate_from_f64(value: f64) -> Result<Rate, NumericError> {
    Rate::from_decimal(decimal_from_f64(value)?)
}

/// Money → wire `f64`, rounded exactly once at the scale-2 boundary.
pub(crate) fn money_to_f64(money: &Money) -> Result<f64, NumericError> {
    Ok(money.to_scaled_i64()? as f64 / 100.0)
}

/// Quantity → wire `f64`, rounded exactly once at the scale-3 boundary.
pub(crate) fn quantity_to_f64(quantity: &Quantity) -> Result<f64, NumericError> {
    Ok(quantity.to_scaled_i64()? as f64 / 1000.0)
}

/// Rate → wire `f64` percent-domain, rounded exactly once at the scale-4 boundary.
pub(crate) fn rate_to_f64(rate: &Rate) -> Result<f64, NumericError> {
    Ok(rate.to_scaled_i64()? as f64 / 10_000.0)
}

fn decimal_from_f64(value: f64) -> Result<Decimal, NumericError> {
    Decimal::from_f64(value).ok_or(NumericError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::float_cmp)]
    fn money_round_trip_is_shortest_repr_deterministic() {
        // 19.99f64 → "19.99" exactly, never the binary expansion.
        let m = money_from_f64(19.99).unwrap();
        assert_eq!(m, Money::parse_str("19.99").unwrap());
        assert_eq!(money_to_f64(&m).unwrap(), 19.99);
        assert_eq!(m.to_scaled_i64().unwrap(), 1999);
    }

    #[test]
    fn quantity_round_trip_preserves_scale_three() {
        let q = quantity_from_f64(0.333).unwrap();
        assert_eq!(q, Quantity::parse_str("0.333").unwrap());
        assert_eq!(quantity_to_f64(&q).unwrap(), 0.333);
        assert_eq!(q.to_scaled_i64().unwrap(), 333);
    }

    #[test]
    fn rate_round_trip_is_percent_domain() {
        let r = rate_from_f64(19.0).unwrap();
        assert_eq!(r, Rate::parse_str("19.0").unwrap());
        assert_eq!(r.to_scaled_i64().unwrap(), 190_000);
    }

    #[test]
    fn boundary_rounding_is_exact_away_from_zero() {
        // 142.975 money → 142.98 at the scale-2 boundary (nearest f64).
        let m = money_from_f64(142.975).unwrap();
        assert_eq!(m.to_scaled_i64().unwrap(), 14298);
        assert_eq!(money_to_f64(&m).unwrap(), 142.98);
    }

    #[test]
    fn nan_and_infinity_fail_closed() {
        assert_eq!(money_from_f64(f64::NAN), Err(NumericError::Parse));
        assert_eq!(money_from_f64(f64::INFINITY), Err(NumericError::Parse));
        assert_eq!(money_from_f64(f64::NEG_INFINITY), Err(NumericError::Parse));
        assert_eq!(quantity_from_f64(f64::NAN), Err(NumericError::Parse));
        assert_eq!(rate_from_f64(f64::NAN), Err(NumericError::Parse));
    }
}
