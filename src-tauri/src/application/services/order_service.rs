//! Order Service Module
//!
//! Business logic for supplier orders and order confirmation (ADR-0055 /
//! SEC-087-F). Supplier and price are ALWAYS backend-resolved from contract
//! entitlement; the caller supplies products + quantities only.
//! SQL is delegated exclusively to repositories (orders/order_allocations/
//! contracts + StockMovementService/FIFO for confirmation).
//!
//! Creation reserves entitlement transactionally. Confirmation re-resolves
//! authoritatively: supplier, recorded price, and allocation must be unchanged.
//!
//! On success confirmation converts the reservation into fulfillment, records
//! stock movement + FIFO, and marks the order Confirmed. Any failure aborts the
//! whole transaction.

use std::collections::HashMap;

use crate::domain::numeric::{legacy_float, Money, Quantity};
use crate::domain::pricing::resolver::{
    plan_request, resolve_supplier_for_item, PlannedPortion, ResolutionCandidate,
};
use crate::domain::validation::{
    check_order_is_editable, validate_create_order_request, validate_update_order_request,
};
use crate::errors::{AppError, BusinessLogicError};
use crate::models::{
    CreateOrderRequest, NewStockMovement, OrderStatus, StockMovementType, UpdateOrderRequest,
};
use crate::repositories::{DbExecutor, RepositoryProvider};

