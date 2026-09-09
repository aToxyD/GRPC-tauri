//! Contract Service Module
//!
//! Contract lifecycle + entitlement operations (ADR-0055 / SEC-087-F).
//! WILAYA-only mutations are enforced at the command/authz layer; this service
//! enforces domain rules (lifecycle transitions, price freeze, single live
//! contract per unit+FiscalYear, per-UNIT obligations, auditable release).
//! SQL is delegated exclusively to `ContractRepository` / `SupplierRepository`
//! / `ProductRepository` / `UnitRepository`.

use crate::domain::numeric::legacy_float;
use crate::domain::pricing::price::compute_contract_fiscal;
use crate::domain::validation::{
    validate_add_contract_product_request, validate_create_contract_request,
    validate_product_units, validate_release_contract_allocation_request,
    validate_set_agreed_price_ht_request,
};
use crate::errors::{AppError, BusinessLogicError};
use crate::models::{
    AddContractProductRequest, Contract, ContractAllocation, ContractAllocationException,
    ContractAllocationView, ContractPriceSnapshot, ContractProduct, ContractStatus,
    CreateContractRequest, ReleaseContractAllocationRequest, SetAgreedPriceHtRequest,
    UnitContractEntitlement,
};
use crate::repositories::{DbExecutor, RepositoryProvider};

