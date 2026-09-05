//! Exact-decimal money (ADR-0048).
//!
//! Invariant: non-negative (`>= 0`), boundary scale 2 (centimes).
//!
//! Internal representation preserves full `Decimal` precision: values with
//! more than 2 decimal places remain representable and are normalized only by
//! [`Money::to_scaled_i64`]. No `Deref`/`AsRef`, no `From<f64>`, no silent
//! rounding at construction.

use rust_decimal::Decimal;

use super::rounding::{from_scaled_i64, to_scaled_i64, MONEY_SCALE};
use super::NumericError;

/// Exact-decimal money, non-negative, boundary scale 2 (centimes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Money(Decimal);

impl Money {
    /// Construct from centimes (boundary-scaled integer at scale 2).
    pub fn from_centimes(centimes: i64) -> Result<Self, NumericError> {
        if centimes < 0 {
            return Err(NumericError::NegativeNotAllowed);
        }
        Ok(Self(from_scaled_i64(centimes, MONEY_SCALE)?))
    }

    /// Construct from an exact `Decimal`. No rounding is applied; only the
    /// non-negative invariant is enforced.
    pub fn from_decimal(value: Decimal) -> Result<Self, NumericError> {
        if value.is_sign_negative() && !value.is_zero() {
            return Err(NumericError::NegativeNotAllowed);
        }
        Ok(Self(value))
    }

    /// Strict decimal parse (no `f64`, no exponent, no NaN/Infinity).
    pub fn parse_str(input: &str) -> Result<Self, NumericError> {
        let value = Decimal::from_str_exact(input).map_err(|_| NumericError::Parse)?;
        Self::from_decimal(value)
    }

    /// Boundary-scaled integer representation at scale 2 (centimes).
    ///
    /// The **single normalization point**: rounds once with
    /// `MidpointAwayFromZero`, fails closed (`Overflow`) if the scaled
    /// magnitude exceeds the `i64` range. Never passes through `f64`.
    pub fn to_scaled_i64(&self) -> Result<i64, NumericError> {
        to_scaled_i64(self.0, MONEY_SCALE)
    }

    /// `true` when the money value is exactly zero.
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// `true` when the money value is strictly greater than zero.
    pub fn is_positive(&self) -> bool {
        self.0.is_sign_positive() && !self.0.is_zero()
    }

    /// Exact addition. Fails closed on `Decimal` overflow.
    pub fn checked_add(self, rhs: Self) -> Result<Self, NumericError> {
        let value = self.0.checked_add(rhs.0).ok_or(NumericError::Overflow)?;
        Ok(Self(value))
    }

    /// Exact subtraction. Fails closed with `NegativeNotAllowed` when the
    /// result would be negative; never saturates to zero.
    pub fn checked_sub(self, rhs: Self) -> Result<Self, NumericError> {
        let value = self.0.checked_sub(rhs.0).ok_or(NumericError::Overflow)?;
        Self::from_decimal(value)
    }

    /// Exact multiplication. Fails closed on overflow.
    pub fn checked_mul(self, rhs: Self) -> Result<Self, NumericError> {
        let value = self.0.checked_mul(rhs.0).ok_or(NumericError::Overflow)?;
        Self::from_decimal(value)
    }

    /// Exact division. Fails closed on division by zero or overflow. The
    /// quotient keeps full `Decimal` precision; rounding to scale 2 happens
    /// only via [`Self::to_scaled_i64`].
    pub fn checked_div(self, rhs: Self) -> Result<Self, NumericError> {
        if rhs.0.is_zero() {
            return Err(NumericError::DivisionByZero);
        }
        let value = self.0.checked_div(rhs.0).ok_or(NumericError::Overflow)?;
        Self::from_decimal(value)
    }

    /// Multiply by an integer factor (e.g. line quantity). Exact; fails
    /// closed on overflow.
    pub fn checked_mul_scalar(self, factor: i64) -> Result<Self, NumericError> {
        let value = self
            .0
            .checked_mul(Decimal::from(factor))
            .ok_or(NumericError::Overflow)?;
        Self::from_decimal(value)
    }
}

#[cfg(test)]
mod tests {
    use super::Money;
    use crate::domain::numeric::NumericError;
    use rust_decimal::Decimal;

    fn money(s: &str) -> Money {
        Money::parse_str(s).unwrap()
    }

    #[test]
    fn zero_and_positive_are_exact() {
        assert!(money("0").is_zero());
        assert!(money("0.00").is_zero());
        assert!(money("1").is_positive());
        assert_eq!(money("1"), money("1.00"));
    }

    #[test]
    fn centimes_constructor_and_scaled_round_trip() {
        assert_eq!(
            Money::from_centimes(1999).unwrap().to_scaled_i64().unwrap(),
            1999
        );
        assert_eq!(money("19.99").to_scaled_i64().unwrap(), 1999);
        assert_eq!(money("100.00").to_scaled_i64().unwrap(), 10_000);
        assert_eq!(money("0.01").to_scaled_i64().unwrap(), 1);
    }

    #[test]
    fn addition_is_exact_decimal_addition() {
        assert_eq!(
            money("0.10").checked_add(money("0.20")).unwrap(),
            money("0.30")
        );
        assert_eq!(
            money("10.00")
                .checked_add(money("5.50"))
                .unwrap()
                .to_scaled_i64()
                .unwrap(),
            1550
        );
    }

    #[test]
    fn subtraction_is_exact_and_fails_closed_on_negative() {
        assert_eq!(
            money("10.00")
                .checked_sub(money("5.50"))
                .unwrap()
                .to_scaled_i64()
                .unwrap(),
            450
        );
        let zero = money("5.00").checked_sub(money("5.00")).unwrap();
        assert!(zero.is_zero());
        assert_eq!(
            money("5.00").checked_sub(money("5.01")),
            Err(NumericError::NegativeNotAllowed)
        );
    }

