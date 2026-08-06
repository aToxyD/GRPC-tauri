//! Import/Export Commands
//!
//! Handles file imports (packages) and exports
//! Strictly follows Clean Architecture: Commands -> Services -> Repositories -> DB

use crate::application::authz::Action;
use crate::application::services::{
    record_export_with_reproducibility, AuditService, AuditTxService, DailyReportService,
    ExportReproducibilityContext, IdentitySignedExportService, NodePackageService, ProductService,
    SettingsService, StockMovementService, SyncPackageIdentityVerificationService, UnitService,
    UserService,
};
use crate::application::services::{
    ImportReproducibilityRecord, ImportReproducibilityService, MaintenanceBlockedOperation,
    SystemIntegrityState,
};
use crate::application::sync::import::{
    ImportAuditEvent, ImportAuditEventType, ImportAuditLogger, ImportFailureReason,
};
use crate::application::sync::{
    PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use crate::application::sync_integrity::transport_guard::{TransportGuard, TransportVerdict};
use crate::application::usecases::exports::types::{
    DailyReportExportInput, ExportProductsInput, MonthlySummaryExportInput,
    StockMovementsExportDataset,
};
use crate::application::usecases::sync::import_daily_report_package::DAILY_REPORT_PACKAGE_KIND;
use crate::application::usecases::sync::import_daily_report_package::{
    execute as apply_daily_report_package, ImportDailyReportPackageInput,
};
use crate::application::usecases::sync::import_monthly_summary_package::MONTHLY_SUMMARY_PACKAGE_KIND;
use crate::application::usecases::sync::import_monthly_summary_package::{
    execute as apply_monthly_summary_package, ImportMonthlySummaryPackageInput,
};
use crate::application::usecases::sync::import_products_package::PRODUCTS_PACKAGE_KIND;
use crate::application::usecases::sync::import_products_package::{
    execute as apply_products_package, ImportProductsPackageInput,
};
use crate::application::usecases::sync::import_stock_movements_package::STOCK_MOVEMENTS_PACKAGE_KIND;
use crate::application::usecases::sync::import_stock_movements_package::{
    execute as apply_stock_movements_package, ImportStockMovementsPackageInput,
};
use crate::application::usecases::sync::import_registry_package::REGISTRY_PACKAGE_KIND;
use crate::application::usecases::sync::import_registry_package::{
    execute as apply_registry_package, ImportRegistryPackageInput,
};
use crate::application::usecases::sync::import_trust_package::TRUST_PACKAGE_KIND;
use crate::application::usecases::sync::import_trust_package::{
    execute as apply_trust_package, ImportTrustPackageInput,
};
use crate::commands::common::{
    db_mut_or_command_error, db_ref_or_command_error, node_key_store, user_ctx_from_session,
};
use crate::commands::guards::{authorize_command, require_maintenance_allows};
use crate::commands::reports::build_report_scope;
use crate::commands::types::AppState;
use crate::domain::audit::AuditAction;
use crate::domain::events::DomainEvent;
use crate::domain::identity::SubjectType;
use crate::domain::session::CurrentSession;
use crate::domain::validation;
use crate::errors::{into_command_error, AppError, BusinessLogicError, ValidationError};
use crate::infrastructure::db::read::import_audit::{
    list_import_audit_events, ImportAuditEventProjection, ImportAuditQuery,
};
use crate::infrastructure::db::sync_import::{
    SqliteImportAuditLogger, SqliteImportedPackageRegistry,
};
use crate::infrastructure::security::resolve_active_signing_key_id;
use crate::infrastructure::sync::{
    read_daily_report_package_from_file, read_monthly_summary_package_from_file,
    read_products_package_from_file, read_registry_package_from_file,
    read_stock_movements_package_from_file, read_trust_package_from_file,
    read_unit_node_package_from_file, resolve_export_source_node_id, HmacPackageSigner,
    PackageBuilder, SerdeJsonSyncPackageSerializer,
};

use crate::models::{
    DailyReportImportResult, PackageExportResult, RegistryPackageImportResult,
    TrustPackageImportResult, XlsxExportResult,
};
use chrono::{Datelike, Utc};
use tauri::State;
use uuid::Uuid;

/// Map the node's operational type to the identity subject used by the local
/// signer resolver. `NodeType` has exactly `Unit`/`Wilaya`, so the mapping is
/// total for configured nodes.
fn export_subject_type(node_type: crate::models::NodeType) -> SubjectType {
    match node_type {
        crate::models::NodeType::Unit => SubjectType::Unit,
        crate::models::NodeType::Wilaya => SubjectType::Wilaya,
    }
}

/// Export products catalog as an encrypted **sync package** (`.sync`) — intended for Wilaya → Units distribution.
#[tauri::command]
pub fn export_products_package(
    state: State<AppState>,
    file_path: String,
) -> Result<PackageExportResult, String> {
    let (session, settings) =
        authorize_command(&state, Action::ExportProducts, None).map_err(into_command_error)?;
    validation::validate_file_path(&file_path, &["sync"]).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let start_time = std::time::Instant::now();

    let dataset = crate::application::usecases::exports::export_products_dataset::execute(
        db.executor(),
        ExportProductsInput,
    )
    .map_err(into_command_error)?;

    let source_node_id =
        resolve_export_source_node_id(db.executor(), &settings).map_err(into_command_error)?;

    let _sequence = IdentitySignedExportService::new(db, &node_key_store())
        .export_v2_package(
            dataset.clone(),
            &source_node_id,
            "products",
            std::path::Path::new(&file_path),
            export_subject_type(settings.node_type),
            &state.crypto_port,
        )
        .map_err(into_command_error)?;

    log::info!(
        target: "grpc::import_export",
        "export_products_package: success path={}",
        file_path
    );

    // B6-B: export_hash is a UNIQUE fiscal-tracking identifier, independent of
    // the transport `package_id` — no code path joins the two (RFC §5 ④b).
    let export_hash = Uuid::new_v4().to_string();
    let result = PackageExportResult::success(
        file_path.clone(),
        dataset.product_rows.len(),
        "encrypted".to_string(),
    );

    let _ = db.with_transaction(|tx| {
        record_export_with_reproducibility(
            tx,
            ExportReproducibilityContext {
                export_hash,
                fiscal_year: settings.current_year,
                generated_by: session.username.clone(),
                movement_count: 0,
                report_count: 0,
                inventory_total_value: 0.0,
                export_reason: "products_sync_package".to_string(),
            },
        )
    });

    let duration = start_time.elapsed().as_millis() as i64;
    let _ = crate::application::services::TelemetryService::new(db.executor()).record_event(
        crate::application::services::TelemetryEventType::SyncExport,
        crate::application::services::TelemetryOutcome::Success,
        Some(duration),
        Some(serde_json::json!({ "path": file_path, "kind": "products" })),
        Some(&session.user_id),
    );

    Ok(result)
}

/// Import products catalog sync package (`.sync`) on UNIT nodes.
#[tauri::command]
pub fn import_products_package(
    state: State<AppState>,
    file_path: String,
) -> Result<crate::models::PackageImportResult, String> {
    run_import_pipeline(
        &state,
        Action::ImportProductsPackage,
        file_path,
        PRODUCTS_PACKAGE_KIND,
        AuditAction::ImportNodePackage,
        read_products_package_from_file,
        |executor, registry, package, session, importer_wilaya: &str| {
            let input = ImportProductsPackageInput {
                package,
                importer_wilaya_code: importer_wilaya.to_string(),
                imported_by: session.username.clone(),
            };
            let outcome = apply_products_package(executor, registry, input)?;
            Ok(crate::models::PackageImportResult {
                added: outcome.imported,
                updated: outcome.updated,
                deleted: outcome.skipped,
            })
        },
    )
}

/// Import single daily report sync package (`.sync`) on Wilaya for a selected unit.
#[tauri::command]
pub fn import_daily_report_package(
    state: State<AppState>,
    file_path: String,
    unit_id: String,
) -> Result<crate::models::DailyReportImportResult, String> {
    run_import_pipeline(
        &state,
        Action::ImportDailyReportPackage,
        file_path,
        DAILY_REPORT_PACKAGE_KIND,
        AuditAction::ImportNodePackage,
        read_daily_report_package_from_file,
        |executor, registry, package, session, importer_wilaya: &str| {
            let input = ImportDailyReportPackageInput {
                package,
                unit_id: unit_id.clone(),
                importer_wilaya_code: importer_wilaya.to_string(),
                imported_by: session.username.clone(),
            };
            let outcome = apply_daily_report_package(executor, registry, input)?;
            Ok(crate::models::DailyReportImportResult {
                report_count: outcome.report_count as i32,
                item_count: outcome.item_count as i32,
                unit_id: Some(outcome.unit_id),
                file_hash: outcome.package_id,
                imported_by: session.username.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
        },
    )
}

/// Export a single daily report as encrypted sync package (`.sync`) from UNIT/WILAYA scope.
#[tauri::command]
pub fn export_daily_report_package(
    state: State<AppState>,
    report_id: String,
    file_path: String,
) -> Result<PackageExportResult, String> {
    let (session, settings) =
        authorize_command(&state, Action::ExportDailyReport, None).map_err(into_command_error)?;
    validation::validate_file_path(&file_path, &["sync"]).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    let executor = db.executor();

    let settings_svc = SettingsService::new(executor);
    let start_time = std::time::Instant::now();
    let (scope, _node_ctx) = build_report_scope(&settings_svc).map_err(into_command_error)?;

    let dataset = crate::application::usecases::exports::export_daily_report_dataset::execute(
        executor,
        scope,
        DailyReportExportInput {
            report_id: report_id.trim().to_string(),
        },
    )
    .map_err(into_command_error)?;

    let settings_row = settings_svc.get_settings().map_err(into_command_error)?;
    let source_node_id =
        resolve_export_source_node_id(executor, &settings_row).map_err(into_command_error)?;

    let _sequence = IdentitySignedExportService::new(db, &node_key_store())
        .export_v2_package(
            dataset.clone(),
            &source_node_id,
            "daily_report",
            std::path::Path::new(&file_path),
            export_subject_type(settings.node_type),
            &state.crypto_port,
        )
        .map_err(into_command_error)?;

    log::info!(
        target: "grpc::import_export",
        "export_daily_report_package: success path={} report_id={}",
        file_path,
        report_id
    );

    let duration = start_time.elapsed().as_millis() as i64;
    let _ = crate::application::services::TelemetryService::new(executor).record_event(
        crate::application::services::TelemetryEventType::SyncExport,
        crate::application::services::TelemetryOutcome::Success,
        Some(duration),
        Some(serde_json::json!({ "path": file_path, "kind": "daily_report", "report_id": report_id })),
        Some(&session.user_id),
    );

    Ok(PackageExportResult::success(
        file_path,
        1 + dataset
            .snapshot
            .meals
            .iter()
            .map(|m| m.items.len())
            .sum::<usize>(),
        "encrypted".to_string(),
    ))
}

/// Export all units' monthly status to Excel (for WILAYA)
#[tauri::command]
pub fn export_all_units_monthly_status_excel(
    state: State<AppState>,
    year: i32,
    month: u32,
    file_path: String,
) -> Result<XlsxExportResult, String> {
    let (_session, _settings) = authorize_command(&state, Action::ExportMonthlySummary, None)
        .map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    // This operation uses DailyReportService to delegate repository access
    let list = DailyReportService::new(db.executor())
        .get_wilaya_monthly_report(year, month as i32)
        .map_err(into_command_error)?;

    use crate::domain::ports::export::ExcelPort;
    use crate::infrastructure::export::XlsxAdapter;
    use std::fs;

    let adapter = XlsxAdapter::new();
    let buffer = adapter
        .export_wilaya_monthly_status(&list)
        .map_err(|e| e.to_string())?;
    fs::write(&file_path, buffer).map_err(|e| e.to_string())?;

    Ok(XlsxExportResult::success(file_path, list.reports.len()))
}

/// Import a full node package (used for synchronization and initial unit bootstrap)
#[tauri::command]
pub fn import_unit_node_package(
    state: State<AppState>,
    file_path: String,
) -> Result<crate::models::dto::UnitNodePackageImportResult, String> {
    // 1. Resolve configuration status to allow bootstrap bypass
    let is_setup = {
        let db_guard = state.get_db().map_err(into_command_error)?;
        let db = db_ref_or_command_error(db_guard.as_ref())?;
        SettingsService::new(db.executor())
            .is_setup_mode()
            .map_err(into_command_error)?
    };

    // 2. Authorization Guard
    // Allow anonymous import ONLY if the system is not yet configured (Bootstrap Phase)
    let user_ctx = if is_setup {
        crate::application::services::UserContext::new("system", "system_bootstrap", None)
    } else {
        let session = authorize_command(&state, Action::AdminOnly, None)
            .map_err(into_command_error)?
            .0;
        user_ctx_from_session(&session)
    };

    state.touch_session();
    validation::validate_file_path(&file_path, &["unit"]).map_err(into_command_error)?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    // ADR-0016: use file-path API — eliminates the fs::read() Vec allocation.
    let package =
        read_unit_node_package_from_file(std::path::Path::new(&file_path), &state.crypto_port)
            .map_err(into_command_error)?;

    let unit_id = package.payload.unit.id.clone();
    AuditTxService::execute_with_audit(db, AuditAction::ImportNodePackage, &user_ctx, |tx| {
        let svc = NodePackageService::new(tx.executor);
        svc.import_unit_node_package(&package.payload)
    })
    .map_err(into_command_error)?;

    Ok(crate::models::dto::UnitNodePackageImportResult {
        unit_id,
        success: true,
    })
}

/// Import Wilaya-facing monthly summary from an encrypted **`sync`** interchange package (JSON envelope).
#[tauri::command]
pub fn import_monthly_summary_package(
    state: State<AppState>,
    file_path: String,
    unit_id: String,
) -> Result<DailyReportImportResult, String> {
    if unit_id.trim().is_empty() {
        return Err(into_command_error(AppError::Validation(
            ValidationError::Required {
                field: "unit_id".into(),
            },
        )));
    }

    run_import_pipeline(
        &state,
        Action::ImportMonthlySummaryPackage,
        file_path,
        MONTHLY_SUMMARY_PACKAGE_KIND,
        AuditAction::ImportNodePackage,
        read_monthly_summary_package_from_file,
        |executor, registry, package, session, importer_wilaya: &str| {
            let usecase_input = ImportMonthlySummaryPackageInput {
                package,
                unit_id: unit_id.clone(),
                importer_wilaya_code: importer_wilaya.to_string(),
                imported_by: session.username.clone(),
            };
            let outcome = apply_monthly_summary_package(executor, registry, usecase_input)?;
            Ok(DailyReportImportResult {
                report_count: outcome.report_count,
                item_count: 0,
                unit_id: Some(outcome.unit_id),
                file_hash: outcome.package_id,
                imported_by: session.username.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
        },
    )
}

/// Read-only operational query for sync import audit trail.
#[tauri::command]
pub fn get_import_audit_events(
    state: State<AppState>,
    query: ImportAuditQuery,
) -> Result<Vec<ImportAuditEventProjection>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadImportAudit, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    list_import_audit_events(db.executor(), &query).map_err(into_command_error)
}

// ===================
// Excel Export Commands (Moved from domain layer)
// ===================

#[tauri::command]
pub fn export_products_excel(
    state: State<AppState>,
    file_path: String,
) -> Result<XlsxExportResult, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ExportProducts, None).map_err(into_command_error)?;
    validation::validate_file_path(&file_path, &["xlsx"]).map_err(|e| e.to_string())?;
    state.touch_session();

    let guard = state.get_db().map_err(|e| e.to_string())?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    let executor = db.executor();

    let products = ProductService::new(executor)
        .list_products()
        .map_err(|e| e.to_string())?;

    use crate::domain::ports::export::ExcelPort;
    use crate::infrastructure::export::XlsxAdapter;
    use std::fs;

    let adapter = XlsxAdapter::new();
    let buffer = adapter
        .export_products(&products)
        .map_err(|e| e.to_string())?;
    fs::write(&file_path, buffer).map_err(|e| e.to_string())?;

    Ok(XlsxExportResult::success(file_path, products.len()))
}

#[tauri::command]
pub fn export_daily_report_excel(
    state: State<AppState>,
    report_id: String,
    file_path: String,
) -> Result<XlsxExportResult, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ExportDailyReport, None).map_err(into_command_error)?;
    validation::validate_file_path(&file_path, &["xlsx"]).map_err(|e| e.to_string())?;
    state.touch_session();

    let guard = state.get_db().map_err(|e| e.to_string())?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    let executor = db.executor();

    let report_svc = DailyReportService::new(executor);
    let report = report_svc
        .get_daily_report(&report_id)
        .map_err(|e| e.to_string())?
        .ok_or("Report not found")?;
    let date = report.date;
    let reports = report_svc
        .list_daily_reports_by_month(date.year(), date.month(), report.unit_id.as_deref())
        .map_err(|e| e.to_string())?;

    use crate::domain::ports::export::ExcelPort;
    use crate::infrastructure::export::XlsxAdapter;
    use std::fs;

    let adapter = XlsxAdapter::new();
    let buffer = adapter
        .export_daily_reports(&reports)
        .map_err(|e| e.to_string())?;
    fs::write(&file_path, buffer).map_err(|e| e.to_string())?;

    Ok(XlsxExportResult::success(file_path, reports.len()))
}

