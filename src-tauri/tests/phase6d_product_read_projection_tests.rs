//! SEC-087 Phase 6D — local `ProductRead` read projection.
//!
//! Pins the Phase 6D boundary:
//!   - `ProductRead` is a LOCAL read-projection DTO feeding the `get_product` /
//!     `list_products` IPC commands. It must return the persisted unit/TVA
//!     configuration codes.
//!   - The V3 sync export boundary MUST remain unchanged: the exporter keeps
//!     consuming the config-free `Product` via `ProductRepository::list_products()`
//!     and embedding it in `ProductExportRow`. It MUST NOT start depending on
//!     `ProductRead`.

use grpc_lib::application::services::ProductService;
use grpc_lib::application::usecases::exports::export_products_dataset;
use grpc_lib::application::usecases::exports::types::{ExportProductsInput, ProductsExportDataset};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::models::{CreateProductRequest, Product};
use grpc_lib::repositories::RepositoryProvider;

/// Create a configured product and return its persisted config codes.
fn create_configured_product(db: &grpc_lib::db::Database, name: &str) -> String {
    ProductService::new(db.executor())
        .create_product(
            &CreateProductRequest {
                name: name.to_string(),
                base_price: 250.0,
                purchase_unit: Some(1),
                consumption_unit: Some(3),
                conversion_factor: Some(4),
                tva_classification: Some(1),
            },
            2026,
        )
        .expect("configured product creation must succeed")
}

#[test]
fn product_read_returns_persisted_config_codes() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let id = create_configured_product(&db, "Projection");

    let read = ProductService::new(db.executor())
        .get_product_read(&id)
        .expect("read must succeed")
        .expect("product must exist");
    assert_eq!(read.id, id);
    assert_eq!(read.name, "Projection");
    assert_eq!(read.purchase_unit, 1);
    assert_eq!(read.consumption_unit, 3);
    assert_eq!(read.conversion_factor, 4);
    assert_eq!(read.tva_classification, 1);

    let listed = ProductService::new(db.executor())
        .list_products_read()
        .expect("list must succeed");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].purchase_unit, 1);
    assert_eq!(listed[0].consumption_unit, 3);
    assert_eq!(listed[0].conversion_factor, 4);
    assert_eq!(listed[0].tva_classification, 1);
}

/// The V3 sync export boundary: `list_products()` -> `ProductExportRow` must
/// keep embedding the config-free [`Product`] and must not start depending on
/// `ProductRead`. The compile-time annotation on `row.product` guards against
/// a regression where the exporter would start emitting `ProductRead`.
#[test]
fn sync_export_boundary_stays_on_config_free_product() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    create_configured_product(&db, "Boundary");

    // The exporter still consumes the legacy config-free read.
    let legacy: Vec<Product> = db
        .executor()
        .products()
        .list_products()
        .expect("legacy list must succeed");
    assert_eq!(legacy.len(), 1);

    // Full production export path (WILAYA exporter).
    let dataset: ProductsExportDataset =
        export_products_dataset::execute(db.executor(), ExportProductsInput)
            .expect("export must succeed");
    assert_eq!(dataset.product_rows.len(), 1);

    let row = &dataset.product_rows[0];
    // COMPILE-TIME BOUNDARY GUARD: the embedded product stays the config-free
    // `Product` DTO. If it were ever changed to `ProductRead`, this assignment
    // stops compiling and CI fails.
    let embedded: &Product = &row.product;
    assert_eq!(embedded.name, "Boundary");
    // The config is carried at the row level (the V3 payload shape), never
    // inside the embedded product DTO.
    assert_eq!(row.purchase_unit, Some(1));
    assert_eq!(row.consumption_unit, Some(3));
    assert_eq!(row.conversion_factor, Some(4));
    assert_eq!(row.tva_classification, Some(1));
}
