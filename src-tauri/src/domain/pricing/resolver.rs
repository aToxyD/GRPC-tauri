//! Supplier/Price Resolution Engine (ADR-0055 / SEC-087-F / ADR-0048).
//!
//! Pure, deterministic domain logic. The application layer feeds this engine
//! snapshot data (contract allocations + contract supplier + price_ttc);
//! the engine never queries storage and never chooses/forges supplier, price,
//! entitlement or TVA. It only applies the resolution rules:
//!
//! 1. Outstanding obligations first (fiscal_year < current FY, remaining > 0).
//! 2. Then the current fiscal year's ACTIVE entitlement.
//! 3. Candidates are ordered fiscal_year ASC -> created_at ASC -> id ASC.
//! 4. The FIRST candidate with sufficient remaining quantity owns the item;
//!    insufficient coverage OR mixed-supplier requirements => REJECT (no
//!    combining, no fallback to base_price, no partial orders).
//! 5. The authoritative unit price is the contract's price_ttc (TTC);
//!    an unset price_ttc is a resolution failure, never a fallback.
//!
//! SEC-087 semantic boundary: `price_ttc` is the sole authoritative TTC
//! pricing source in the resolver. It is never derived from, or substituted
//! by, `agreed_price_ht` or any other column.
//!
//! Quantities and prices are **exact `Decimal`** (ADR-0048): remaining is a
//! component-based subtraction, comparisons are exact (no float epsilon) and
//! full-coverage is an exact `>=` check.

use std::collections::HashMap;

use crate::domain::numeric::{Money, NumericError, Quantity};
use crate::errors::{AppError, BusinessLogicError};

/// Candidate entitlement row fed to the resolver (data in, decisions out).
///
/// SEC-087 Phase 3: `price_ttc` is the sole authoritative TTC pricing source.
/// The repository reads the column strictly (`cp.price_ttc`, no fallback); a
/// row carrying no `price_ttc` yields a candidate with `price_ttc = None`,
/// which `resolve_item` rejects — fails closed, never a fallback.
#[derive(Debug, Clone)]
pub struct ResolutionCandidate {
    pub allocation_id: String,
    pub allocation_fiscal_year: i32,
    pub supplier_id: String,
    pub supplier_name: String,
    pub price_ttc: Option<Money>,
    /// SEC-087 Phase 5: persisted purchase→consumption unit snapshot carried
    /// from `contract_products` (all-or-nothing; all `None` = legacy contract).
    /// Never used to choose pricing — only snapshotted onto order items.
    pub purchase_unit: Option<i32>,
    pub consumption_unit: Option<i32>,
    pub conversion_factor: Option<i32>,
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
    /// Authoritative unit price: contract price_ttc (TTC base).
    pub unit_price: Money,
    /// Allocation leg committed to fulfill/reserve this item's quantity.
    pub allocation_id: String,
    /// SEC-087 Phase 5: purchase→consumption snapshot of the winning candidate
    /// (raw persisted codes, all-or-nothing).
    pub purchase_unit: Option<i32>,
    pub consumption_unit: Option<i32>,
    pub conversion_factor: Option<i32>,
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

    let unit_price = pick.price_ttc.ok_or_else(|| {
        AppError::BusinessLogic(BusinessLogicError::PriceCalculation {
            message: format!(
                "سعر TTC غير محدد للمنتج {product_id} (المورد {})",
                pick.supplier_name
            ),
        })
    })?;

    Ok(ItemResolution {
        supplier_id: pick.supplier_id.clone(),
        unit_price,
        allocation_id: pick.allocation_id.clone(),
        purchase_unit: pick.purchase_unit,
        consumption_unit: pick.consumption_unit,
        conversion_factor: pick.conversion_factor,
    })
}

/// One planned portion of a whole-request allocation plan (ADR-0056,
/// Phase 4B).
///
/// Exactly one `SupplierOrderItem` row is materialized per portion, backed by
/// exactly one allocation leg (invariants I1, I3, I4). A portion is either a
/// FULL drain of an eligible allocation's effective remaining quantity
/// (greedy-drain invariant I6) or — only for the FINAL portion that completes
/// the request — a partial draw.
#[derive(Debug, Clone)]
pub struct PlannedPortion {
    pub product_id: String,
    pub allocation_id: String,
    pub allocation_fiscal_year: i32,
    pub supplier_id: String,
    pub supplier_name: String,
    /// Authoritative unit price: the allocation's `price_ttc` (sole source).
    pub unit_price: Money,
    /// SEC-087 Phase 5: purchase→consumption snapshot of the portion's
    /// allocation (raw persisted codes, all-or-nothing).
    pub purchase_unit: Option<i32>,
    pub consumption_unit: Option<i32>,
    pub conversion_factor: Option<i32>,
    pub quantity: Quantity,
}

