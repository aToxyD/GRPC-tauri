//! Central scale definitions and boundary conversion helpers (ADR-0048 §5).
//!
//! These are the **only** places where boundary scales and the rounding
//! strategy are defined. No other code in the numeric domain re-derives values
//! such as `100`, `1000` or `10000`, and no mid-arithmetic rounding is
//! performed anywhere: rounding happens exclusively here, at persistence/wire
//! boundaries.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};

use super::NumericError;

/// Money boundary scale: centimes (×10²). One scaled unit = `0.01`.
pub const MONEY_SCALE: u32 = 2;
/// Quantity boundary scale: thousandths (×10³). One scaled unit = `0.001`.
pub const QUANTITY_SCALE: u32 = 3;
/// Rate boundary scale: ten-thousandths of a percent (×10⁴).
/// One scaled unit = `0.0001%`.
pub const RATE_SCALE: u32 = 4;

/// Round `value` to `scale` decimal places using `MidpointAwayFromZero`.
///
/// Exclusively a boundary operation: it is never called in the middle of
/// arithmetic. Callers must name the boundary scale from the constants above.
#[must_use]
pub fn round_to_scale(value: Decimal, scale: u32) -> Decimal {
    value.round_dp_with_strategy(scale, RoundingStrategy::MidpointAwayFromZero)
}

/// Recover the exact `Decimal` from a boundary-scaled integer
/// (`scaled == value × 10^scale`, e.g. Money `1999` ↔ `19.99`).
///
/// Always exact. Called only with boundary scale constants (2, 3, 4), so no
/// failure is possible.
pub fn from_scaled_i64(scaled: i64, scale: u32) -> Result<Decimal, NumericError> {
    debug_assert!(scale <= 28, "boundary scales must fit the Decimal range");
    Ok(Decimal::from_i128_with_scale(scaled as i128, scale))
}

/// Normalize `value` to its boundary-scaled integer representation.
///
/// This is the **single boundary normalization point**: `value` is rounded to
/// `scale` decimal places with `MidpointAwayFromZero`, scaled by `10^scale`,
/// and converted to `i64`. Fails closed (`Overflow`) if the scaled magnitude
/// exceeds the `i64` range or the `Decimal` coefficient. Never passes through
/// `f64`.
///
/// `Decimal::to_i64` truncates toward zero, so the scaled value is verified to
/// be integral before conversion; a non-integral result is treated as an
/// overflow rather than being silently truncated.
pub fn to_scaled_i64(value: Decimal, scale: u32) -> Result<i64, NumericError> {
    let rounded = round_to_scale(value, scale);
    let scaled = rounded
        .checked_mul(pow10(scale))
        .ok_or(NumericError::Overflow)?;
    if !scaled.fract().is_zero() {
        return Err(NumericError::Overflow);
    }
    scaled.to_i64().ok_or(NumericError::Overflow)
}

fn pow10(scale: u32) -> Decimal {
    // Boundary scales are compile-time constants (2, 3, 4), so 10^scale fits
    // comfortably in i64. Guard against accidental misuse with larger scales.
    debug_assert!(scale <= 18, "boundary scale powers of ten must fit in i64");
    Decimal::from_i128_with_scale(10_i128.pow(scale), 0)
}

