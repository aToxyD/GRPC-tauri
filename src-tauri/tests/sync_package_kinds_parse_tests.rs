//! Roundtrip + validation tests for additional sync package kinds.

use std::path::Path;

use chrono::{NaiveDate, TimeZone, Utc};

use grpc_lib::application::sync::{
    validate_daily_report_package_for_import, validate_products_package_for_import, PackageId,
    SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::application::usecases::exports::types::{
    DailyReportExportDataset, ProductsExportDataset,
};
use grpc_lib::infrastructure::sync::{
    read_daily_report_package_from_file, read_products_package_from_file,
};
use grpc_lib::models::{
    DailyConsumptionSyncLine, DailyReportSyncSnapshot, MealSectionSyncSnapshot, MealType, Product,
    ProductExportRow,
};

fn fixture_products_package() -> SyncPackage<ProductsExportDataset> {
    let created_at = Utc.with_ymd_and_hms(2026, 2, 1, 10, 0, 0).unwrap();
    let updated_at = Utc.with_ymd_and_hms(2026, 2, 1, 10, 5, 0).unwrap();
    let product_created_at = Utc.with_ymd_and_hms(2026, 1, 1, 8, 0, 0).unwrap();
    let p = Product {
        id: "prod-1".into(),
        name: "Item A".into(),
        base_price: 10.0,
        tva: 19.0,
        supplier_name: Some("Supp".into()),
        year: 2026,
        created_at: product_created_at,
    };
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at,
            source_node_id: "16".into(),
            package_sequence: None,
            issuer_identity_id: None,
            package_id: PackageId("pkg-products-rt-1".into()),
            signature_version: None,
            signing_key_id: None,
            integrity_hash: None,
            signature: None,
        },
        payload: ProductsExportDataset {
            product_rows: vec![ProductExportRow {
                product: p,
                updated_at: updated_at.to_rfc3339(),
                node_id: "16".into(),
                deleted: 0,
            }],
        },
    }
}

fn fixture_daily_package() -> SyncPackage<DailyReportExportDataset> {
    let created_at = Utc.with_ymd_and_hms(2026, 2, 10, 12, 0, 0).unwrap();
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at,
            source_node_id: "unit-alpha".into(),
            package_sequence: None,
            issuer_identity_id: None,
            package_id: PackageId("pkg-daily-rt-1".into()),
            signature_version: None,
            signing_key_id: None,
            integrity_hash: None,
            signature: None,
        },
        payload: DailyReportExportDataset {
            snapshot: DailyReportSyncSnapshot {
                report_id: "rep-1".into(),
                date: NaiveDate::from_ymd_opt(2026, 2, 10).unwrap(),
                total_daily_cost: 120.0,
                total_daily_average: 10.0,
                total_daily_beneficiaries: 12,
                meals: vec![MealSectionSyncSnapshot {
                    meal_type: MealType::Breakfast,
                    staff_24h_count: 5,
                    staff_8h_count: 3,
                    reservation_count: 1,
                    mission_count: 1,
                    guest_count: 2,
                    total_beneficiaries: 12,
                    total_meal_cost: 120.0,
                    meal_average: 10.0,
                    items: vec![DailyConsumptionSyncLine {
                        product_id: "prod-1".into(),
                        product_name: "Item A".into(),
                        quantity: 1.0,
                        unit_price: 10.0,
                        total_cost: 10.0,
                    }],
                }],
            },
        },
    }
}

#[test]
fn products_package_roundtrip_and_validation() {
    let pkg = fixture_products_package();
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let file_path = temp_dir.path().join("products.sync");

    let crypto_port = grpc_lib::infrastructure::security::AgeFileEncryptionProvider;
    let serializer = grpc_lib::infrastructure::sync::SerdeJsonSyncPackageSerializer;
    let signer = grpc_lib::infrastructure::sync::HmacPackageSigner;
    let builder = grpc_lib::infrastructure::sync::PackageBuilder::new();

    builder
        .build_encrypted_stream_path(&pkg, &serializer, &signer, &crypto_port, &file_path)
        .expect("build package");

    let decoded = read_products_package_from_file(&file_path, &crypto_port).expect("read+decode");
    validate_products_package_for_import(&decoded).expect("validate");
    assert_eq!(decoded.payload.product_rows.len(), 1);
}

#[test]
fn daily_package_roundtrip_and_validation() {
    let pkg = fixture_daily_package();
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let file_path = temp_dir.path().join("daily.sync");

    let crypto_port = grpc_lib::infrastructure::security::AgeFileEncryptionProvider;
    let serializer = grpc_lib::infrastructure::sync::SerdeJsonSyncPackageSerializer;
    let signer = grpc_lib::infrastructure::sync::HmacPackageSigner;
    let builder = grpc_lib::infrastructure::sync::PackageBuilder::new();

    builder
        .build_encrypted_stream_path(&pkg, &serializer, &signer, &crypto_port, &file_path)
        .expect("build package");

    let decoded =
        read_daily_report_package_from_file(&file_path, &crypto_port).expect("read+decode");
    validate_daily_report_package_for_import(&decoded).expect("validate");
    assert_eq!(decoded.payload.snapshot.meals.len(), 1);
    assert_eq!(decoded.payload.snapshot.meals[0].items.len(), 1);
}

#[test]
fn products_package_json_matches_golden() {
    let pkg = fixture_products_package();
    let actual = serde_json::to_value(&pkg).expect("value");
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots/sync/products_package_plain.json");
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&p).expect("golden")).expect("json");
    assert_eq!(actual, expected);
}

#[test]
fn daily_package_json_matches_golden() {
    let pkg = fixture_daily_package();
    let actual = serde_json::to_value(&pkg).expect("value");
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots/sync/daily_report_package_plain.json");
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&p).expect("golden")).expect("json");
    assert_eq!(actual, expected);
}
