//! Apply a decrypted ContractCatalog interchange package (WILAYA → UNIT,
//! ADR-0055 / SEC-087-F).

use crate::application::services::SettingsService;
use crate::application::services::SyncImportExecutionService;
use crate::application::sync::{
    products_source_allowed_for_unit, validate_contract_catalog_package_for_import,
    ImportedPackageRegistry, PackageExportMode, SyncPackage, SyncPackageMetadata,
};
use crate::application::usecases::exports::types::ContractCatalogExportDataset;
use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
use crate::models::NodeType;
use crate::repositories::{DbExecutor, UnitRepository};

pub const CONTRACT_CATALOG_PACKAGE_KIND: &str = "contract_catalog";

#[derive(Debug, Clone)]
pub struct ImportContractCatalogPackageInput {
    pub package: SyncPackage<ContractCatalogExportDataset>,
    pub importer_wilaya_code: String,
    pub imported_by: String,
}

#[derive(Debug, Clone)]
pub struct ImportContractCatalogPackageOutcome {
    pub imported: usize,
    pub updated: usize,
    pub skipped: usize,
    pub package_id: String,
}

/// The importing node's own unit identity, carrying both the canonical
/// `units.id` (row scoping) and the authoritative `units.code` (target
/// binding). WILAYA importers carry no unit scope.
#[derive(Debug, Clone)]
struct ImporterUnit {
    id: String,
    code: String,
}

/// The importing node's own unit identity.
///
/// UNIT importers scope the projection to their own unit row (FKs target the
/// local `units` table, which holds exactly the node's own unit) and expose
/// the authoritative `units.code` for SEC-087 C4 target binding. WILAYA
/// importers carry no unit scope — they apply the full catalog (restore path).
fn resolve_importer_unit(executor: DbExecutor<'_>) -> AppResult<Option<ImporterUnit>> {
    let settings = SettingsService::new(executor).get_settings()?;
    let unit_code = match settings.node_type {
        NodeType::Unit => settings.unit_code.as_deref().ok_or_else(|| {
            AppError::Validation(ValidationError::Required {
                field: "unit_code".into(),
            })
        })?,
        NodeType::Wilaya => return Ok(None),
    };
    let unit = UnitRepository::new(executor)
        .get_unit_by_code(unit_code)?
        .ok_or_else(|| {
            AppError::Validation(ValidationError::InvalidFormat {
                field: "unit_code".into(),
                message: format!("لا توجد وحدة محلية بالرمز «{unit_code}»"),
            })
        })?;
    Ok(Some(ImporterUnit {
        id: unit.id,
        code: unit.code,
    }))
}

