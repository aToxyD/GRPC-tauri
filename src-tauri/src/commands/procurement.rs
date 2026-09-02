//! Procurement Management Commands (ADR-0055 / SEC-087-F)
//!
//! Suppliers, contracts, per-UNIT allocations, obligation releases, and
//! fiscal-year TVA policy. Every write is WILAYA-admin gated through authz and
//! persisted atomically with its audit entry. UNIT nodes consume procurement
//! state exclusively through the ContractCatalog sync packages.
//!
//! Clean Architecture: Commands -> Services -> Repositories -> DB.

use crate::application::authz::Action;
use crate::application::services::{
    AuditTxService, ContractService, FiscalTaxPolicyService, SupplierService,
};
use crate::commands::common::{
    db_mut_or_command_error, db_ref_or_command_error, user_ctx_from_session,
};
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::domain::audit::AuditAction;
use crate::errors::into_command_error;
use crate::models::{
    AddContractProductRequest, AssociateUnitSupplierRequest, Contract, ContractAllocation,
    ContractAllocationException, ContractProduct, ContractTransitionRequest, CreateContractRequest,
    CreateSupplierRequest, FiscalYearTaxPolicy, ReleaseContractAllocationRequest,
    RevokeContractAllocationReleaseRequest, SetAgreedPriceRequest, SetSupplierActiveRequest,
    SetTaxPolicyRequest, Supplier, UpdateSupplierRequest,
};
use tauri::State;

// ---------------------------------------------------------------------------
// Suppliers (WILAYA admin)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn create_supplier(
    state: State<AppState>,
    request: CreateSupplierRequest,
) -> Result<Supplier, String> {
    let (session, _) =
        authorize_command(&state, Action::ManageSuppliers, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::CreateSupplier, &user_ctx, |tx| {
        SupplierService::new(tx.executor).create_supplier(&request)
    })
    .map_err(into_command_error)
}

#[tauri::command]
pub fn update_supplier(
    state: State<AppState>,
    request: UpdateSupplierRequest,
) -> Result<Supplier, String> {
    let (session, _) =
        authorize_command(&state, Action::ManageSuppliers, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::UpdateSupplier, &user_ctx, |tx| {
        SupplierService::new(tx.executor).update_supplier(&request)
    })
    .map_err(into_command_error)
}

#[tauri::command]
pub fn set_supplier_active(
    state: State<AppState>,
    request: SetSupplierActiveRequest,
) -> Result<Supplier, String> {
    let (session, _) =
        authorize_command(&state, Action::ManageSuppliers, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::SetSupplierActive, &user_ctx, |tx| {
        SupplierService::new(tx.executor).set_supplier_active(&request.supplier_id, request.active)
    })
    .map_err(into_command_error)
}

#[tauri::command]
pub fn associate_supplier_with_unit(
    state: State<AppState>,
    request: AssociateUnitSupplierRequest,
) -> Result<(), String> {
    let (session, _) =
        authorize_command(&state, Action::ManageSuppliers, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::AssociateUnitSupplier, &user_ctx, |tx| {
        SupplierService::new(tx.executor)
            .associate_with_unit(&request.unit_id, &request.supplier_id)
    })
    .map_err(into_command_error)
}

#[tauri::command]
pub fn disassociate_supplier_from_unit(
    state: State<AppState>,
    request: AssociateUnitSupplierRequest,
) -> Result<(), String> {
    let (session, _) =
        authorize_command(&state, Action::ManageSuppliers, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::DisassociateUnitSupplier, &user_ctx, |tx| {
        SupplierService::new(tx.executor)
            .disassociate_from_unit(&request.unit_id, &request.supplier_id)
    })
    .map_err(into_command_error)
}

// ---------------------------------------------------------------------------
// Contracts (WILAYA admin)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn create_contract(
    state: State<AppState>,
    request: CreateContractRequest,
) -> Result<Contract, String> {
    let (session, _) =
        authorize_command(&state, Action::ManageContracts, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::CreateContract, &user_ctx, |tx| {
        ContractService::new(tx.executor).create_contract(&request)
    })
    .map_err(into_command_error)
}

