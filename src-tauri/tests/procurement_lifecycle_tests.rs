//! Procurement lifecycle integration tests (ADR-0055 / SEC-087-F).
//!
//! Exercises the exact application-service surface the Stage-11 IPC commands
//! delegate to: suppliers, contracts, per-UNIT allocations, obligation
//! release/revoke, and the fiscal-year TVA policy. All writes run through the
//! same services as `commands::procurement` under WILAYA context.

use grpc_lib::application::services::{ContractService, FiscalTaxPolicyService, SupplierService};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::errors::AppError;
use grpc_lib::models::{
    AddContractProductRequest, ContractStatus, CreateContractRequest, CreateSupplierRequest,
    NodeType, Product, ReleaseContractAllocationRequest, ReleaseReasonCode, SetAgreedPriceRequest,
    SetTaxPolicyRequest, WilayaNodeConfiguration,
};
use grpc_lib::repositories::{
    ProductRepository, RepositoryProvider, SettingsRepository, UnitRepository,
};

const WILAYA: &str = "16";

fn seed_wilaya_context(db: &mut Database) {
    SettingsRepository::new(db.executor())
        .configure_wilaya(&WilayaNodeConfiguration {
            node_type: NodeType::Wilaya,
            wilaya_code: Some(WILAYA.into()),
            wilaya_name: Some("Alger".into()),
        })
        .expect("configure wilaya");
    let now = "2026-01-01T00:00:00Z";
    for (id, code, name) in [("unit-a", "A1", "unit a"), ("unit-b", "B1", "unit b")] {
        UnitRepository::new(db.executor())
            .upsert_raw_unit(id, code, name, WILAYA, now)
            .expect("seed unit");
    }
    for (id, name) in [("prod-1", "Product One"), ("prod-2", "Product Two")] {
        ProductRepository::new(db.executor())
            .insert_raw_product(
                &Product {
                    id: id.into(),
                    name: name.into(),
                    base_price: 100.0,
                    year: 2026,
                    created_at: "2026-01-01T00:00:00Z".parse().expect("ts"),
                },
                now,
            )
            .expect("seed product");
    }
}

