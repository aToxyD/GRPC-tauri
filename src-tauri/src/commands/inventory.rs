//! Inventory and Stock Commands
//!
//! Stock movement ledger and inventory snapshots
//! Strictly follows Clean Architecture: Commands -> Services -> Repositories -> DB

use crate::application::authz::Action;
use crate::application::services::{
    AuditTxService, InventorySnapshotService, StockLevelService, StockMovementService,
};
use crate::commands::common::{
    db_mut_or_app_error, db_mut_or_command_error, db_ref_or_command_error, user_ctx_from_session,
};
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::domain::audit::AuditAction;
use crate::domain::validation;
use crate::errors::into_command_error;
use crate::models::{
    ComputeSnapshotResult, FifoStockLayer, InventoryLayerConsumption, InventoryStockPageView,
    StockMovementFilters, StockMovementResponse, StockSummary, UnitInventoryView, XlsxExportResult,
};

use tauri::State;

/// Get stock movements with filtering
#[tauri::command]
pub fn get_stock_movements(
    state: State<AppState>,
    filters: StockMovementFilters,
    page: usize,
    page_size: usize,
) -> Result<StockMovementResponse, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadInventory, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    StockMovementService::new(db.executor())
        .get_stock_movements(&filters, page, page_size)
        .map_err(into_command_error)
}

/// Get stock summary with automatic fiscal-year scoping for movement statistics.
///
/// For Unit nodes, the open fiscal year is resolved automatically via
/// `StockLevelService::get_stock_summary_scoped()`. Movement-based statistics
/// (total_in, total_out, movement_count) are scoped to that year.
/// `current_quantity` always reflects the real inventory stock.
/// Wilaya nodes see the unfiltered summary (all years).
#[tauri::command]
pub fn get_stock_summary(state: State<AppState>) -> Result<Vec<StockSummary>, String> {
    let (_session, settings) =
        authorize_command(&state, Action::ReadInventory, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    let unit_node = matches!(settings.node_type, crate::models::NodeType::Unit);
    StockLevelService::new(db.executor())
        .get_stock_summary_scoped(unit_node)
        .map_err(into_command_error)
}

/// Export stock movements to Excel
#[tauri::command]
pub fn export_stock_movements_excel(
    state: State<AppState>,
    product_id: Option<String>,
    file_path: String,
) -> Result<XlsxExportResult, String> {
    let (_session, _settings) = authorize_command(&state, Action::ExportStockMovements, None)
        .map_err(into_command_error)?;
    validation::validate_file_path(&file_path, &["xlsx"]).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    let filters = StockMovementFilters {
        product_id,
        ..Default::default()
    };

    let result = StockMovementService::new(db.executor())
        .get_stock_movements(&filters, 0, 10000)
        .map_err(into_command_error)?;

    use crate::domain::ports::export::ExcelPort;
    use crate::infrastructure::export::XlsxAdapter;
    use std::fs;

    let adapter = XlsxAdapter::new();
    let buffer = adapter
        .export_stock_movements(&result.movements)
        .map_err(|e| e.to_string())?;
    fs::write(&file_path, buffer).map_err(|e| e.to_string())?;

    let count = result.movements.len();
    Ok(XlsxExportResult::success(file_path, count))
}

/// Compute unit inventory snapshot
#[tauri::command]
pub fn compute_unit_inventory_snapshot(
    state: State<AppState>,
    unit_id: String,
    year: i32,
    month: u32,
    force_recompute: bool,
) -> Result<ComputeSnapshotResult, String> {
    let (session, _settings) = authorize_command(&state, Action::ManageInventory, Some(&unit_id))
        .map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let user_ctx = user_ctx_from_session(&session);

    let result =
        AuditTxService::execute_with_audit(db, AuditAction::CreateSnapshot, &user_ctx, |tx| {
            InventorySnapshotService::new(tx.executor).compute_and_store_unit_snapshot(
                &unit_id,
                year,
                month,
                force_recompute,
            )
        })
        .map_err(into_command_error)?;

    Ok(result)
}

/// Get unit inventory view
#[tauri::command]
pub fn get_unit_inventory_view(
    state: State<AppState>,
    unit_id: String,
    year: i32,
    month: u32,
) -> Result<Option<UnitInventoryView>, String> {
    let (_session, _settings) = authorize_command(&state, Action::ReadInventory, Some(&unit_id))
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_id, year, month)
        .map_err(into_command_error)
}

/// Get available report months for unit
#[tauri::command]
pub fn get_available_report_months(
    state: State<AppState>,
    unit_id: String,
) -> Result<Vec<(i32, u32)>, String> {
    let (_session, _settings) = authorize_command(&state, Action::ReadInventory, Some(&unit_id))
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    InventorySnapshotService::new(db.executor())
        .get_available_report_months_for_unit(&unit_id)
        .map_err(into_command_error)
}

/// Export unit inventory to Excel
#[tauri::command]
pub fn export_unit_inventory_excel(
    state: State<AppState>,
    unit_id: String,
    year: i32,
    month: u32,
    file_path: String,
) -> Result<XlsxExportResult, String> {
    let (_session, _settings) = authorize_command(&state, Action::ReadInventory, Some(&unit_id))
        .map_err(into_command_error)?;
    validation::validate_file_path(&file_path, &["xlsx"]).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    let view = InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_id, year, month)
        .map_err(into_command_error)?
        .ok_or_else(|| "لا توجد بيانات لهذه الوحدة في هذا الشهر".to_string())?;

    use crate::domain::ports::export::ExcelPort;
    use crate::infrastructure::export::XlsxAdapter;
    use std::fs;

    let adapter = XlsxAdapter::new();
    let buffer = adapter
        .export_unit_inventory(&view)
        .map_err(|e| e.to_string())?;
    fs::write(&file_path, buffer).map_err(|e| e.to_string())?;

    let count = view.items.len();
    Ok(XlsxExportResult::success(file_path, count))
}