/// SEC-087 Phase 2 — C4: Contract Catalog target binding (ADR-0059 §13–15).
///
/// Enforces the authenticated `export_mode` / `target_node_id` matrix against
/// the importer's own node before any database mutation or replay marking.
/// Only authenticated metadata is interpreted (the payload is integrity-bound
/// and Ed25519-signed upstream); the retired `SyncImportRequest.target_node_id`
/// is never consulted. Matching uses EXACT string equality against the
/// authoritative local `units.code` — no trimming, normalization, or case
/// folding.
///
/// Matrix:
/// - `export_mode = None`            → REJECT (legacy targetless contract
///   catalog packages are rejected; no inferred mode fallback).
/// - `FleetRestore` + no target      → ACCEPT only on WILAYA, REJECT on UNIT.
/// - `FleetRestore` + target         → REJECT everywhere.
/// - `UnitDistribution` + target     → ACCEPT only on a UNIT whose exact
///   `units.code` equals the package target; REJECT on WILAYA and on any
///   other UNIT.
/// - `UnitDistribution` + no target  → REJECT.
fn validate_contract_catalog_target_for_import(
    metadata: &SyncPackageMetadata,
    importer_unit: &Option<ImporterUnit>,
) -> AppResult<()> {
    let mode = metadata.export_mode;
    match mode {
        None => {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "export_mode".into(),
                message: "الحزمة لا تحمل نمط تصدير موثّقًا — كتالوج العقود غير موجّه مرفوض".into(),
            }));
        }
        Some(PackageExportMode::FleetRestore) => {
            if metadata.target_node_id.is_some() {
                return Err(AppError::Validation(ValidationError::InvalidFormat {
                    field: "target_node_id".into(),
                    message: "نمط استعادة الأسطول غير قابل للتوجيه إلى وحدة".into(),
                }));
            }
            if importer_unit.is_some() {
                return Err(AppError::Validation(ValidationError::InvalidFormat {
                    field: "export_mode".into(),
                    message: "لا يمكن للوحدة استيراد كتالوج استعادة الأسطول".into(),
                }));
            }
        }
        Some(PackageExportMode::UnitDistribution) => {
            let local_code = match importer_unit.as_ref().map(|u| u.code.as_str()) {
                None => {
                    return Err(AppError::Validation(ValidationError::InvalidFormat {
                        field: "target_node_id".into(),
                        message: "لا يمكن للولاية استيراد كتالوج موزّع على وحدة".into(),
                    }));
                }
                Some(code) => code,
            };
            let package_target = metadata.target_node_id.as_deref().ok_or_else(|| {
                AppError::Validation(ValidationError::InvalidFormat {
                    field: "target_node_id".into(),
                    message: "نمط التوزيع على الوحدة يتطلب وحدة مستهدفة معلّنة".into(),
                })
            })?;
            if package_target != local_code {
                return Err(AppError::Validation(ValidationError::InvalidFormat {
                    field: "target_node_id".into(),
                    message: "هدف الحزمة لا يطابق الوحدة المحلية".into(),
                }));
            }
        }
    }
    Ok(())
}

pub fn execute(
    executor: DbExecutor<'_>,
    registry: &impl ImportedPackageRegistry,
    input: ImportContractCatalogPackageInput,
) -> AppResult<ImportContractCatalogPackageOutcome> {
    validate_contract_catalog_package_for_import(&input.package)?;

    let importer_wilaya = input.importer_wilaya_code.trim();
    if importer_wilaya.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "wilaya_code".into(),
        }));
    }

    if !products_source_allowed_for_unit(importer_wilaya, &input.package.metadata.source_node_id) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "source_node_id".into(),
            message: format!(
                "مصدر الحزمة «{}» غير مطابق لرمز الولاية «{}»",
                input.package.metadata.source_node_id.trim(),
                importer_wilaya
            ),
        }));
    }

    let importer_unit = resolve_importer_unit(executor)?;

    // SEC-087 Phase 2 — C4 (ADR-0059 §13–15): target binding is enforced
    // AFTER structural validation and BEFORE replay marking, dataset
    // application, or any business mutation. Rejected packages consume no
    // replay slot and write no catalog rows.
    validate_contract_catalog_target_for_import(&input.package.metadata, &importer_unit)?;

    let package_id = input.package.metadata.package_id.clone();
    if registry.has_imported(&package_id)? {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::DuplicateSyncPackage {
                package_id: package_id.0.clone(),
            },
        ));
    }

    let summary = SyncImportExecutionService::new(executor).import_contract_catalog_sync(
        &input.package.payload,
        importer_unit.as_ref().map(|u| u.id.as_str()),
    )?;
    registry.mark_imported(&package_id)?;

    let imported = summary.suppliers_imported
        + summary.contracts_imported
        + summary.product_lines_imported
        + summary.allocations_imported
        + summary.exceptions_imported
        + summary.links_applied
        + summary.tax_policies_applied;
    let updated = summary.suppliers_updated
        + summary.contracts_updated
        + summary.product_lines_updated
        + summary.allocations_updated;
    let skipped = summary.suppliers_skipped
        + summary.links_skipped
        + summary.contracts_skipped
        + summary.allocations_skipped
        + summary.exceptions_skipped;

    Ok(ImportContractCatalogPackageOutcome {
        imported,
        updated,
        skipped,
        package_id: package_id.0.clone(),
    })
}