#[tauri::command]
pub fn export_monthly_summary_excel(
    state: State<AppState>,
    year: i32,
    month: i32,
    file_path: String,
) -> Result<XlsxExportResult, String> {
    let (_session, _settings) = authorize_command(&state, Action::ExportMonthlySummary, None)
        .map_err(into_command_error)?;
    validation::validate_file_path(&file_path, &["xlsx"]).map_err(|e| e.to_string())?;
    state.touch_session();

    let guard = state.get_db().map_err(|e| e.to_string())?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    let reports = DailyReportService::new(db.executor())
        .list_daily_reports_by_month(year, month as u32, None)
        .map_err(|e| e.to_string())?;

    use crate::domain::ports::export::ExcelPort;
    use crate::infrastructure::export::XlsxAdapter;
    use std::fs;

    let adapter = XlsxAdapter::new();
    let buffer = adapter
        .export_daily_reports(&reports)
        .map_err(|e| e.to_string())?;
    fs::write(&file_path, buffer).map_err(|e| e.to_string())?;

    Ok(XlsxExportResult::success(file_path, reports.len()))
}

// ===================
// Sync Package Export Commands (Moved from domain layer)
// ===================

#[tauri::command]
pub fn export_monthly_summary_package(
    state: State<AppState>,
    year: i32,
    month: i32,
    file_path: String,
) -> Result<PackageExportResult, String> {
    let (session, _settings) = authorize_command(&state, Action::ExportMonthlySummary, None)
        .map_err(into_command_error)?;
    validation::validate_file_path(&file_path, &["sync"]).map_err(into_command_error)?;
    validation::validate_calendar_month(year, month).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    let start_time = std::time::Instant::now();
    let executor = db.executor();

    let settings_svc = SettingsService::new(executor);

    let settings_row = settings_svc.get_settings().map_err(into_command_error)?;
    let unit_scope_id = settings_svc
        .get_current_unit_id()
        .map_err(into_command_error)?;

    let effective_unit_id = if settings_row.node_type == crate::models::NodeType::Unit {
        unit_scope_id
    } else {
        None
    };

    let input = MonthlySummaryExportInput { year, month };
    let dataset = crate::application::usecases::exports::export_monthly_summary_dataset::execute(
        executor,
        input,
        effective_unit_id.as_deref(),
    )
    .map_err(into_command_error)?;

    let source_node_id =
        resolve_export_source_node_id(executor, &settings_row).map_err(into_command_error)?;

    let _sequence = IdentitySignedExportService::new(db, &node_key_store())
        .export_v2_package(
            dataset.clone(),
            &source_node_id,
            "monthly_summary",
            std::path::Path::new(&file_path),
            export_subject_type(settings_row.node_type),
            &state.crypto_port,
        )
        .map_err(into_command_error)?;

    log::info!(
        target: "grpc::import_export",
        "export_monthly_summary_package: success path={} year={} month={}",
        file_path,
        year,
        month
    );

    let duration = start_time.elapsed().as_millis() as i64;
    let _ = crate::application::services::TelemetryService::new(executor).record_event(
        crate::application::services::TelemetryEventType::SyncExport,
        crate::application::services::TelemetryOutcome::Success,
        Some(duration),
        Some(serde_json::json!({ "path": file_path, "kind": "monthly_summary", "year": year, "month": month })),
        Some(&session.user_id),
    );

    Ok(PackageExportResult::success(
        file_path,
        dataset.daily_detail_rows.len() + 1,
        "encrypted".to_string(),
    ))
}

