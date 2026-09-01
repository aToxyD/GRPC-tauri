//! Roundtrip + validation tests for additional sync package kinds.
//!
//! SEC-007 (ADR-0047): fixtures are V2-only (Ed25519). Legacy V1 shape is
//! rejected by the deserializer and the import-validators.

use chrono::{NaiveDate, TimeZone, Utc};

use grpc_lib::application::sync::{
    validate_daily_report_package_for_import, validate_products_package_for_import, PackageId,
    SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::application::usecases::exports::types::{
    DailyReportExportDataset, ProductsExportDataset,
};
use grpc_lib::infrastructure::sync::packages::signing::{
    Ed25519PackageSigner, DEFAULT_SIGNATURE_VERSION,
};
use grpc_lib::infrastructure::sync::{
    read_daily_report_package_from_file, read_products_package_from_file,
};
use grpc_lib::models::{
    DailyConsumptionSyncLine, DailyReportSyncSnapshot, MealSectionSyncSnapshot, MealType, Product,
    ProductExportRow,
};

// Fixed test signing key (RFC 8032 — deterministic for fixed secret).
fn v2_signer() -> Ed25519PackageSigner {
    Ed25519PackageSigner::new([0u8; 32])
}

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
            issuer_identity_id: None,
            package_id: PackageId("pkg-products-rt-1".into()),
            signature_version: Some(DEFAULT_SIGNATURE_VERSION),
            signing_key_id: Some(v2_signer().public_key_hex()),
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
            issuer_identity_id: None,
            package_id: PackageId("pkg-daily-rt-1".into()),
            signature_version: Some(DEFAULT_SIGNATURE_VERSION),
            signing_key_id: Some(v2_signer().public_key_hex()),
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
    let signer = v2_signer();
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
    let signer = v2_signer();
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
fn v1_shaped_package_is_rejected_by_deserializer() {
    // SEC-007 (ADR-0047): the generic V1 shape (no signature_version, no
    // integrity_hash, no signature) must fail closed on the read path.
    let pkg = SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at: Utc.with_ymd_and_hms(2026, 2, 1, 10, 0, 0).unwrap(),
            source_node_id: "16".into(),
            issuer_identity_id: None,
            package_id: PackageId("pkg-v1-rt-1".into()),
            signature_version: None,
            signing_key_id: Some("default".into()),
            integrity_hash: None,
            signature: None,
        },
        payload: ProductsExportDataset {
            product_rows: Vec::new(),
        },
    };
    let plaintext = serde_json::to_vec(&pkg).expect("serialize");
    let err =
        grpc_lib::infrastructure::sync::SerdeJsonSyncPackageDeserializer::products_from_reader(
            std::io::BufReader::new(std::io::Cursor::new(plaintext)),
        )
        .expect_err("V1 legacy package must be rejected");
    assert!(matches!(err, grpc_lib::errors::AppError::Validation(_)));
}
