//! Exact-decimal quantity (ADR-0048).
//!
//! Invariant: non-negative (`>= 0`), boundary scale 3 (thousandths).
//!
//! Sign of intermediates is out of scope for this type: `checked_sub` fails
//! closed with `NegativeNotAllowed` on a negative result and never saturates.
//! Signed intermediate arithmetic is an explicit Phase 3 concern.

use rust_decimal::Decimal;

use super::rounding::{from_scaled_i64, to_scaled_i64, QUANTITY_SCALE};
use super::NumericError;

/// Exact-decimal quantity, non-negative, boundary scale 3 (thousandths).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Quantity(Decimal);

impl Quantity {
    /// The zero value.
    pub fn zero() -> Self {
        Self(Decimal::ZERO)
    }

    /// Construct from a boundary-scaled integer at scale 3 (thousandths).
    pub fn from_scaled_i64(scaled: i64) -> Result<Self, NumericError> {
        if scaled < 0 {
            return Err(NumericError::NegativeNotAllowed);
        }
        Ok(Self(from_scaled_i64(scaled, QUANTITY_SCALE)?))
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

    /// Boundary-scaled integer representation at scale 3 (thousandths).
    ///
    /// The **single normalization point**: rounds once with
    /// `MidpointAwayFromZero`, fails closed (`Overflow`) if the scaled
    /// magnitude exceeds the `i64` range. Never passes through `f64`.
    pub fn to_scaled_i64(&self) -> Result<i64, NumericError> {
        to_scaled_i64(self.0, QUANTITY_SCALE)
    }

    /// `true` when the quantity is exactly zero.
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// `true` when the quantity is strictly greater than zero.
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
    /// quotient keeps full `Decimal` precision; rounding to scale 3 happens
    /// only via [`Self::to_scaled_i64`].
    pub fn checked_div(self, rhs: Self) -> Result<Self, NumericError> {
        if rhs.0.is_zero() {
            return Err(NumericError::DivisionByZero);
        }
        let value = self.0.checked_div(rhs.0).ok_or(NumericError::Overflow)?;
        Self::from_decimal(value)
    }

    /// Exact multiplication by an integer scalar. Fails closed on overflow and
    /// on a negative product.
    pub fn checked_mul_scalar(self, factor: i64) -> Result<Self, NumericError> {
        let value = self
            .0
            .checked_mul(Decimal::from(factor))
            .ok_or(NumericError::Overflow)?;
        Self::from_decimal(value)
    }

    /// Internal exact-`Decimal` accessor — reserved for the numeric module and
    /// the legacy wire adapter; never exposed outside the crate.
    pub(crate) fn raw(&self) -> Decimal {
        self.0
    }
}

impl core::fmt::Display for Quantity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::Quantity;
    use crate::domain::numeric::NumericError;
    use rust_decimal::Decimal;

    fn quantity(s: &str) -> Quantity {
        Quantity::parse_str(s).unwrap()
    }

    #[test]
    fn parsing_and_scaled_round_trip() {
        assert!(quantity("0").is_zero());
        assert_eq!(quantity("0.001").to_scaled_i64().unwrap(), 1);
        assert_eq!(quantity("0.125").to_scaled_i64().unwrap(), 125);
        assert_eq!(quantity("1").to_scaled_i64().unwrap(), 1000);
        assert_eq!(quantity("1.000").to_scaled_i64().unwrap(), 1000);
        assert_eq!(Quantity::from_scaled_i64(125).unwrap(), quantity("0.125"));
    }

    #[test]
    fn exact_subtraction_leaves_no_dust() {
        // 8.000 − 5.000 == 3.000 exactly (scale preserved, value exact).
        let result = quantity("8.000").checked_sub(quantity("5.000")).unwrap();
        assert_eq!(result, quantity("3.000"));
        assert_eq!(result.to_scaled_i64().unwrap(), 3000);
        assert_eq!(result.0.scale(), 3);
    }

    #[test]
    fn repeated_fractional_subtraction_is_exact() {
        // 1.000 − 0.333 − 0.333 − 0.333 == 0.001; no binary-float residue.
        let mut remaining = quantity("1.000");
        for _ in 0..3 {
            remaining = remaining.checked_sub(quantity("0.333")).unwrap();
        }
        assert!(!remaining.is_zero());
        assert!(remaining.is_positive());
        assert_eq!(remaining.to_scaled_i64().unwrap(), 1);
    }