#[tauri::command]
pub fn export_unit_node_package(
    state: State<AppState>,
    unit_id: String,
    file_path: String,
) -> Result<PackageExportResult, String> {
    // 1. Check authorization
    let (session, settings) =
        authorize_command(&state, Action::ManageUnits, None).map_err(into_command_error)?;
    state.touch_session();

    // 2. Perform export logic
    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    let start_time = std::time::Instant::now();
    let executor = db.executor();

    validation::validate_file_path(&file_path, &["unit"]).map_err(into_command_error)?;

    let password_port = state.password_port.as_ref();
    let unit = UnitService::new(executor, password_port)
        .get_unit(&unit_id)
        .map_err(into_command_error)?
        .ok_or("Unit not found")?;
    let user = if let Some(ref uid) = unit.user_id {
        UserService::new(executor, password_port)
            .get_user_by_id(uid)
            .map_err(into_command_error)?
    } else {
        None
    };
    let user = user.ok_or("User not found for unit")?;

    let package_data = crate::models::UnitNodePackage {
        unit: unit.clone(),
        user: crate::models::UserExport {
            username: user.username.clone(),
            password_hash: user.password_hash.clone(),
            role: user.role.to_string(),
        },
    };

    let source_node_id =
        resolve_export_source_node_id(executor, &settings).map_err(into_command_error)?;

    let package = SyncPackage {
            metadata: SyncPackageMetadata {
                schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
                created_at: Utc::now(),
                source_node_id,
                package_sequence: None,
                issuer_identity_id: None,
                package_id: PackageId(Uuid::new_v4().to_string()),
                signature_version: None,
                signing_key_id: resolve_active_signing_key_id(),
                integrity_hash: None,
                signature: None,
            },
        payload: package_data,
    };

    let builder = PackageBuilder::new();
    builder
        .build_encrypted_stream_path(
            &package,
            &SerdeJsonSyncPackageSerializer,
            &HmacPackageSigner,
            &state.crypto_port,
            std::path::Path::new(&file_path),
        )
        .map_err(into_command_error)?;

    log::info!(
        target: "grpc::import_export",
        "export_unit_node_package: success path={} unit_id={}",
        file_path,
        unit_id
    );

    let duration = start_time.elapsed().as_millis() as i64;
    let _ = crate::application::services::TelemetryService::new(executor).record_event(
        crate::application::services::TelemetryEventType::SyncExport,
        crate::application::services::TelemetryOutcome::Success,
        Some(duration),
        Some(serde_json::json!({ "path": file_path, "kind": "unit_node", "unit_id": unit_id })),
        Some(&session.user_id),
    );

    Ok(PackageExportResult::success(
        file_path,
        1,
        "encrypted".to_string(),
    ))
}

