//! Supplier/Price Resolution Engine (ADR-0055 / SEC-087-F).
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

use crate::errors::{AppError, BusinessLogicError};

/// Candidate entitlement row fed to the resolver (data in, decisions out).
#[derive(Debug, Clone)]
pub struct ResolutionCandidate {
    pub allocation_id: String,
    pub allocation_fiscal_year: i32,
    pub supplier_id: String,
    pub supplier_name: String,
    pub agreed_price: Option<f64>,
    pub contracted_quantity: f64,
    pub fulfilled_quantity: f64,
    pub released_quantity: f64,
    pub reserved_quantity: f64,
    pub entitlement_state: String,
}

impl ResolutionCandidate {
    /// Component-based remaining quantity (never stored authoritatively).
    pub fn effective_remaining(&self) -> f64 {
        self.contracted_quantity
            - self.fulfilled_quantity
            - self.released_quantity
            - self.reserved_quantity
    }
}

/// Authoritative resolution of ONE order item against entitlement.
#[derive(Debug, Clone)]
pub struct ItemResolution {
    /// Supplier owning this item (identical across all items of an order).
    pub supplier_id: String,
    /// Authoritative unit price: contract agreed price (TTC base).
    pub unit_price: f64,
    /// Allocation leg committed to fulfill/reserve this item's quantity.
    pub allocation_id: String,
}