    #[test]
    fn scalar_multiply_is_exact() {
        // 19.99 × 3 must be exactly 59.97 — never a binary-float artifact.
        let product = money("19.99").checked_mul_scalar(3).unwrap();
        assert_eq!(product.to_scaled_i64().unwrap(), 5997);
    }

    #[test]
    fn multiply_is_exact() {
        let product = money("19.99").checked_mul(money("3.00")).unwrap();
        assert_eq!(product.to_scaled_i64().unwrap(), 5997);
    }

    #[test]
    fn division_rounds_only_at_boundary_conversion() {
        // 1.00 / 3.00 keeps 28 internal digits; 0.33 results only from the
        // explicit boundary conversion (MidpointAwayFromZero of 0.333... → 0.33).
        let q = money("1.00").checked_div(money("3.00")).unwrap();
        assert_eq!(q.to_scaled_i64().unwrap(), 33);
    }

    #[test]
    fn division_by_zero_fails_closed() {
        assert_eq!(
            money("1.00").checked_div(money("0.00")),
            Err(NumericError::DivisionByZero)
        );
    }

    #[test]
    fn half_ties_at_scale_two_round_away_from_zero() {
        assert_eq!(money("100.455").to_scaled_i64().unwrap(), 10_046);
        assert_eq!(money("100.445").to_scaled_i64().unwrap(), 10_045);
    }

    #[test]
    fn oversize_rejects_at_boundary_conversion() {
        // 92_233_720_368_547_758.08 × 10² exceeds i64::MAX.
        assert_eq!(
            money("92233720368547758.08").to_scaled_i64(),
            Err(NumericError::Overflow)
        );
    }

    #[test]
    fn negative_values_are_rejected() {
        assert_eq!(
            Money::parse_str("-0.01"),
            Err(NumericError::NegativeNotAllowed)
        );
        assert_eq!(
            Money::from_centimes(-1),
            Err(NumericError::NegativeNotAllowed)
        );
        let negative = Decimal::from_str_exact("-1.00").unwrap();
        assert_eq!(
            Money::from_decimal(negative),
            Err(NumericError::NegativeNotAllowed)
        );
    }

    #[test]
    fn strict_parsing_rejects_non_decimal_input() {
        for bad in ["abc", "1e3", "NaN", "inf", "19.99.5"] {
            assert_eq!(Money::parse_str(bad), Err(NumericError::Parse), "{bad:?}");
        }
    }

    #[test]
    fn empty_string_is_rejected() {
        assert_eq!(Money::parse_str(""), Err(NumericError::Parse));
        assert_eq!(Money::parse_str("   "), Err(NumericError::Parse));
    }

    #[test]
    fn leading_plus_and_underscore_separators_pin_observed_behavior() {
        // rust_decimal's exact parser accepts a leading `+` and `_` digit
        // separators; ADR-0048 only forbids f64/exponent/NaN/Infinity, so this
        // observed behavior is pinned rather than made stricter.
        assert_eq!(Money::parse_str("+19.99").unwrap(), money("19.99"));
        assert_eq!(Money::parse_str("1_9.9_9").unwrap(), money("19.99"));
    }

    #[test]
    fn negative_zero_parses_as_zero() {
        // "-0.0" is accepted as zero: non-negative means no value below 0, and
        // -0.0 and +0.0 are equal under Decimal's value-based comparison.
        let neg_zero = Money::parse_str("-0.0").unwrap();
        assert_eq!(neg_zero, money("0.00"));
        assert!(neg_zero.is_zero());
        assert!(!neg_zero.is_positive());
    }

    #[test]
    fn five_decimal_money_is_preserved_internally_and_rounds_at_boundary() {
        // A 5-dp input is NOT rounded at construction: internal scale stays 5
        // and normalization to scale 2 happens only at the boundary conversion
        // (MidpointAwayFromZero: 19.99500 → 20.00, 19.99499 → 19.99).
        let m = money("19.99500");
        assert_eq!(m.0.scale(), 5);
        assert_eq!(m.to_scaled_i64().unwrap(), 2000);
        assert_eq!(money("19.99499").to_scaled_i64().unwrap(), 1999);
    }

    #[test]
    fn scalar_multiply_zero_and_negative_factor() {
        let zero = money("19.99").checked_mul_scalar(0).unwrap();
        assert!(zero.is_zero());
        assert_eq!(zero.to_scaled_i64().unwrap(), 0);
        assert_eq!(
            money("19.99").checked_mul_scalar(-1),
            Err(NumericError::NegativeNotAllowed)
        );
    }

    #[test]
    fn decimal_coefficient_overflow_fails_closed() {
        // At the 96-bit coefficient boundary the next representable step
        // overflows the Decimal itself, not just the i64 scaled surface.
        let max = Money::from_decimal(Decimal::MAX).unwrap();
        let one = Money::from_decimal(Decimal::ONE).unwrap();
        assert_eq!(max.checked_add(one), Err(NumericError::Overflow));
        assert_eq!(max.checked_mul_scalar(2), Err(NumericError::Overflow));
    }

    #[test]
    fn boundary_round_trip_is_idempotent() {
        for i in 0..=10_000_i64 {
            assert_eq!(Money::from_centimes(i).unwrap().to_scaled_i64().unwrap(), i);
        }
        assert_eq!(
            Money::from_centimes(i64::MAX)
                .unwrap()
                .to_scaled_i64()
                .unwrap(),
            i64::MAX
        );
    }
}
