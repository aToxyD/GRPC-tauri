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
    PackageExportMode, PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
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

/// SEC-087 Phase 2 — C4 (ADR-0059 §13–15): fixtures carry EXPLICIT
/// authenticated `export_mode` / `target_node_id`. Legacy targetless fixtures
/// are only produced by the dedicated legacy-rejection tests.
fn package(
    pkg_id: &str,
    dataset: ContractCatalogExportDataset,
    source: &str,
    mode: Option<PackageExportMode>,
    target: Option<&str>,
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
            export_mode: mode,
            target_node_id: target.map(str::to_string),
        },
        payload: dataset,
    }
}

/// WILAYA-broadcast fleet-restore package (unscoped, no target).
fn fleet_package(
    pkg_id: &str,
    dataset: ContractCatalogExportDataset,
    source: &str,
) -> SyncPackage<ContractCatalogExportDataset> {
    package(
        pkg_id,
        dataset,
        source,
        Some(PackageExportMode::FleetRestore),
        None,
    )
}

/// UNIT-scoped package explicitly bound to the given target unit code.
fn unit_package(
    pkg_id: &str,
    dataset: ContractCatalogExportDataset,
    source: &str,
    target: &str,
) -> SyncPackage<ContractCatalogExportDataset> {
    package(
        pkg_id,
        dataset,
        source,
        Some(PackageExportMode::UnitDistribution),
        Some(target),
    )
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
            &grpc_lib::models::ProductUnitConfigCodes {
                purchase_unit: 1,
                consumption_unit: 1,
                conversion_factor: 1,
                tva_classification: 0,
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

    let pkg = fleet_package("pkg-cc-full-1", full_catalog(), WILAYA);
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

    let pkg = fleet_package("pkg-cc-replay", full_catalog(), WILAYA);
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

    let pkg = unit_package("pkg-cc-scope", mixed_catalog(), WILAYA, "L01");
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

    let v1 = unit_package("pkg-cc-v1", mixed_catalog(), WILAYA, "L01");
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
    let v2 = unit_package("pkg-cc-v2", next_catalog, WILAYA, "L01");
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
    // Target binding passes first (correct mode/target), then provenance fails.
    let pkg = unit_package("pkg-cc-forge", mixed_catalog(), "05", "L01");
    let err = apply_package(&mut db, &pkg).expect_err("mismatched source must fail");
    assert!(matches!(err, AppError::Validation(_)));
}

// ─── SEC-087 Phase 2 — C4: Contract Catalog target binding (ADR-0059 §13–15) ──

/// Matrix row: `export_mode = None` (with or without a stray target) is
/// rejected regardless of importer node — legacy targetless V3 contract
/// catalog packages are rejected, with NO inferred-mode fallback.
#[test]
fn legacy_targetless_contract_catalog_package_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed_wilaya(&mut db);

    let none_none = package("pkg-cc-legacy", full_catalog(), WILAYA, None, None);
    let err = apply_package(&mut db, &none_none)
        .expect_err("None/None must be rejected on a WILAYA node");
    assert!(
        matches!(&err, AppError::Validation(_)),
        "None/None rejected as validation: {err:?}"
    );

    let none_target = package("pkg-cc-legacy-2", full_catalog(), WILAYA, None, Some("A1"));
    let err = apply_package(&mut db, &none_target)
        .expect_err("None/target must be rejected on a WILAYA node");
    assert!(
        matches!(&err, AppError::Validation(_)),
        "None/target rejected as validation: {err:?}"
    );

    let mut db_unit = ConnectionFactory::new_for_test().expect("db");
    seed_unit(&mut db_unit);
    let legacy_unit = package("pkg-cc-legacy-3", mixed_catalog(), WILAYA, None, None);
    let err = apply_package(&mut db_unit, &legacy_unit)
        .expect_err("None/None must be rejected on a UNIT node too");
    assert!(
        matches!(&err, AppError::Validation(_)),
        "None/None rejected on UNIT as validation: {err:?}"
    );
}

