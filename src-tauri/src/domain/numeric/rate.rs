//! Exact-decimal rate, percent-domain (ADR-0048).
//!
//! Invariant: `Rate ∈ [0, 100]`. `Rate` is a **percentage**, not a fraction:
//! `19.0` represents 19% (boundary scale 4 → `190_000`). The pre-existing
//! `tva_rate: 0.19` fixtures in some test datasets represent the fraction form
//! and are corrected during Phase 3; they never redefine this contract.
//!
//! Construction rejects values outside `[0, 100]` with `OutOfRange`. Checked
//! operations fail closed: a negative result yields `NegativeNotAllowed`, a
//! result above 100 yields `OutOfRange`.

use rust_decimal::Decimal;

use super::rounding::{from_scaled_i64, to_scaled_i64, RATE_SCALE};
use super::NumericError;

const RATE_MAX: i64 = 100;

/// Exact-decimal percentage rate in the closed interval `[0, 100]`,
/// boundary scale 4 (ten-thousandths of a percent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rate(Decimal);

impl Rate {
    /// Construct from a boundary-scaled integer at scale 4
    /// (ten-thousandths of a percent); e.g. `190_000` == 19%.
    pub fn from_scaled_i64(scaled: i64) -> Result<Self, NumericError> {
        let value = from_scaled_i64(scaled, RATE_SCALE)?;
        Self::from_decimal(value)
    }

    /// Construct from an exact `Decimal` percent. No rounding is applied;
    /// the `[0, 100]` invariant is enforced.
    pub fn from_decimal(value: Decimal) -> Result<Self, NumericError> {
        Self::enforce_range(value)
    }

    /// Strict decimal parse (no `f64`, no exponent, no NaN/Infinity).
    pub fn parse_str(input: &str) -> Result<Self, NumericError> {
        let value = Decimal::from_str_exact(input).map_err(|_| NumericError::Parse)?;
        Self::from_decimal(value)
    }

    /// Boundary-scaled integer representation at scale 4 (ten-thousandths of
    /// a percent).
    ///
    /// The **single normalization point**: rounds once with
    /// `MidpointAwayFromZero`, fails closed (`Overflow`) if the scaled
    /// magnitude exceeds the `i64` range. Never passes through `f64`.
    pub fn to_scaled_i64(&self) -> Result<i64, NumericError> {
        to_scaled_i64(self.0, RATE_SCALE)
    }

    /// `true` when the rate is exactly 0%.
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// Exact addition. Fails closed with `OutOfRange` when the sum exceeds
    /// 100%.
    pub fn checked_add(self, rhs: Self) -> Result<Self, NumericError> {
        let value = self.0.checked_add(rhs.0).ok_or(NumericError::Overflow)?;
        Self::after_checked_op(value)
    }

    /// Exact subtraction. Fails closed with `NegativeNotAllowed` when the
    /// result would be negative.
    pub fn checked_sub(self, rhs: Self) -> Result<Self, NumericError> {
        let value = self.0.checked_sub(rhs.0).ok_or(NumericError::Overflow)?;
        Self::after_checked_op(value)
    }

    /// Exact multiplication. Fails closed when the result leaves `[0, 100]`.
    pub fn checked_mul(self, rhs: Self) -> Result<Self, NumericError> {
        let value = self.0.checked_mul(rhs.0).ok_or(NumericError::Overflow)?;
        Self::after_checked_op(value)
    }

    /// Exact division. Fails closed on division by zero or when the result
    /// leaves `[0, 100]`.
    pub fn checked_div(self, rhs: Self) -> Result<Self, NumericError> {
        if rhs.0.is_zero() {
            return Err(NumericError::DivisionByZero);
        }
        let value = self.0.checked_div(rhs.0).ok_or(NumericError::Overflow)?;
        Self::after_checked_op(value)
    }

    fn enforce_range(value: Decimal) -> Result<Self, NumericError> {
        let max = Decimal::from(RATE_MAX);
        if value.is_sign_negative() && !value.is_zero() {
            return Err(NumericError::OutOfRange);
        }
        if value > max {
            return Err(NumericError::OutOfRange);
        }
        Ok(Self(value))
    }

    fn after_checked_op(value: Decimal) -> Result<Self, NumericError> {
        if value.is_sign_negative() && !value.is_zero() {
            return Err(NumericError::NegativeNotAllowed);
        }
        Self::enforce_range(value)
    }
}

#[cfg(test)]
mod tests {
    use super::Rate;
    use crate::domain::numeric::NumericError;

    fn rate(s: &str) -> Rate {
        Rate::parse_str(s).unwrap()
    }

