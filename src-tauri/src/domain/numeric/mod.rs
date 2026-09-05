//! Exact decimal numeric types for accounting values (ADR-0048, Phase 2).
//!
//! Accounting values are **exact `Decimal`** inside the domain and **scaled
//! integers** at persistence/wire boundaries. Boundary scales:
//!
//! * Money ×10² (centimes)
//! * Quantity ×10³ (thousandths)
//! * Rate ×10⁴ (ten-thousandths of a percent; `Rate` is percent-domain
//!   `[0, 100]`, so `19.0` means 19%)
//!
//! `Decimal` is finite: up to 28 significant digits over a 96-bit coefficient.
//! It is **not** infinite precision — division of non-terminating ratios is
//! deterministic but not exact, and all money/quantity-producing division must
//! be explicitly rounded only when converting to a boundary scale.
//!
//! Constructors never silently round: values with more decimal places than the
//! boundary scale remain representable internally (full precision preserved)
//! and are normalized only by the explicit `to_scaled_i64` boundary
//! conversion.
//!
//! Rounding is `MidpointAwayFromZero`, applied exclusively at the boundary
//! conversion point — never in the middle of arithmetic.
//!
//! The types fail closed: checked arithmetic rejects negative results
//! (`NegativeNotAllowed`) and overflow/division-by-zero; there is no
//! saturation, no `f64` pathway, and no NaN/Infinity representation.

pub(crate) mod legacy_float;
mod money;
mod quantity;
mod rate;
mod rounding;

pub use money::Money;
pub use quantity::Quantity;
pub use rate::Rate;

/// Errors raised by the numeric domain types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericError {
    /// Input string is not a valid decimal number (no `f64`, no exponent, no
    /// NaN/Infinity).
    Parse,
    /// A checked operation would produce a negative value, violating the
    /// non-negative invariant.
    NegativeNotAllowed,
    /// The result is outside the type's allowed interval (e.g. `Rate > 100`).
    OutOfRange,
    /// The operation or conversion would overflow the target representation.
    Overflow,
    /// Division by zero.
    DivisionByZero,
}

impl core::fmt::Display for NumericError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse => write!(f, "invalid decimal input"),
            Self::NegativeNotAllowed => {
                write!(f, "negative value violates the non-negative invariant")
            }
            Self::OutOfRange => write!(f, "value outside the allowed interval"),
            Self::Overflow => write!(f, "numeric overflow"),
            Self::DivisionByZero => write!(f, "division by zero"),
        }
    }
}