/// Check stock availability
#[tauri::command]
pub fn check_stock_availability(
    state: State<AppState>,
    items: Vec<crate::models::ConsumptionItemInput>,
) -> Result<Vec<crate::models::StockCheckResult>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadInventory, None).map_err(into_command_error)?;

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    StockLevelService::new(db.executor())
        .check_stock_availability(items)
        .map_err(into_command_error)
}

/// Get stock for a product
#[tauri::command]
pub fn get_stock(
    state: State<AppState>,
    product_id: String,
) -> Result<Option<crate::models::InventoryStock>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadInventory, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    StockLevelService::new(db.executor())
        .get_stock(&product_id)
        .map_err(into_command_error)
}

/// Get current stock (alias for get_all_stocks for backward compatibility)
#[tauri::command]
pub fn get_current_stock(
    state: State<AppState>,
) -> Result<Vec<crate::models::InventoryStock>, String> {
    get_all_stocks(state)
}

/// Get all stocks
#[tauri::command]
pub fn get_all_stocks(
    state: State<AppState>,
) -> Result<Vec<crate::models::InventoryStock>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadInventory, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    StockLevelService::new(db.executor())
        .get_all_stocks()
        .map_err(into_command_error)
}

#[tauri::command]
pub fn verify_inventory_integrity(
    state: tauri::State<crate::commands::types::AppState>,
    year: i32,
) -> Result<
    crate::application::services::inventory_integrity_service::InventoryIntegrityReport,
    String,
> {
    verify_inventory_integrity_impl(&state, year).map_err(crate::errors::into_command_error)
}

/// Command-boundary implementation of `verify_inventory_integrity`.
///
/// SEC-005-A: the integrity-scan command performs a DB write
/// (`integrity_attempts`) and MUST be gated at the command boundary —
/// `ViewSystemHealth` (the same authorization used by `verify_integrity` and
/// `get_advanced_diagnostics_bundle`). Extracted for command-boundary testing.
pub fn verify_inventory_integrity_impl(
    state: &crate::commands::types::AppState,
    year: i32,
) -> Result<
    crate::application::services::inventory_integrity_service::InventoryIntegrityReport,
    crate::errors::AppError,
> {
    let (_session, _settings) = authorize_command(state, Action::ViewSystemHealth, None)?;
    state.touch_session();

    let mut guard = state.get_db()?;
    let db = db_mut_or_app_error(guard.as_mut())?;

    // Wrap in a transaction so we can append an integrity attempt row atomically.
    db.with_transaction(|tx| {
        let report = crate::application::services::inventory_integrity_service::
            InventoryIntegrityService::new(tx)
            .verify_inventory_consistency(year)?;

        // Append-only attempt log (Anomaly E source).
        let recorder = crate::application::services::IntegrityAttemptRecorder::new(tx);
        if report.mismatch_count == 0 {
            recorder.record(
                crate::application::services::VerificationType::Inventory,
                crate::application::services::VerificationOutcome::Pass,
                Some(&format!(
                    "fiscal_year={} checked={}",
                    year, report.checked_products
                )),
            );
        } else {
            recorder.record(
                crate::application::services::VerificationType::Inventory,
                crate::application::services::VerificationOutcome::Fail,
                Some(&format!(
                    "fiscal_year={} mismatches={}",
                    year, report.mismatch_count
                )),
            );
        }
        Ok(report)
    })
}

#[tauri::command]
pub fn get_fifo_layers(
    state: State<AppState>,
    unit_id: String,
    product_id: String,
) -> Result<Vec<FifoStockLayer>, String> {
    let (_session, _settings) = authorize_command(&state, Action::ReadInventory, Some(&unit_id))
        .map_err(into_command_error)?;

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    StockLevelService::new(db.executor())
        .get_remaining_layers(&unit_id, &product_id)
        .map_err(|e: crate::errors::AppError| e.to_string())
}

#[tauri::command]
pub fn get_fifo_consumption_history(
    state: State<AppState>,
    movement_id: String,
) -> Result<Vec<InventoryLayerConsumption>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadInventory, None).map_err(into_command_error)?;

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    StockLevelService::new(db.executor())
        .get_consumption_history(&movement_id)
        .map_err(|e: crate::errors::AppError| e.to_string())
}

/// Full FIFO inventory view for the UNIT StockPage.
/// Auto-resolves the unit_id from settings (no client-supplied unit_id needed).
#[tauri::command]
pub fn get_inventory_fifo_view(state: State<AppState>) -> Result<InventoryStockPageView, String> {
    let (_session, settings) =
        authorize_command(&state, Action::ReadInventory, None).map_err(into_command_error)?;
    state.touch_session();

    if !settings.is_unit() {
        return Err("هذه الميزة متاحة فقط للعقد UNIT".to_string());
    }

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    let unit_id = crate::application::services::SettingsService::new(db.executor())
        .get_current_unit_id()
        .map_err(into_command_error)?
        .ok_or_else(|| "لا يوجد معرف وحدة للمستخدم الحالي".to_string())?;

    StockLevelService::new(db.executor())
        .get_inventory_fifo_view(&unit_id)
        .map_err(into_command_error)
}

#[tauri::command]
pub fn get_total_inventory_value(state: State<AppState>) -> Result<f64, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadInventory, None).map_err(into_command_error)?;

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    StockLevelService::new(db.executor())
        .get_total_inventory_value()
        .map_err(|e: crate::errors::AppError| e.to_string())
}