/// A UNIT importer rejects a UnitDistribution package bound to a DIFFERENT
/// unit code, with no rows written and no replay slot consumed.
#[test]
fn unit_rejects_distribution_bound_to_another_unit() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed_unit(&mut db);

    // Target "O02" is a real, existing unit — a *different* one.
    let wrong_target = unit_package("pkg-cc-wrong", mixed_catalog(), WILAYA, "O02");
    let err =
        apply_package(&mut db, &wrong_target).expect_err("wrong target unit must be rejected");
    assert!(
        matches!(&err, AppError::Validation(_)),
        "wrong target rejected as validation: {err:?}"
    );

    assert!(
        ContractRepository::new(db.executor())
            .get_contract("ctr-local")
            .expect("read")
            .is_none(),
        "rejected package must write no catalog rows"
    );

    // No replay slot consumed: the SAME package_id with the correct target
    // imports successfully on retry.
    let corrected = unit_package("pkg-cc-wrong", mixed_catalog(), WILAYA, "L01");
    let outcome = apply_package(&mut db, &corrected).expect("same-id corrected retry succeeds");
    assert!(outcome.imported > 0, "corrected retry imports rows");
}

/// A UNIT importer never applies a FleetRestore catalog.
#[test]
fn unit_rejects_fleet_restore_catalog() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed_unit(&mut db);

    let pkg = fleet_package("pkg-cc-fleet-unit", mixed_catalog(), WILAYA);
    let err =
        apply_package(&mut db, &pkg).expect_err("fleet restore on a UNIT node must be rejected");
    assert!(
        matches!(&err, AppError::Validation(_)),
        "fleet restore rejected on UNIT as validation: {err:?}"
    );
}

/// A WILAYA importer never applies a unit-distribution (targeted) catalog.
#[test]
fn wilaya_rejects_unit_distribution_catalog() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed_wilaya(&mut db);

    let pkg = unit_package("pkg-cc-ud-wilaya", mixed_catalog(), WILAYA, "A1");
    let err = apply_package(&mut db, &pkg)
        .expect_err("unit distribution on a WILAYA node must be rejected");
    assert!(
        matches!(&err, AppError::Validation(_)),
        "unit distribution rejected on WILAYA as validation: {err:?}"
    );
}

/// A FleetRestore catalog that carries a target is malformed and rejected
/// everywhere (the exporter never produces this; proving fail-closed).
#[test]
fn fleet_restore_catalog_with_target_is_rejected_everywhere() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed_wilaya(&mut db);

    let pkg = package(
        "pkg-cc-fleet-target",
        full_catalog(),
        WILAYA,
        Some(PackageExportMode::FleetRestore),
        Some("A1"),
    );
    let err =
        apply_package(&mut db, &pkg).expect_err("fleet restore with a target must be rejected");
    assert!(
        matches!(&err, AppError::Validation(_)),
        "fleet restore with target rejected as validation: {err:?}"
    );
}

/// A UnitDistribution catalog without a declared target is rejected — the
/// target is mandatory for that mode (no fallback).
#[test]
fn unit_distribution_without_target_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed_unit(&mut db);

    let pkg = package(
        "pkg-cc-ud-none",
        mixed_catalog(),
        WILAYA,
        Some(PackageExportMode::UnitDistribution),
        None,
    );
    let err = apply_package(&mut db, &pkg)
        .expect_err("unit distribution without a target must be rejected");
    assert!(
        matches!(&err, AppError::Validation(_)),
        "targetless unit distribution rejected as validation: {err:?}"
    );
}

/// A UnitDistribution package bound to the importer's own unit MAY carry an
/// EMPTY contract set (ADR-0059 §10) and must import cleanly.
#[test]
fn unit_distribution_with_empty_contracts_is_accepted_for_the_target() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed_unit(&mut db);

    let empty = ContractCatalogExportDataset {
        suppliers: Vec::new(),
        unit_supplier_links: Vec::new(),
        contracts: Vec::new(),
        tax_policies: Vec::new(),
    };
    let pkg = unit_package("pkg-cc-empty", empty, WILAYA, "L01");
    let outcome = apply_package(&mut db, &pkg)
        .expect("empty unit-distribution catalog accepted for the target unit");
    assert_eq!(outcome.imported, 0, "no rows to import");
}
