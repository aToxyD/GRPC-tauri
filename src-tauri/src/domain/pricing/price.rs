//! Deterministic pricing arithmetic (SEC-087-F / ADR-0055).
//!
//! Single owner of price-with-TVA calculation so presentation and reporting
//! layers never reimplement financial arithmetic (P2 / A5).

/// All-in price including the statutory VAT rate.
///
/// `tva_rate` is expressed in percent (e.g. `19.0`); the base price is the
/// pre-tax unit price. Direct equality on the result must not be compared
/// without an epsilon (B4).
pub fn price_with_tva(base_price: f64, tva_rate: f64) -> f64 {
    base_price * (1.0 + tva_rate / 100.0)
}

#[cfg(test)]
mod tests {
    use super::price_with_tva;

    #[test]
    fn zero_rate_returns_base_price() {
        assert!((price_with_tva(100.0, 0.0) - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn positive_rate_applies_ratio() {
        assert!((price_with_tva(200.0, 19.0) - 238.0).abs() < f64::EPSILON);
    }

    #[test]
    fn deterministic_across_calls() {
        assert_eq!(price_with_tva(133.0, 7.5), price_with_tva(133.0, 7.5));
    }
}
