//! Deterministic contract pricing arithmetic (SEC-087 / ADR-0055 / ADR-0048).
//!
//! Single owner of the HT → TVA → TTC contract pricing chain so presentation
//! and reporting layers never reimplement financial arithmetic (P2 / A5).
//! All values are **exact `Decimal`** (ADR-0048); the only rounding is the
//! scale-2 boundary normalization at a defined point.
//!
//! Canonical contract chain (SEC-087):
//!
//! ```text
//! tva_amount = round_2dp(HT × rate / 100)     // tax TERM rounded once
//! price_ttc  = HT + tva_amount
//! ```
//!
//! The tax **term** is rounded to scale 2 (`MidpointAwayFromZero`); `price_ttc`
//! is always `HT + tva_amount`. Whole-price `HT × (1 + rate/100)` rounding is
//! deliberately NOT the canonical path: it can diverge from the tax-term rule
//! at half-cent boundaries. This module is the single implementation.

use rust_decimal::Decimal;

use crate::domain::numeric::{Money, NumericError, Rate};

/// Exact breakdown of the canonical contract fiscal chain (SEC-087).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractFiscalBreakdown {
    /// The tax term `round_2dp(HT × rate/100)`, scale 2.
    pub tva_amount: Money,
    /// `HT + tva_amount` — the per-purchase-unit TTC operational cost base.
    pub price_ttc: Money,
}

/// Canonical HT → TVA → TTC contract pricing (SEC-087).
///
/// `rate` is a percent-domain [`Rate`] (e.g. `19.0` means 19%). The tax term is
/// rounded to scale 2 exactly once with `MidpointAwayFromZero` at the boundary
/// conversion point (ADR-0048). Fails closed on overflow; never passes through
/// `f64`.
pub fn compute_contract_fiscal(
    ht: &Money,
    rate: &Rate,
) -> Result<ContractFiscalBreakdown, NumericError> {
    // Exact tax = HT × rate% — rate is percent-domain Decimal, so ÷100 is exact.
    // Use checked_mul to fail closed on overflow rather than panicking.
    let product = ht
        .raw()
        .checked_mul(rate.raw())
        .ok_or(NumericError::Overflow)?;
    let tax = product
        .checked_div(Decimal::from(100))
        .ok_or(NumericError::Overflow)?;
    // Full-precision Money; rounding happens once at to_scaled_i64 (ADR-0048).
    let tva_unrounded = Money::from_decimal(tax)?;
    let tva_amount = Money::from_centimes(tva_unrounded.to_scaled_i64()?)?;
    let price_ttc = ht.checked_add(tva_amount)?;
    Ok(ContractFiscalBreakdown {
        tva_amount,
        price_ttc,
    })
}