    #[test]
    fn percent_domain_scaled_representations() {
        assert!(rate("0").is_zero());
        assert_eq!(rate("0").to_scaled_i64().unwrap(), 0);
        assert_eq!(rate("19").to_scaled_i64().unwrap(), 190_000);
        assert_eq!(rate("19.0").to_scaled_i64().unwrap(), 190_000);
        assert_eq!(rate("19.5").to_scaled_i64().unwrap(), 195_000);
        assert_eq!(rate("7.07").to_scaled_i64().unwrap(), 70_700);
        assert_eq!(rate("100").to_scaled_i64().unwrap(), 1_000_000);
        assert_eq!(Rate::from_scaled_i64(190_000).unwrap(), rate("19.0"));
    }

    #[test]
    fn percent_not_fraction() {
        // 19.0 parses as the percent value 19, not the fraction 0.19.
        assert_eq!(rate("19.0").to_scaled_i64().unwrap(), 190_000);
        assert_ne!(rate("19.0").to_scaled_i64().unwrap(), 19);
    }

    #[test]
    fn values_outside_zero_to_hundred_are_rejected() {
        assert_eq!(Rate::parse_str("-0.0001"), Err(NumericError::OutOfRange));
        assert_eq!(Rate::parse_str("-19"), Err(NumericError::OutOfRange));
        assert_eq!(Rate::parse_str("100.0001"), Err(NumericError::OutOfRange));
        assert_eq!(Rate::from_scaled_i64(-1), Err(NumericError::OutOfRange));
    }

    #[test]
    fn half_ties_at_scale_four_round_away_from_zero() {
        assert_eq!(rate("19.99995").to_scaled_i64().unwrap(), 200_000);
        assert_eq!(rate("19.99994").to_scaled_i64().unwrap(), 199_999);
    }

    #[test]
    fn checked_add_beyond_hundred_fails_closed() {
        assert_eq!(
            rate("60").checked_add(rate("60")),
            Err(NumericError::OutOfRange)
        );
        assert_eq!(
            rate("40")
                .checked_add(rate("50"))
                .unwrap()
                .to_scaled_i64()
                .unwrap(),
            900_000
        );
    }

    #[test]
    fn checked_sub_negative_fails_closed() {
        assert_eq!(
            rate("5").checked_sub(rate("6")),
            Err(NumericError::NegativeNotAllowed)
        );
        assert!(rate("6").checked_sub(rate("6")).unwrap().is_zero());
    }

    #[test]
    fn division_by_zero_fails_closed() {
        assert_eq!(
            rate("60").checked_div(rate("0")),
            Err(NumericError::DivisionByZero)
        );
    }

    #[test]
    fn scaled_overflow_rejects_fail_closed() {
        // The [0,100] domain × 10⁴ can never overflow i64, so the guard is
        // construction-time range enforcement: an oversized scaled integer is
        // rejected instead of silently accepted.
        assert_eq!(
            Rate::from_scaled_i64(i64::MAX),
            Err(NumericError::OutOfRange)
        );
        assert_eq!(
            Rate::from_scaled_i64(1_000_001),
            Err(NumericError::OutOfRange)
        );
        assert_eq!(
            Rate::from_scaled_i64(1_000_000)
                .unwrap()
                .to_scaled_i64()
                .unwrap(),
            1_000_000
        );
    }

    #[test]
    fn strict_parsing_rejects_non_decimal_input() {
        for bad in ["abc", "1e3", "NaN", "inf", "19..5", "", "   "] {
            assert_eq!(Rate::parse_str(bad), Err(NumericError::Parse), "{bad:?}");
        }
    }

    #[test]
    fn percent_quantum_units() {
        // One rate unit is one ten-thousandth of a percent (scale 4).
        assert_eq!(rate("0.0001").to_scaled_i64().unwrap(), 1);
        assert_eq!(rate("99.9999").to_scaled_i64().unwrap(), 999_999);
    }

    #[test]
    fn negative_zero_parses_as_zero() {
        let neg_zero = Rate::parse_str("-0.0").unwrap();
        assert_eq!(neg_zero, rate("0"));
        assert!(neg_zero.is_zero());
    }

    #[test]
    fn boundary_round_trip_is_idempotent() {
        // The full [0, 100] rate domain is only 1_000_000 scaled units, so the
        // round-trip is pinned exhaustively and stays fast.
        for i in 0..=1_000_000_i64 {
            assert_eq!(
                Rate::from_scaled_i64(i).unwrap().to_scaled_i64().unwrap(),
                i
            );
        }
    }
}
