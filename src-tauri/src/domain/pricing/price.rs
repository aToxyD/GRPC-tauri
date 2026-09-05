//! Deterministic pricing arithmetic (SEC-087-F / ADR-0055 / ADR-0048).
//!
//! Single owner of price-with-TVA calculation so presentation and reporting
//! layers never reimplement financial arithmetic (P2 / A5). Price-with-TVA is
//! **exact `Decimal`** (ADR-0048): `price_with_tva(200.00, 19) == 238.00`
//! exactly — never `237.999...`.

use crate::domain::numeric::{Money, NumericError, Rate};

/// All-in price including the statutory VAT rate.
///
/// `tva_rate` is a percent-domain `Rate` (e.g. `19.0` means 19%, exactly
/// 19% — not the fraction 0.19). The base price is the pre-tax unit price.
/// Exact decimal arithmetic; the only rounding is the caller's boundary
/// conversion via `Money::to_scaled_i64`.
pub fn price_with_tva(base_price: &Money, tva_rate: &Rate) -> Result<Money, NumericError> {
    base_price.checked_apply_rate(tva_rate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::numeric::{Money, Rate};

    fn money(s: &str) -> Money {
        Money::parse_str(s).unwrap()
    }

    fn rate(s: &str) -> Rate {
        Rate::parse_str(s).unwrap()
    }

    #[test]
    fn zero_rate_is_exact_identity() {
        assert_eq!(
            price_with_tva(&money("100.00"), &rate("0")).unwrap(),
            money("100.00")
        );
    }

    #[test]
    fn positive_rate_is_exact() {
        // 200.00 @ 19% → 238.00 exactly, at the cent boundary.
        let result = price_with_tva(&money("200.00"), &rate("19.0")).unwrap();
        assert_eq!(result, money("238.00"));
        assert_eq!(result.to_scaled_i64().unwrap(), 23800);
    }

    #[test]
    fn non_round_base_price_is_exact() {
        // 19.99 @ 19% → 23.7881 → 2379 centimes at the boundary (no float dust).
        let result = price_with_tva(&money("19.99"), &rate("19")).unwrap();
        assert_eq!(result.to_scaled_i64().unwrap(), 2379);
    }

    #[test]
    fn fractional_rate_rounds_only_at_boundary() {
        // 133.00 @ 7.5% → 142.975 → 142.98 (MidpointAwayFromZero at scale 2).
        let result = price_with_tva(&money("133.00"), &rate("7.5")).unwrap();
        assert_eq!(result.to_scaled_i64().unwrap(), 14298);
    }

    #[test]
    fn repeatable_across_calls() {
        let a = price_with_tva(&money("133.00"), &rate("7.5")).unwrap();
        let b = price_with_tva(&money("133.00"), &rate("7.5")).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn out_of_domain_money_fails_closed() {
        assert_eq!(
            price_with_tva(&money("0.00"), &rate("17")),
            Ok(money("0.00"))
        );
    }

    #[test]
    fn overflow_fails_closed() {
        // At the 96-bit Decimal coefficient boundary, +100% doubles the value
        // and must overflow the Decimal itself — never silently wrap.
        let big = Money::from_decimal(rust_decimal::Decimal::MAX).unwrap();
        assert!(price_with_tva(&big, &rate("100")).is_err());
    }

    #[test]
    fn comparison_as_single_source_of_truth() {
        // P2: the frontend/format layer must never reimplement this arithmetic;
        // a presentation-only version would diverge on 19.99 @ 19%.
        let expected = price_with_tva(&money("19.99"), &rate("19")).unwrap();
        assert_eq!(expected.to_scaled_i64().unwrap(), 2379);
    }
}
