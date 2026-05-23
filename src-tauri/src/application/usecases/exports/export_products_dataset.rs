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
        product_rows.push(crate::models::ProductExportRow {
            updated_at: updated_at_val.unwrap_or_else(|| p.created_at.to_rfc3339()),
            node_id: node_id_val.unwrap_or_else(|| "legacy".to_string()),
            deleted: deleted_val.unwrap_or(0),
            product: p,
        });
    }

    Ok(ProductsExportDataset { product_rows })
}