    #[test]
    fn above_boundary_precision_is_preserved_internally() {
        // A 4-dp input is NOT rounded at construction: internal scale stays 4
        // and normalization to scale 3 happens only at the boundary conversion
        // (MidpointAwayFromZero: 0.1255 → 0.126).
        let q = quantity("0.1255");
        assert_eq!(q.0.scale(), 4);
        assert_eq!(q.to_scaled_i64().unwrap(), 126);

        let diff = quantity("8.0000").checked_sub(quantity("5.0000")).unwrap();
        assert_eq!(diff.0.scale(), 4);
        assert_eq!(diff.to_scaled_i64().unwrap(), 3000);
        assert_eq!(
            quantity("8.0000").checked_sub(quantity("5.0000")).unwrap(),
            quantity("3.000")
        );
    }

    #[test]
    fn half_ties_at_scale_three_round_away_from_zero() {
        assert_eq!(quantity("0.0005").to_scaled_i64().unwrap(), 1);
        assert_eq!(quantity("0.0015").to_scaled_i64().unwrap(), 2);
    }

    #[test]
    fn exact_division_preserves_internal_precision() {
        let q = quantity("1.000").checked_div(quantity("3.000")).unwrap();
        assert_eq!(q.0.scale(), 28);
        assert_eq!(q.to_scaled_i64().unwrap(), 333);
    }

    #[test]
    fn division_by_zero_fails_closed() {
        assert_eq!(
            quantity("1.000").checked_div(quantity("0.000")),
            Err(NumericError::DivisionByZero)
        );
    }

    #[test]
    fn negative_subtraction_fails_closed() {
        assert_eq!(
            quantity("5.000").checked_sub(quantity("5.001")),
            Err(NumericError::NegativeNotAllowed)
        );
    }

    #[test]
    fn negative_values_are_rejected() {
        assert_eq!(
            Quantity::parse_str("-0.001"),
            Err(NumericError::NegativeNotAllowed)
        );
        assert_eq!(
            Quantity::from_scaled_i64(-1),
            Err(NumericError::NegativeNotAllowed)
        );
    }

    #[test]
    fn oversize_rejects_at_boundary_conversion() {
        // 9_223_372_036_854_775.808 × 10³ exceeds i64::MAX.
        assert_eq!(
            quantity("9223372036854775.808").to_scaled_i64(),
            Err(NumericError::Overflow)
        );
    }

    #[test]
    fn strict_parsing_rejects_non_decimal_input() {
        for bad in ["abc", "1e3", "NaN", "inf", "0.1.2", "", "   "] {
            assert_eq!(
                Quantity::parse_str(bad),
                Err(NumericError::Parse),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn negative_zero_parses_as_zero() {
        let neg_zero = Quantity::parse_str("-0.0").unwrap();
        assert_eq!(neg_zero, quantity("0.000"));
        assert!(neg_zero.is_zero());
        assert!(!neg_zero.is_positive());
    }

    #[test]
    fn non_terminating_division_boundary_rounds_away() {
        // 2.000 / 3.000 = 0.6666... rounds at the scale-3 boundary with
        // MidpointAwayFromZero: 0.6665 half-tie is crossed → 667.
        let q = quantity("2.000").checked_div(quantity("3.000")).unwrap();
        assert_eq!(q.to_scaled_i64().unwrap(), 667);
    }

    #[test]
    fn decimal_coefficient_overflow_fails_closed() {
        let max = Quantity::from_decimal(Decimal::MAX).unwrap();
        let one = Quantity::from_decimal(Decimal::ONE).unwrap();
        assert_eq!(max.checked_add(one), Err(NumericError::Overflow));
    }

    #[test]
    fn boundary_round_trip_is_idempotent() {
        for i in 0..=10_000_i64 {
            assert_eq!(
                Quantity::from_scaled_i64(i)
                    .unwrap()
                    .to_scaled_i64()
                    .unwrap(),
                i
            );
        }
    }

    #[test]
    fn scalar_multiply_is_exact() {
        let q = quantity("0.333").checked_mul_scalar(3).unwrap();
        assert_eq!(q, quantity("0.999"));
        assert_eq!(q.to_scaled_i64().unwrap(), 999);
        let zero = quantity("10.000").checked_mul_scalar(0).unwrap();
        assert!(zero.is_zero());
        assert_eq!(
            quantity("0.333").checked_mul_scalar(-1),
            Err(NumericError::NegativeNotAllowed)
        );
    }

    #[test]
    fn ordering_is_exact() {
        assert!(quantity("0.333") < quantity("0.334"));
        assert!(quantity("0.001") < quantity("0.01"));
        assert_eq!(quantity("0.1000"), quantity("0.10"));
        assert!(quantity("1000.000") > quantity("999.999"));
    }

    #[test]
    fn display_renders_exact_value() {
        assert_eq!(quantity("0.333").to_string(), "0.333");
        assert_eq!(quantity("0.00").to_string(), "0.00");
    }
}