#[tauri::command]
pub fn add_contract_product(
    state: State<AppState>,
    request: AddContractProductRequest,
) -> Result<(String, String), String> {
    let (session, _) =
        authorize_command(&state, Action::ManageContracts, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::AddContractProduct, &user_ctx, |tx| {
        ContractService::new(tx.executor).add_contract_product(&request)
    })
    .map_err(into_command_error)
}

#[tauri::command]
pub fn set_contract_product_agreed_price(
    state: State<AppState>,
    request: SetAgreedPriceRequest,
) -> Result<f64, String> {
    let (session, _) = authorize_command(&state, Action::ApproveContractPrice, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::SetAgreedPrice, &user_ctx, |tx| {
        ContractService::new(tx.executor).set_agreed_price(&request)
    })
    .map_err(into_command_error)
}

#[tauri::command]
pub fn accept_contract(
    state: State<AppState>,
    request: ContractTransitionRequest,
) -> Result<Contract, String> {
    let (session, _) =
        authorize_command(&state, Action::ManageContracts, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::AcceptContract, &user_ctx, |tx| {
        let at = chrono::Utc::now().to_rfc3339();
        ContractService::new(tx.executor).accept_contract(&request.contract_id, &at)
    })
    .map_err(into_command_error)
}

#[tauri::command]
pub fn activate_contract(
    state: State<AppState>,
    request: ContractTransitionRequest,
) -> Result<Contract, String> {
    let (session, _) =
        authorize_command(&state, Action::ManageContracts, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::ActivateContract, &user_ctx, |tx| {
        let at = chrono::Utc::now().to_rfc3339();
        ContractService::new(tx.executor).activate_contract(&request.contract_id, &at)
    })
    .map_err(into_command_error)
}

#[tauri::command]
pub fn end_contract(
    state: State<AppState>,
    request: ContractTransitionRequest,
) -> Result<Contract, String> {
    let (session, _) =
        authorize_command(&state, Action::CloseContract, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::EndContract, &user_ctx, |tx| {
        let at = chrono::Utc::now().to_rfc3339();
        ContractService::new(tx.executor).end_contract(&request.contract_id, &at)
    })
    .map_err(into_command_error)
}

#[tauri::command]
pub fn cancel_contract(
    state: State<AppState>,
    request: ContractTransitionRequest,
) -> Result<Contract, String> {
    let (session, _) =
        authorize_command(&state, Action::CloseContract, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::CancelContract, &user_ctx, |tx| {
        let at = chrono::Utc::now().to_rfc3339();
        ContractService::new(tx.executor).cancel_contract(&request.contract_id, &at)
    })
    .map_err(into_command_error)
}