#[test]
fn full_procurement_lifecycle_via_services() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed_wilaya_context(&mut db);

    let db_ref = &db;

    // ---- Suppliers ---------------------------------------------------------
    let supplier = SupplierService::new(db_ref.executor())
        .create_supplier(&CreateSupplierRequest {
            name: "Fournisseur Alpha".into(),
            contact_info: Some("algiers".into()),
        })
        .expect("create supplier");
    assert!(supplier.active);

    SupplierService::new(db_ref.executor())
        .associate_with_unit("unit-a", &supplier.id)
        .expect("associate supplier with unit");

    let unit_suppliers = SupplierService::new(db_ref.executor())
        .list_unit_suppliers("unit-a")
        .expect("list unit suppliers");
    assert!(unit_suppliers.iter().any(|s| s.id == supplier.id));

    // Duplicate association is idempotent.
    SupplierService::new(db_ref.executor())
        .associate_with_unit("unit-a", &supplier.id)
        .expect("associate again (idempotent)");

    // ---- Fiscal-year TVA policy -------------------------------------------
    let policy = FiscalTaxPolicyService::new(db_ref.executor())
        .set_policy(
            &SetTaxPolicyRequest {
                fiscal_year: 2026,
                tva_rate: 19.0,
            },
            "wilaya-admin",
        )
        .expect("set tax policy");
    assert!((policy.tva_rate - 19.0).abs() < f64::EPSILON);

    // ---- Contract lifecycle ------------------------------------------------
    let contract = ContractService::new(db_ref.executor())
        .create_contract(&CreateContractRequest {
            unit_id: "unit-a".into(),
            supplier_id: supplier.id.clone(),
            fiscal_year: 2026,
            contract_reference: "REF-2026-A".into(),
            notes: None,
        })
        .expect("create contract");
    assert_eq!(contract.status, ContractStatus::Proposed);

    let (cpid, alloc_id) = ContractService::new(db_ref.executor())
        .add_contract_product(&AddContractProductRequest {
            contract_id: contract.id.clone(),
            product_id: "prod-1".into(),
            proposed_price: 120.0,
            agreed_price: None,
            contracted_quantity: 100.0,
        })
        .expect("add contract product");

    ContractService::new(db_ref.executor())
        .set_agreed_price(&SetAgreedPriceRequest {
            contract_product_id: cpid.clone(),
            agreed_price: 110.0,
        })
        .expect("set agreed price");

    let accepted = ContractService::new(db_ref.executor())
        .accept_contract(&contract.id, "2026-01-02T00:00:00Z")
        .expect("accept contract");
    assert_eq!(accepted.status, ContractStatus::Accepted);

    let active = ContractService::new(db_ref.executor())
        .activate_contract(&contract.id, "2026-01-02T00:00:00Z")
        .expect("activate contract");
    assert_eq!(active.status, ContractStatus::Active);

    // A second live contract for the same (unit, year) is rejected.
    let duplicate =
        ContractService::new(db_ref.executor()).create_contract(&CreateContractRequest {
            unit_id: "unit-a".into(),
            supplier_id: supplier.id.clone(),
            fiscal_year: 2026,
            contract_reference: "REF-2026-A-DUP".into(),
            notes: None,
        });
    assert!(matches!(duplicate, Err(AppError::BusinessLogic(_))));

    // ---- Release / revoke ----------------------------------------------------
    let allocation = db_ref
        .executor()
        .contracts()
        .get_allocation(&alloc_id)
        .expect("read allocation")
        .expect("allocation present");
    let remaining_before = allocation.effective_remaining();

    let exception = ContractService::new(db_ref.executor())
        .release_allocation(
            &ReleaseContractAllocationRequest {
                allocation_id: alloc_id.clone(),
                released_quantity: 10.0,
                reason_code: ReleaseReasonCode::SupplierDelay,
                reason_note: Some("delivered late".into()),
            },
            "wilaya-admin",
        )
        .expect("release allocation");
    assert_eq!(exception.reason_code, "SUPPLIER_DELAY");

    let allocation_after = db_ref
        .executor()
        .contracts()
        .get_allocation(&alloc_id)
        .expect("read allocation after release")
        .expect("allocation present");
    assert!(
        (allocation_after.effective_remaining() - (remaining_before - 10.0)).abs() < f64::EPSILON
    );

    ContractService::new(db_ref.executor())
        .revoke_release(&exception.id)
        .expect("revoke release");
    let allocation_restored = db_ref
        .executor()
        .contracts()
        .get_allocation(&alloc_id)
        .expect("read allocation after revoke")
        .expect("allocation present");
    assert!((allocation_restored.effective_remaining() - remaining_before).abs() < f64::EPSILON);
    assert!(db_ref
        .executor()
        .contracts()
        .get_exception(&exception.id)
        .expect("read exception")
        .is_none());

    // ---- End contract (obligations persist) ----------------------------------
    let ended = ContractService::new(db_ref.executor())
        .end_contract(&contract.id, "2026-12-31T00:00:00Z")
        .expect("end contract");
    assert_eq!(ended.status, ContractStatus::Ended);

    // ENDED contract with remaining obligation is still readable.
    let allocations = ContractService::new(db_ref.executor())
        .list_allocations_for_contract(&contract.id)
        .expect("list allocations");
    assert_eq!(allocations.len(), 1);
}

#[test]
fn price_has_single_owner_domain_arithmetic() {
    // The TVA arithmetic is owned by domain/pricing; both report services
    // and the IPC command delegate to it (P2 / A5).
    let got = grpc_lib::domain::pricing::price::price_with_tva(200.0, 19.0);
    assert!((got - 238.0).abs() < f64::EPSILON);
    let via_service =
        grpc_lib::application::services::ReportCalculationService::calculate_product_price_with_tva(
            200.0, 19.0,
        );
    assert!((via_service - got).abs() < f64::EPSILON);
}
