use std::fmt;

use crate::domain::invariants::Invariant;

// ── OriginImmutable ────────────────────────────────────────────────────

/// Violation raised when a layer's `origin_fiscal_year` is mutated.
#[derive(Debug, Clone, PartialEq)]
pub enum OriginImmutableViolation {
    OriginFiscalYearChanged {
        layer_id: String,
        expected_origin_year: i32,
        actual_origin_year: i32,
    },
}

impl fmt::Display for OriginImmutableViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OriginImmutableViolation::OriginFiscalYearChanged { layer_id, expected_origin_year, actual_origin_year } => {
                write!(
                    f,
                    "layer {} origin_fiscal_year changed: expected {}, got {}",
                    layer_id, expected_origin_year, actual_origin_year
                )
            }
        }
    }
}

/// Input context for the OriginImmutable invariant.
#[derive(Debug, Clone)]
pub struct OriginContext {
    pub layers: Vec<OriginYearRecord>,
}

/// A record of a layer's origin fiscal year for checking.
#[derive(Debug, Clone)]
pub struct OriginYearRecord {
    pub layer_id: String,
    pub expected_origin_year: i32,
    pub actual_origin_year: i32,
}

/// Invariant: `origin_fiscal_year` on FIFO layers and fiscal records
/// must never be modified after creation.
pub struct OriginImmutable;

impl Invariant for OriginImmutable {
    type Context = OriginContext;
    type Violation = OriginImmutableViolation;

    fn check(ctx: &OriginContext) -> Vec<OriginImmutableViolation> {
        let mut violations = Vec::new();

        for layer in &ctx.layers {
            if layer.actual_origin_year != layer.expected_origin_year {
                violations.push(OriginImmutableViolation::OriginFiscalYearChanged {
                    layer_id: layer.layer_id.clone(),
                    expected_origin_year: layer.expected_origin_year,
                    actual_origin_year: layer.actual_origin_year,
                });
            }
        }

        violations
    }
}

/// Convenience: check that layers' origin fiscal years are unchanged.
pub fn check_origin_immutable(layers: Vec<OriginYearRecord>) -> Vec<OriginImmutableViolation> {
    let ctx = OriginContext { layers };
    OriginImmutable::check(&ctx)
}

// ── SingleOpenFiscalYear ───────────────────────────────────────────────

/// Violation raised when fiscal year lifecycle rules are broken.
#[derive(Debug, Clone, PartialEq)]
pub enum SingleOpenFiscalYearViolation {
    MultipleOpenYears { open_years: Vec<i32> },
    NoOpenYear,
}

impl fmt::Display for SingleOpenFiscalYearViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SingleOpenFiscalYearViolation::MultipleOpenYears { open_years } => {
                write!(f, "multiple fiscal years are open: {:?}", open_years)
            }
            SingleOpenFiscalYearViolation::NoOpenYear => {
                write!(f, "no fiscal year is currently open")
            }
        }
    }
}

/// A fiscal year entry with its status.
#[derive(Debug, Clone)]
pub struct FiscalYearEntry {
    pub year: i32,
    pub status: String,
}

/// Input context for the SingleOpenFiscalYear invariant.
#[derive(Debug, Clone)]
pub struct FiscalYearContext {
    pub years: Vec<FiscalYearEntry>,
}

/// Invariant: Exactly one fiscal year may be open at any time.
pub struct SingleOpenFiscalYear;

impl Invariant for SingleOpenFiscalYear {
    type Context = FiscalYearContext;
    type Violation = SingleOpenFiscalYearViolation;

    fn check(ctx: &FiscalYearContext) -> Vec<SingleOpenFiscalYearViolation> {
        let mut violations = Vec::new();
        let open_years: Vec<i32> = ctx
            .years
            .iter()
            .filter(|y| y.status == "open")
            .map(|y| y.year)
            .collect();

        if !open_years.is_empty() {
            if open_years.len() > 1 {
                violations.push(SingleOpenFiscalYearViolation::MultipleOpenYears {
                    open_years,
                });
            }
        } else {
            violations.push(SingleOpenFiscalYearViolation::NoOpenYear);
        }

        violations
    }
}

