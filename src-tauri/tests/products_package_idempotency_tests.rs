//! Idempotency / replay-guard tests for products package imports.

use chrono::Utc;

use grpc_lib::application::sync::{
    PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::application::usecases::exports::types::ProductsExportDataset;
use grpc_lib::application::usecases::sync::import_products_package::{
    execute as apply_products_package, ImportProductsPackageInput, PRODUCTS_PACKAGE_KIND,
};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::errors::{AppError, BusinessLogicError};
use grpc_lib::infrastructure::db::sync_import::SqliteImportedPackageRegistry;
use grpc_lib::models::{NodeType, WilayaNodeConfiguration};
use grpc_lib::models::{Product, ProductExportRow};
use grpc_lib::repositories::SettingsRepository;

fn fixture_products_package(pkg_id: &str, source: &str) -> SyncPackage<ProductsExportDataset> {
    let p = Product {
        id: "p1".into(),
        name: "Prod 1".into(),
        base_price: 10.0,
        tva: 0.0,
        supplier_name: None,
        year: 2026,
        created_at: Utc::now(),
    };
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at: Utc::now(),
            source_node_id: source.into(),
            package_sequence: None,
            issuer_identity_id: None,
            package_id: PackageId(pkg_id.into()),
            signature_version: None,
            signing_key_id: None,
            integrity_hash: None,
            signature: None,
        },
        payload: ProductsExportDataset {
            product_rows: vec![ProductExportRow {
                product: p,
                updated_at: Utc::now().to_rfc3339(),
                node_id: "wilaya".into(),
                deleted: 0,
            }],
        },
    }
}

#[test]
fn products_package_same_id_is_rejected_second_time() {
    let mut db = ConnectionFactory::new_for_test().expect("db");

    // Configure node as UNIT-like context in settings: we only need wilaya_code for provenance checks.
    // SettingsRepository doesn't have a direct helper, so we set wilaya fields via wilaya config
    // and rely on the usecase's importer_wilaya_code input.
    SettingsRepository::new(db.executor())
        .configure_wilaya(&WilayaNodeConfiguration {
            node_type: NodeType::Wilaya,
            wilaya_code: Some("16".into()),
            wilaya_name: Some("Alger".into()),
        })
        .expect("configure wilaya");

    let pkg_id = "pkg-products-001";
    let package = fixture_products_package(pkg_id, "16");

    let input = ImportProductsPackageInput {
        package: package.clone(),
        importer_wilaya_code: "16".into(),
        imported_by: "admin".into(),
    };

    let source_for_reg = package.metadata.source_node_id.trim().to_string();

    db.with_transaction(|tx| {
        let src_opt = (!source_for_reg.is_empty()).then_some(source_for_reg.as_str());
        let reg = SqliteImportedPackageRegistry::new(tx, PRODUCTS_PACKAGE_KIND, src_opt, "admin", None, None);
        apply_products_package(tx, &reg, input.clone())
    })
    .expect("first ok");

    let err = db
        .with_transaction(|tx| {
            let src_opt = (!source_for_reg.is_empty()).then_some(source_for_reg.as_str());
            let reg =
                SqliteImportedPackageRegistry::new(tx, PRODUCTS_PACKAGE_KIND, src_opt, "admin", None, None);
            apply_products_package(tx, &reg, input)
        })
        .expect_err("second must fail");

    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage { .. })
    ));
}
