//! End-to-end ContractCatalog import tests (ADR-0055 / SEC-087-F).
//!
//! Covers:
//! - WILAYA full-catalog apply (restore path)
//! - UNIT-scoped apply (only the importer's own unit rows are written)
//! - WILAYA-owned upsert semantics (UNIT-owned runtime state preserved)
//! - Replay rejection via the imported-package registry
//! - Fail-closed source provenance

use chrono::{TimeZone, Utc};

use grpc_lib::application::sync::{
    PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::application::usecases::exports::types::{
    ContractCatalogAllocationRow, ContractCatalogContractRow, ContractCatalogExportDataset,
    ContractCatalogProductLine, ContractCatalogUnitSupplierLink,
};
use grpc_lib::application::usecases::sync::import_contract_catalog_package::{
    execute as apply_contract_catalog_package, ImportContractCatalogPackageInput,
    ImportContractCatalogPackageOutcome, CONTRACT_CATALOG_PACKAGE_KIND,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::errors::{AppError, BusinessLogicError};
use grpc_lib::infrastructure::db::sync_import::SqliteImportedPackageRegistry;
use grpc_lib::models::{
    Contract, ContractAllocation, ContractAllocationException, ContractStatus, FiscalYearTaxPolicy,
    NodeType, Product, Supplier, WilayaNodeConfiguration,
};
use grpc_lib::repositories::{
    ContractRepository, FiscalYearTaxPolicyRepository, ProductRepository, SettingsRepository,
    SupplierRepository, UnitRepository,
};

const WILAYA: &str = "16";

fn stamped() -> String {
    "2026-01-01T08:00:00Z".to_string()
}

fn stamped_dt() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 8, 0, 0).unwrap()
}

// ─── Fixtures ────────────────────────────────────────────────────────────────

fn supplier(id: &str, name: &str) -> Supplier {
    Supplier {
        id: id.to_string(),
        name: name.to_string(),
        contact_info: Some("contact".into()),
        active: true,
        created_at: stamped_dt(),
    }
}

fn contract(id: &str, reference: &str, unit_id: &str, supplier_id: &str) -> Contract {
    Contract {
        id: id.to_string(),
        contract_reference: reference.to_string(),
        unit_id: unit_id.to_string(),
        supplier_id: supplier_id.to_string(),
        fiscal_year: 2026,
        status: ContractStatus::Active,
        proposed_at: Some(stamped()),
        accepted_at: Some(stamped()),
        activated_at: Some(stamped()),
        ended_at: None,
        cancelled_at: None,
        notes: None,
        created_at: Some(stamped_dt()),
    }
}

fn line(id: &str, contract_id: &str, product_id: &str) -> ContractCatalogProductLine {
    ContractCatalogProductLine {
        id: id.to_string(),
        contract_id: contract_id.to_string(),
        product_id: product_id.to_string(),
        proposed_price_ht: 120.0,
        agreed_price_ht: Some(110.0),
        tva_classification: Some(0),
        tva_rate: Some(0.0),
        tva_amount: Some(0.0),
        price_ttc: Some(110.0),
        purchase_unit: Some(1),
        consumption_unit: Some(1),
        conversion_factor: Some(1),
        created_at: stamped(),
    }
}

fn allocation(
    id: &str,
    contract_id: &str,
    cpid: &str,
    unit_id: &str,
    product_id: &str,
    contracted: f64,
    fulfilled: f64,
) -> ContractCatalogAllocationRow {
    ContractCatalogAllocationRow {
        allocation: ContractAllocation {
            id: id.to_string(),
            contract_id: contract_id.to_string(),
            contract_product_id: cpid.to_string(),
            unit_id: unit_id.to_string(),
            product_id: product_id.to_string(),
            fiscal_year: 2026,
            contracted_quantity: contracted,
            fulfilled_quantity: fulfilled,
            released_quantity: 0.0,
            reserved_quantity: 0.0,
            entitlement_state: "ACTIVE".into(),
            version: 3,
        },
        created_at: stamped(),
    }
}

