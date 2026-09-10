//! Order Management Commands
//!
//! Supplier order creation and confirmation
//! Strictly follows Clean Architecture: Commands -> Services -> Repositories -> DB

use crate::application::authz::Action;
use crate::application::services::MaintenanceBlockedOperation;
use crate::application::services::{AuditTxService, OrderService};
use crate::commands::common::{
    db_mut_or_command_error, db_ref_or_command_error, user_ctx_from_session,
};
use crate::commands::guards::{authorize_command, require_maintenance_allows};
use crate::commands::types::AppState;
use crate::domain::audit::AuditAction;
use crate::errors::into_command_error;
use crate::models::{CreateOrderRequest, SupplierOrder, SupplierOrderItem, UpdateOrderRequest};
use tauri::State;

/// Create supplier order(s) for a request.
///
/// ADR-0056 (Phase 4B): a request may split into MULTIPLE supplier orders —
/// one per supplier, all planned and materialized atomically inside a single
/// transaction. Supplier, allocation and price are backend-resolved from
/// contract entitlement (I9); the caller supplies products + quantities only.
/// Unit context is resolved from node settings.
#[tauri::command]
pub fn create_supplier_order(
    state: State<AppState>,
    request: CreateOrderRequest,
) -> Result<Vec<(String, f64)>, String> {
    let (session, settings) =
        authorize_command(&state, Action::ManageOrders, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    // Orders are unit-scoped: resolve the ordering unit from node settings.
    let unit_id = resolve_order_unit(&settings, db).map_err(into_command_error)?;
    let fiscal_year = current_fiscal_year(db, &settings).map_err(into_command_error)?;

    let user_ctx = user_ctx_from_session(&session);

    // Create every resulting (order_id, total_amount) atomically in one
    // transaction — no partial orders (ADR-0056 I8).
    let created =
        AuditTxService::execute_with_audit(db, AuditAction::CreateOrder, &user_ctx, |tx| {
            OrderService::new(tx.executor).create_supplier_orders(&request, &unit_id, fiscal_year)
        })
        .map_err(into_command_error)?;

    Ok(created)
}

/// Confirm supplier order with atomic audit.
///
/// Confirmation re-resolves entitlement authoritatively; the unit context is
/// taken from the order itself (stored at creation).
#[tauri::command]
pub fn confirm_order(state: State<AppState>, order_id: String) -> Result<(), String> {
    let (session, _settings) =
        authorize_command(&state, Action::ManageOrders, None).map_err(into_command_error)?;
    require_maintenance_allows(&state, MaintenanceBlockedOperation::StockWrite)
        .map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let user_ctx = user_ctx_from_session(&session);

    // Confirm the order - unit derives from the order itself.
    AuditTxService::execute_with_audit(db, AuditAction::ConfirmOrder, &user_ctx, |tx| {
        OrderService::new(tx.executor).confirm_order_atomic(
            &order_id,
            &session.user_id,
            &session.username,
        )
    })
    .map_err(into_command_error)?;

    Ok(())
}

/// Update a draft supplier order
#[tauri::command]
pub fn update_supplier_order(
    state: State<AppState>,
    request: UpdateOrderRequest,
) -> Result<f64, String> {
    let (session, settings) =
        authorize_command(&state, Action::ManageOrders, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    let unit_id = resolve_order_unit(&settings, db).map_err(into_command_error)?;
    let fiscal_year = current_fiscal_year(db, &settings).map_err(into_command_error)?;

    AuditTxService::execute_with_audit(db, AuditAction::UpdateOrder, &user_ctx, |tx| {
        OrderService::new(tx.executor).update_supplier_order(&request, &unit_id, fiscal_year)
    })
    .map_err(into_command_error)
}

/// Delete a draft supplier order
#[tauri::command]
pub fn delete_supplier_order(state: State<AppState>, order_id: String) -> Result<(), String> {
    let (session, _) =
        authorize_command(&state, Action::ManageOrders, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::DeleteOrder, &user_ctx, |tx| {
        OrderService::new(tx.executor).delete_supplier_order(&order_id)
    })
    .map_err(into_command_error)?;

    Ok(())
}

/// Get supplier order by ID
#[tauri::command]
pub fn get_supplier_order(
    state: State<AppState>,
    order_id: String,
) -> Result<Option<SupplierOrder>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadOrders, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    OrderService::new(db.executor())
        .get_supplier_order(&order_id)
        .map_err(into_command_error)
}

/// Get order items
#[tauri::command]
pub fn get_supplier_order_items(
    state: State<AppState>,
    order_id: String,
) -> Result<Vec<SupplierOrderItem>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadOrders, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    OrderService::new(db.executor())
        .get_supplier_order_items(&order_id)
        .map_err(into_command_error)
}

/// List all supplier orders (optional `fiscal_year` filter; default unchanged when omitted).
#[tauri::command]
pub fn list_supplier_orders(
    state: State<AppState>,
    fiscal_year: Option<i32>,
) -> Result<Vec<SupplierOrder>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadOrders, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    OrderService::new(db.executor())
        .list_supplier_orders(fiscal_year)
        .map_err(into_command_error)
}

/// Alias for list_supplier_orders
#[tauri::command]
pub fn get_orders(
    state: State<AppState>,
    fiscal_year: Option<i32>,
) -> Result<Vec<SupplierOrder>, String> {
    list_supplier_orders(state, fiscal_year)
}

/// Alias for create_supplier_order
#[tauri::command]
pub fn create_order(
    state: State<AppState>,
    request: CreateOrderRequest,
) -> Result<Vec<(String, f64)>, String> {
    create_supplier_order(state, request)
}

/// Resolve the ordering unit for a UNIT node. Orders are unit-scoped; a
/// WILAYA node has no unit context and cannot create unit orders directly.
fn resolve_order_unit(
    settings: &crate::models::Settings,
    db: &mut crate::db::Database,
) -> Result<String, crate::errors::AppError> {
    if matches!(settings.node_type, crate::models::NodeType::Unit) {
        let settings_svc = crate::application::services::SettingsService::new(db.executor());
        settings_svc.get_current_unit_id()?.ok_or_else(|| {
            crate::errors::AppError::BusinessLogic(
                crate::errors::BusinessLogicError::OperationNotPermitted {
                    message: "الوحدة الحالية غير محددة — لا يمكن إنشاء طلبية".to_string(),
                },
            )
        })
    } else {
        Err(crate::errors::AppError::BusinessLogic(
            crate::errors::BusinessLogicError::OperationNotPermitted {
                message: "الطلبيات تُنشأ من عقدة الوحدة فقط".to_string(),
            },
        ))
    }
}

/// Resolve the authoritative active fiscal year (persisted settings, not wall
/// clock) used to anchor entitlement resolution.
fn current_fiscal_year(
    db: &mut crate::db::Database,
    _settings: &crate::models::Settings,
) -> Result<i32, crate::errors::AppError> {
    crate::application::services::fiscal_scope::resolve_active_fiscal_year(db.executor())
}
