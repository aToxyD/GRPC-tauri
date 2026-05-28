//! Apply decrypted stock movements interchange package on Wilaya node.

use crate::application::services::SyncImportExecutionService;
use crate::application::sync::{source_id_allowed_for_unit, ImportedPackageRegistry, SyncPackage};
use crate::application::usecases::exports::types::StockMovementsExportDataset;
use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
use crate::repositories::{DbExecutor, RepositoryProvider};

pub const STOCK_MOVEMENTS_PACKAGE_KIND: &str = "stock_movements";

#[derive(Debug, Clone)]
pub struct ImportStockMovementsPackageInput {
    pub package: SyncPackage<StockMovementsExportDataset>,
    pub unit_id: String,
    pub importer_wilaya_code: String,
    pub imported_by: String,
}

#[derive(Debug, Clone)]
pub struct ImportStockMovementsPackageOutcome {
    pub movement_count: usize,
    pub unit_id: String,
    pub package_id: String,
}

pub fn execute(
    executor: DbExecutor<'_>,
    registry: &impl ImportedPackageRegistry,
    input: ImportStockMovementsPackageInput,
) -> AppResult<ImportStockMovementsPackageOutcome> {
    let unit_trim = input.unit_id.trim();
    let importer_wilaya_trim = input.importer_wilaya_code.trim();

    if unit_trim.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "unit_id".into(),
        }));
    }
    if importer_wilaya_trim.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "wilaya_code".into(),
        }));
    }

    // 1. Get Unit Info
    let unit_row = executor.units().get_unit(unit_trim)?.ok_or_else(|| {
        AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
            resource: "الوحدة".into(),
            id: unit_trim.to_string(),
        })
    })?;

    // 2. Cross-Wilaya check
    if unit_row.wilaya_code != importer_wilaya_trim {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::CrossWilayaForbidden {
                resource: "حركة_مخزون_sync".into(),
                from_wilaya: unit_row.wilaya_code.clone(),
                to_wilaya: importer_wilaya_trim.to_string(),
            },
        ));
    }

    // 3. Source Node Authorization check
    if !source_id_allowed_for_unit(
        input.package.metadata.source_node_id.as_str(),
        &unit_row,
        importer_wilaya_trim,
    ) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "source_node_id".into(),
            message: format!(
                "مصدر الحزمة «{}» لا يطابق الوحدة المختارة ولا ولاية المستورد",
                input.package.metadata.source_node_id.trim()
            ),
        }));
    }

    // 4. Idempotency check
    let package_id = input.package.metadata.package_id.clone();
    if registry.has_imported(&package_id)? {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::DuplicateSyncPackage {
                package_id: package_id.0.clone(),
            },
        ));
    }

    // 5. Perform import
    let movement_count = SyncImportExecutionService::new(executor)
        .import_stock_movements(input.package.payload.movements, Some(unit_trim))?;

    // 6. Mark as imported
    registry.mark_imported(&package_id)?;

    Ok(ImportStockMovementsPackageOutcome {
        movement_count,
        unit_id: unit_trim.to_string(),
        package_id: package_id.0.clone(),
    })
}