fn exception(id: &str, allocation_id: &str) -> ContractAllocationException {
    ContractAllocationException {
        id: id.to_string(),
        allocation_id: allocation_id.to_string(),
        released_quantity: 5.0,
        reason_code: "SUPPLIER_DELAY".into(),
        reason_note: None,
        created_by: "wilaya-admin".into(),
        created_at: stamped(),
    }
}

/// Two-unit WILAYA-authoritative catalog (restore path).
fn full_catalog() -> ContractCatalogExportDataset {
    ContractCatalogExportDataset {
        suppliers: vec![
            supplier("sup-1", "Fournisseur A"),
            supplier("sup-2", "Fournisseur B"),
        ],
        unit_supplier_links: vec![
            ContractCatalogUnitSupplierLink {
                unit_id: "unit-a".into(),
                supplier_id: "sup-1".into(),
            },
            ContractCatalogUnitSupplierLink {
                unit_id: "unit-b".into(),
                supplier_id: "sup-2".into(),
            },
        ],
        contracts: vec![
            ContractCatalogContractRow {
                contract: contract("ctr-1", "REF-2026-A", "unit-a", "sup-1"),
                product_lines: vec![line("ctr-prod-1", "ctr-1", "prod-1")],
                allocations: vec![allocation(
                    "alloc-1",
                    "ctr-1",
                    "ctr-prod-1",
                    "unit-a",
                    "prod-1",
                    100.0,
                    0.0,
                )],
                exceptions: vec![exception("exc-1", "alloc-1")],
            },
            ContractCatalogContractRow {
                contract: contract("ctr-2", "REF-2026-B", "unit-b", "sup-2"),
                product_lines: vec![
                    line("ctr-prod-2", "ctr-2", "prod-1"),
                    line("ctr-prod-3", "ctr-2", "prod-2"),
                ],
                allocations: vec![
                    allocation(
                        "alloc-2",
                        "ctr-2",
                        "ctr-prod-2",
                        "unit-b",
                        "prod-1",
                        200.0,
                        0.0,
                    ),
                    allocation(
                        "alloc-3",
                        "ctr-2",
                        "ctr-prod-3",
                        "unit-b",
                        "prod-2",
                        150.0,
                        0.0,
                    ),
                ],
                exceptions: vec![],
            },
        ],
        tax_policies: vec![FiscalYearTaxPolicy {
            fiscal_year: 2026,
            tva_rate: 19.0,
            frozen: false,
            set_by: "wilaya-admin".into(),
            created_at: stamped(),
        }],
    }
}

/// Catalog containing the UNIT's own unit plus another unit (scoping check).
fn mixed_catalog() -> ContractCatalogExportDataset {
    ContractCatalogExportDataset {
        suppliers: vec![
            supplier("sup-local", "Fournisseur Local"),
            supplier("sup-other", "Fournisseur Autre"),
        ],
        unit_supplier_links: vec![
            ContractCatalogUnitSupplierLink {
                unit_id: "unit-local".into(),
                supplier_id: "sup-local".into(),
            },
            ContractCatalogUnitSupplierLink {
                unit_id: "unit-other".into(),
                supplier_id: "sup-other".into(),
            },
        ],
        contracts: vec![
            ContractCatalogContractRow {
                contract: contract("ctr-local", "REF-LOCAL", "unit-local", "sup-local"),
                product_lines: vec![line("ctr-prod-local", "ctr-local", "prod-1")],
                allocations: vec![allocation(
                    "alloc-local",
                    "ctr-local",
                    "ctr-prod-local",
                    "unit-local",
                    "prod-1",
                    100.0,
                    0.0,
                )],
                exceptions: vec![],
            },
            ContractCatalogContractRow {
                contract: contract("ctr-other", "REF-OTHER", "unit-other", "sup-other"),
                product_lines: vec![line("ctr-prod-other", "ctr-other", "prod-2")],
                allocations: vec![allocation(
                    "alloc-other",
                    "ctr-other",
                    "ctr-prod-other",
                    "unit-other",
                    "prod-2",
                    500.0,
                    0.0,
                )],
                exceptions: vec![],
            },
        ],
        tax_policies: vec![FiscalYearTaxPolicy {
            fiscal_year: 2026,
            tva_rate: 19.0,
            frozen: false,
            set_by: "wilaya-admin".into(),
            created_at: stamped(),
        }],
    }
}

