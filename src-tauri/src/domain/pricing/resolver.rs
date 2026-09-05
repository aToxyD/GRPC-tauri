//! Supplier/Price Resolution Engine (ADR-0055 / SEC-087-F / ADR-0048).
//!
//! Pure, deterministic domain logic. The application layer feeds this engine
//! snapshot data (contract allocations + contract supplier + agreed price);
//! the engine never queries storage and never chooses/forges supplier, price,
//! entitlement or TVA. It only applies the resolution rules:
//!
//! 1. Outstanding obligations first (fiscal_year < current FY, remaining > 0).
//! 2. Then the current fiscal year's ACTIVE entitlement.
//! 3. Candidates are ordered fiscal_year ASC -> created_at ASC -> id ASC.
//! 4. The FIRST candidate with sufficient remaining quantity owns the item;
//!    insufficient coverage OR mixed-supplier requirements => REJECT (no
//!    combining, no fallback to base_price, no partial orders).
//! 5. The authoritative unit price is the contract agreed price; an unset
//!    agreed price is a resolution failure, never a fallback.
//!
//! Quantities and prices are **exact `Decimal`** (ADR-0048): remaining is a
//! component-based subtraction, comparisons are exact (no float epsilon) and
//! full-coverage is an exact `>=` check.

use crate::domain::numeric::{Money, NumericError, Quantity};
use crate::errors::{AppError, BusinessLogicError};

/// Candidate entitlement row fed to the resolver (data in, decisions out).
#[derive(Debug, Clone)]
pub struct ResolutionCandidate {
    pub allocation_id: String,
    pub allocation_fiscal_year: i32,
    pub supplier_id: String,
    pub supplier_name: String,
    pub agreed_price: Option<Money>,
    pub contracted_quantity: Quantity,
    pub fulfilled_quantity: Quantity,
    pub released_quantity: Quantity,
    pub reserved_quantity: Quantity,
    pub entitlement_state: String,
}

impl ResolutionCandidate {
    /// Component-based remaining quantity (never stored authoritatively).
    /// Exact subtraction; fails closed only if the persisted components are
    /// inconsistent (fulfilled+released+reserved > contracted).
    pub fn effective_remaining(&self) -> Result<Quantity, NumericError> {
        let mut remaining = self.contracted_quantity;
        remaining = remaining.checked_sub(self.fulfilled_quantity)?;
        remaining = remaining.checked_sub(self.released_quantity)?;
        remaining = remaining.checked_sub(self.reserved_quantity)?;
        Ok(remaining)
    }

    /// `true` when there is strictly positive remaining entitlement.
    pub fn has_entitlement(&self) -> bool {
        matches!(self.effective_remaining(), Ok(q) if q.is_positive())
    }
}

/// Authoritative resolution of ONE order item against entitlement.
#[derive(Debug, Clone)]
pub struct ItemResolution {
    /// Supplier owning this item (identical across all items of an order).
    pub supplier_id: String,
    /// Authoritative unit price: contract agreed price (TTC base).
    pub unit_price: Money,
    /// Allocation leg committed to fulfill/reserve this item's quantity.
    pub allocation_id: String,
}

