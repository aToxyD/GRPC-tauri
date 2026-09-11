use crate::application::usecases::exports::types::{ExportProductsInput, ProductsExportDataset};
use crate::errors::AppResult;
use crate::repositories::{DbExecutor, ProductRepository};

pub fn execute<'a>(
    executor: DbExecutor<'a>,
    _input: ExportProductsInput,
) -> AppResult<ProductsExportDataset> {
    let product_repo = ProductRepository::new(executor);
    let products = product_repo.list_products()?;

    let mut product_rows = Vec::with_capacity(products.len());
    for p in products {
        let (updated_at_val, node_id_val, deleted_val) = product_repo.get_sync_info(&p.id)?;
        // SEC-087 Phase 6B (ADR-0057 §3.4): the exporter emits the
        // WILAYA-authoritative unit/TVA configuration wire codes. Rows whose
        // local configuration is absent (legacy NULLs) export `None` and are
        // rejected fail-closed by the importer's semantic validator — config
        // is never fabricated or normalized at export time.
        let codes = product_repo.get_product_sync_config(&p.id)?.unwrap_or(
            crate::models::ProductUnitConfigCodes {
                purchase_unit: None,
                consumption_unit: None,
                conversion_factor: None,
                tva_classification: None,
            },
        );
        product_rows.push(crate::models::ProductExportRow {
            updated_at: updated_at_val.unwrap_or_else(|| p.created_at.to_rfc3339()),
            node_id: node_id_val.unwrap_or_else(|| "legacy".to_string()),
            deleted: deleted_val.unwrap_or(0),
            purchase_unit: codes.purchase_unit,
            consumption_unit: codes.consumption_unit,
            conversion_factor: codes.conversion_factor,
            tva_classification: codes.tva_classification,
            product: p,
        });
    }

    Ok(ProductsExportDataset { product_rows })
}