fn package(
    pkg_id: &str,
    dataset: ContractCatalogExportDataset,
    source: &str,
) -> SyncPackage<ContractCatalogExportDataset> {
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at: Utc::now(),
            source_node_id: source.to_string(),
            issuer_identity_id: None,
            package_id: PackageId(pkg_id.to_string()),
            signature_version: None,
            signing_key_id: None,
            integrity_hash: None,
            signature: None,
        },
        payload: dataset,
    }
}

fn apply_package(
    db: &mut Database,
    package: &SyncPackage<ContractCatalogExportDataset>,
) -> Result<ImportContractCatalogPackageOutcome, AppError> {
    let source = (!package.metadata.source_node_id.trim().is_empty())
        .then(|| package.metadata.source_node_id.trim().to_string());
    db.with_transaction(|tx| {
        let reg = SqliteImportedPackageRegistry::new(
            tx,
            CONTRACT_CATALOG_PACKAGE_KIND,
            source.as_deref(),
            "admin",
            None,
        );
        apply_contract_catalog_package(
            tx,
            &reg,
            ImportContractCatalogPackageInput {
                package: package.clone(),
                importer_wilaya_code: WILAYA.to_string(),
                imported_by: "admin".to_string(),
            },
        )
    })
}

// ─── Seeding ─────────────────────────────────────────────────────────────────

fn seed_products(db: &Database) {
    let now = "2026-01-01T00:00:00Z".to_string();
    let repo = ProductRepository::new(db.executor());
    for (pid, name) in [("prod-1", "P1"), ("prod-2", "P2"), ("prod-3", "P3")] {
        repo.insert_raw_product(
            &Product {
                id: pid.to_string(),
                name: name.to_string(),
                base_price: 100.0,
                year: 2026,
                created_at: Utc::now(),
            },
            &now,
        )
        .expect("seed product");
    }
}

fn seed_wilaya(db: &mut Database) {
    SettingsRepository::new(db.executor())
        .configure_wilaya(&WilayaNodeConfiguration {
            node_type: NodeType::Wilaya,
            wilaya_code: Some(WILAYA.into()),
            wilaya_name: Some("Alger".into()),
        })
        .expect("configure wilaya");
    let now = "2026-01-01T00:00:00Z";
    for (id, code) in [("unit-a", "A1"), ("unit-b", "B1")] {
        UnitRepository::new(db.executor())
            .upsert_raw_unit(id, code, &format!("{id} name"), WILAYA, now)
            .expect("seed unit");
    }
    seed_products(db);
}

fn seed_unit(db: &mut Database) {
    SettingsRepository::new(db.executor())
        .update_unit_node_settings("local unit", WILAYA)
        .expect("configure unit");
    UnitRepository::new(db.executor())
        .upsert_raw_unit(
            "unit-local",
            "L01",
            "local unit",
            WILAYA,
            "2026-01-01T00:00:00Z",
        )
        .expect("seed local unit");
    UnitRepository::new(db.executor())
        .upsert_raw_unit(
            "unit-other",
            "O02",
            "other unit",
            WILAYA,
            "2026-01-01T00:00:00Z",
        )
        .expect("seed other unit");
    seed_products(db);
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn wilaya_full_catalog_restore_import_persists_everything() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed_wilaya(&mut db);

    let pkg = package("pkg-cc-full-1", full_catalog(), WILAYA);
    let outcome = apply_package(&mut db, &pkg).expect("apply full catalog");
    assert!(outcome.imported > 0, "full restore must import rows");
    assert_eq!(outcome.skipped, 0, "no rows out of WILAYA scope");

    let contracts = ContractRepository::new(db.executor());
    let c1 = contracts
        .get_contract("ctr-1")
        .expect("read")
        .expect("ctr-1 present");
    assert_eq!(c1.status, ContractStatus::Active);
    assert_eq!(c1.contract_reference, "REF-2026-A");
    let c2 = contracts
        .get_contract("ctr-2")
        .expect("read")
        .expect("ctr-2 present");
    assert_eq!(c2.supplier_id, "sup-2");

    let a1 = contracts
        .get_allocation("alloc-1")
        .expect("read")
        .expect("alloc-1 present");
    assert_eq!(a1.contracted_quantity, 100.0);
    assert_eq!(a1.entitlement_state, "ACTIVE");
    // Exceptions ride along (WILAYA releases).
    assert!(contracts.get_exception("exc-1").expect("read").is_some());

    let suppliers = SupplierRepository::new(db.executor());
    assert!(suppliers.get_supplier("sup-2").expect("read").is_some());
    assert!(suppliers
        .supplier_associated_with_unit("unit-a", "sup-1")
        .expect("read"));

    let policy = FiscalYearTaxPolicyRepository::new(db.executor())
        .get_policy(2026)
        .expect("read")
        .expect("policy present");
    assert_eq!(policy.tva_rate, 19.0);
}