#[tauri::command]
pub fn release_contract_allocation(
    state: State<AppState>,
    request: ReleaseContractAllocationRequest,
) -> Result<ContractAllocationException, String> {
    let (session, _) = authorize_command(&state, Action::ReleaseContractAllocation, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(
        db,
        AuditAction::ReleaseContractAllocation,
        &user_ctx,
        |tx| ContractService::new(tx.executor).release_allocation(&request, &session.user_id),
    )
    .map_err(into_command_error)
}

#[tauri::command]
pub fn revoke_contract_allocation_release(
    state: State<AppState>,
    request: RevokeContractAllocationReleaseRequest,
) -> Result<(), String> {
    let (session, _) = authorize_command(&state, Action::RevokeContractAllocationRelease, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(
        db,
        AuditAction::RevokeContractAllocationRelease,
        &user_ctx,
        |tx| ContractService::new(tx.executor).revoke_release(&request.exception_id),
    )
    .map_err(into_command_error)
}

// ---------------------------------------------------------------------------
// Fiscal-year TVA policy (WILAYA admin)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn set_fiscal_tax_policy(
    state: State<AppState>,
    request: SetTaxPolicyRequest,
) -> Result<FiscalYearTaxPolicy, String> {
    let (session, _) =
        authorize_command(&state, Action::ManageTaxPolicy, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::SetTaxPolicy, &user_ctx, |tx| {
        FiscalTaxPolicyService::new(tx.executor).set_policy(&request, &session.user_id)
    })
    .map_err(into_command_error)
}

// ---------------------------------------------------------------------------
// Read projections (WILAYA admin owner)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_supplier(
    state: State<AppState>,
    supplier_id: String,
) -> Result<Option<Supplier>, String> {
    let _ = authorize_command(&state, Action::ReadContractProjection, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    SupplierService::new(db.executor())
        .get_supplier(&supplier_id)
        .map_err(into_command_error)
}

#[tauri::command]
pub fn list_suppliers(state: State<AppState>) -> Result<Vec<Supplier>, String> {
    let _ = authorize_command(&state, Action::ReadContractProjection, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    SupplierService::new(db.executor())
        .list_suppliers()
        .map_err(into_command_error)
}

#[tauri::command]
pub fn list_unit_suppliers(
    state: State<AppState>,
    unit_id: String,
) -> Result<Vec<Supplier>, String> {
    let _ = authorize_command(&state, Action::ReadContractProjection, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    SupplierService::new(db.executor())
        .list_unit_suppliers(&unit_id)
        .map_err(into_command_error)
}

#[tauri::command]
pub fn get_contract(
    state: State<AppState>,
    contract_id: String,
) -> Result<Option<Contract>, String> {
    let _ = authorize_command(&state, Action::ReadContractProjection, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    ContractService::new(db.executor())
        .get_contract(&contract_id)
        .map_err(into_command_error)
}

#[tauri::command]
pub fn list_contracts(
    state: State<AppState>,
    unit_id: Option<String>,
    supplier_id: Option<String>,
    fiscal_year: Option<i32>,
) -> Result<Vec<Contract>, String> {
    let _ = authorize_command(&state, Action::ReadContractProjection, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    ContractService::new(db.executor())
        .list_contracts(unit_id.as_deref(), supplier_id.as_deref(), fiscal_year)
        .map_err(into_command_error)
}

#[tauri::command]
pub fn get_contract_products(
    state: State<AppState>,
    contract_id: String,
) -> Result<Vec<ContractProduct>, String> {
    let _ = authorize_command(&state, Action::ReadContractProjection, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    ContractService::new(db.executor())
        .get_contract_products(&contract_id)
        .map_err(into_command_error)
}

#[tauri::command]
pub fn list_contract_allocations(
    state: State<AppState>,
    contract_id: String,
) -> Result<Vec<ContractAllocation>, String> {
    let _ = authorize_command(&state, Action::ReadContractProjection, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    ContractService::new(db.executor())
        .list_allocations_for_contract(&contract_id)
        .map_err(into_command_error)
}

#[tauri::command]
pub fn list_allocation_exceptions(
    state: State<AppState>,
    allocation_id: String,
) -> Result<Vec<ContractAllocationException>, String> {
    let _ = authorize_command(&state, Action::ReadContractProjection, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    ContractService::new(db.executor())
        .list_exceptions_for_allocation(&allocation_id)
        .map_err(into_command_error)
}

#[tauri::command]
pub fn get_fiscal_tax_policy(
    state: State<AppState>,
    fiscal_year: i32,
) -> Result<Option<FiscalYearTaxPolicy>, String> {
    let _ = authorize_command(&state, Action::ReadContractProjection, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    FiscalTaxPolicyService::new(db.executor())
        .get_policy(fiscal_year)
        .map_err(into_command_error)
}

#[tauri::command]
pub fn list_fiscal_tax_policies(
    state: State<AppState>,
) -> Result<Vec<FiscalYearTaxPolicy>, String> {
    let _ = authorize_command(&state, Action::ReadContractProjection, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    FiscalTaxPolicyService::new(db.executor())
        .list_policies()
        .map_err(into_command_error)
}