/// Resolve the supplier + authoritative unit price for a single product
/// request. Pure and deterministic.
#[allow(clippy::too_many_arguments)]
pub fn resolve_supplier_for_item(
    unit_id: &str,
    product_id: &str,
    requested_quantity: f64,
    current_fiscal_year: i32,
    candidates: &[ResolutionCandidate],
) -> Result<ItemResolution, AppError> {
    if requested_quantity <= 0.0 {
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
            && c.effective_remaining() > f64::EPSILON
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
    // entitlement. Partial or combined coverage is forbidden.
    if pick.effective_remaining() + f64::EPSILON < requested_quantity {
        return Err(AppError::BusinessLogic(BusinessLogicError::PriceCalculation {
            message: format!(
                "الكمية المطلوبة ({requested_quantity:.2}) تتجاوز الرصيد المتاح للمورد {} ({:.2}) للمنتج {product_id}",
                pick.supplier_name,
                pick.effective_remaining()
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
        contracted: f64,
        fulfilled: f64,
        released: f64,
        reserved: f64,
        state: &str,
        price: Option<f64>,
    ) -> ResolutionCandidate {
        ResolutionCandidate {
            allocation_id: id.to_string(),
            allocation_fiscal_year: fy,
            supplier_id: format!("sup-{supplier}"),
            supplier_name: supplier.to_string(),
            agreed_price: price,
            contracted_quantity: contracted,
            fulfilled_quantity: fulfilled,
            released_quantity: released,
            reserved_quantity: reserved,
            entitlement_state: state.to_string(),
        }
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
                1000.0,
                800.0,
                0.0,
                0.0,
                "ENDED",
                Some(30.0),
            ),
            candidate("a2", 2027, "Y", 500.0, 0.0, 0.0, 0.0, "ACTIVE", Some(35.0)),
        ];
        let r = resolve_supplier_for_item("u1", "p1", 200.0, 2027, &candidates).unwrap();
        assert_eq!(r.supplier_id, "sup-X");
        assert_eq!(r.unit_price, 30.0);
        assert_eq!(r.allocation_id, "a1");
    }

    #[test]
    fn old_obligation_exhausted_then_current_supplier() {
        let candidates = vec![
            candidate(
                "a1",
                2026,
                "X",
                1000.0,
                1000.0,
                0.0,
                0.0,
                "ENDED",
                Some(30.0),
            ),
            candidate("a2", 2027, "Y", 500.0, 0.0, 0.0, 0.0, "ACTIVE", Some(35.0)),
        ];
        let r = resolve_supplier_for_item("u1", "p1", 100.0, 2027, &candidates).unwrap();
        assert_eq!(r.supplier_id, "sup-Y");
        assert_eq!(r.unit_price, 35.0);
    }

    #[test]
    fn over_request_rejects_entire_order_item() {
        // X remaining 200; requested 250 => reject (no combining with Y).
        let candidates = vec![
            candidate(
                "a1",
                2026,
                "X",
                1000.0,
                800.0,
                0.0,
                0.0,
                "ENDED",
                Some(30.0),
            ),
            candidate("a2", 2027, "Y", 500.0, 0.0, 0.0, 0.0, "ACTIVE", Some(35.0)),
        ];
        let r = resolve_supplier_for_item("u1", "p1", 250.0, 2027, &candidates);
        assert!(r.is_err());
    }

    #[test]
    fn no_entitlement_rejects() {
        let candidates = vec![];
        let r = resolve_supplier_for_item("u1", "p1", 10.0, 2027, &candidates);
        assert!(r.is_err());
    }

    #[test]
    fn cancelled_never_resolves() {
        let candidates = vec![
            candidate(
                "a1",
                2026,
                "X",
                1000.0,
                0.0,
                0.0,
                0.0,
                "CANCELLED",
                Some(30.0),
            ),
            candidate("a2", 2027, "Y", 500.0, 0.0, 0.0, 0.0, "ACTIVE", Some(35.0)),
        ];
        let r = resolve_supplier_for_item("u1", "p1", 50.0, 2027, &candidates).unwrap();
        assert_eq!(r.supplier_id, "sup-Y");
    }

    #[test]
    fn released_quantity_reduces_remaining() {
        // 500 contracted, 300 released by WILAYA => 200 remaining.
        let candidates = vec![candidate(
            "a1",
            2026,
            "X",
            500.0,
            0.0,
            300.0,
            0.0,
            "ENDED",
            Some(30.0),
        )];
        let r = resolve_supplier_for_item("u1", "p1", 200.0, 2027, &candidates).unwrap();
        assert_eq!(r.supplier_id, "sup-X");
        // 250 > 200 remaining => reject.
        let r2 = resolve_supplier_for_item("u1", "p1", 250.0, 2027, &candidates);
        assert!(r2.is_err());
    }

    #[test]
    fn reserved_quantity_reduces_remaining() {
        let candidates = vec![candidate(
            "a1",
            2026,
            "X",
            200.0,
            0.0,
            0.0,
            80.0,
            "ENDED",
            Some(30.0),
        )];
        // Remaining = 200 - 80 = 120.
        let r = resolve_supplier_for_item("u1", "p1", 120.0, 2027, &candidates).unwrap();
        assert_eq!(r.allocation_id, "a1");
        let r2 = resolve_supplier_for_item("u1", "p1", 121.0, 2027, &candidates);
        assert!(r2.is_err());
    }

    #[test]
    fn multiple_old_obligations_oldest_first() {
        // 2025 X (remaining), 2026 X (remaining), 2027 Y ACTIVE.
        let candidates = vec![
            candidate("a25", 2025, "X", 100.0, 50.0, 0.0, 0.0, "ENDED", Some(28.0)),
            candidate(
                "a26",
                2026,
                "X",
                400.0,
                300.0,
                0.0,
                0.0,
                "ENDED",
                Some(30.0),
            ),
            candidate("a27", 2027, "Z", 800.0, 0.0, 0.0, 0.0, "ACTIVE", Some(35.0)),
        ];
        let r = resolve_supplier_for_item("u1", "p1", 50.0, 2027, &candidates).unwrap();
        assert_eq!(r.allocation_id, "a25");
        // 60 > 50 remaining of a25 => reject (still X but insufficient, no combine).
        let r2 = resolve_supplier_for_item("u1", "p1", 60.0, 2027, &candidates);
        assert!(r2.is_err());
    }

    #[test]
    fn agreed_price_required() {
        let candidates = vec![candidate(
            "a1", 2026, "X", 100.0, 0.0, 0.0, 0.0, "ENDED", None,
        )];
        let r = resolve_supplier_for_item("u1", "p1", 10.0, 2027, &candidates);
        assert!(r.is_err());
    }
}