#[test]
fn replay_same_package_id_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed_wilaya(&mut db);

    let pkg = package("pkg-cc-replay", full_catalog(), WILAYA);
    apply_package(&mut db, &pkg).expect("first import ok");

    let err = apply_package(&mut db, &pkg).expect_err("replay must fail");
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage { .. })
    ));
}

#[test]
fn unit_import_is_scoped_to_local_unit() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed_unit(&mut db);

    let pkg = package("pkg-cc-scope", mixed_catalog(), WILAYA);
    let outcome = apply_package(&mut db, &pkg).expect("apply scoped catalog");
    assert!(outcome.skipped > 0, "out-of-scope rows must be skipped");

    let contracts = ContractRepository::new(db.executor());
    assert!(contracts.get_contract("ctr-local").expect("read").is_some());
    assert!(contracts
        .get_allocation("alloc-local")
        .expect("read")
        .is_some());

    let other = contracts.get_contract("ctr-other").expect("read");
    assert!(
        other.is_none(),
        "contracts for other units must NOT be applied on a UNIT node"
    );
    assert!(contracts
        .get_allocation("alloc-other")
        .expect("read")
        .is_none());
    assert!(
        SupplierRepository::new(db.executor())
            .get_supplier("sup-other")
            .expect("read")
            .is_none(),
        "out-of-scope supplier must not be inserted"
    );
}

#[test]
fn unit_reimport_refreshes_wilaya_owned_and_preserves_local_runtime() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed_unit(&mut db);

    let v1 = package("pkg-cc-v1", mixed_catalog(), WILAYA);
    apply_package(&mut db, &v1).expect("apply v1");

    // Local UNIT runtime state (deliveries consumed from the entitlement).
    db.executor()
        .execute(
            "UPDATE contract_allocations SET fulfilled_quantity = 25000 WHERE id = 'alloc-local'",
            [],
        )
        .expect("mark local fulfillment");

    // Next-day catalog: contracted raised to 150 for the same allocation.
    let mut next_catalog = mixed_catalog();
    next_catalog.contracts[0].allocations[0]
        .allocation
        .contracted_quantity = 150.0;
    let v2 = package("pkg-cc-v2", next_catalog, WILAYA);
    apply_package(&mut db, &v2).expect("apply v2");

    let a = ContractRepository::new(db.executor())
        .get_allocation("alloc-local")
        .expect("read")
        .expect("alloc-local present");
    assert_eq!(
        a.contracted_quantity, 150.0,
        "WILAYA-owned value must refresh"
    );
    assert_eq!(
        a.fulfilled_quantity, 25.0,
        "UNIT-owned runtime state must be preserved"
    );
}

#[test]
fn source_provenance_mismatch_is_fail_closed() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed_unit(&mut db);

    // A package whose source node id does not match the importer's wilaya.
    let pkg = package("pkg-cc-forge", mixed_catalog(), "05");
    let err = apply_package(&mut db, &pkg).expect_err("mismatched source must fail");
    assert!(matches!(err, AppError::Validation(_)));
}