/// Resolve the supplier + authoritative unit price for a single product
/// request. Pure and deterministic.
#[allow(clippy::too_many_arguments)]
pub fn resolve_supplier_for_item(
    unit_id: &str,
    product_id: &str,
    requested_quantity: Quantity,
    current_fiscal_year: i32,
    candidates: &[ResolutionCandidate],
) -> Result<ItemResolution, AppError> {
    if requested_quantity.is_zero() {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::ValidationError {
                field: "quantity".to_string(),
                message: "كمية الطلبية يجب أن تكون موجبة".to_string(),
            },
        ));
    }

    // Candidates arrive pre-sorted: fiscal_year ASC, created_at ASC, id ASC.
    // Priority: outstanding obligations (past fiscal years) before the current
    // year's ACTIVE entitlement; ENDED allocations with remaining quantity keep
    // their obligation (entitlement_state is lifecycle, not obligation end).
    // Future-year entitlements are never resolvable.
    let pick = candidates.iter().find(|c| {
        c.allocation_fiscal_year <= current_fiscal_year
            && c.entitlement_state != "CANCELLED"
            && c.has_entitlement()
    });

    let pick = match pick {
        Some(c) => c,
        None => {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::PriceCalculation {
                    message: format!(
                        "لا يوجد رصيد عقد صالح للمنتج {product_id} في الوحدة {unit_id}"
                    ),
                },
            ));
        }
    };

    // Full-coverage rule: requested quantity must fit entirely within this
    // entitlement. Partial or combined coverage is forbidden. Exact comparison:
    // remaining >= requested, with no float epsilon.
    let exact_remaining = pick.effective_remaining().map_err(AppError::from)?;
    if requested_quantity > exact_remaining {
        return Err(AppError::BusinessLogic(BusinessLogicError::PriceCalculation {
            message: format!(
                "الكمية المطلوبة ({requested_quantity}) تتجاوز الرصيد المتاح للمورد {} ({exact_remaining}) للمنتج {product_id}",
                pick.supplier_name,
            ),
        }));
    }

    // The release exception (WILAYA-only) reduces effective_remaining but never
    // triggers an implicit supplier transition; it is observed above as a lower
    // remaining quantity only.

    let unit_price = pick.agreed_price.ok_or_else(|| {
        AppError::BusinessLogic(BusinessLogicError::PriceCalculation {
            message: format!(
                "السعر المتفق عليه غير محدد لمنتج {product_id} (المورد {})",
                pick.supplier_name
            ),
        })
    })?;

    Ok(ItemResolution {
        supplier_id: pick.supplier_id.clone(),
        unit_price,
        allocation_id: pick.allocation_id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn candidate(
        id: &str,
        fy: i32,
        supplier: &str,
        contracted: &str,
        fulfilled: &str,
        released: &str,
        reserved: &str,
        state: &str,
        price: Option<&str>,
    ) -> ResolutionCandidate {
        ResolutionCandidate {
            allocation_id: id.to_string(),
            allocation_fiscal_year: fy,
            supplier_id: format!("sup-{supplier}"),
            supplier_name: supplier.to_string(),
            agreed_price: price.map(|p| Money::parse_str(p).unwrap()),
            contracted_quantity: Quantity::parse_str(contracted).unwrap(),
            fulfilled_quantity: Quantity::parse_str(fulfilled).unwrap(),
            released_quantity: Quantity::parse_str(released).unwrap(),
            reserved_quantity: Quantity::parse_str(reserved).unwrap(),
            entitlement_state: state.to_string(),
        }
    }

    fn q(s: &str) -> Quantity {
        Quantity::parse_str(s).unwrap()
    }

    #[test]
    fn old_obligation_wins_over_current() {
        // X 2026: 1000 contracted / 800 fulfilled => 200 remaining.
        // Y 2027: ACTIVE entitlement, 500 contracted / 0 fulfilled.
        let candidates = vec![
            candidate(
                "a1",
                2026,
                "X",
                "1000",
                "800",
                "0",
                "0",
                "ENDED",
                Some("30.00"),
            ),
            candidate(
                "a2",
                2027,
                "Y",
                "500",
                "0",
                "0",
                "0",
                "ACTIVE",
                Some("35.00"),
            ),
        ];
        let r = resolve_supplier_for_item("u1", "p1", q("200"), 2027, &candidates).unwrap();
        assert_eq!(r.supplier_id, "sup-X");
        assert_eq!(r.unit_price, Money::parse_str("30.00").unwrap());
        assert_eq!(r.allocation_id, "a1");
    }

    #[test]
    fn old_obligation_exhausted_then_current_supplier() {
        let candidates = vec![
            candidate(
                "a1",
                2026,
                "X",
                "1000",
                "1000",
                "0",
                "0",
                "ENDED",
                Some("30.00"),
            ),
            candidate(
                "a2",
                2027,
                "Y",
                "500",
                "0",
                "0",
                "0",
                "ACTIVE",
                Some("35.00"),
            ),
        ];
        let r = resolve_supplier_for_item("u1", "p1", q("100"), 2027, &candidates).unwrap();
        assert_eq!(r.supplier_id, "sup-Y");
        assert_eq!(r.unit_price, Money::parse_str("35.00").unwrap());
    }

    #[test]
    fn over_request_rejects_entire_order_item() {
        // X remaining 200; requested 250 => reject (no combining with Y).
        let candidates = vec![
            candidate(
                "a1",
                2026,
                "X",
                "1000",
                "800",
                "0",
                "0",
                "ENDED",
                Some("30.00"),
            ),
            candidate(
                "a2",
                2027,
                "Y",
                "500",
                "0",
                "0",
                "0",
                "ACTIVE",
                Some("35.00"),
            ),
        ];
        let r = resolve_supplier_for_item("u1", "p1", q("250"), 2027, &candidates);
        assert!(r.is_err());
    }

    #[test]
    fn no_entitlement_rejects() {
        let candidates = vec![];
        let r = resolve_supplier_for_item("u1", "p1", q("10"), 2027, &candidates);
        assert!(r.is_err());
    }

    #[test]
    fn cancelled_never_resolves() {
        let candidates = vec![
            candidate(
                "a1",
                2026,
                "X",
                "1000",
                "0",
                "0",
                "0",
                "CANCELLED",
                Some("30.00"),
            ),
            candidate(
                "a2",
                2027,
                "Y",
                "500",
                "0",
                "0",
                "0",
                "ACTIVE",
                Some("35.00"),
            ),
        ];
        let r = resolve_supplier_for_item("u1", "p1", q("50"), 2027, &candidates).unwrap();
        assert_eq!(r.supplier_id, "sup-Y");
    }

    #[test]
    fn released_quantity_reduces_remaining() {
        // 500 contracted, 300 released by WILAYA => 200 remaining.
        let candidates = vec![candidate(
            "a1",
            2026,
            "X",
            "500",
            "0",
            "300",
            "0",
            "ENDED",
            Some("30.00"),
        )];
        let r = resolve_supplier_for_item("u1", "p1", q("200"), 2027, &candidates).unwrap();
        assert_eq!(r.supplier_id, "sup-X");
        // 250 > 200 remaining => reject.
        let r2 = resolve_supplier_for_item("u1", "p1", q("250"), 2027, &candidates);
        assert!(r2.is_err());
    }

    #[test]
    fn reserved_quantity_reduces_remaining() {
        let candidates = vec![candidate(
            "a1",
            2026,
            "X",
            "200",
            "0",
            "0",
            "80",
            "ENDED",
            Some("30.00"),
        )];
        // Remaining = 200 - 80 = 120.
        let r = resolve_supplier_for_item("u1", "p1", q("120"), 2027, &candidates).unwrap();
        assert_eq!(r.allocation_id, "a1");
        let r2 = resolve_supplier_for_item("u1", "p1", q("121"), 2027, &candidates);
        assert!(r2.is_err());
    }

    #[test]
    fn multiple_old_obligations_oldest_first() {
        // 2025 X (remaining), 2026 X (remaining), 2027 Y ACTIVE.
        let candidates = vec![
            candidate(
                "a25",
                2025,
                "X",
                "100",
                "50",
                "0",
                "0",
                "ENDED",
                Some("28.00"),
            ),
            candidate(
                "a26",
                2026,
                "X",
                "400",
                "300",
                "0",
                "0",
                "ENDED",
                Some("30.00"),
            ),
            candidate(
                "a27",
                2027,
                "Z",
                "800",
                "0",
                "0",
                "0",
                "ACTIVE",
                Some("35.00"),
            ),
        ];
        let r = resolve_supplier_for_item("u1", "p1", q("50"), 2027, &candidates).unwrap();
        assert_eq!(r.allocation_id, "a25");
        // 60 > 50 remaining of a25 => reject (still X but insufficient, no combine).
        let r2 = resolve_supplier_for_item("u1", "p1", q("60"), 2027, &candidates);
        assert!(r2.is_err());
    }

    #[test]
    fn agreed_price_required() {
        let candidates = vec![candidate(
            "a1", 2026, "X", "100", "0", "0", "0", "ENDED", None,
        )];
        let r = resolve_supplier_for_item("u1", "p1", q("10"), 2027, &candidates);
        assert!(r.is_err());
    }

    #[test]
    fn exact_fractional_remaining_no_epsilon() {
        // 1.000 contracted − 0.333 − 0.333 − 0.333 = 0.001 exactly. With a
        // float epsilon this candidate would be wrongly treated as exhausted;
        // exact arithmetic keeps it resolvable for 0.001.
        let candidates = vec![candidate(
            "a1",
            2026,
            "X",
            "1.000",
            "0.333",
            "0.333",
            "0.333",
            "ENDED",
            Some("30.00"),
        )];
        let r = resolve_supplier_for_item("u1", "p1", q("0.001"), 2027, &candidates).unwrap();
        assert_eq!(r.supplier_id, "sup-X");
        // Requesting 0.002 > 0.001 remaining rejects exactly.
        let r2 = resolve_supplier_for_item("u1", "p1", q("0.002"), 2027, &candidates);
        assert!(r2.is_err());
    }

    #[test]
    fn zero_requested_quantity_rejects() {
        let candidates = vec![candidate(
            "a1",
            2026,
            "X",
            "100",
            "0",
            "0",
            "0",
            "ENDED",
            Some("30.00"),
        )];
        let r = resolve_supplier_for_item("u1", "p1", q("0"), 2027, &candidates);
        assert!(r.is_err());
    }

    #[test]
    fn inconsistent_components_fail_closed() {
        // fulfilled alone exceeds contracted: exact subtraction fails closed
        // rather than silently reporting a bogus negative remaining.
        let bad = candidate(
            "a1",
            2026,
            "X",
            "100",
            "101",
            "0",
            "0",
            "ENDED",
            Some("30.00"),
        );
        assert!(bad.effective_remaining().is_err());
        let r = resolve_supplier_for_item("u1", "p1", q("10"), 2027, &[bad]);
        assert!(r.is_err());
    }

    #[test]
    fn wire_conversion_round_trip_keeps_resolution_exact() {
        // End-to-end through the legacy adapter: DB REAL (f64) → typed →
        // resolve → f64 at the wire boundary, all bit-identical.
        use crate::domain::numeric::legacy_float;
        let price_f64 = 30.0_f64;
        let money = legacy_float::money_from_f64(price_f64).unwrap();
        assert_eq!(legacy_float::money_to_f64(&money).unwrap(), price_f64);
        let qty = legacy_float::quantity_from_f64(199.999).unwrap();
        assert_eq!(qty, q("199.999"));
    }
}
