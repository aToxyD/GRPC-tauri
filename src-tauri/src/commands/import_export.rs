//! Import/Export Commands
//!
//! Handles file imports (packages) and exports
//! Strictly follows Clean Architecture: Commands -> Services -> Repositories -> DB

use crate::application::authz::policies::{resolve_identity_access_import_path, IdentityAccessImportPath};
use crate::application::authz::Action;
use crate::application::services::{
    record_export_with_reproducibility, AuditService, AuditTxService, B8FirstImportPredicatesService,
    DailyReportService, ExportReproducibilityContext, IdentityProvisioningService,
    IdentitySignedExportService, NodePackageService, ProductService, SettingsService,
    StockMovementService, SyncPackageIdentityVerificationService, UnitService, UserAccountSyncService,
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
    SyncPackage, SyncPackageMetadata,
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
use crate::application::usecases::sync::import_identity_access_package::IDENTITY_ACCESS_PACKAGE_KIND;
use crate::application::usecases::sync::import_identity_access_package::{
    execute as apply_identity_access_package, ImportIdentityAccessPackageInput,
};
use crate::application::usecases::sync::import_monthly_summary_package::MONTHLY_SUMMARY_PACKAGE_KIND;
use crate::application::usecases::sync::import_monthly_summary_package::{
    execute as apply_monthly_summary_package, ImportMonthlySummaryPackageInput,
};
use crate::application::usecases::sync::import_products_package::PRODUCTS_PACKAGE_KIND;
use crate::application::usecases::sync::import_products_package::{
    execute as apply_products_package, ImportProductsPackageInput,
};
use crate::application::usecases::sync::import_registry_package::REGISTRY_PACKAGE_KIND;
use crate::application::usecases::sync::import_registry_package::{
    execute as apply_registry_package, ImportRegistryPackageInput,
};
use crate::application::usecases::sync::import_stock_movements_package::STOCK_MOVEMENTS_PACKAGE_KIND;
use crate::application::usecases::sync::import_stock_movements_package::{
    execute as apply_stock_movements_package, ImportStockMovementsPackageInput,
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
use crate::infrastructure::sync::{
    read_daily_report_package_from_file, read_identity_access_package_from_file,
    read_monthly_summary_package_from_file, read_products_package_from_file,
    read_registry_package_from_file, read_stock_movements_package_from_file,
    read_trust_package_from_file, read_unit_node_package_from_file, resolve_export_source_node_id,
};

use crate::models::{
    DailyReportImportResult, IdentityAccessPackageImportResult, IdentityAccessPayload,
    PackageExportResult, RegistryPackageImportResult, Settings, TrustPackageImportResult,
    XlsxExportResult,
};
use chrono::Datelike;
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

/// `signature_version` value for Ed25519 (RFC 8032) issuer-bound signing —
/// mirror of `SIGNATURE_VERSION_ED25519` (`domain::identity::signing`).
///
/// Rule 126 confines the asymmetric-identity symbols themselves to identity
/// layers (RFC 2026-08-04-node-identity-trust / ADR-0038); this boundary
/// policy lives in the command layer and only needs the numeric extension
/// value. Keeping the mirror local is deliberate: commands must not import
/// identity-layer constants.
const SIGNATURE_VERSION_V2: u16 = 2;

/// Package kinds whose authenticity is security-critical (SEC-003-02).
/// These MUST be `signature_version = 2` (Ed25519, issuer-bound). Anything less
/// — unsigned, V1/HMAC, or missing issuer/sequence/integrity metadata — is a
/// downgrade/forgery window and is rejected at the import boundary.
const SECURITY_CRITICAL_KINDS: &[&str] = &[
    IDENTITY_ACCESS_PACKAGE_KIND,
    TRUST_PACKAGE_KIND,
    REGISTRY_PACKAGE_KIND,
];

/// Data/report package kinds that remain on the V1-with-signature compatibility
/// window: `signature` and `integrity_hash` must be present, but V1 is still
/// accepted until the window closes.
const DATA_PACKAGE_KINDS: &[&str] = &[
    PRODUCTS_PACKAGE_KIND,
    DAILY_REPORT_PACKAGE_KIND,
    MONTHLY_SUMMARY_PACKAGE_KIND,
    STOCK_MOVEMENTS_PACKAGE_KIND,
];

/// Kind-aware authenticity requirements for the `.unit` setup-mode import
/// (SEC-003-05-D).
///
/// The generic legacy deserializer tolerates a missing V1 signature (legacy
/// compatibility); this boundary does NOT. Requiring a present, non-empty
/// `signature` forces the deserializer's existing HMAC verification path to
/// actually run, and `source_node_id` pins the signer identity for the
/// trusted-signer check. A `.unit` failing these requirements is rejected
/// before any unit/user/settings write.
fn validate_unit_package_security_requirements(
    metadata: &SyncPackageMetadata,
) -> Result<(), AppError> {
    if metadata.signature.as_deref().is_none_or(str::is_empty) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "signature".into(),
            message: "حزمة العقدة بدون توقيع — مرفوضة (متطلبات الأمان)".into(),
        }));
    }
    if metadata.source_node_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "source_node_id".into(),
            message: "حزمة العقدة بدون مصدر — مرفوضة (متطلبات الأمان)".into(),
        }));
    }
    Ok(())
}