/// Per-consumption-unit cost from an exact purchase-funds cost and the
/// purchase→consumption conversion factor (SEC-087).
///
/// `purchase_cost / conversion_factor` is rounded **once** to scale-2 [`Money`]
/// (`MidpointAwayFromZero`). The quotient may be non-terminating (e.g.
/// `7.00 / 3 → 2.33` with a 0.01 residue); the exact purchase value is
/// preserved authoritatively in the persisted purchase-snapshot columns, never
/// re-derived here. Fails closed on division by zero and on a negative result.
pub fn conversion_cost(
    purchase_cost: &Money,
    conversion_factor: i64,
) -> Result<Money, NumericError> {
    let per_unit = purchase_cost.checked_div_scalar(conversion_factor)?;
    // Rounding to scale 2 happens once, at this boundary conversion (ADR-0048).
    Money::from_centimes(per_unit.to_scaled_i64()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::numeric::{Money, Rate};
    use crate::domain::units::TvaClassification;

    fn money(s: &str) -> Money {
        Money::parse_str(s).unwrap()
    }

    fn rate(s: &str) -> Rate {
        Rate::parse_str(s).unwrap()
    }

    #[test]
    fn zero_rate_is_exact_identity() {
        let fb = compute_contract_fiscal(&money("100.00"), &rate("0")).unwrap();
        assert_eq!(fb.tva_amount, money("0.00"));
        assert_eq!(fb.price_ttc, money("100.00"));
    }

    #[test]
    fn standard_rate_is_exact_at_cent_boundary() {
        // 200.00 @ 19% → tax 38.00, TTC 238.00 exactly.
        let fb = compute_contract_fiscal(&money("200.00"), &rate("19.0")).unwrap();
        assert_eq!(fb.tva_amount, money("38.00"));
        assert_eq!(fb.tva_amount.to_scaled_i64().unwrap(), 3800);
        assert_eq!(fb.price_ttc, money("238.00"));
        assert_eq!(fb.price_ttc.to_scaled_i64().unwrap(), 23800);
    }

    #[test]
    fn plan_boundary_case_199_97_at_19_percent() {
        // SEC-087 acceptance: 199.97 @ 19% → TVA 37.99 → TTC 237.96.
        let fb = compute_contract_fiscal(&money("199.97"), &rate("19")).unwrap();
        assert_eq!(fb.tva_amount, money("37.99"));
        assert_eq!(fb.tva_amount.to_scaled_i64().unwrap(), 3799);
        assert_eq!(fb.price_ttc, money("237.96"));
        assert_eq!(fb.price_ttc.to_scaled_i64().unwrap(), 23796);
    }

    #[test]
    fn tax_term_rounds_once_away_from_zero_at_half_cent() {
        // 133.00 @ 7.5% → tax exactly 9.975 → 9.98 (MidpointAwayFromZero).
        let fb = compute_contract_fiscal(&money("133.00"), &rate("7.5")).unwrap();
        assert_eq!(fb.tva_amount.to_scaled_i64().unwrap(), 998);
        assert_eq!(fb.price_ttc.to_scaled_i64().unwrap(), 14298);
        // Just below the midpoint: 9.9617 → 9.96.
        let below = compute_contract_fiscal(&money("133.00"), &rate("7.49")).unwrap();
        assert_eq!(below.tva_amount.to_scaled_i64().unwrap(), 996);
    }

    #[test]
    fn classification_rates_are_the_only_sourced_rates() {
        // The product's TVA classification is the single rate source.
        let cases = [
            (TvaClassification::Exonere, "100.00", 0, 10000),
            (TvaClassification::NinePercent, "200.00", 1800, 21800),
            (TvaClassification::NineteenPercent, "199.97", 3799, 23796),
        ];
        for (class, ht, tax_cents, ttc_cents) in cases {
            let fb = compute_contract_fiscal(&money(ht), &class.rate()).unwrap();
            assert_eq!(
                fb.tva_amount.to_scaled_i64().unwrap(),
                tax_cents,
                "{class:?}"
            );
            assert_eq!(
                fb.price_ttc.to_scaled_i64().unwrap(),
                ttc_cents,
                "{class:?}"
            );
        }
    }

    #[test]
    fn tax_is_term_rounded_never_whole_price() {
        // 19.99 @ 19% → tax 3.7981 → 3.80; TTC 23.79.
        let fb = compute_contract_fiscal(&money("19.99"), &rate("19")).unwrap();
        assert_eq!(fb.tva_amount.to_scaled_i64().unwrap(), 380);
        assert_eq!(fb.price_ttc.to_scaled_i64().unwrap(), 2379);
        // The tax term equals TTC − HT exactly (no straddle).
        let implied = fb.price_ttc.checked_sub(money("19.99")).unwrap();
        assert_eq!(implied, fb.tva_amount);
    }

    #[test]
    fn zero_ht_stays_zero() {
        let fb = compute_contract_fiscal(&money("0.00"), &rate("17")).unwrap();
        assert_eq!(fb.tva_amount, money("0.00"));
        assert_eq!(fb.price_ttc, money("0.00"));
    }

    #[test]
    fn repeatable_across_calls() {
        let a = compute_contract_fiscal(&money("133.00"), &rate("7.5")).unwrap();
        let b = compute_contract_fiscal(&money("133.00"), &rate("7.5")).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn overflow_fails_closed() {
        // At the 96-bit Decimal coefficient boundary, HT + tax doubles the
        // coefficient and must overflow — never wrap.
        let big = Money::from_decimal(Decimal::MAX).unwrap();
        assert!(compute_contract_fiscal(&big, &rate("100")).is_err());
    }

    #[test]
    fn conversion_cost_rounds_once_to_scale_two() {
        // Terminating cases are exact.
        assert_eq!(conversion_cost(&money("7.00"), 1).unwrap(), money("7.00"));
        assert_eq!(conversion_cost(&money("7.00"), 2).unwrap(), money("3.50"));
        assert_eq!(
            conversion_cost(&money("133.00"), 4).unwrap(),
            money("33.25")
        );
        // Non-terminating 7.00 / 3 → 2.33 (rounded once; the exact purchase
        // value lives in the persisted purchase snapshot, not here).
        assert_eq!(conversion_cost(&money("7.00"), 3).unwrap(), money("2.33"));
        assert_eq!(
            conversion_cost(&money("7.00"), 3)
                .unwrap()
                .to_scaled_i64()
                .unwrap(),
            233
        );
        // 7.00 / 6 → 1.166_6… → 1.17 (midpoint-away).
        assert_eq!(
            conversion_cost(&money("7.00"), 6)
                .unwrap()
                .to_scaled_i64()
                .unwrap(),
            117
        );
    }

    #[test]
    fn conversion_cost_fails_closed() {
        assert_eq!(
            conversion_cost(&money("7.00"), 0),
            Err(NumericError::DivisionByZero)
        );
        assert_eq!(
            conversion_cost(&money("7.00"), -2),
            Err(NumericError::NegativeNotAllowed)
        );
    }

    #[test]
    fn conversion_identity_when_factor_is_one() {
        // purchase_unit == consumption_unit ⇒ factor 1 ⇒ exact identity.
        assert_eq!(
            conversion_cost(&money("237.96"), 1).unwrap(),
            money("237.96")
        );
    }
}