/// Service for supplier order business logic
pub struct OrderService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> OrderService<'a> {
    /// Create a new OrderService with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Resolve every item against contract entitlement (pure decision phase).
    ///
    /// Enforces: all items of an order belong to exactly ONE supplier.
    /// Quantities/prices are exact `Decimal` (ADR-0048); `f64` appears only at
    /// the repo/wire boundary via `legacy_float`.
    #[allow(clippy::type_complexity)]
    fn resolve_items(
        &self,
        items: &[crate::models::OrderItemInput],
        unit_id: &str,
        current_fiscal_year: i32,
    ) -> Result<
        (
            Vec<(crate::models::OrderItemInput, String, Money)>,
            String,
            String,
            f64,
        ),
        AppError,
    > {
        let contracts = self.executor.contracts();

        let mut resolutions: Vec<(
            crate::models::OrderItemInput,
            crate::domain::pricing::resolver::ItemResolution,
        )> = Vec::new();
        for item in items {
            let candidates =
                contracts.list_resolution_candidates_full(unit_id, &item.product_id)?;
            let requested = legacy_float::quantity_from_f64(item.quantity)?;
            let resolution = resolve_supplier_for_item(
                unit_id,
                &item.product_id,
                requested,
                current_fiscal_year,
                &candidates,
            )?;
            resolutions.push((item.clone(), resolution));
        }

        // Single-supplier rule: every item must resolve to the same supplier.
        let first_supplier = &resolutions[0].1.supplier_id;
        for (item, resolution) in &resolutions {
            if &resolution.supplier_id != first_supplier {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::PriceCalculation {
                        message: format!(
                            "أصناف الطلبية تتطلب أكثر من مورد (المنتج {} من مورد مختلف)",
                            item.product_id
                        ),
                    },
                ));
            }
        }

        let supplier = self
            .executor
            .suppliers()
            .get_supplier(first_supplier)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
                    resource: "مورد".to_string(),
                    id: first_supplier.clone(),
                })
            })?;

        // Exact total: unit_price (Money) × quantity (Quantity) per item; the
        // `f64` total is produced only at the header-write boundary.
        let weight = |item: &crate::models::OrderItemInput| -> Result<_, AppError> {
            Ok(legacy_float::quantity_from_f64(item.quantity)?)
        };
        let mut total = Money::zero();
        for (item, res) in &resolutions {
            let line = res.unit_price.checked_mul_quantity(&weight(item)?)?;
            total = total.checked_add(line)?;
        }
        let legs: Vec<(crate::models::OrderItemInput, String, Money)> = resolutions
            .into_iter()
            .map(|(item, res)| (item, res.allocation_id, res.unit_price))
            .collect();

        Ok((
            legs,
            supplier.id.clone(),
            supplier.name.clone(),
            legacy_float::money_to_f64(&total)?,
        ))
    }

    /// Reserve entitlement and write order item + reservation-leg rows.
    ///
    /// The order header must already exist (its FK is referenced by items).
    /// Legs mirror the exact resolution allocation so confirmation's
    /// re-resolution verifies an unchanged state. `f64` conversion happens only
    /// at the repo write boundary.
    fn reserve_and_write(
        &self,
        legs: &[(crate::models::OrderItemInput, String, Money)],
        order_id: &str,
        unit_id: &str,
        current_fiscal_year: i32,
        now: &str,
    ) -> Result<(), AppError> {
        let contracts = self.executor.contracts();
        for (item, allocation_id, unit_price) in legs {
            let reserved = contracts.try_increment_reserved(allocation_id, item.quantity)?;
            if reserved == 0 {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::PriceCalculation {
                        message: format!(
                            "فشل حجز الكمية المطلوبة للمنتج {} — الرصيد غير كافٍ",
                            item.product_id
                        ),
                    },
                ));
            }
            let item_id = uuid::Uuid::new_v4().to_string();
            let quantity = legacy_float::quantity_from_f64(item.quantity)?;
            let total_cost = unit_price.checked_mul_quantity(&quantity)?;
            self.executor.orders().insert_order_item(
                &item_id,
                order_id,
                item,
                legacy_float::money_to_f64(unit_price)?,
                legacy_float::money_to_f64(&total_cost)?,
                unit_id,
                current_fiscal_year,
            )?;
            self.executor.order_allocations().insert(
                &uuid::Uuid::new_v4().to_string(),
                &item_id,
                allocation_id,
                item.quantity,
                legacy_float::money_to_f64(unit_price)?,
                legacy_float::money_to_f64(&total_cost)?,
                now,
            )?;
        }
        Ok(())
    }

    pub fn create_supplier_order(
        &self,
        req: &CreateOrderRequest,
        unit_id: &str,
        current_fiscal_year: i32,
    ) -> Result<(String, f64), AppError> {
        validate_create_order_request(req)?;

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let order_date = chrono::Utc::now().date_naive().to_string();

        // Resolve FIRST (header needs the backend-derived supplier + total),
        // then write the header before any item rows (items FK to it).
        let (legs, supplier_id, supplier_name, total) =
            self.resolve_items(&req.items, unit_id, current_fiscal_year)?;

        self.executor.orders().create_supplier_order_header(
            &id,
            &req.reference_number,
            &supplier_id,
            &supplier_name,
            unit_id,
            current_fiscal_year,
            total,
            &order_date,
            &now,
        )?;

        self.reserve_and_write(&legs, &id, unit_id, current_fiscal_year, &now)?;

        Ok((id, total))
    }

    /// Create one or more supplier orders for a request (ADR-0056, Phase 4B).
    ///
    /// Authoritative multi-supplier creation path used by the IPC command. The
    /// caller supplies products + quantities only; supplier, allocation and
    /// price are backend-derived by `plan_request` (I9). The whole request is
    /// planned COMPLETELY before any write, and every resulting header, item,
    /// reservation and audit entry is materialized inside the caller's single
    /// `AuditTxService(CreateOrder)` transaction — any failure aborts
    /// everything (no partial orders, I8).
    ///
    /// Result: exactly one `SupplierOrder` per supplier. Different suppliers
    /// each get their own header; same-supplier portions share one header
    /// (ADR-0056 I1/I2, rules 5/9/14). Each planned portion becomes exactly one
    /// `SupplierOrderItem` row backed by exactly one allocation leg (I3/I4).
    /// Every produced header is anchored to the caller-resolved current fiscal
    /// year (I10) and copies the request `reference_number` (rule 26); the
    /// allocation's own fiscal year is carried only by the allocation itself.
    pub fn create_supplier_orders(
        &self,
        req: &CreateOrderRequest,
        unit_id: &str,
        current_fiscal_year: i32,
    ) -> Result<Vec<(String, f64)>, AppError> {
        validate_create_order_request(req)?;

        // Plan COMPLETELY before any write: every product's candidates are
        // fetched first (same transactional snapshot), the whole request is
        // planned, and only a fully-valid plan proceeds to materialization.
        let contracts = self.executor.contracts();
        let mut candidates_by_product: HashMap<String, Vec<ResolutionCandidate>> = HashMap::new();
        for item in &req.items {
            if !candidates_by_product.contains_key(&item.product_id) {
                let candidates =
                    contracts.list_resolution_candidates_full(unit_id, &item.product_id)?;
                candidates_by_product.insert(item.product_id.clone(), candidates);
            }
        }
        let requests: Vec<(String, Quantity)> = req
            .items
            .iter()
            .map(|item| {
                Ok::<_, AppError>((
                    item.product_id.clone(),
                    legacy_float::quantity_from_f64(item.quantity)?,
                ))
            })
            .collect::<Result<_, _>>()?;

        let portions = plan_request(
            unit_id,
            &requests,
            current_fiscal_year,
            &candidates_by_product,
        )?;
        if portions.is_empty() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::PriceCalculation {
                    message: "الطلب لا يحتوي على منتجات قابلة للتخطيط".to_string(),
                },
            ));
        }

        // Group planned portions by supplier: one header per supplier, in first
        // occurrence order (deterministic; the same supplier is never
        // duplicated across headers).
        struct SupplierGroup {
            supplier_id: String,
            supplier_name: String,
            portions: Vec<PlannedPortion>,
        }
        let mut groups: Vec<SupplierGroup> = Vec::new();
        let mut group_index: HashMap<String, usize> = HashMap::new();
        for portion in portions {
            match group_index.get(&portion.supplier_id).copied() {
                Some(i) => groups[i].portions.push(portion),
                None => {
                    group_index.insert(portion.supplier_id.clone(), groups.len());
                    groups.push(SupplierGroup {
                        supplier_id: portion.supplier_id.clone(),
                        supplier_name: portion.supplier_name.clone(),
                        portions: vec![portion],
                    });
                }
            }
        }

        let now = chrono::Utc::now().to_rfc3339();
        let order_date = chrono::Utc::now().date_naive().to_string();
        let mut results: Vec<(String, f64)> = Vec::new();

        for group in &groups {
            let order_id = uuid::Uuid::new_v4().to_string();

            // Exact totals: Money × Quantity per portion; f64 appears only at
            // the repo/wire boundary via legacy_float (ADR-0048).
            let mut total = Money::zero();
            for portion in &group.portions {
                let line = portion.unit_price.checked_mul_quantity(&portion.quantity)?;
                total = total.checked_add(line)?;
            }
            let total_f64 = legacy_float::money_to_f64(&total)?;

            self.executor.orders().create_supplier_order_header(
                &order_id,
                &req.reference_number,
                &group.supplier_id,
                &group.supplier_name,
                unit_id,
                current_fiscal_year,
                total_f64,
                &order_date,
                &now,
            )?;

            // One item row + one allocation leg per portion (I3/I4), using the
            // existing exact write path.
            let legs: Vec<(crate::models::OrderItemInput, String, Money)> = group
                .portions
                .iter()
                .map(|portion| {
                    Ok::<_, AppError>((
                        crate::models::OrderItemInput {
                            product_id: portion.product_id.clone(),
                            quantity: legacy_float::quantity_to_f64(&portion.quantity)?,
                        },
                        portion.allocation_id.clone(),
                        portion.unit_price,
                    ))
                })
                .collect::<Result<_, _>>()?;
            self.reserve_and_write(&legs, &order_id, unit_id, current_fiscal_year, &now)?;

            results.push((order_id, total_f64));
        }

        Ok(results)
    }

    pub fn update_supplier_order(
        &self,
        req: &UpdateOrderRequest,
        unit_id: &str,
        current_fiscal_year: i32,
    ) -> Result<f64, AppError> {
        validate_update_order_request(req)?;

        let repo = self.executor.orders();
        let order = repo.get_supplier_order(&req.id)?.ok_or_else(|| {
            AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
                resource: "طلبية".to_string(),
                id: req.id.clone(),
            })
        })?;
        check_order_is_editable(&order)?;
        let unit_id_for_order = order.unit_id.clone().unwrap_or_else(|| unit_id.to_string());

        // Release previously reserved entitlement before rebuilding items.
        self.release_order_reservations(&req.id)?;

        // Drop the previous items (+ their allocation legs via FK cascade)
        // BEFORE re-inserting, otherwise stale legs would double-count at
        // confirmation. Runs inside the caller's transaction: any later
        // failure rolls everything back.
        repo.delete_order_items(&req.id)?;

        let now = chrono::Utc::now().to_rfc3339();
        let (legs, supplier_id, supplier_name, total) =
            self.resolve_items(&req.items, &unit_id_for_order, current_fiscal_year)?;

        self.reserve_and_write(
            &legs,
            &req.id,
            &unit_id_for_order,
            current_fiscal_year,
            &now,
        )?;

        let updated = self.executor.orders().update_supplier_order_header(
            &req.id,
            &supplier_id,
            &supplier_name,
            &req.reference_number,
            total,
        )?;
        if updated == 0 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OrderAlreadyConfirmed {
                    order_id: req.id.clone(),
                },
            ));
        }

        Ok(total)
    }

    pub fn delete_supplier_order(&self, order_id: &str) -> Result<(), AppError> {
        let repo = self.executor.orders();
        let order = repo.get_supplier_order(order_id)?.ok_or_else(|| {
            AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
                resource: "طلبية".to_string(),
                id: order_id.to_string(),
            })
        })?;
        check_order_is_editable(&order)?;

        self.release_order_reservations(order_id)?;
        repo.delete_supplier_order(order_id)
    }

    /// Release reservation quantities recorded for an order's items.
    /// SQL-only: reads legs (per allocation) and decrements guarded.
    fn release_order_reservations(&self, order_id: &str) -> Result<(), AppError> {
        let legs = self.executor.order_allocations().list_for_order(order_id)?;
        let contracts = self.executor.contracts();
        for leg in &legs {
            let n = contracts.try_release_reserved(&leg.allocation_id, leg.quantity)?;
            if n == 0 {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::OperationNotPermitted {
                        message: format!(
                            "تعذر تحرير الحجز على الرصيد {} — تناقض في الكميات",
                            leg.allocation_id
                        ),
                    },
                ));
            }
        }
        Ok(())
    }

    /// Confirm an order atomically:
    /// - re-resolve every item authoritatively and verify unchanged state
    /// - verify supplier is identical to the stored supplier (immutable)
    /// - full-coverage re-verification (any failure => reject ENTIRE order)
    /// - convert reservation into fulfillment (guarded UPDATE)
    /// - record IN stock movement + FIFO layer
    /// - mark order Confirmed
    ///
    /// The caller wraps this in a transaction via AuditTxService.
    pub fn confirm_order_atomic(
        &self,
        order_id: &str,
        user_id: &str,
        username: &str,
    ) -> Result<(), AppError> {
        let repo = self.executor.orders();
        let contracts = self.executor.contracts();
        let stock_repo = crate::application::services::StockMovementService::new(self.executor);
        let fifo_repo = self.executor.fifo_layers();
        let status_repo = self.executor.fiscal_year_status();

        let order = repo
            .get_supplier_order(order_id)?
            .ok_or_else(|| AppError::Internal(format!("Order not found: {order_id}")))?;

        if order.status == OrderStatus::Confirmed {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OrderAlreadyConfirmed {
                    order_id: order_id.to_string(),
                },
            ));
        }

        let unit_id = order.unit_id.as_deref().ok_or_else(|| {
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "الطلبية لا تحمل وحدة — غير صالحة للتأكيد".to_string(),
            })
        })?;

        // Anchor fiscal year = order's fiscal year (fallback: open year).
        let anchor_fy = match order.fiscal_year {
            Some(fy) => fy,
            None => status_repo.get_open_year()?.ok_or_else(|| {
                AppError::Internal("No open fiscal year found for order confirmation".to_string())
            })?,
        };
        crate::application::services::FiscalValidationService::new(self.executor)
            .assert_fiscal_year_open(anchor_fy)?;

        // Phase 4A (SEC-087): the order's own reserved quantity per
        // allocation, summed across its persisted legs. Confirmation re-resolves
        // authoritatively, but this order's own reservation must not be counted
        // against itself: the resolver sees `current remaining including this
        // order` and only THIS order's own reservation is subtracted. Every
        // other order's reservation remains counted. Fails closed on any
        // inconsistent persisted components.
        let mut own_reserved: HashMap<String, Quantity> = HashMap::new();
        for leg in self.executor.order_allocations().list_for_order(order_id)? {
            let leg_qty = legacy_float::quantity_from_f64(leg.quantity)?;
            let acc = own_reserved
                .entry(leg.allocation_id.clone())
                .or_insert(Quantity::zero());
            *acc = acc.checked_add(leg_qty)?;
        }
        // Tracks how much of this order's own reservation was already converted
        // to fulfillment within this loop, so a later item drawing the same
        // allocation nets only the still-reserved share (zero-sum transfer: the
        // converted quantity has already left reserved_quantity in the DB).
        let mut converted_own: HashMap<String, Quantity> = HashMap::new();

        // Authoritative re-resolution + unchanged-state verification.
        // Each recorded leg must still resolve to the identical allocation with
        // the identical price_ttc; the supplier must be identical to the
        // stored (creation-time) supplier.
        let order_supplier_id = order.supplier_id.clone();
        let mut legs: Vec<(String, Quantity, Money)> = Vec::new();
        for recorded in repo.get_order_items_for_confirmation(order_id)? {
            let (product_id, quantity, _product_name, recorded_price, allocation_id) = recorded;
            let mut candidates = contracts.list_resolution_candidates_full(unit_id, &product_id)?;

            // Subtract THIS order's still-reserved quantity from the recorded
            // allocation's candidate only; reservations belonging to every other
            // order stay available to the resolver. `checked_sub` fails closed
            // (NegativeNotAllowed) if the repository state is inconsistent.
            if let Some(own_total) = own_reserved.get(&allocation_id).copied() {
                let converted = match converted_own.get(&allocation_id) {
                    Some(v) => *v,
                    None => Quantity::zero(),
                };
                let still_own = own_total.checked_sub(converted)?;
                if still_own.is_positive() {
                    for candidate in candidates.iter_mut() {
                        if candidate.allocation_id == allocation_id {
                            candidate.reserved_quantity =
                                candidate.reserved_quantity.checked_sub(still_own)?;
                        }
                    }
                }
            }

            let requested = legacy_float::quantity_from_f64(quantity)?;
            let resolution =
                resolve_supplier_for_item(unit_id, &product_id, requested, anchor_fy, &candidates)?;

            if resolution.supplier_id != order_supplier_id {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::PriceCalculation {
                        message: format!(
                            "المورد لم يعد مؤهلاً للمنتج {product_id} — تأكيد الطلبية مرفوض بالكامل"
                        ),
                    },
                ));
            }
            if resolution.allocation_id != allocation_id {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::PriceCalculation {
                        message: format!(
                            "رصيد العقد تحرك بعد الإنشاء للمنتج {product_id} — أعد إنشاء الطلبية"
                        ),
                    },
                ));
            }
            // Exact price compare (no float epsilon): the recorded price is a
            // serialized copy of the same decimal, so disagreement means the
            // contract moved after creation.
            let recorded_money = legacy_float::money_from_f64(recorded_price)?;
            if resolution.unit_price != recorded_money {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::PriceCalculation {
                        message: format!(
                            "تم تغيير سعر العقد للمنتج {product_id} — أعد إنشاء الطلبية"
                        ),
                    },
                ));
            }

            // Guarded conversion reservation -> fulfillment.
            let n =
                contracts.try_convert_reserved_to_fulfilled(&resolution.allocation_id, quantity)?;
            if n == 0 {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::PriceCalculation {
                        message: format!("فشل تحويل الحجز إلى استهلاك للمنتج {product_id}"),
                    },
                ));
            }
            // This order's own reservation on this allocation was just converted
            // (moved from reserved_quantity to fulfilled_quantity in the DB).
            let converted = converted_own
                .entry(allocation_id.clone())
                .or_insert(Quantity::zero());
            *converted = converted.checked_add(requested)?;
            legs.push((product_id, requested, resolution.unit_price));
        }

        // Stock movement + FIFO for every item (all within the surrounding tx).
        let now = chrono::Utc::now().to_rfc3339();
        for (product_id, quantity, unit_price) in &legs {
            let movement = NewStockMovement {
                product_id: product_id.clone(),
                movement_type: StockMovementType::In,
                quantity: legacy_float::quantity_to_f64(quantity)?,
                reference_type: Some("Order".to_string()),
                reference_id: Some(order_id.to_string()),
                notes: Some(format!("طلبية من: {}", order.supplier_name)),
                user_id: user_id.to_string(),
                username: username.to_string(),
                unit_id: Some(unit_id.to_string()),
                unit_cost: Some(legacy_float::money_to_f64(unit_price)?),
            };
            stock_repo.record_stock_movement(&movement)?;

            fifo_repo.create_layer(
                unit_id,
                product_id,
                "ORDER",
                Some(order_id),
                legacy_float::money_to_f64(unit_price)?,
                legacy_float::quantity_to_f64(quantity)?,
                &now,
                user_id,
                anchor_fy,
            )?;
        }

        repo.set_order_confirmed(order_id)?;

        Ok(())
    }

    pub fn get_supplier_order(
        &self,
        order_id: &str,
    ) -> Result<Option<crate::models::SupplierOrder>, AppError> {
        self.executor.orders().get_supplier_order(order_id)
    }

    pub fn get_supplier_order_items(
        &self,
        order_id: &str,
    ) -> Result<Vec<crate::models::SupplierOrderItem>, AppError> {
        self.executor.orders().get_supplier_order_items(order_id)
    }

    pub fn list_supplier_orders(
        &self,
        fiscal_year: Option<i32>,
    ) -> Result<Vec<crate::models::SupplierOrder>, AppError> {
        self.executor.orders().list_supplier_orders(fiscal_year)
    }
}