/// Kind-aware cryptographic authenticity requirements (SEC-003-02).
///
/// Runs after deserialization and before replay/sequence processing, the
/// `ValidationGate`, and any state mutation. The generic deserializer remains
/// legacy-tolerant; this boundary owns the per-kind policy.
///
/// - Security-critical kinds MUST be V2 (Ed25519) with a present, non-empty
///   signature, issuer identity, package sequence, and integrity hash.
/// - Data kinds MUST carry a present signature and integrity hash; V1-with-
///   signature remains valid during the compatibility window; V2 remains valid.
fn validate_import_security_requirements(
    package_kind: &str,
    metadata: &SyncPackageMetadata,
) -> Result<(), AppError> {
    let reject = |field: &str| {
        AppError::Validation(ValidationError::InvalidFormat {
            field: field.into(),
            message: format!("حزمة {package_kind} بدون {field} — مرفوضة (متطلبات الأمان)"),
        })
    };

    if SECURITY_CRITICAL_KINDS.contains(&package_kind) {
        if metadata.signature_version != Some(SIGNATURE_VERSION_V2) {
            return Err(reject("signature_version=V2"));
        }
        if metadata.signature.as_deref().is_none_or(str::is_empty) {
            return Err(reject("signature"));
        }
        if metadata.issuer_identity_id.is_none() {
            return Err(reject("issuer_identity_id"));
        }
        if metadata.package_sequence.is_none() {
            return Err(reject("package_sequence"));
        }
        if metadata.integrity_hash.as_deref().is_none_or(str::is_empty) {
            return Err(reject("integrity_hash"));
        }
    } else if DATA_PACKAGE_KINDS.contains(&package_kind) {
        if metadata.signature.as_deref().is_none_or(str::is_empty) {
            return Err(reject("signature"));
        }
        if metadata.integrity_hash.as_deref().is_none_or(str::is_empty) {
            return Err(reject("integrity_hash"));
        }
    }

    Ok(())
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

    // SEC-003-05-D boundary: `.unit` packages must pin a non-empty
    // source_node_id (signer identity) regardless of signature version.
    if package.metadata.source_node_id.trim().is_empty() {
        return Err(into_command_error(AppError::Validation(
            ValidationError::InvalidFormat {
                field: "source_node_id".into(),
                message: "حزمة العقدة بدون مصدر — مرفوضة (متطلبات الأمان)".into(),
            },
        )));
    }

    // Trust-First V2 path (ADR-0044, RFC §3.10): Ed25519 by the WILAYA
    // issuing identity, verified against the locally installed ACTIVE trust
    // anchor (Root → WILAYA → Ed25519). Anchor-first: no anchor → no V2
    // `.unit` acceptance. The `.unit` is a one-time bootstrap artifact: fixed
    // sequence 1, never advancing the per-issuer transport ledger.
    let unit_id = package.payload.unit.id.clone();
    let unit_subject_id = Uuid::parse_str(&unit_id).map_err(|e| {
        into_command_error(AppError::Internal(format!(
            "units.id '{unit_id}' is not a valid UUID: {e}"
        )))
    })?;

    // ADR-0044 packaged-identity bootstrap — step 8 pre-write validation,
    // before ANY persistence. A packaged identity is only accepted on the
    // V2-authenticated transport; a legacy (V1/HMAC) package carrying identity
    // fields is refused fail-closed (no WILAYA issuer authenticity).
    let packaged_identity = if package.metadata.signature_version == Some(SIGNATURE_VERSION_V2) {
        let issuer = package.metadata.issuer_identity_id.ok_or_else(|| {
            into_command_error(AppError::Validation(ValidationError::InvalidFormat {
                field: "issuer_identity_id".into(),
                message: "حزمة عقدة V2 بدون هوية مُصدِر — مرفوضة".into(),
            }))
        })?;
        SyncPackageIdentityVerificationService::verify_v2_signature(db.executor(), &package)
            .map_err(into_command_error)?;
        B8FirstImportPredicatesService::verify_unit_v2_acceptance(
            &db.executor(),
            &issuer.to_string(),
            package.metadata.package_sequence,
        )
        .map_err(into_command_error)?;
        // `issuer` is the package issuer, already authenticated by
        // verify_v2_signature + B8 against the installed ACTIVE WILAYA anchor.
        IdentityProvisioningService::extract_packaged_unit_identity(
            &package.payload,
            &unit_subject_id,
            &issuer,
        )
        .map_err(into_command_error)?
    } else {
        // Legacy V1/HMAC window (A44-07): present, non-empty signature so the
        // deserializer's HMAC verification path runs, plus the source pin
        // above. No new V1 packages are produced (V2-only export).
        validate_unit_package_security_requirements(&package.metadata)
            .map_err(into_command_error)?;
        if package.payload.unit_certificate.is_some()
            || package.payload.unit_private_key.is_some()
        {
            return Err(into_command_error(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "A packaged UNIT identity requires the V2-authenticated transport; refused on a legacy package"
                        .into(),
                },
            )));
        }
        None
    };

    AuditTxService::execute_with_audit(db, AuditAction::ImportNodePackage, &user_ctx, |tx| {
        let executor = tx.executor;
        let svc = NodePackageService::new(executor);
        svc.import_unit_node_package(&package.payload)?;

        // ADR-0044 packaged-identity bootstrap: install the packaged UNIT key
        // and identity INSIDE the same audit transaction as the base import.
        // The key write is guarded (`install_node_key_matching` never
        // overwrites a different key); the identity upsert is idempotent. A
        // crash between the two leaves "key present, identity absent", which a
        // re-import of the SAME package recovers from (both steps re-run).
        if let Some(pkg_identity) = &packaged_identity {
            let nks = node_key_store();
            IdentityProvisioningService::install_node_key_matching(
                &nks,
                &pkg_identity.secret_key,
                &pkg_identity.certificate.public_key,
            )?;
            let now = chrono::Utc::now().to_rfc3339();
            IdentityProvisioningService::install_unit_identity_on_executor(
                executor,
                &pkg_identity.certificate,
                &nks,
                &now,
            )?;
        }
        Ok(())
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
    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
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

    let source_node_id =
        resolve_export_source_node_id(executor, &settings).map_err(into_command_error)?;

    // ADR-0044 packaged-identity bootstrap: the WILAYA generates the UNIT
    // Ed25519 keypair IN MEMORY (never written to the WILAYA NodeKeyStore),
    // signs the UNIT certificate with the ACTIVE local WILAYA identity, and
    // registers the WILAYA-side Issuer Local State. RE-EXPORT RULE: the
    // duplicate-ACTIVE guard inside `sign_unit_bootstrap_request` rejects a
    // second export once an ACTIVE UNIT identity exists (RE-EXPORT = REJECT).
    let unit_subject_id = Uuid::parse_str(&unit_id).map_err(|e| {
        into_command_error(AppError::Internal(format!(
            "units.id '{unit_id}' is not a valid UUID: {e}"
        )))
    })?;
    let now = chrono::Utc::now().to_rfc3339();
    let (unit_secret, unit_cert) = {
        let provisioning = IdentityProvisioningService::new(db);
        let (secret, request) = provisioning
            .generate_unit_identity_request_for_package(unit_subject_id)
            .map_err(into_command_error)?;
        let cert = provisioning
            .sign_unit_bootstrap_request(&request, &node_key_store(), &now)
            .map_err(into_command_error)?;
        (secret, cert)
    };

    let package_data = crate::models::UnitNodePackage {
        unit: unit.clone(),
        user: crate::models::UserExport {
            username: user.username.clone(),
            password_hash: user.password_hash.clone(),
            role: user.role.to_string(),
        },
        unit_certificate: Some(unit_cert),
        unit_private_key: Some(unit_secret.to_vec()),
    };

    // ADR-0044 A44-07/08: `.unit` export is V2-only (Ed25519 by the WILAYA
    // identity) — no new V1/HMAC packages are produced (RFC §3.10). The
    // bootstrap artifact carries the FIXED sequence 1 and never touches the
    // per-issuer ledger; the receiving UNIT accepts it anchor-first.
    IdentitySignedExportService::new(db, &node_key_store())
        .export_v2_bootstrap_package(
            package_data.clone(),
            &source_node_id,
            std::path::Path::new(&file_path),
            export_subject_type(settings.node_type),
            &state.crypto_port,
        )
        .map_err(into_command_error)?;

    log::info!(
        target: "grpc::import_export",
        "export_unit_node_package: success path={} unit_id={}",
        file_path,
        unit_id
    );

    let duration = start_time.elapsed().as_millis() as i64;
    let _ = crate::application::services::TelemetryService::new(db.executor()).record_event(
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

// ─────────────────────────────────────────────────────────────────────────────
// B8 — Identity & Access Synchronization (ADR-0040)
// ─────────────────────────────────────────────────────────────────────────────

/// Wilaya: set the fleet-wide `admin` password (admin derivation domain).
///
/// The hash authenticates `admin` on every node. Local modification on UNIT
/// nodes is forbidden by Invariant 13; only the Wilaya IPC command reaches this
/// mutation (authz `Action::ManageAccountSync` → Wilaya + AdminOnly).
#[tauri::command]
pub fn set_fleet_admin_password(state: State<AppState>, password: String) -> Result<(), String> {
    let (session, _settings) =
        authorize_command(&state, Action::ManageAccountSync, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let user_ctx = user_ctx_from_session(&session);
    let password_port = state.password_port.as_ref();

    AuditTxService::execute_with_audit(
        db,
        AuditAction::FleetAdminPasswordUpdated,
        &user_ctx,
        |tx| {
            UserAccountSyncService::new(tx.executor, password_port)
                .set_fleet_admin_password(&password)
        },
    )
    .map_err(into_command_error)?;

    Ok(())
}

/// Wilaya: set a unit's `user` password (node-bound to the unit code).
///
/// The hash authenticates `user` only on the target unit. Wilaya-only (authz
/// `Action::ManageAccountSync` → Wilaya + AdminOnly).
#[tauri::command]
pub fn set_unit_user_password(
    state: State<AppState>,
    unit_code: String,
    password: String,
) -> Result<(), String> {
    let (session, _settings) =
        authorize_command(&state, Action::ManageAccountSync, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let user_ctx = user_ctx_from_session(&session);
    let password_port = state.password_port.as_ref();

    AuditTxService::execute_with_audit(db, AuditAction::UnitUserPasswordUpdated, &user_ctx, |tx| {
        UserAccountSyncService::new(tx.executor, password_port)
            .set_unit_user_password(&unit_code, &password)
    })
    .map_err(into_command_error)?;

    Ok(())
}

/// Wilaya: enable / disable an account (soft-delete semantics).
///
/// `enabled = false` maps to `deleted = 1`. Wilaya-only (authz
/// `Action::ManageAccountSync` → Wilaya + AdminOnly).
#[tauri::command]
pub fn set_account_status(
    state: State<AppState>,
    username: String,
    enabled: bool,
) -> Result<(), String> {
    let (session, _settings) =
        authorize_command(&state, Action::ManageAccountSync, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let user_ctx = user_ctx_from_session(&session);
    let password_port = state.password_port.as_ref();

    AuditTxService::execute_with_audit(db, AuditAction::AccountStatusChanged, &user_ctx, |tx| {
        UserAccountSyncService::new(tx.executor, password_port)
            .set_account_status(&username, enabled)
    })
    .map_err(into_command_error)?;

    Ok(())
}

/// Wilaya: export one unit's Identity & Access package (encrypted `.sync`).
///
/// B8 (ADR-0040): kind = `identity_access`, one package per unit, signed V2 via
/// `IdentitySignedExportService`. Fails closed when the fleet `admin` password
/// is unset or the fleet `admin` is disabled. Wilaya-only (authz
/// `Action::ExportIdentityAccessPackage` → Wilaya + AdminOnly).
#[tauri::command]
pub fn export_identity_access_package(
    state: State<AppState>,
    unit_code: String,
    file_path: String,
) -> Result<PackageExportResult, String> {
    export_identity_access_package_impl(&state, unit_code, file_path)
}

/// Implementation of `export_identity_access_package` (testable without a
/// Tauri runtime).
///
/// F-1 Option A (ADR-0045 §26.9): the producer allocates the transport
/// sequence from the per-`(issuer, target_unit_code)` stream
/// (`IdentitySignedExportService::export_v2_identity_access_package`). The
/// target unit is the authoritative server-side one: `UserAccountSyncService`
/// resolves `unit_code` against the local `units` table and builds the payload
/// from that row (fails closed when the unit does not exist) — the renderer
/// value can only select an existing unit, never choose a sequence stream.
pub fn export_identity_access_package_impl(
    state: &AppState,
    unit_code: String,
    file_path: String,
) -> Result<PackageExportResult, String> {
    let (session, settings) = authorize_command(state, Action::ExportIdentityAccessPackage, None)
        .map_err(into_command_error)?;
    validation::validate_file_path(&file_path, &["sync"]).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    let start_time = std::time::Instant::now();

    let password_port = state.password_port.as_ref();
    let payload = UserAccountSyncService::new(db.executor(), password_port)
        .export(&unit_code)
        .map_err(into_command_error)?;

    let source_node_id =
        resolve_export_source_node_id(db.executor(), &settings).map_err(into_command_error)?;

    let _sequence = IdentitySignedExportService::new(db, &node_key_store())
        .export_v2_identity_access_package(
            payload,
            &source_node_id,
            std::path::Path::new(&file_path),
            export_subject_type(settings.node_type),
            &state.crypto_port,
        )
        .map_err(into_command_error)?;

    log::info!(
        target: "grpc::import_export",
        "export_identity_access_package: success path={} unit_code={}",
        file_path,
        unit_code
    );

    let export_hash = Uuid::new_v4().to_string();
    let result = PackageExportResult::success(file_path.clone(), 1, "encrypted".to_string());

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
                export_reason: "identity_access_sync_package".to_string(),
            },
        )
    });

    let duration = start_time.elapsed().as_millis() as i64;
    let _ = crate::application::services::TelemetryService::new(db.executor()).record_event(
        crate::application::services::TelemetryEventType::SyncExport,
        crate::application::services::TelemetryOutcome::Success,
        Some(duration),
        Some(serde_json::json!({
            "path": file_path,
            "kind": IDENTITY_ACCESS_PACKAGE_KIND,
            "unit_code": unit_code,
        })),
        Some(&session.user_id),
    );

    Ok(result)
}

/// UNIT: import an Identity & Access package (encrypted `.sync`).
///
/// B8 (ADR-0040): kind = `identity_access` — canonical account reconciliation on
/// the UNIT node (rename + canonical upserts). Replay protection is owned by
/// `run_import_pipeline` (Transport Guard + `ImportedPackageRegistry`).
/// Unit-only (authz `Action::ImportIdentityAccessPackage` → Unit + authenticated).
#[tauri::command]
pub fn import_identity_access_package(
    state: State<AppState>,
    file_path: String,
) -> Result<IdentityAccessPackageImportResult, String> {
    import_identity_access_package_impl(&state, file_path)
}

/// Implementation of `import_identity_access_package` (testable without a
/// Tauri runtime).
pub fn import_identity_access_package_impl(
    state: &AppState,
    file_path: String,
) -> Result<IdentityAccessPackageImportResult, String> {
    // B8 (ADR-0045, RFC 2026-08-04 §3.12): on a fresh UNIT node no Admin
    // session exists yet — the command-authorization layer is anonymous for
    // the FIRST identity_access import. A User session is admitted ONLY
    // through the fail-closed first-import predicates (re-evaluated inside
    // the import transaction); every other caller keeps the AdminOnly policy.
    // `AuthenticatedOnly` establishes the session; the Admin-vs-User routing
    // is an authorization decision resolved in the authz layer.
    let (session, settings) = authorize_command(state, Action::AuthenticatedOnly, None)
        .map_err(into_command_error)?;
    match resolve_identity_access_import_path(&session.user_snapshot.role, settings.node_type)
        .map_err(|e| into_command_error(AppError::Authorization(e)))?
    {
        IdentityAccessImportPath::FirstImportBootstrap => {
            // Captured BEFORE the pipeline acquires the DB guard (the importer
            // closure runs inside the guard-held transaction).
            let local_unit_code: Option<String> = settings.unit_code.clone();
            run_import_pipeline_bootstrap(
                state,
                file_path,
                IDENTITY_ACCESS_PACKAGE_KIND,
                read_identity_access_package_from_file,
                |executor, registry, package, session, _importer_wilaya: &str| {
                    let verdict = B8FirstImportPredicatesService::evaluate(
                        &executor,
                        &package.payload.unit_code,
                        package
                            .metadata
                            .issuer_identity_id
                            .as_ref()
                            .map(|u| u.to_string())
                            .as_deref(),
                        local_unit_code.as_deref(),
                    )?;
                    if !verdict.all_hold() {
                        return Err(AppError::BusinessLogic(
                            BusinessLogicError::OperationNotPermitted {
                                message: B8FirstImportPredicatesService::rejection_message(
                                    &verdict,
                                )
                                .to_string(),
                            },
                        ));
                    }
                    apply_identity_access_package_import(
                        state, executor, registry, package, session,
                    )
                },
            )
        }
        IdentityAccessImportPath::AdminOnly => run_import_pipeline(
            state,
            Action::ImportIdentityAccessPackage,
            file_path,
            IDENTITY_ACCESS_PACKAGE_KIND,
            AuditAction::IdentityAccessPackageImported,
            read_identity_access_package_from_file,
            |executor, registry, package, session, _importer_wilaya: &str| {
                apply_identity_access_package_import(state, executor, registry, package, session)
            },
        ),
    }
}

fn apply_identity_access_package_import(
    state: &AppState,
    executor: crate::repositories::executor::DbExecutor<'_>,
    registry: &SqliteImportedPackageRegistry<'_>,
    package: SyncPackage<IdentityAccessPayload>,
    session: &CurrentSession,
) -> Result<IdentityAccessPackageImportResult, AppError> {
    let password_port = state.password_port.as_ref();
    let input = ImportIdentityAccessPackageInput {
        package,
        imported_by: session.username.clone(),
    };
    let outcome = apply_identity_access_package(executor, registry, password_port, input)?;
    Ok(IdentityAccessPackageImportResult {
        admin_updated: outcome.admin_updated,
        user_updated: outcome.user_updated,
        user_renamed: outcome.user_renamed,
        package_id: outcome.package_id,
        imported_by: session.username.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

/// Core pipeline for encrypted sync package imports.
/// Handles: Authorization, Session Touch, Package Loading, Audit Logging (Start/Success/Failure),
/// Transaction Orchestration (AuditTxService), and Error Mapping.
fn run_import_pipeline<T, R, L, I>(
    state: &AppState,
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
    run_import_pipeline_core(
        state,
        file_path,
        package_kind,
        audit_action,
        None,
        loader,
        importer,
        |state| authorize_command(state, action, None),
    )
}

/// B8 first-import variant of the import pipeline (ADR-0045).
///
/// The command-authorization layer is anonymous: a User session is admitted
/// when the fail-closed first-import predicates (enforced inside the import
/// transaction, on the same snapshot) hold. No other caller reaches this
/// path — Admin sessions always use `run_import_pipeline` with the AdminOnly
/// policy. Once the first import succeeds a canonical Admin exists and the
/// exemption is self-terminating.
fn run_import_pipeline_bootstrap<T, R, L, I>(
    state: &AppState,
    file_path: String,
    package_kind: &str,
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
    run_import_pipeline_core(
        state,
        file_path,
        package_kind,
        AuditAction::IdentityAccessBootstrapImported,
        Some(AuditAction::IdentityAccessBootstrapImportFailed),
        loader,
        importer,
        |state| {
            // Any authenticated session on the UNIT node is admitted here —
            // the B8 first-import predicates (enforced inside the import
            // transaction) are the actual gate; the structural guard below
            // restricts the exemption to UNIT nodes only.
            let (session, settings) = authorize_command(state, Action::AuthenticatedOnly, None)?;
            // Structural guard: the first-import exemption exists only on a
            // UNIT node. On a WILAYA node the AdminOnly policy is absolute.
            if settings.node_type != crate::models::NodeType::Unit {
                return Err(AppError::Authorization(
                    crate::errors::AuthorizationError::InsufficientPermissions,
                ));
            }
            Ok((session, settings))
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn run_import_pipeline_core<T, R, L, I, A>(
    state: &AppState,
    file_path: String,
    package_kind: &str,
    audit_action: AuditAction,
    failure_audit_action: Option<AuditAction>,
    loader: L,
    importer: I,
    authorize: A,
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
    A: FnOnce(&AppState) -> Result<(CurrentSession, Settings), AppError>,
{
    // 1. Authorization & Session Touch
    let (session, settings) = authorize(state).map_err(into_command_error)?;
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

    // SEC-003-02: kind-aware authenticity requirements. Rejects unsigned / V1
    // downgraded critical kinds before replay/sequence processing, the
    // ValidationGate, or any state mutation.
    validate_import_security_requirements(package_kind, &package.metadata)
        .map_err(into_command_error)?;

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
            // B8: distinguishable failure audit for the first-import path
            // (ADR-0045 §25 A45-05), written alongside the ImportRejected
            // import-audit event above.
            if let Some(fail_action) = failure_audit_action {
                let entity_type = fail_action.default_entity_type();
                let _ = AuditService::new(db.executor()).log_failure(
                    &session.user_id,
                    &session.username,
                    fail_action,
                    entity_type,
                    Some(&package_id),
                    reason.code(),
                    None,
                );
            }
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

#[cfg(test)]
mod security_requirement_tests {
    use super::*;
    use chrono::Utc;
    use crate::application::sync::{PackageId, SchemaVersion};
    use crate::application::usecases::exports::types::ProductsExportDataset;
    use crate::infrastructure::security::AgeFileEncryptionProvider;
    use crate::infrastructure::sync::packages::signing::HmacPackageSigner;
    use crate::infrastructure::sync::packages::{
        PackageBuilder, SerdeJsonSyncPackageDeserializer, SerdeJsonSyncPackageSerializer,
    };
    use crate::models::{Unit, UnitNodePackage, UserExport};

    fn metadata() -> SyncPackageMetadata {
        SyncPackageMetadata {
            created_at: Utc::now(),
            integrity_hash: Some("a".repeat(64)),
            package_sequence: Some(1),
            issuer_identity_id: Some(Uuid::new_v4()),
            package_id: PackageId(Uuid::new_v4().to_string()),
            schema_version: SchemaVersion::V2,
            signature: Some("sig".to_string()),
            signature_version: Some(SIGNATURE_VERSION_V2),
            signing_key_id: Some("default".to_string()),
            source_node_id: "wilaya-a".to_string(),
        }
    }

    fn assert_rejected(kind: &str, metadata: &SyncPackageMetadata) {
        let result = validate_import_security_requirements(kind, metadata);
        assert!(
            result.is_err(),
            "{kind} must be rejected, got Ok: {metadata:?}"
        );
    }

    fn assert_accepted(kind: &str, metadata: &SyncPackageMetadata) {
        let result = validate_import_security_requirements(kind, metadata);
        assert!(
            result.is_ok(),
            "{kind} must be accepted, got Err: {result:?}"
        );
    }

    // ── Security-critical kinds (identity_access, trust, registry) ─────────

    #[test]
    fn critical_kinds_require_complete_v2_metadata() {
        for kind in SECURITY_CRITICAL_KINDS {
            // Valid V2 → accepted.
            assert_accepted(kind, &metadata());

            // 1. unsigned → rejected
            let mut unsigned = metadata();
            unsigned.signature = None;
            assert_rejected(kind, &unsigned);

            // 2. signature_version = None → rejected
            let mut no_version = metadata();
            no_version.signature_version = None;
            assert_rejected(kind, &no_version);

            // 3. V1 → rejected
            let mut v1 = metadata();
            v1.signature_version = Some(1);
            assert_rejected(kind, &v1);

            // 4. missing issuer → rejected
            let mut no_issuer = metadata();
            no_issuer.issuer_identity_id = None;
            assert_rejected(kind, &no_issuer);

            // 5. missing package sequence → rejected
            let mut no_sequence = metadata();
            no_sequence.package_sequence = None;
            assert_rejected(kind, &no_sequence);

            // 6. missing integrity hash → rejected
            let mut no_hash = metadata();
            no_hash.integrity_hash = None;
            assert_rejected(kind, &no_hash);

            // 7. empty signature → rejected
            let mut empty_sig = metadata();
            empty_sig.signature = Some(String::new());
            assert_rejected(kind, &empty_sig);
        }
    }

    #[test]
    fn unknown_kind_is_not_required_to_be_v2() {
        // A package kind outside the known critical/data sets is untouched by
        // the gate (forward compatibility for new data kinds).
        assert_accepted("future_kind", &metadata());
    }

    // ── Data/report package kinds (compatibility window) ───────────────────

    #[test]
    fn data_kinds_require_signature_and_hash_but_accept_v1() {
        for kind in DATA_PACKAGE_KINDS {
            // Valid V2 → accepted.
            assert_accepted(kind, &metadata());

            // V1-with-signature → accepted during the compatibility window.
            let mut v1 = metadata();
            v1.signature_version = Some(1);
            v1.issuer_identity_id = None;
            v1.package_sequence = None;
            assert_accepted(kind, &v1);

            // unsigned → rejected
            let mut unsigned = metadata();
            unsigned.signature = None;
            assert_rejected(kind, &unsigned);

            // missing integrity hash → rejected
            let mut no_hash = metadata();
            no_hash.integrity_hash = None;
            assert_rejected(kind, &no_hash);
        }
    }

    // ── Regression: the generic legacy deserializer still parses V1 data ──

    #[test]
    fn legacy_v1_data_package_still_parses_and_passes_pipeline_policy() {
        let package: SyncPackage<ProductsExportDataset> = SyncPackage {
            metadata: SyncPackageMetadata {
                created_at: Utc::now(),
                integrity_hash: None,
                package_sequence: None,
                issuer_identity_id: None,
                package_id: PackageId(Uuid::new_v4().to_string()),
                schema_version: SchemaVersion::V1,
                signature: None,
                signature_version: None,
                signing_key_id: Some("default".to_string()),
                source_node_id: "unit-a".to_string(),
            },
            payload: ProductsExportDataset {
                product_rows: Vec::new(),
            },
        };

        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        PackageBuilder::new()
            .build_encrypted_stream_path(
                &package,
                &SerdeJsonSyncPackageSerializer,
                &HmacPackageSigner,
                &AgeFileEncryptionProvider::new(),
                &path,
            )
            .expect("V1 package builds");

        // Encrypted on disk — decrypt, then feed plaintext to the generic
        // legacy deserializer (the same shape the encrypted_package_reader
        // hands to it in production).
        let encrypted = std::fs::read(&path).unwrap();
        let plaintext = AgeFileEncryptionProvider::new()
            .decrypt_data(&encrypted)
            .expect("decrypts with the test/fallback key");

        let round_trip = SerdeJsonSyncPackageDeserializer::products_from_reader(
            std::io::BufReader::new(std::io::Cursor::new(plaintext)),
        )
        .expect("generic legacy deserializer still parses V1 fixtures");

        // The builder always fills hash + signature; version stays legacy.
        assert!(round_trip.metadata.signature.is_some());
        assert!(round_trip.metadata.integrity_hash.is_some());
        assert_eq!(round_trip.metadata.signature_version, None);

        // And the pipeline gate accepts it for the data kind (window open).
        validate_import_security_requirements(
            PRODUCTS_PACKAGE_KIND,
            &round_trip.metadata,
        )
        .expect("V1-with-signature data package accepted during compatibility window");
    }

    // ── SEC-003-05-D: `.unit` setup-mode import boundary ──────────────────

    fn unit_metadata() -> SyncPackageMetadata {
        SyncPackageMetadata {
            created_at: Utc::now(),
            integrity_hash: Some("a".repeat(64)),
            package_sequence: None,
            issuer_identity_id: None,
            package_id: PackageId(Uuid::new_v4().to_string()),
            schema_version: SchemaVersion::V1,
            signature: Some("sig".to_string()),
            signature_version: None,
            signing_key_id: Some("default".to_string()),
            source_node_id: "wilaya-a".to_string(),
        }
    }

    #[test]
    fn unsigned_unit_package_is_rejected() {
        let mut metadata = unit_metadata();
        metadata.signature = None;
        assert!(
            validate_unit_package_security_requirements(&metadata).is_err(),
            "unsigned .unit must be rejected"
        );
    }

    #[test]
    fn empty_signature_unit_package_is_rejected() {
        let mut metadata = unit_metadata();
        metadata.signature = Some(String::new());
        assert!(
            validate_unit_package_security_requirements(&metadata).is_err(),
            "empty-signature .unit must be rejected"
        );
    }

    #[test]
    fn unit_package_without_source_node_id_is_rejected() {
        let mut metadata = unit_metadata();
        metadata.source_node_id = String::new();
        assert!(
            validate_unit_package_security_requirements(&metadata).is_err(),
            ".unit without source_node_id must be rejected"
        );
    }

    #[test]
    fn signed_unit_package_with_source_is_accepted() {
        assert!(validate_unit_package_security_requirements(&unit_metadata()).is_ok());
    }

    #[test]
    fn legitimate_wilaya_exporter_unit_package_passes_boundary() {
        // Mirror the exact shape produced by export_unit_node_package:
        // V1/HMAC, signing_key_id = active key, source_node_id = WILAYA id,
        // no V2 issuer metadata — the signature is added by PackageBuilder and
        // verified by the deserializer's HMAC path before this boundary.
        let package: SyncPackage<UnitNodePackage> = SyncPackage {
            metadata: SyncPackageMetadata {
                created_at: Utc::now(),
                integrity_hash: None,
                package_sequence: None,
                issuer_identity_id: None,
                package_id: PackageId(Uuid::new_v4().to_string()),
                schema_version: SchemaVersion::V1,
                signature: None,
                signature_version: None,
                signing_key_id: Some("default".to_string()),
                source_node_id: "wilaya-a".to_string(),
            },
            payload: UnitNodePackage {
                unit: Unit {
                    id: "unit-a".into(),
                    code: "UA".into(),
                    name: "Unit A".into(),
                    wilaya_code: "16".into(),
                    user_id: None,
                    created_at: Utc::now(),
                },
                user: UserExport {
                    username: "op".into(),
                    password_hash: "hash".into(),
                    role: "User".into(),
                },
                unit_certificate: None,
                unit_private_key: None,
            },
        };

        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        PackageBuilder::new()
            .build_encrypted_stream_path(
                &package,
                &SerdeJsonSyncPackageSerializer,
                &HmacPackageSigner,
                &AgeFileEncryptionProvider::new(),
                &path,
            )
            .expect(".unit package builds");

        // Decrypt and run the same reader the importer uses (HMAC verified).
        let encrypted = std::fs::read(&path).unwrap();
        let plaintext = AgeFileEncryptionProvider::new()
            .decrypt_data(&encrypted)
            .expect("decrypts with the test/fallback key");
        let round_trip = SerdeJsonSyncPackageDeserializer::unit_node_package_from_reader(
            std::io::BufReader::new(std::io::Cursor::new(plaintext)),
        )
        .expect("legitimate .unit round-trips through the HMAC-verified reader");

        assert!(round_trip.metadata.signature.is_some());
        assert!(!round_trip.metadata.source_node_id.is_empty());

        validate_unit_package_security_requirements(&round_trip.metadata)
            .expect("legitimate .unit passes the setup-import boundary");
    }
}
