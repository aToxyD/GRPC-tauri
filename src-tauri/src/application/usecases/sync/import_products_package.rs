//! Apply a decrypted products interchange package (Wilaya → Unit).

use crate::application::services::SyncImportExecutionService;
use crate::application::sync::{
    products_source_allowed_for_unit, validate_products_package_for_import,
    ImportedPackageRegistry, SyncPackage,
};
use crate::application::usecases::exports::types::ProductsExportDataset;
use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
use crate::models::ProductSyncRecord;
use crate::repositories::DbExecutor;

pub const PRODUCTS_PACKAGE_KIND: &str = "products";

#[derive(Debug, Clone)]
pub struct ImportProductsPackageInput {
    pub package: SyncPackage<ProductsExportDataset>,
    pub importer_wilaya_code: String,
    pub imported_by: String,
}

#[derive(Debug, Clone)]
pub struct ImportProductsPackageOutcome {
    pub imported: usize,
    pub updated: usize,
    pub skipped: usize,
    pub package_id: String,
}

pub fn execute(
    executor: DbExecutor<'_>,
    registry: &impl ImportedPackageRegistry,
    input: ImportProductsPackageInput,
) -> AppResult<ImportProductsPackageOutcome> {
    validate_products_package_for_import(&input.package)?;

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

    let records: Vec<ProductSyncRecord> = input
        .package
        .payload
        .product_rows
        .into_iter()
        .map(|r| ProductSyncRecord {
            id: r.product.id,
            name: r.product.name,
            base_price: r.product.base_price,
            year: r.product.year,
            created_at: r.product.created_at.to_rfc3339(),
            updated_at: r.updated_at,
            node_id: r.node_id,
            deleted: r.deleted,
            // SEC-087 Phase 6B: the record carries the V3 configuration just
            // validated by validate_products_package_for_import (called at the
            // top of this function) — these are Some and mutually consistent.
            purchase_unit: r.purchase_unit,
            consumption_unit: r.consumption_unit,
            conversion_factor: r.conversion_factor,
            tva_classification: r.tva_classification,
        })
        .collect();

    let (imported, updated, skipped) =
        SyncImportExecutionService::new(executor).import_products_sync(&records, 0, "unused")?;
    registry.mark_imported(&package_id)?;

    Ok(ImportProductsPackageOutcome {
        imported,
        updated,
        skipped,
        package_id: package_id.0.clone(),
    })
}
