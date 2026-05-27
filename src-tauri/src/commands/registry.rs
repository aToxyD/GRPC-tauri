use crate::commands;
use tauri::ipc::Invoke;

pub fn get_invoke_handler() -> impl Fn(Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        // Authentication
        commands::login,
        commands::change_password,
        commands::get_current_user,
        // Configuration
        commands::get_settings,
        commands::configure_as_wilaya,
        commands::is_configured,
        // Products
        commands::create_product,
        commands::update_product,
        commands::delete_product,
        commands::get_product,
        commands::list_products,
        // Units
        commands::create_unit,
        commands::update_unit,
        commands::delete_unit,
        commands::get_unit,
        commands::list_units,
        // Stock
        commands::get_stock,
        commands::get_all_stocks,
        commands::check_stock_availability,
        commands::get_fifo_layers,
        commands::get_fifo_consumption_history,
        commands::get_total_inventory_value,
        commands::get_inventory_fifo_view,
        // Orders
        commands::create_supplier_order,
        commands::update_supplier_order,
        commands::delete_supplier_order,
        commands::confirm_order,
        commands::get_supplier_order,
        commands::get_supplier_order_items,
        commands::list_supplier_orders,
        // Daily Reports
        commands::preview_daily_consumption_fifo,
        commands::create_daily_report,
        commands::get_daily_report,
        commands::list_daily_reports,
        commands::list_wilaya_reports,
        // Calculations
        commands::calculate_meal_cost,
        commands::calculate_meal_rate,
        commands::calculate_product_price_with_tva,
        commands::get_monthly_summary,
        // Dashboard helpers
        commands::get_current_stock,
        commands::get_daily_consumption,
        // Reports
        commands::get_report_data,
        commands::generate_reports,
        // Orders aliases
        commands::create_order,
        commands::get_orders,
        // Sync Package Export
        commands::export_products_package,
        commands::export_daily_report_package,
        commands::export_monthly_summary_package,
        commands::export_unit_node_package,
        // Excel Export
        commands::export_products_excel,
        commands::export_daily_report_excel,
        commands::export_monthly_summary_excel,
        commands::export_all_units_monthly_status_excel,
        // Sync Package Import (SECURE - with file hash verification)
        commands::import_products_package,
        commands::import_daily_report_package,
        commands::import_unit_node_package,
        commands::import_monthly_summary_package,
        commands::import_stock_movements_package,
        commands::export_stock_movements_package,
        commands::get_import_audit_events,
        commands::record_consumption,
        commands::verify_inventory_integrity,
        commands::run_fiscal_integrity_scan,
        // Backup Management
        commands::create_backup,
        commands::list_backups,
        commands::restore_backup,
        // System Metrics
        commands::get_system_metrics,
        commands::get_login_metrics,
        commands::get_sync_security_diagnostics,
        commands::sync_preflight_check,
        // Audit Trail
        commands::get_audit_log,
        commands::get_audit_stats,
        commands::verify_audit_chain,
        commands::get_user_activity,
        commands::export_audit_log_excel,
        commands::cleanup_audit_logs,
        // Session Management
        commands::logout,
        commands::check_session,
        commands::touch_session,
        // Stock Movement Ledger
        commands::get_stock_movements,
        commands::get_stock_summary,
        commands::export_stock_movements_excel,
        // Unit Monthly Inventory Snapshots
        commands::compute_unit_inventory_snapshot,
        commands::get_unit_inventory_view,
        commands::get_available_report_months,
        commands::export_unit_inventory_excel,
        // Observability
        commands::get_audit_chain_status,
        commands::get_audit_health,
        commands::get_system_health,
        commands::get_sync_health,
        commands::get_build_info,
        commands::get_recent_telemetry,
        commands::get_conflict_summary,
        commands::list_sync_conflicts,
        commands::resolve_sync_conflict,
        // Fiscal lifecycle
        commands::close_fiscal_year,
        commands::get_fiscal_year_status,
        commands::export_fiscal_closure_package,
        commands::preview_fiscal_closure_package,
        commands::apply_fiscal_closure_package,
        commands::get_fiscal_transition_history,
        commands::list_fiscal_package_registry,
        commands::update_fiscal_package_retention_status,
        // Controlled Operational Intelligence Phase
        commands::run_operational_analysis,
        commands::create_fiscal_operational_snapshot,
        commands::list_fiscal_operational_snapshots,
        commands::get_operational_recommendations,
        commands::build_fiscal_timeline,
        commands::close_fiscal_year_confirmed,
        commands::issue_operation_execution_token,
        commands::archive_fiscal_year_confirmed,
        commands::validate_archive_confirmation,
        commands::validate_restore_confirmation,
        commands::validate_historical_import_confirmation,
        commands::get_advanced_diagnostics_bundle,
        commands::verify_deployment_readiness,
        commands::verify_operational_consistency,
        commands::verify_integrity,
        commands::get_system_maintenance_state,
        commands::list_operational_sessions,
    ]
}