#[tauri::command]
pub fn export_stock_movements_package(
    state: State<AppState>,
    start_date: String,
    end_date: String,
    file_path: String,
) -> Result<PackageExportResult, String> {
    let (session, _settings) = authorize_command(&state, Action::ExportStockMovements, None)
        .map_err(into_command_error)?;
    validation::validate_file_path(&file_path, &["sync"]).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    let start_time = std::time::Instant::now();
    let executor = db.executor();

    let settings_svc = SettingsService::new(executor);
    let settings_row = settings_svc.get_settings().map_err(into_command_error)?;

    let movements = StockMovementService::new(executor)
        .get_stock_movements_in_range(&start_date, &end_date)
        .map_err(into_command_error)?;

    let dataset = StockMovementsExportDataset { movements };

    let source_node_id =
        resolve_export_source_node_id(executor, &settings_row).map_err(into_command_error)?;

    let _sequence = IdentitySignedExportService::new(db, &node_key_store())
        .export_v2_package(
            dataset.clone(),
            &source_node_id,
            "stock_movements",
            std::path::Path::new(&file_path),
            export_subject_type(settings_row.node_type),
            &state.crypto_port,
        )
        .map_err(into_command_error)?;

    log::info!(
        target: "grpc::import_export",
        "export_stock_movements_package: success path={} range={}..{}",
        file_path,
        start_date,
        end_date
    );

    let duration = start_time.elapsed().as_millis() as i64;
    let _ = crate::application::services::TelemetryService::new(executor).record_event(
        crate::application::services::TelemetryEventType::SyncExport,
        crate::application::services::TelemetryOutcome::Success,
        Some(duration),
        Some(serde_json::json!({ "path": file_path, "kind": "stock_movements", "start_date": start_date, "end_date": end_date })),
        Some(&session.user_id),
    );

    Ok(PackageExportResult::success(
        file_path,
        dataset.movements.len(),
        "encrypted".to_string(),
    ))
}