#[cfg(test)]
mod tests {
    use super::{
        from_scaled_i64, round_to_scale, to_scaled_i64, MONEY_SCALE, QUANTITY_SCALE, RATE_SCALE,
    };
    use crate::domain::numeric::NumericError;
    use rust_decimal::Decimal;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str_exact(s).expect(s)
    }

    #[test]
    fn decimal_addition_is_exact_for_hundredths() {
        assert_eq!(dec("0.1") + dec("0.2"), dec("0.3"));
    }

    #[test]
    fn decimal_equality_is_scale_agnostic() {
        assert_eq!(dec("0.1"), dec("0.10"));
        assert_eq!(dec("3.0"), dec("3.00"));
    }

    #[test]
    fn division_pins_internal_precision() {
        // Internal Decimal division preserves precision up to the 28-digit /
        // 96-bit coefficient limit. The quotient is NOT rounded to any
        // accounting boundary here; rounding happens only via to_scaled_i64.
        assert_eq!(
            (dec("1") / dec("3")).to_string(),
            "0.3333333333333333333333333333"
        );
        assert_eq!((dec("1") / dec("3")).scale(), 28);
        assert_eq!(
            (dec("10") / dec("6")).to_string(),
            "1.6666666666666666666666666667"
        );
        assert_eq!(
            (dec("100") / dec("7")).to_string(),
            "14.285714285714285714285714286"
        );
        assert_eq!((dec("100") / dec("7")).scale(), 27);
    }

    #[test]
    fn half_ties_round_away_from_zero() {
        assert_eq!(round_to_scale(dec("100.455"), MONEY_SCALE), dec("100.46"));
        assert_eq!(round_to_scale(dec("100.445"), MONEY_SCALE), dec("100.45"));
        assert_eq!(round_to_scale(dec("-100.455"), MONEY_SCALE), dec("-100.46"));
        assert_eq!(round_to_scale(dec("-2.505"), 2), dec("-2.51"));
        assert_eq!(round_to_scale(dec("0.0005"), QUANTITY_SCALE), dec("0.001"));
        assert_eq!(round_to_scale(dec("19.99995"), RATE_SCALE), dec("20.0000"));
    }

    #[test]
    fn scaled_i64_round_trip_is_exact() {
        let money_1999 = from_scaled_i64(1999, MONEY_SCALE).unwrap();
        assert_eq!(money_1999, dec("19.99"));
        assert_eq!(to_scaled_i64(money_1999, MONEY_SCALE).unwrap(), 1999);
        assert_eq!(to_scaled_i64(dec("0.125"), QUANTITY_SCALE).unwrap(), 125);
        assert_eq!(to_scaled_i64(dec("19.5"), RATE_SCALE).unwrap(), 195_000);
    }

    #[test]
    fn to_scaled_i64_is_boundary_rounded() {
        assert_eq!(to_scaled_i64(dec("1") / dec("3"), MONEY_SCALE).unwrap(), 33);
        assert_eq!(to_scaled_i64(dec("100.455"), MONEY_SCALE).unwrap(), 10_046);
    }

    #[test]
    fn to_scaled_i64_rejects_overflow_fail_closed() {
        // 92_233_720_368_547_758.08 × 10² = 9_223_372_036_854_775_808 > i64::MAX.
        assert_eq!(
            to_scaled_i64(dec("92233720368547758.08"), MONEY_SCALE),
            Err(NumericError::Overflow)
        );
        assert_eq!(
            to_scaled_i64(dec("92233720368547758.07"), MONEY_SCALE),
            Ok(i64::MAX)
        );
    }

    #[test]
    fn round_to_scale_scale_increase_does_not_alter_value() {
        // Requesting a coarser→finer scale (2 → 4) must not alter the value;
        // rust_decimal does not pad the stored scale attribute, only the
        // value is the invariant.
        let upsacled = round_to_scale(dec("1.5"), 4);
        assert_eq!(upsacled, dec("1.5"));
        assert_eq!(upsacled, dec("1.5000"));
    }

    #[test]
    fn to_scaled_i64_asymmetric_i64_min_edge() {
        // Public types are non-negative, so only this internal helper can
        // reach the asymmetric i64 edge: i64::MIN itself is representable, one
        // further step is not.
        assert_eq!(
            to_scaled_i64(dec("-92233720368547758.08"), MONEY_SCALE),
            Ok(i64::MIN)
        );
        assert_eq!(
            to_scaled_i64(dec("-92233720368547758.09"), MONEY_SCALE),
            Err(NumericError::Overflow)
        );
    }

    #[test]
    fn strict_decimal_parsing_rejects_non_decimal_input() {
        for bad in ["NaN", "inf", "1e3", "1E3", " 1.5", "1.5 ", "abc"] {
            assert!(
                Decimal::from_str_exact(bad).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
    }
}
