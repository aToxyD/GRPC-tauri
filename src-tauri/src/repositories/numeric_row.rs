//! Repository-side mapping of scaled SQLite INTEGER columns to the domain
//! Money/Quantity/Rate types and back (Phase 4: direct INTEGER accounting schema).
//!
//! Read boundary:   INTEGER  --from_scaled_i64/from_centimes-->  domain  --*_to_f64-->  DTO f64
//! Write boundary:  domain   --to_scaled_i64-->                  INTEGER
//!
//! Repositories are row-mapping only (no arithmetic): table rows are converted
//! at the SQLite boundary into the exact domain representation, then the wire
//! (DTO) f64 value is derived from the domain value — never the reverse.
//! `f64` never enters the accounting path; the domain is constructed directly
//! from the persisted scaled integer.

use crate::domain::numeric::{legacy_float, Money, NumericError, Quantity, Rate};
use crate::errors::AppError;
use rusqlite::types::Type;
use rusqlite::Error as SqliteError;

fn conversion_error(idx: usize, source: NumericError) -> SqliteError {
    SqliteError::FromSqlConversionFailure(idx, Type::Integer, Box::new(AppError::from(source)))
}

/// (qty, money, rate) read/write helpers for scaled SQLite INTEGER columns.
/// Reads a scaled-3 Quantity column and returns the DTO f64 mantissa.
pub(crate) fn qty_col(idx: usize, scaled: i64) -> Result<f64, SqliteError> {
    Quantity::from_scaled_i64(scaled)
        .and_then(|q| legacy_float::quantity_to_f64(&q))
        .map_err(|e| conversion_error(idx, e))
}

/// Reads a scaled-2 Money column and returns the DTO f64 mantissa.
pub(crate) fn money_col(idx: usize, scaled: i64) -> Result<f64, SqliteError> {
    Money::from_centimes(scaled)
        .and_then(|m| legacy_float::money_to_f64(&m))
        .map_err(|e| conversion_error(idx, e))
}

/// Reads a scale-4 Rate column and returns the DTO f64 percent-domain value.
pub(crate) fn rate_col(idx: usize, scaled: i64) -> Result<f64, SqliteError> {
    Rate::from_scaled_i64(scaled)
        .and_then(|r| legacy_float::rate_to_f64(&r))
        .map_err(|e| conversion_error(idx, e))
}

/// Reads an optional scaled-3 Quantity column (`NULL` → `None`).
pub(crate) fn opt_qty_col(idx: usize, scaled: Option<i64>) -> Result<Option<f64>, SqliteError> {
    match scaled {
        Some(v) => Ok(Some(qty_col(idx, v)?)),
        None => Ok(None),
    }
}

/// Reads an optional scaled-2 Money column (`NULL` → `None`).
pub(crate) fn opt_money_col(idx: usize, scaled: Option<i64>) -> Result<Option<f64>, SqliteError> {
    match scaled {
        Some(v) => Ok(Some(money_col(idx, v)?)),
        None => Ok(None),
    }
}

/// Wire f64 quantity → scaled-3 INTEGER, exactly once through the domain type.
pub(crate) fn qty_scaled(wire: f64) -> Result<i64, NumericError> {
    legacy_float::quantity_from_f64(wire)?.to_scaled_i64()
}

/// Wire f64 money → scaled-2 INTEGER, exactly once through the domain type.
pub(crate) fn money_scaled(wire: f64) -> Result<i64, NumericError> {
    legacy_float::money_from_f64(wire)?.to_scaled_i64()
}

/// Wire f64 rate (percent) → scale-4 INTEGER, exactly once through the domain type.
pub(crate) fn rate_scaled(wire: f64) -> Result<i64, NumericError> {
    legacy_float::rate_from_f64(wire)?.to_scaled_i64()
}

/// Signed scale-3 reporting value (e.g. snapshot variance), may be negative —
/// pure DTO wire conversion, never an accounting domain value.
pub(crate) fn signed_qty_col(idx: usize, scaled: i64) -> Result<f64, SqliteError> {
    let _ = idx;
    Ok(scaled as f64 / 1000.0)
}

/// Nonnegative scaled-3 quantity i64 → wire f64 (single-row COALESCE/SUM reads).
pub(crate) fn qty_scaled_i64_to_f64(scaled: i64) -> Result<f64, NumericError> {
    legacy_float::quantity_to_f64(&Quantity::from_scaled_i64(scaled)?)
}

/// Aggregated Money from `SUM(qty_scaled * unit_cost_scaled)` over FIFO layers.
///
/// scale-3 × scale-2 storage makes the exact integer sum equal `value_DA × 100000`;
/// `/1000` with MidpointAwayFromZero (for non-negative aggregates) yields exact
/// centimes at the money boundary — no f64 ever enters the accounting path.
pub(crate) fn money_sum(sum: i64) -> Result<Money, NumericError> {
    if sum < 0 {
        return Err(NumericError::NegativeNotAllowed);
    }
    let centimes = sum.checked_add(500).ok_or(NumericError::Overflow)? / 1000;
    Money::from_centimes(centimes)
}

/// Wire f64 of [`money_sum`], for repositories returning DTO mantissas.
pub(crate) fn money_sum_col(sum: i64) -> Result<f64, NumericError> {
    legacy_float::money_to_f64(&money_sum(sum)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_sum_zero_is_zero() {
        assert_eq!(money_sum(0).unwrap(), Money::from_centimes(0).unwrap());
    }

    #[test]
    fn money_sum_rounds_boundary_values_to_centimes() {
        assert_eq!(
            money_sum(100_000).unwrap(),
            Money::from_centimes(100).unwrap()
        );
        assert_eq!(
            money_sum(100_499).unwrap(),
            Money::from_centimes(100).unwrap()
        );
        assert_eq!(
            money_sum(100_500).unwrap(),
            Money::from_centimes(101).unwrap()
        );
    }

    #[test]
    fn money_sum_fails_closed_on_negative_aggregate() {
        assert_eq!(money_sum(-1), Err(NumericError::NegativeNotAllowed));
        assert_eq!(money_sum(i64::MIN), Err(NumericError::NegativeNotAllowed));
    }

    #[test]
    fn money_sum_fails_closed_on_overflow() {
        assert_eq!(money_sum(i64::MAX), Err(NumericError::Overflow));
    }
}