/// Service for contract business logic
pub struct ContractService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> ContractService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    fn not_found(resource: &str, id: &str) -> AppError {
        AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
            resource: resource.to_string(),
            id: id.to_string(),
        })
    }

    /// Create a contract header (status `proposed`). WILAYA only.
    ///
    /// Exactly ONE live (proposed/accepted/active) contract per
    /// (unit_id, fiscal_year) — enforced structurally by the partial unique
    /// index `idx_contracts_unit_year_live` and guarded here for a
    /// human-readable error.
    pub fn create_contract(&self, req: &CreateContractRequest) -> Result<Contract, AppError> {
        validate_create_contract_request(req)?;

        if self
            .executor
            .suppliers()
            .get_supplier(&req.supplier_id)?
            .is_none()
        {
            return Err(Self::not_found("مورد", &req.supplier_id));
        }
        if self.executor.units().get_unit(&req.unit_id)?.is_none() {
            return Err(Self::not_found("وحدة", &req.unit_id));
        }
        if self
            .executor
            .contracts()
            .count_live_contracts_for_unit_year(&req.unit_id, req.fiscal_year)?
            > 0
        {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "توجد عقد نشط بالفعل للوحدة {} والسنة المالية {} — لا يمكن إنشاء عقد آخر",
                        req.unit_id, req.fiscal_year
                    ),
                },
            ));
        }

        let now = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        self.executor.contracts().insert_contract(&id, req, &now)?;

        if let Some(notes) = &req.notes {
            let _ = self.executor.contracts().update_contract_notes(
                &id,
                &Some(notes.clone()),
                &["proposed"],
            )?;
        }

        Ok(self
            .executor
            .contracts()
            .get_contract(&id)?
            .expect("inserted contract must exist"))
    }

    /// Add a product line to a `proposed` contract: creates the
    /// `contract_products` row with `proposed_price_ht` (+ `agreed_price_ht`
    /// when given) and the per-UNIT `contract_allocations` obligation row.
    pub fn add_contract_product(
        &self,
        req: &AddContractProductRequest,
    ) -> Result<(String, String), AppError> {
        validate_add_contract_product_request(req)?;

        let contract = self
            .executor
            .contracts()
            .get_contract(&req.contract_id)?
            .ok_or_else(|| Self::not_found("عقد", &req.contract_id))?;
        if contract.status != ContractStatus::Proposed {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "لا يمكن إضافة منتج لعقد لم يعد في طور الاقتراح".to_string(),
                },
            ));
        }
        if self
            .executor
            .products()
            .get_product(&req.product_id)?
            .is_none()
        {
            return Err(Self::not_found("منتج", &req.product_id));
        }
        if self
            .executor
            .contracts()
            .product_in_contract(&req.contract_id, &req.product_id)?
        {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "المنتج مضاف مسبقاً إلى هذا العقد".to_string(),
                },
            ));
        }

        let now = chrono::Utc::now().to_rfc3339();
        let contract_product_id = uuid::Uuid::new_v4().to_string();
        let allocation_id = uuid::Uuid::new_v4().to_string();

        self.executor
            .contracts()
            .insert_contract_product(&contract_product_id, req, &now)?;
        self.executor.contracts().insert_allocation(
            &allocation_id,
            &contract.id,
            &contract_product_id,
            &contract.unit_id,
            &req.product_id,
            contract.fiscal_year,
            req.contracted_quantity,
            &now,
        )?;

        Ok((contract_product_id, allocation_id))
    }

    /// Freeze the agreed HT price and persist the authoritative ordered-price
    /// snapshot. This is the SEC-087 Phase 3 price-agreement boundary: it
    /// resolves the product's authoritative TVA classification and unit
    /// configuration (fail closed on any gap — never consulting the legacy
    /// fiscal-year policy), derives exact TVA/TTC via `compute_contract_fiscal`,
    /// and persists the full snapshot atomically. Allowed only while the
    /// contract is `proposed`; after acceptance the price is immutable.
    pub fn set_agreed_price_ht(&self, req: &SetAgreedPriceHtRequest) -> Result<f64, AppError> {
        validate_set_agreed_price_ht_request(req)?;

        // Resolve the contract-product row to locate its product.
        let cp = self
            .executor
            .contracts()
            .get_contract_product(&req.contract_product_id)?
            .ok_or_else(|| Self::not_found("سطر عقد", &req.contract_product_id))?;

        // Fail closed on an incomplete product configuration: the product
        // master is the sole current source for unit/TVA configuration. There
        // is deliberately NO fallback to fiscal_year_tax_policy (SEC-087).
        let config_codes = self
            .executor
            .products()
            .get_product_config_codes(&cp.product_id)?
            .ok_or_else(|| Self::not_found("منتج", &cp.product_id))?;
        let config = validate_product_units(
            config_codes.purchase_unit,
            config_codes.consumption_unit,
            config_codes.conversion_factor,
            config_codes.tva_classification,
        )?;

        // Exact HT authority (Money), never reinterpreted from a historical TTC.
        let agreed_price_ht = legacy_float::money_from_f64(req.agreed_price_ht)?;
        let breakdown =
            compute_contract_fiscal(&agreed_price_ht, &config.tva_classification.rate())?;

        let snapshot = ContractPriceSnapshot {
            contract_product_id: req.contract_product_id.clone(),
            agreed_price_ht_scaled: agreed_price_ht.to_scaled_i64()?,
            tva_classification_code: config.tva_classification.code(),
            tva_rate_scaled: config.tva_classification.rate().to_scaled_i64()?,
            tva_amount_scaled: breakdown.tva_amount.to_scaled_i64()?,
            price_ttc_scaled: breakdown.price_ttc.to_scaled_i64()?,
            purchase_unit_code: config.purchase_unit.code(),
            consumption_unit_code: config.consumption_unit.code(),
            conversion_factor: config.conversion_factor,
        };

        let n = self.executor.contracts().freeze_contract_price(&snapshot)?;
        if n == 0 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "السعر مجمد — العقد لم يعد في طور الاقتراح أو السطر غير موجود"
                        .to_string(),
                },
            ));
        }

        legacy_float::money_to_f64(&breakdown.price_ttc).map_err(AppError::from)
    }

    /// Accept a `proposed` contract. Every product line must have persisted its
    /// authoritative pricing snapshot (`price_ttc` present) before acceptance —
    /// this is the price immutability boundary. A line without a computed
    /// `price_ttc` is rejected, so acceptance fails closed.
    pub fn accept_contract(&self, contract_id: &str, at: &str) -> Result<Contract, AppError> {
        let contract = self
            .executor
            .contracts()
            .get_contract(contract_id)?
            .ok_or_else(|| Self::not_found("عقد", contract_id))?;
        if !contract.status.can_accept() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!("لا يمكن قبول العقد في الحالة {}", contract.status),
                },
            ));
        }
        let products = self
            .executor
            .contracts()
            .get_contract_products(contract_id)?;
        if products.iter().any(|cp| cp.price_ttc.is_none()) {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "لا يمكن قبول العقد قبل تثبيت سعر الاتفاق (سعر TTC) لكل المنتجات"
                        .to_string(),
                },
            ));
        }
        let n = self.executor.contracts().update_contract_status(
            contract_id,
            &["proposed"],
            "accepted",
            "accepted_at",
            at,
        )?;
        if n == 0 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "تعذر قبول العقد (تغيرت الحالة)".to_string(),
                },
            ));
        }
        Ok(self
            .executor
            .contracts()
            .get_contract(contract_id)?
            .expect("contract must exist after accept"))
    }

    /// Activate an `accepted` (or re-affirm `active`) contract.
    pub fn activate_contract(&self, contract_id: &str, at: &str) -> Result<Contract, AppError> {
        let contract = self
            .executor
            .contracts()
            .get_contract(contract_id)?
            .ok_or_else(|| Self::not_found("عقد", contract_id))?;
        if !contract.status.can_activate() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!("لا يمكن تفعيل العقد في الحالة {}", contract.status),
                },
            ));
        }
        let n = self.executor.contracts().update_contract_status(
            contract_id,
            &["accepted", "active"],
            "active",
            "activated_at",
            at,
        )?;
        if n == 0 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "تعذر تفعيل العقد (تغيرت الحالة)".to_string(),
                },
            ));
        }
        Ok(self
            .executor
            .contracts()
            .get_contract(contract_id)?
            .expect("contract must exist after activate"))
    }

    /// End an `accepted`/`active` contract. ENDED contracts with
    /// `effective_remaining > 0` remain fulfillable obligations.
    pub fn end_contract(&self, contract_id: &str, at: &str) -> Result<Contract, AppError> {
        let contract = self
            .executor
            .contracts()
            .get_contract(contract_id)?
            .ok_or_else(|| Self::not_found("عقد", contract_id))?;
        if !contract.status.can_end() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!("لا يمكن إنهاء العقد في الحالة {}", contract.status),
                },
            ));
        }
        let n = self.executor.contracts().update_contract_status(
            contract_id,
            &["accepted", "active"],
            "ended",
            "ended_at",
            at,
        )?;
        if n == 0 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "تعذر إنهاء العقد (تغيرت الحالة)".to_string(),
                },
            ));
        }
        Ok(self
            .executor
            .contracts()
            .get_contract(contract_id)?
            .expect("contract must exist after end"))
    }

    /// Cancel a `proposed`/`accepted`/`active` contract. All its allocations
    /// are marked CANCELLED (never resolvable again; fulfillment kept).
    pub fn cancel_contract(&self, contract_id: &str, at: &str) -> Result<Contract, AppError> {
        let contract = self
            .executor
            .contracts()
            .get_contract(contract_id)?
            .ok_or_else(|| Self::not_found("عقد", contract_id))?;
        if !contract.status.can_cancel() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!("لا يمكن إلغاء العقد في الحالة {}", contract.status),
                },
            ));
        }
        let n = self.executor.contracts().update_contract_status(
            contract_id,
            &["proposed", "accepted", "active"],
            "cancelled",
            "cancelled_at",
            at,
        )?;
        if n == 0 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "تعذر إلغاء العقد (تغيرت الحالة)".to_string(),
                },
            ));
        }
        for allocation in self
            .executor
            .contracts()
            .list_allocations_for_contract(contract_id)?
        {
            if allocation.entitlement_state != "CANCELLED" {
                let _ = self
                    .executor
                    .contracts()
                    .set_entitlement_state(&allocation.id, "CANCELLED")?;
            }
        }
        Ok(self
            .executor
            .contracts()
            .get_contract(contract_id)?
            .expect("contract must exist after cancel"))
    }

    /// WILAYA-authorized obligation release. Recorded as an auditable
    /// exception; reduces the releasing allocation's effective remaining so a
    /// new supplier entitlement becomes usable. Never rewrites fulfilled
    /// quantities.
    pub fn release_allocation(
        &self,
        req: &ReleaseContractAllocationRequest,
        actor: &str,
    ) -> Result<ContractAllocationException, AppError> {
        validate_release_contract_allocation_request(req)?;
        let allocation = self
            .executor
            .contracts()
            .get_allocation(&req.allocation_id)?
            .ok_or_else(|| Self::not_found("رصيد عقد", &req.allocation_id))?;
        if allocation.entitlement_state == "CANCELLED" {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "لا يمكن تحرير رصيد ملغى".to_string(),
                },
            ));
        }
        if req.released_quantity > allocation.effective_remaining() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "الكمية المطلوب تحريرها ({}) تتجاوز المتبقي الفعلي ({})",
                        req.released_quantity,
                        allocation.effective_remaining()
                    ),
                },
            ));
        }
        // Physical backstop (SQL guard).
        if self
            .executor
            .contracts()
            .try_increment_released(&allocation.id, req.released_quantity)?
            == 0
        {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "تعذر تحرير الكمية — تجاوز الكمية المتعاقد عليها".to_string(),
                },
            ));
        }
        let now = chrono::Utc::now().to_rfc3339();
        let exception_id = uuid::Uuid::new_v4().to_string();
        self.executor.contracts().insert_exception(
            &exception_id,
            &allocation.id,
            req.reason_code,
            req.released_quantity,
            &req.reason_note,
            actor,
            &now,
        )?;
        Ok(self
            .executor
            .contracts()
            .get_exception(&exception_id)?
            .expect("inserted exception must exist"))
    }

    /// Revoke a prior release: only while the released quantity is still
    /// fully releasable (never drives `released_quantity` negative).
    pub fn revoke_release(&self, exception_id: &str) -> Result<(), AppError> {
        let exception = self
            .executor
            .contracts()
            .get_exception(exception_id)?
            .ok_or_else(|| Self::not_found("استثناء تحرير", exception_id))?;
        if self
            .executor
            .contracts()
            .try_decrement_released(&exception.allocation_id, exception.released_quantity)?
            == 0
        {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "تعذر إلغاء التحرير — الحالة تغيرت، الكمية المحررة مستهلكة"
                        .to_string(),
                },
            ));
        }
        self.executor.contracts().delete_exception(exception_id)?;
        Ok(())
    }

    /// Fiscal close integration: `accepted`/`active` contracts for
    /// (unit, year) become ENDED (obligations persist); `proposed` contracts
    /// become CANCELLED (never ratified, no obligation). Additive to
    /// `FiscalClosingService::close_year`.
    pub fn end_live_contracts_for_unit_year(
        &self,
        unit_id: &str,
        year: i32,
        at: &str,
    ) -> Result<usize, AppError> {
        let mut affected = 0usize;
        for contract in self
            .executor
            .contracts()
            .list_contracts(Some(unit_id), None, Some(year))?
        {
            match contract.status {
                ContractStatus::Proposed => {
                    if self.executor.contracts().update_contract_status(
                        &contract.id,
                        &["proposed"],
                        "cancelled",
                        "cancelled_at",
                        at,
                    )? != 0
                    {
                        for allocation in self
                            .executor
                            .contracts()
                            .list_allocations_for_contract(&contract.id)?
                        {
                            let _ = self
                                .executor
                                .contracts()
                                .set_entitlement_state(&allocation.id, "CANCELLED")?;
                        }
                        affected += 1;
                    }
                }
                ContractStatus::Accepted | ContractStatus::Active => {
                    if self.executor.contracts().update_contract_status(
                        &contract.id,
                        &["accepted", "active"],
                        "ended",
                        "ended_at",
                        at,
                    )? != 0
                    {
                        affected += 1;
                    }
                }
                ContractStatus::Ended | ContractStatus::Cancelled => {}
            }
        }
        Ok(affected)
    }

    // ------------------------------------------------------------------
    // Read/query surface
    // ------------------------------------------------------------------

    pub fn get_contract(&self, contract_id: &str) -> Result<Option<Contract>, AppError> {
        self.executor.contracts().get_contract(contract_id)
    }

    pub fn list_contracts(
        &self,
        unit_id: Option<&str>,
        supplier_id: Option<&str>,
        fiscal_year: Option<i32>,
    ) -> Result<Vec<Contract>, AppError> {
        self.executor
            .contracts()
            .list_contracts(unit_id, supplier_id, fiscal_year)
    }

    pub fn get_contract_products(
        &self,
        contract_id: &str,
    ) -> Result<Vec<ContractProduct>, AppError> {
        self.executor.contracts().get_contract_products(contract_id)
    }

    pub fn list_allocations_for_contract(
        &self,
        contract_id: &str,
    ) -> Result<Vec<ContractAllocation>, AppError> {
        self.executor
            .contracts()
            .list_allocations_for_contract(contract_id)
    }

    /// Backend-derived obligation projection (A5/P2): `effective_remaining` is
    /// computed in the domain model (`ContractAllocationView`) — frontends and
    /// sync consumers must never re-derive obligations.
    pub fn list_allocation_views(
        &self,
        contract_id: &str,
    ) -> Result<Vec<ContractAllocationView>, AppError> {
        let rows = self
            .executor
            .contracts()
            .list_allocations_for_contract(contract_id)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Fleet-wide entitlement projection (WILAYA Excel export surface).
    pub fn list_all_allocation_views(&self) -> Result<Vec<ContractAllocationView>, AppError> {
        let rows = self.executor.contracts().list_all_allocations()?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub fn list_active_allocations_for_unit_year(
        &self,
        unit_id: &str,
        fiscal_year: i32,
    ) -> Result<Vec<ContractAllocation>, AppError> {
        self.executor
            .contracts()
            .list_active_allocations_for_unit_year(unit_id, fiscal_year)
    }

    /// Read-only UNIT projection of the locally imported ContractCatalog
    /// allocations (Phase 4). `effective_remaining` is derived in the service
    /// via the single-source domain helper (`From<UnitEntitlementRow>`); the
    /// frontend and the repository perform no obligation arithmetic (A5/P2).
    pub fn list_unit_entitlements(
        &self,
        unit_id: &str,
    ) -> Result<Vec<UnitContractEntitlement>, AppError> {
        let rows = self.executor.contracts().list_unit_entitlements(unit_id)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub fn list_exceptions_for_allocation(
        &self,
        allocation_id: &str,
    ) -> Result<Vec<ContractAllocationException>, AppError> {
        self.executor
            .contracts()
            .list_exceptions_for_allocation(allocation_id)
    }
}

impl crate::architecture::Service for ContractService<'_> {}
