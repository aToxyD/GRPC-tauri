//! Roundtrip + validation tests for additional sync package kinds.
//!
//! SEC-007 (ADR-0047): fixtures are V2-only (Ed25519). Legacy V1 shape is
//! rejected by the deserializer and the import-validators.

use chrono::{NaiveDate, TimeZone, Utc};

use grpc_lib::application::sync::{
    validate_contract_catalog_package_for_import, validate_daily_report_package_for_import,
    validate_products_package_for_import, PackageId, SyncPackage, SyncPackageMetadata,
    SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::application::usecases::exports::types::{
    ContractCatalogAllocationRow, ContractCatalogContractRow, ContractCatalogExportDataset,
    ContractCatalogProductLine, ContractCatalogUnitSupplierLink, DailyReportExportDataset,
    ProductsExportDataset,
};
use grpc_lib::infrastructure::sync::packages::signing::{
    Ed25519PackageSigner, DEFAULT_SIGNATURE_VERSION,
};
use grpc_lib::infrastructure::sync::{
    read_contract_catalog_package_from_file, read_daily_report_package_from_file,
    read_products_package_from_file,
};
use grpc_lib::models::{
    Contract, ContractAllocation, ContractAllocationException, ContractStatus,
    DailyConsumptionSyncLine, DailyReportSyncSnapshot, FiscalYearTaxPolicy,
    MealSectionSyncSnapshot, MealType, Product, ProductExportRow, Supplier,
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

fn fixture_contract_catalog_package() -> SyncPackage<ContractCatalogExportDataset> {
    let created_at = Utc.with_ymd_and_hms(2026, 2, 1, 10, 0, 0).unwrap();
    let supplier = Supplier {
        id: "sup-1".into(),
        name: "Fournisseur A".into(),
        contact_info: Some("contact".into()),
        active: true,
        created_at,
    };
    let contract = Contract {
        id: "ctr-1".into(),
        contract_reference: "REF-2026-001".into(),
        unit_id: "unit-alpha".into(),
        supplier_id: "sup-1".into(),
        fiscal_year: 2026,
        status: ContractStatus::Active,
        proposed_at: Some("2026-01-01T08:00:00Z".into()),
        accepted_at: Some("2026-01-02T08:00:00Z".into()),
        activated_at: Some("2026-01-03T08:00:00Z".into()),
        ended_at: None,
        cancelled_at: None,
        notes: None,
        created_at: Some(created_at),
    };
    let allocation = ContractAllocation {
        id: "alloc-1".into(),
        contract_id: "ctr-1".into(),
        contract_product_id: "ctr-prod-1".into(),
        unit_id: "unit-alpha".into(),
        product_id: "prod-1".into(),
        fiscal_year: 2026,
        contracted_quantity: 100.0,
        fulfilled_quantity: 20.0,
        released_quantity: 0.0,
        reserved_quantity: 0.0,
        entitlement_state: "open".into(),
        version: 1,
    };
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at,
            source_node_id: "16".into(),
            issuer_identity_id: None,
            package_id: PackageId("pkg-contract-catalog-rt-1".into()),
            signature_version: Some(DEFAULT_SIGNATURE_VERSION),
            signing_key_id: Some(v2_signer().public_key_hex()),
            integrity_hash: None,
            signature: None,
        },
        payload: ContractCatalogExportDataset {
            suppliers: vec![supplier],
            unit_supplier_links: vec![ContractCatalogUnitSupplierLink {
                unit_id: "unit-alpha".into(),
                supplier_id: "sup-1".into(),
            }],
            contracts: vec![ContractCatalogContractRow {
                contract,
                product_lines: vec![ContractCatalogProductLine {
                    id: "ctr-prod-1".into(),
                    contract_id: "ctr-1".into(),
                    product_id: "prod-1".into(),
                    proposed_price_ht: 120.0,
                    agreed_price_ht: Some(110.0),
                    tva_classification: Some(0),
                    tva_rate: Some(0.0),
                    tva_amount: Some(0.0),
                    price_ttc: Some(110.0),
                    purchase_unit: Some(1),
                    consumption_unit: Some(1),
                    conversion_factor: Some(1),
                    created_at: created_at.to_rfc3339(),
                }],
                allocations: vec![ContractCatalogAllocationRow {
                    allocation,
                    created_at: created_at.to_rfc3339(),
                }],
                exceptions: vec![ContractAllocationException {
                    id: "exc-1".into(),
                    allocation_id: "alloc-1".into(),
                    released_quantity: 5.0,
                    reason_code: "SUPPLIER_DELAY".into(),
                    reason_note: None,
                    created_by: "wilaya-admin".into(),
                    created_at: created_at.to_rfc3339(),
                }],
            }],
            tax_policies: vec![FiscalYearTaxPolicy {
                fiscal_year: 2026,
                tva_rate: 19.0,
                frozen: false,
                set_by: "wilaya-admin".into(),
                created_at: created_at.to_rfc3339(),
            }],
        },
    }
}

#[test]
fn contract_catalog_package_roundtrip_and_validation() {
    let pkg = fixture_contract_catalog_package();
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let file_path = temp_dir.path().join("contract_catalog.sync");

    let crypto_port = grpc_lib::infrastructure::security::AgeFileEncryptionProvider;
    let serializer = grpc_lib::infrastructure::sync::SerdeJsonSyncPackageSerializer;
    let signer = v2_signer();
    let builder = grpc_lib::infrastructure::sync::PackageBuilder::new();

    builder
        .build_encrypted_stream_path(&pkg, &serializer, &signer, &crypto_port, &file_path)
        .expect("build package");

    let decoded =
        read_contract_catalog_package_from_file(&file_path, &crypto_port).expect("read+decode");
    validate_contract_catalog_package_for_import(&decoded).expect("validate");
    assert_eq!(decoded.payload.contracts.len(), 1);
    assert_eq!(decoded.payload.contracts[0].allocations.len(), 1);
    assert_eq!(decoded.payload.contracts[0].exceptions.len(), 1);
    assert_eq!(decoded.payload.suppliers.len(), 1);
    assert_eq!(decoded.payload.tax_policies.len(), 1);
}

#[test]
fn contract_catalog_package_without_contracts_is_rejected() {
    // Fail-closed: a catalog with no contracts carries no projection.
    let mut pkg = fixture_contract_catalog_package();
    pkg.payload.contracts.clear();
    let err = validate_contract_catalog_package_for_import(&pkg)
        .expect_err("empty catalog must be rejected");
    assert!(matches!(err, grpc_lib::errors::AppError::Validation(_)));
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