/// Plan a whole order request against the current entitlement snapshot
/// (ADR-0056, Phase 4B). Pure and deterministic.
///
/// The caller supplies raw input rows, which MAY repeat a product; this engine
/// aggregates duplicate `product_id` rows at the request boundary into one
/// requested quantity per product (ADR-0056 §Allocation-Plan Semantics) before
/// planning. Candidates arrive pre-sorted by the repository
/// (`fiscal_year ASC, created_at ASC, id ASC`); this engine never reorders and
/// never derives order from storage row order.
///
/// Enforced invariants (fail closed):
/// - **I6 greedy-drain**: allocations are consumed oldest-first and each is
///   drained to its FULL effective remaining quantity; only the FINAL portion
///   completing the request may be partial. The planner never skips an eligible
///   allocation to reach a younger one, so non-greedy plans are impossible by
///   construction — any inconsistency (component overflow, over-request) fails
///   closed with an error instead of silently reordering or shaping the plan.
/// - **I7**: outstanding obligations (older fiscal years) resolve before the
///   current year's ACTIVE entitlement; future-year entitlement is never
///   resolvable.
/// - **I8/I22**: over-request (aggregated requested quantity greater than the
///   total effective remaining across eligible allocations) rejects the ENTIRE
///   request.
/// - **Price authority**: each portion's `unit_price` is the allocation's
///   `price_ttc`; an unset price fails closed — never a fallback.
pub fn plan_request(
    unit_id: &str,
    requests: &[(String, Quantity)],
    current_fiscal_year: i32,
    candidates_by_product: &HashMap<String, Vec<ResolutionCandidate>>,
) -> Result<Vec<PlannedPortion>, AppError> {
    // Aggregate duplicate product rows at the boundary: one quantity per
    // product, in first-seen order (deterministic).
    let mut aggregated: Vec<(String, Quantity)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (product_id, requested) in requests {
        if !requested.is_positive() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::ValidationError {
                    field: "quantity".to_string(),
                    message: format!("كمية الطلبية يجب أن تكون موجبة للمنتج {product_id}"),
                },
            ));
        }
        match index.get(product_id) {
            Some(&i) => {
                let current = aggregated[i].1;
                aggregated[i].1 = current.checked_add(*requested)?;
            }
            None => {
                index.insert(product_id.clone(), aggregated.len());
                aggregated.push((product_id.clone(), *requested));
            }
        }
    }

    let mut plan: Vec<PlannedPortion> = Vec::new();

    for (product_id, requested_total) in aggregated {
        let candidates = candidates_by_product
            .get(&product_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        let mut remaining_requested = requested_total;
        for candidate in candidates {
            // Priority pass mirrors `resolve_supplier_for_item`: obligations
            // first, current-year ACTIVE entitlement, never future-year,
            // never CANCELLED (repository already excludes; kept defensively).
            if candidate.allocation_fiscal_year > current_fiscal_year {
                continue;
            }
            if candidate.entitlement_state == "CANCELLED" {
                continue;
            }
            let remaining = candidate.effective_remaining().map_err(AppError::from)?;
            if !remaining.is_positive() || remaining_requested.is_zero() {
                continue;
            }

            // Greedy-drain: take the FULL effective remaining of this
            // allocation, unless this is the final portion (request satisfied
            // mid-allocation → partial draw allowed only here).
            let take = if remaining_requested <= remaining {
                remaining_requested
            } else {
                remaining
            };

            let unit_price = candidate.price_ttc.ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::PriceCalculation {
                    message: format!(
                        "سعر TTC غير محدد للمنتج {product_id} (المورد {})",
                        candidate.supplier_name
                    ),
                })
            })?;

            plan.push(PlannedPortion {
                product_id: product_id.clone(),
                allocation_id: candidate.allocation_id.clone(),
                allocation_fiscal_year: candidate.allocation_fiscal_year,
                supplier_id: candidate.supplier_id.clone(),
                supplier_name: candidate.supplier_name.clone(),
                unit_price,
                purchase_unit: candidate.purchase_unit,
                consumption_unit: candidate.consumption_unit,
                conversion_factor: candidate.conversion_factor,
                quantity: take,
            });
            remaining_requested = remaining_requested.checked_sub(take)?;
        }

        if !remaining_requested.is_zero() {
            return Err(AppError::BusinessLogic(BusinessLogicError::PriceCalculation {
                message: format!(
                    "الكمية المطلوبة للمنتج {product_id} تتجاوز إجمالي الرصيد المتاح في الوحدة {unit_id} (المتبقي {remaining_requested} غير مغطى)"
                ),
            }));
        }
    }

    Ok(plan)
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
        price_ttc: Option<&str>,
    ) -> ResolutionCandidate {
        ResolutionCandidate {
            allocation_id: id.to_string(),
            allocation_fiscal_year: fy,
            supplier_id: format!("sup-{supplier}"),
            supplier_name: supplier.to_string(),
            price_ttc: price_ttc.map(|p| Money::parse_str(p).unwrap()),
            purchase_unit: Some(1),
            consumption_unit: Some(1),
            conversion_factor: Some(1),
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
    fn snapshot_codes_pass_through_resolution() {
        let mut c = candidate(
            "a1",
            2026,
            "X",
            "100",
            "0",
            "0",
            "0",
            "ENDED",
            Some("30.00"),
        );
        c.purchase_unit = Some(8);
        c.consumption_unit = Some(1);
        c.conversion_factor = Some(10);
        let r = resolve_supplier_for_item("u1", "p1", q("10"), 2027, &[c]).unwrap();
        assert_eq!(r.purchase_unit, Some(8));
        assert_eq!(r.consumption_unit, Some(1));
        assert_eq!(r.conversion_factor, Some(10));
    }

    #[test]
    fn legacy_candidate_snapshot_is_all_none() {
        let mut c = candidate(
            "a1",
            2026,
            "X",
            "100",
            "0",
            "0",
            "0",
            "ENDED",
            Some("30.00"),
        );
        c.purchase_unit = None;
        c.consumption_unit = None;
        c.conversion_factor = None;
        let r = resolve_supplier_for_item("u1", "p1", q("10"), 2027, &[c]).unwrap();
        assert_eq!(r.purchase_unit, None);
        assert_eq!(r.consumption_unit, None);
        assert_eq!(r.conversion_factor, None);
    }

    #[test]
    fn plan_portions_carry_snapshot_codes() {
        let mut c = candidate(
            "a1",
            2026,
            "X",
            "100",
            "0",
            "0",
            "0",
            "ENDED",
            Some("30.00"),
        );
        c.purchase_unit = Some(8);
        c.consumption_unit = Some(1);
        c.conversion_factor = Some(10);
        let candidates = candidate_map(&[(product("p1"), vec![c])]);
        let portions = plan_request("u1", &[(product("p1"), q("10"))], 2027, &candidates).unwrap();
        assert_eq!(portions.len(), 1);
        assert_eq!(portions[0].purchase_unit, Some(8));
        assert_eq!(portions[0].consumption_unit, Some(1));
        assert_eq!(portions[0].conversion_factor, Some(10));
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
    fn price_ttc_required() {
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

    fn candidate_map(
        map: &[(String, Vec<ResolutionCandidate>)],
    ) -> HashMap<String, Vec<ResolutionCandidate>> {
        map.iter().cloned().collect()
    }

    fn product(product_id: &str) -> String {
        product_id.to_string()
    }

    #[test]
    fn plan_aggregates_duplicate_product_rows_at_boundary() {
        // Two input rows for the same product must be summed into one requested
        // quantity BEFORE planning (ADR-0056 §Allocation-Plan Semantics) so the
        // planner sees 300, not two independent 150 requests.
        let candidates = candidate_map(&[(
            product("p1"),
            vec![
                candidate(
                    "a1",
                    2026,
                    "X",
                    "200",
                    "0",
                    "0",
                    "0",
                    "ENDED",
                    Some("30.00"),
                ),
                candidate(
                    "a2",
                    2027,
                    "X",
                    "300",
                    "0",
                    "0",
                    "0",
                    "ACTIVE",
                    Some("35.00"),
                ),
            ],
        )]);
        let portions = plan_request(
            "u1",
            &[(product("p1"), q("150")), (product("p1"), q("150"))],
            2027,
            &candidates,
        )
        .unwrap();
        assert_eq!(portions.len(), 2, "aggregated 300 drains both allocations");
        assert_eq!(portions[0].allocation_id, "a1");
        assert_eq!(
            portions[0].quantity,
            q("200"),
            "A drained to its full remaining"
        );
        assert_eq!(portions[1].allocation_id, "a2");
        assert_eq!(portions[1].quantity, q("100"), "final portion partial on B");
        assert_eq!(portions[0].supplier_id, "sup-X");
        assert_eq!(portions[1].supplier_id, "sup-X");
    }

    #[test]
    fn plan_cross_fiscal_year_oldest_first() {
        // A FY2026 (X, rem 200) before B FY2027 (Y, rem 300): request 250 must
        // drain A fully, then B partially (obligation priority I7).
        let candidates = candidate_map(&[(
            product("p1"),
            vec![
                candidate(
                    "a1",
                    2026,
                    "X",
                    "200",
                    "0",
                    "0",
                    "0",
                    "ENDED",
                    Some("30.00"),
                ),
                candidate(
                    "a2",
                    2027,
                    "Y",
                    "300",
                    "0",
                    "0",
                    "0",
                    "ACTIVE",
                    Some("35.00"),
                ),
            ],
        )]);
        let portions = plan_request("u1", &[(product("p1"), q("250"))], 2027, &candidates).unwrap();
        assert_eq!(portions.len(), 2);
        assert_eq!(portions[0].allocation_id, "a1");
        assert_eq!(portions[0].quantity, q("200"));
        assert_eq!(portions[1].allocation_id, "a2");
        assert_eq!(portions[1].quantity, q("50"));
        // Oldest obligation keeps its historical supplier for the first portion,
        // the remaining portion belongs to the current entitlement supplier.
        assert_eq!(portions[0].supplier_id, "sup-X");
        assert_eq!(portions[1].supplier_id, "sup-Y");
    }

    #[test]
    fn plan_greedy_full_then_partial() {
        // Greedy-drain (I6): the older allocation is filled entirely (200/200),
        // never partially, and only the final portion may be a partial draw.
        let candidates = candidate_map(&[(
            product("p1"),
            vec![
                candidate(
                    "a1",
                    2026,
                    "X",
                    "200",
                    "0",
                    "0",
                    "0",
                    "ENDED",
                    Some("30.00"),
                ),
                candidate(
                    "a2",
                    2027,
                    "X",
                    "300",
                    "0",
                    "0",
                    "0",
                    "ACTIVE",
                    Some("35.00"),
                ),
            ],
        )]);
        let portions = plan_request("u1", &[(product("p1"), q("250"))], 2027, &candidates).unwrap();
        assert_eq!(portions.len(), 2);
        assert_eq!(portions[0].allocation_id, "a1");
        assert_eq!(
            portions[0].quantity,
            q("200"),
            "older allocation fully drained"
        );
        assert_eq!(portions[1].allocation_id, "a2");
        assert_eq!(
            portions[1].quantity,
            q("50"),
            "only final portion is partial"
        );
    }

    #[test]
    fn plan_over_request_rejects_entire_request() {
        // Requested 400 > 200 (A) + 100 (B) total remaining: the whole request
        // must be rejected (I8/I22) with no partial plan.
        let candidates = candidate_map(&[(
            product("p1"),
            vec![
                candidate(
                    "a1",
                    2026,
                    "X",
                    "200",
                    "0",
                    "0",
                    "0",
                    "ENDED",
                    Some("30.00"),
                ),
                candidate(
                    "a2",
                    2027,
                    "Y",
                    "400",
                    "300",
                    "0",
                    "0",
                    "ACTIVE",
                    Some("35.00"),
                ),
            ],
        )]);
        let r = plan_request("u1", &[(product("p1"), q("400"))], 2027, &candidates);
        assert!(r.is_err(), "over-request must reject the entire request");
    }

    #[test]
    fn plan_fails_closed_rather_than_skipping_older_allocation() {
        // Non-greedy "skip the older allocation to buy only from the younger
        // supplier" is impossible by construction: the planner always starts at
        // the oldest eligible allocation. An inconsistent component state also
        // fails closed instead of silently shaping a partial plan.
        let candidates = candidate_map(&[(
            product("p1"),
            vec![
                candidate(
                    "a1",
                    2026,
                    "X",
                    "200",
                    "0",
                    "0",
                    "0",
                    "ENDED",
                    Some("30.00"),
                ),
                candidate(
                    "a2",
                    2027,
                    "Y",
                    "300",
                    "0",
                    "0",
                    "0",
                    "ACTIVE",
                    Some("35.00"),
                ),
            ],
        )]);
        // Even though only Y is preferred by the (imaginary) caller, the plan
        // MUST begin with the oldest obligation.
        let portions = plan_request("u1", &[(product("p1"), q("100"))], 2027, &candidates).unwrap();
        assert_eq!(portions[0].allocation_id, "a1");
        assert_eq!(portions[0].supplier_id, "sup-X");

        // Inconsistent components (fulfilled > contracted) -> fail closed.
        let broken = candidate_map(&[(
            product("p1"),
            vec![candidate(
                "a1",
                2026,
                "X",
                "100",
                "101",
                "0",
                "0",
                "ENDED",
                Some("30.00"),
            )],
        )]);
        let r = plan_request("u1", &[(product("p1"), q("10"))], 2027, &broken);
        assert!(r.is_err());
    }

    #[test]
    fn plan_multi_supplier_keeps_per_supplier_portions() {
        // Product P has obligations on X (FY2026) and Y (FY2027); product Q has
        // an entitlement on Y (FY2027). The plan must expose the supplier on
        // every portion so the create path can group into one order per supplier.
        let candidates = candidate_map(&[
            (
                product("p1"),
                vec![
                    candidate(
                        "a1",
                        2026,
                        "X",
                        "200",
                        "0",
                        "0",
                        "0",
                        "ENDED",
                        Some("30.00"),
                    ),
                    candidate(
                        "a2",
                        2027,
                        "Y",
                        "300",
                        "0",
                        "0",
                        "0",
                        "ACTIVE",
                        Some("35.00"),
                    ),
                ],
            ),
            (
                product("q1"),
                vec![candidate(
                    "b1",
                    2027,
                    "Y",
                    "400",
                    "0",
                    "0",
                    "0",
                    "ACTIVE",
                    Some("40.00"),
                )],
            ),
        ]);
        let requests = vec![(product("p1"), q("250")), (product("q1"), q("100"))];
        let portions = plan_request("u1", &requests, 2027, &candidates).unwrap();
        assert_eq!(portions.len(), 3, "P: X portion + Y portion, Q: Y portion");
        // P1: oldest obligation first (X), then Y; Q is a separate Y portion.
        assert_eq!(portions[0].product_id, "p1");
        assert_eq!(portions[0].supplier_id, "sup-X");
        assert_eq!(portions[1].product_id, "p1");
        assert_eq!(portions[1].supplier_id, "sup-Y");
        assert_eq!(portions[2].product_id, "q1");
        assert_eq!(portions[2].supplier_id, "sup-Y");
        // Grouping shape: X owns [p1 a1], Y owns [p1 a2, q1 b1].
        let mut x_allocs: Vec<&str> = Vec::new();
        let mut y_allocs: Vec<&str> = Vec::new();
        for portion in &portions {
            match portion.supplier_id.as_str() {
                "sup-X" => x_allocs.push(&portion.allocation_id),
                "sup-Y" => y_allocs.push(&portion.allocation_id),
                other => panic!("unexpected supplier {other}"),
            }
        }
        assert_eq!(x_allocs, vec!["a1"]);
        assert_eq!(y_allocs, vec!["a2", "b1"]);
    }

    #[test]
    fn plan_missing_price_fails_closed() {
        let candidates = candidate_map(&[(
            product("p1"),
            vec![candidate(
                "a1", 2026, "X", "200", "0", "0", "0", "ENDED", None,
            )],
        )]);
        let r = plan_request("u1", &[(product("p1"), q("10"))], 2027, &candidates);
        assert!(r.is_err(), "unset price_ttc must reject, never fallback");
    }

    #[test]
    fn plan_future_year_entitlement_never_resolves() {
        // A FY2028 (future) must be skipped; nothing left => over-request reject.
        let candidates = candidate_map(&[(
            product("p1"),
            vec![candidate(
                "a1",
                2028,
                "X",
                "200",
                "0",
                "0",
                "0",
                "ACTIVE",
                Some("35.00"),
            )],
        )]);
        let r = plan_request("u1", &[(product("p1"), q("10"))], 2027, &candidates);
        assert!(r.is_err(), "future-year entitlement is not resolvable");
    }

    #[test]
    fn plan_zero_or_negative_request_rejects() {
        let candidates = candidate_map(&[]);
        let r = plan_request("u1", &[(product("p1"), q("0"))], 2027, &candidates);
        assert!(r.is_err());
    }
}