#[tauri::command]
pub fn import_stock_movements_package(
    state: State<AppState>,
    file_path: String,
    unit_id: String,
) -> Result<crate::models::StockMovementsImportResult, String> {
    run_import_pipeline(
        &state,
        Action::ImportStockMovements,
        file_path,
        STOCK_MOVEMENTS_PACKAGE_KIND,
        AuditAction::ImportNodePackage,
        read_stock_movements_package_from_file,
        |executor, registry, package, session, importer_wilaya: &str| {
            let input = ImportStockMovementsPackageInput {
                package,
                unit_id: unit_id.clone(),
                importer_wilaya_code: importer_wilaya.to_string(),
                imported_by: session.username.clone(),
            };
            let outcome = apply_stock_movements_package(executor, registry, input)?;
            Ok(crate::models::StockMovementsImportResult {
                movement_count: outcome.movement_count as i32,
                unit_id: unit_id.clone(),
                file_hash: outcome.package_id,
                imported_by: session.username.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
        },
    )
}

/// Import a Trust Package (certificates + revocations) from an encrypted `.sync` file.
///
/// B4 (RFC 2026-08-04 §3.9): kind = `trust` — the ONLY trust-distribution channel.
/// Transport Guard (`run_import_pipeline`) enforces per-issuer sequence continuity.
#[tauri::command]
pub fn import_trust_package(
    state: State<AppState>,
    file_path: String,
) -> Result<TrustPackageImportResult, String> {
    run_import_pipeline(
        &state,
        Action::ImportTrustPackage,
        file_path,
        TRUST_PACKAGE_KIND,
        AuditAction::ImportTrustPackage,
        read_trust_package_from_file,
        |executor, registry, package, session, _importer_wilaya: &str| {
            let input = ImportTrustPackageInput {
                package,
                imported_by: session.username.clone(),
            };
            let outcome = apply_trust_package(executor, registry, input)?;
            Ok(TrustPackageImportResult {
                certificate_count: outcome.certificate_count,
                revocation_count: outcome.revocation_count,
                package_id: outcome.package_id,
                imported_by: session.username.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
        },
    )
}

/// Import a Registry Package (fleet-state snapshot) from an encrypted `.sync` file.
///
/// B4 (RFC 2026-08-04 §3.9): kind = `registry` — snapshot persisted verbatim (P4).
#[tauri::command]
pub fn import_registry_package(
    state: State<AppState>,
    file_path: String,
) -> Result<RegistryPackageImportResult, String> {
    run_import_pipeline(
        &state,
        Action::ImportRegistryPackage,
        file_path,
        REGISTRY_PACKAGE_KIND,
        AuditAction::ImportRegistryPackage,
        read_registry_package_from_file,
        |executor, registry, package, session, _importer_wilaya: &str| {
            let input = ImportRegistryPackageInput {
                package,
                imported_by: session.username.clone(),
            };
            let outcome = apply_registry_package(executor, registry, input)?;
            Ok(RegistryPackageImportResult {
                snapshot_version: outcome.snapshot_version,
                unit_count: outcome.unit_count,
                package_id: outcome.package_id,
                imported_by: session.username.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
        },
    )
}

/// Core pipeline for encrypted sync package imports.
/// Handles: Authorization, Session Touch, Package Loading, Audit Logging (Start/Success/Failure),
/// Transaction Orchestration (AuditTxService), and Error Mapping.
fn run_import_pipeline<T, R, L, I>(
    state: &State<AppState>,
    action: Action,
    file_path: String,
    package_kind: &str,
    audit_action: AuditAction,
    loader: L,
    importer: I,
) -> Result<R, String>
where
    T: serde::de::DeserializeOwned + Clone + Send + Sync + serde::Serialize,
    L: FnOnce(
        &std::path::Path,
        &crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider,
    ) -> Result<SyncPackage<T>, AppError>,
    I: FnOnce(
        crate::repositories::executor::DbExecutor<'_>,
        &SqliteImportedPackageRegistry<'_>,
        SyncPackage<T>,
        &CurrentSession,
        &str,
    ) -> Result<R, AppError>,
{
    // 1. Authorization & Session Touch
    let (session, settings) = authorize_command(state, action, None).map_err(into_command_error)?;
    require_maintenance_allows(state, MaintenanceBlockedOperation::Import)
        .map_err(into_command_error)?;
    state.touch_session();
    validation::validate_file_path(&file_path, &["sync"]).map_err(into_command_error)?;

    // 2. Context Extraction
    let importer_wilaya = settings.wilaya_code.as_deref().ok_or_else(|| {
        into_command_error(AppError::Validation(ValidationError::Required {
            field: "wilaya_code".into(),
        }))
    })?;

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let start_time = std::time::Instant::now();

    // 3. Package Loading (Encrypted)
    let package =
        loader(std::path::Path::new(&file_path), &state.crypto_port).map_err(into_command_error)?;

    let package_id = package.metadata.package_id.0.clone();
    let source_node_id =
        Some(package.metadata.source_node_id.trim().to_string()).filter(|s| !s.is_empty());

    // B4 transport metadata (RFC 2026-08-04 §3.4.1): per-issuer sequence ledger.
    let package_sequence = package.metadata.package_sequence;
    let issuer_identity_id = package.metadata.issuer_identity_id.map(|u| u.to_string());

    let kind = package_kind.to_string();

    // 4. Initial Audit Log (Start)
    let logger = SqliteImportAuditLogger::new(db.executor());
    let _ = logger.log(&ImportAuditEvent {
        event_type: ImportAuditEventType::ImportStarted,
        package_id: package_id.clone(),
        package_kind: kind.clone(),
        source_node_id: source_node_id.clone(),
        reason_code: None,
    });

    let imported_by_ref = session.username.trim();

    // 5. Transaction & Audit Orchestration
    // ADR-0016 + Consistency: Atomic mutation + domain event persistence + audit log entry.
    let outcome = db.with_event_persistence(|ctx| {
        let executor = ctx.executor();
        let registry = SqliteImportedPackageRegistry::new(
            executor,
            package_kind,
            source_node_id.as_deref(),
            imported_by_ref,
            package_sequence,
            issuer_identity_id.as_deref(),
        );

        let out = {
            // B4 signature_version=2 verification (RFC 2026-08-04 §3.10): Ed25519
            // node identity against the Identity Store. Runs before the Transport
            // Guard so unauthenticated packages cannot probe sequence state.
            SyncPackageIdentityVerificationService::verify_v2_signature(executor, &package)?;

            // B4 Transport Guard (RFC 2026-08-04 §3.4.1): per-issuer sequence
            // continuity, enforced ONLY in the import pipeline.
            if let Some(issuer) = issuer_identity_id.as_deref() {
                let sequence = package_sequence.ok_or_else(|| {
                    AppError::Validation(ValidationError::InvalidFormat {
                        field: "package_sequence".into(),
                        message: "حزمة موقّعة بلا رقم تسلسل نقل".into(),
                    })
                })?;
                let last_applied =
                    SyncPackageIdentityVerificationService::last_applied_sequence(executor, issuer)?;
                match TransportGuard::check(issuer, sequence, last_applied) {
                    TransportVerdict::Accept { .. } => {}
                    TransportVerdict::OutOfOrder { expected, got, .. } => {
                        log::warn!(
                            target: "grpc::import_export",
                            "import pipeline transport guard: package_id={} issuer={} reason=OUT_OF_ORDER expected={} got={}",
                            package_id,
                            issuer,
                            expected,
                            got
                        );
                        return Err(AppError::BusinessLogic(
                            BusinessLogicError::OperationNotPermitted {
                                message: format!(
                                    "انتهاك ترتيب النقل: المُصدِر {issuer} يُتوقّع التسلسل {expected} ووصل {got}"
                                ),
                            },
                        ));
                    }
                    TransportVerdict::Replay { .. } => {
                        log::warn!(
                            target: "grpc::import_export",
                            "import pipeline transport guard: package_id={} issuer={} reason=REPLAY sequence={}",
                            package_id,
                            issuer,
                            sequence
                        );
                        return Err(AppError::BusinessLogic(
                            BusinessLogicError::OperationNotPermitted {
                                message: format!(
                                    "إعادة بث الحزمة رقم {sequence} من المُصدِر {issuer} مرفوضة"
                                ),
                            },
                        ));
                    }
                }
            }

            let out = importer(executor, &registry, package, &session, importer_wilaya)?;

            // B4: advance the per-issuer transport ledger atomically with the import.
            if let Some(issuer) = issuer_identity_id.as_deref() {
                if let Some(sequence) = package_sequence {
                    SyncPackageIdentityVerificationService::advance_issuer_sequence(
                        executor, issuer, sequence,
                    )?;
                }
            }

            out
        };

        // Main audit log entry (same transaction — was handled by AuditTxService)
        AuditService::new(executor).log_success(
            &session.user_id,
            &session.username,
            audit_action.clone(),
            audit_action.default_entity_type(),
            None,
            None,
            None,
            None,
            None,
            None,
        )?;

        // Success Audit (Import Audit Table)
        SqliteImportAuditLogger::new(executor).log(&ImportAuditEvent {
            event_type: ImportAuditEventType::ImportSucceeded,
            package_id: package_id.clone(),
            package_kind: kind.clone(),
            source_node_id: source_node_id.clone(),
            reason_code: None,
        })?;

        ctx.emit(DomainEvent::SyncPackageImported {
            package_id: package_id.clone(),
            kind: package_kind.to_string(),
            source_node_id: source_node_id.clone(),
            sequence_number: None,
        });

        Ok(out)
    });

    // 6. Outcome Mapping & Failure Audit
    let source_integrity = SystemIntegrityState::resolve_from_executor(db.executor())
        .ok()
        .map(|s| format!("{:?}", s));

    match outcome {
        Ok((v, _buf)) => {
            let _ = ImportReproducibilityService::new(db.executor()).record_import_metadata(
                &ImportReproducibilityRecord {
                    package_id: package_id.clone(),
                    package_kind: kind.clone(),
                    imported_at: chrono::Utc::now().to_rfc3339(),
                    imported_by: session.username.clone(),
                    validation_state: "VALIDATED".to_string(),
                    source_integrity_state: source_integrity.clone(),
                    rejected_records_count: 0,
                },
            );
            log::info!(
                target: "grpc::import_export",
                "import pipeline succeeded kind={} package_id={}",
                package_kind,
                package_id
            );

            let duration = start_time.elapsed().as_millis() as i64;
            let _ = crate::application::services::TelemetryService::new(db.executor())
                .record_event(
                    crate::application::services::TelemetryEventType::SyncImport,
                    crate::application::services::TelemetryOutcome::Success,
                    Some(duration),
                    Some(serde_json::json!({ "package_id": package_id, "kind": package_kind })),
                    Some(&session.user_id),
                );

            Ok(v)
        }

        Err(e) => {
            let reason = ImportFailureReason::classify(&e);
            let logger = SqliteImportAuditLogger::new(db.executor());
            let _ = logger.log(&ImportAuditEvent {
                event_type: ImportAuditEventType::ImportRejected,
                package_id: package_id.clone(),
                package_kind: kind.clone(),
                source_node_id: source_node_id.clone(),
                reason_code: Some(reason.code().to_string()),
            });
            let _ = ImportReproducibilityService::new(db.executor()).record_import_metadata(
                &ImportReproducibilityRecord {
                    package_id: package_id.clone(),
                    package_kind: kind.clone(),
                    imported_at: chrono::Utc::now().to_rfc3339(),
                    imported_by: session.username.clone(),
                    validation_state: format!("REJECTED:{}", reason.code()),
                    source_integrity_state: source_integrity,
                    rejected_records_count: 1,
                },
            );
            log::warn!(
                target: "grpc::import_export",
                "import pipeline failed kind={} package_id={} reason={}",
                package_kind,
                package_id,
                reason.code()
            );

            let duration = start_time.elapsed().as_millis() as i64;
            let _ = crate::application::services::TelemetryService::new(db.executor()).record_event(
                crate::application::services::TelemetryEventType::SyncImport,
                crate::application::services::TelemetryOutcome::Failure,
                Some(duration),
                Some(serde_json::json!({ "package_id": package_id, "kind": package_kind, "reason": reason.code() })),
                Some(&session.user_id),
            );

            Err(into_command_error(e))
        }
    }
}