/// Convenience: check that at most one fiscal year is open.
pub fn check_single_open_fiscal_year(years: Vec<FiscalYearEntry>) -> Vec<SingleOpenFiscalYearViolation> {
    let ctx = FiscalYearContext { years };
    SingleOpenFiscalYear::check(&ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── OriginImmutable tests ──────────────────────────────────────────

    #[test]
    fn unchanged_origin_is_valid() {
        let layers = vec![OriginYearRecord {
            layer_id: "l1".into(),
            expected_origin_year: 2025,
            actual_origin_year: 2025,
        }];
        assert!(check_origin_immutable(layers).is_empty());
    }

    #[test]
    fn changed_origin_triggers_violation() {
        let layers = vec![OriginYearRecord {
            layer_id: "l1".into(),
            expected_origin_year: 2025,
            actual_origin_year: 2026,
        }];
        let v = check_origin_immutable(layers);
        assert_eq!(v.len(), 1);
        match &v[0] {
            OriginImmutableViolation::OriginFiscalYearChanged { layer_id, expected_origin_year, actual_origin_year } => {
                assert_eq!(layer_id, "l1");
                assert_eq!(*expected_origin_year, 2025);
                assert_eq!(*actual_origin_year, 2026);
            }
        }
    }

    #[test]
    fn multiple_layers_all_valid() {
        let layers = vec![
            OriginYearRecord { layer_id: "l1".into(), expected_origin_year: 2024, actual_origin_year: 2024 },
            OriginYearRecord { layer_id: "l2".into(), expected_origin_year: 2025, actual_origin_year: 2025 },
        ];
        assert!(check_origin_immutable(layers).is_empty());
    }

    #[test]
    fn mixed_violations_collected() {
        let layers = vec![
            OriginYearRecord { layer_id: "l1".into(), expected_origin_year: 2025, actual_origin_year: 2025 },
            OriginYearRecord { layer_id: "l2".into(), expected_origin_year: 2025, actual_origin_year: 2024 },
            OriginYearRecord { layer_id: "l3".into(), expected_origin_year: 2025, actual_origin_year: 2026 },
        ];
        let v = check_origin_immutable(layers);
        assert_eq!(v.len(), 2);
    }

    // ── SingleOpenFiscalYear tests ─────────────────────────────────────

    #[test]
    fn exactly_one_open_year_is_valid() {
        let years = vec![
            FiscalYearEntry { year: 2024, status: "closed".into() },
            FiscalYearEntry { year: 2025, status: "open".into() },
            FiscalYearEntry { year: 2026, status: "archived".into() },
        ];
        assert!(check_single_open_fiscal_year(years).is_empty());
    }

    #[test]
    fn multiple_open_years_triggers_violation() {
        let years = vec![
            FiscalYearEntry { year: 2025, status: "open".into() },
            FiscalYearEntry { year: 2026, status: "open".into() },
        ];
        let v = check_single_open_fiscal_year(years);
        assert_eq!(v.len(), 1);
        match &v[0] {
            SingleOpenFiscalYearViolation::MultipleOpenYears { open_years } => {
                assert_eq!(open_years.len(), 2);
                assert!(open_years.contains(&2025));
                assert!(open_years.contains(&2026));
            }
            _ => panic!("wrong violation type"),
        }
    }

    #[test]
    fn no_open_year_triggers_violation() {
        let years = vec![
            FiscalYearEntry { year: 2024, status: "closed".into() },
            FiscalYearEntry { year: 2025, status: "archived".into() },
        ];
        let v = check_single_open_fiscal_year(years);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0], SingleOpenFiscalYearViolation::NoOpenYear);
    }

    #[test]
    fn all_archived_triggers_no_open() {
        let years = vec![
            FiscalYearEntry { year: 2023, status: "archived".into() },
            FiscalYearEntry { year: 2024, status: "archived".into() },
        ];
        let v = check_single_open_fiscal_year(years);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0], SingleOpenFiscalYearViolation::NoOpenYear);
    }

    #[test]
    fn empty_years_triggers_no_open() {
        let v = check_single_open_fiscal_year(vec![]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0], SingleOpenFiscalYearViolation::NoOpenYear);
    }

    #[test]
    fn only_one_open_no_other_years_is_valid() {
        let years = vec![
            FiscalYearEntry { year: 2025, status: "open".into() },
        ];
        assert!(check_single_open_fiscal_year(years).is_empty());
    }
}
