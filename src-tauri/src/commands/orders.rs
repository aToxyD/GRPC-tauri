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

/// Create supplier order
#[tauri::command]
pub fn create_supplier_order(
    state: State<AppState>,
    request: CreateOrderRequest,
) -> Result<(String, f64), String> {
    let (session, _settings) =
        authorize_command(&state, Action::ManageOrders, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let user_ctx = user_ctx_from_session(&session);

    // Create the order - returns (order_id, total_amount)
    let (order_id, total) =
        AuditTxService::execute_with_audit(db, AuditAction::CreateOrder, &user_ctx, |tx| {
            OrderService::new(tx.executor).create_supplier_order(&request)
        })
        .map_err(into_command_error)?;

    Ok((order_id, total))
}

/// Confirm supplier order with atomic audit
#[tauri::command]
pub fn confirm_order(state: State<AppState>, order_id: String) -> Result<(), String> {
    let (session, settings) =
        authorize_command(&state, Action::ManageOrders, None).map_err(into_command_error)?;
    require_maintenance_allows(&state, MaintenanceBlockedOperation::StockWrite)
        .map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    // Get unit_id for UNIT nodes
    let unit_id = if matches!(settings.node_type, crate::models::NodeType::Unit) {
        settings.unit_name.clone()
    } else {
        None
    };

    let user_ctx = user_ctx_from_session(&session);

    // Confirm the order - pass unit_id for IN movements
    AuditTxService::execute_with_audit(db, AuditAction::ConfirmOrder, &user_ctx, |tx| {
        OrderService::new(tx.executor).confirm_order_atomic(
            &order_id,
            &session.user_id,
            &session.username,
            unit_id.as_deref(),
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
    let (session, _) =
        authorize_command(&state, Action::ManageOrders, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::UpdateOrder, &user_ctx, |tx| {
        OrderService::new(tx.executor).update_supplier_order(&request)
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
) -> Result<(String, f64), String> {
    create_supplier_order(state, request)
}
