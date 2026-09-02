//! Apply a decrypted ContractCatalog interchange package (WILAYA → UNIT,
//! ADR-0055 / SEC-087-F).

use crate::application::services::SettingsService;
use crate::application::services::SyncImportExecutionService;
use crate::application::sync::{
    products_source_allowed_for_unit, validate_contract_catalog_package_for_import,
    ImportedPackageRegistry, SyncPackage,
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

/// The importing node's own unit identity.
///
/// UNIT importers scope the projection to their own unit row (FKs target the
/// local `units` table, which holds exactly the node's own unit). WILAYA
/// importers carry no unit scope — they apply the full catalog (restore path).
fn resolve_importer_unit_id(executor: DbExecutor<'_>) -> AppResult<Option<String>> {
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
    Ok(Some(unit.id))
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

    let package_id = input.package.metadata.package_id.clone();
    if registry.has_imported(&package_id)? {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::DuplicateSyncPackage {
                package_id: package_id.0.clone(),
            },
        ));
    }

    let importer_unit_id = resolve_importer_unit_id(executor)?;

    let summary = SyncImportExecutionService::new(executor)
        .import_contract_catalog_sync(&input.package.payload, importer_unit_id.as_deref())?;
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
