//! SEC-087 Phase 3 — authoritative contract-pricing snapshot (ADR-0056).
//!
//! Chain under test:
//!   product TvaClassification + agreed_price_ht
//!     -> compute_contract_fiscal (exact Money/Rate, no f64 accounting)
//!     -> tva_rate / tva_amount / price_ttc
//!     -> contract_products.price_ttc (the persisted snapshot row)
//!     -> resolver -> SupplierOrderItem.unit_price.
//!
//! Invariants asserted across the suite:
//!   - exact TVA arithmetic: Exonere / 9% / 19% / half-cent rounding;
//!   - the snapshot is persisted CLOSED at the price-agreement boundary and is
//!     independent of later product-master changes (historical fact);
//!   - fail closed: missing/invalid unit or TVA configuration rejects;
//!   - resolution reads strictly `cp.price_ttc` and never falls back to any
//!     other price source; a row without `price_ttc` fails closed;
//!   - acceptance is a completeness gate on `price_ttc` and remains atomic;
//!   - the pricing commands stay WILAYA-admin scoped (authz preserved).

use chrono::Utc;
use grpc_lib::application::authz::Action;
use grpc_lib::application::services::{ContractService, OrderService};
use grpc_lib::commands::{authorize_command, AppState};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::models::{
    AddContractProductRequest, ContractStatus, CreateContractRequest, CreateOrderRequest,
    CreateProductRequest, CreateSupplierRequest, OrderItemInput, SetAgreedPriceHtRequest, UserRole,
};
use grpc_lib::repositories::RepositoryProvider;
use rusqlite::params;
use uuid::Uuid;

const FY: i32 = 2024;

/// Insert a product carrying full SEC-087 unit/TVA configuration.
fn insert_configured_product(
    ex: &grpc_lib::repositories::DbExecutor<'_>,
    name: &str,
    tva_code: i32,
) -> String {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    let config = grpc_lib::domain::validation::validate_product_units(
        Some(1),
        Some(1),
        Some(1),
        Some(tva_code),
    )
    .expect("valid product config");
    ex.products()
        .insert_product(
            &id,
            &CreateProductRequest {
                name: name.to_string(),
                base_price: 500.0,
                purchase_unit: Some(1),
                consumption_unit: Some(1),
                conversion_factor: Some(1),
                tva_classification: Some(tva_code),
            },
            FY,
            &config,
            &now,
        )
        .unwrap();
    id
}

fn seed_unit_and_fiscal(ex: &grpc_lib::repositories::DbExecutor<'_>, unit_id: &str) {
    let now = Utc::now().to_rfc3339();
    ex.units()
        .upsert_raw_unit(unit_id, "UNIT_S", "Price Unit", "01", &now)
        .unwrap();
    ex.execute(
        "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1, 'open', ?2)",
        params![FY, now],
    )
    .unwrap();
}

fn seed_supplier(ex: &grpc_lib::repositories::DbExecutor<'_>) -> String {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    ex.suppliers()
        .insert_supplier(
            &id,
            &CreateSupplierRequest {
                name: "Snapshot Supplier".to_string(),
                contact_info: None,
            },
            &now,
        )
        .unwrap();
    id
}

fn seed_contract(
    ex: &grpc_lib::repositories::DbExecutor<'_>,
    unit_id: &str,
    supplier_id: &str,
) -> String {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    ex.contracts()
        .insert_contract(
            &id,
            &CreateContractRequest {
                unit_id: unit_id.to_string(),
                supplier_id: supplier_id.to_string(),
                fiscal_year: FY,
                contract_reference: "CTR-SNAP".to_string(),
                notes: None,
            },
            &now,
        )
        .unwrap();
    id
}

/// A product line + ACTIVE allocation for the contract (no price frozen yet).
fn seed_product_line(
    db: &grpc_lib::db::Database,
    contract_id: &str,
    product_id: &str,
    unit_id: &str,
) -> String {
    let now = Utc::now().to_rfc3339();
    let ex = db.executor();
    let cp_id = Uuid::new_v4().to_string();
    let alloc_id = Uuid::new_v4().to_string();
    ex.contracts()
        .insert_contract_product(
            &cp_id,
            &AddContractProductRequest {
                contract_id: contract_id.to_string(),
                product_id: product_id.to_string(),
                proposed_price_ht: 150.0,
                agreed_price_ht: None,
                contracted_quantity: 1000.0,
            },
            &now,
        )
        .unwrap();
    ex.contracts()
        .insert_allocation(
            &alloc_id,
            contract_id,
            &cp_id,
            &unit_id,
            &product_id,
            FY,
            1000.0,
            &now,
        )
        .unwrap();
    cp_id
}

fn freeze(db: &grpc_lib::db::Database, cp_id: &str, ht: f64) -> f64 {
    ContractService::new(db.executor())
        .set_agreed_price_ht(&SetAgreedPriceHtRequest {
            contract_product_id: cp_id.to_string(),
            agreed_price_ht: ht,
        })
        .expect("freeze agreed HT price snapshot")
}

// ─────────────────────────────────────────────────────────────────────────────
// Exact TVA arithmetic
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn exonere_ttc_equals_ht() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let unit_id = Uuid::new_v4().to_string();
    let product = insert_configured_product(&ex, "ProdExonere", 0);
    let supplier = seed_supplier(&ex);
    seed_unit_and_fiscal(&ex, &unit_id);
    let contract = seed_contract(&ex, &unit_id, &supplier);
    let cp = seed_product_line(&db, &contract, &product, &unit_id);

    let ttc = freeze(&db, &cp, 100.0);
    assert_eq!(ttc, 100.0, "exonere must not add any TVA");

    let row = db
        .executor()
        .contracts()
        .get_contract_product(&cp)
        .unwrap()
        .unwrap();
    assert_eq!(row.agreed_price_ht, Some(100.0));
    assert_eq!(row.tva_classification, Some(0));
    assert_eq!(row.tva_rate, Some(0.0));
    assert_eq!(row.tva_amount, Some(0.0));
    assert_eq!(row.price_ttc, Some(100.0));
}

#[test]
fn nine_percent_tva_exact() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let unit_id = Uuid::new_v4().to_string();
    let product = insert_configured_product(&ex, "ProdNine", 1);
    let supplier = seed_supplier(&ex);
    seed_unit_and_fiscal(&ex, &unit_id);
    let contract = seed_contract(&ex, &unit_id, &supplier);
    let cp = seed_product_line(&db, &contract, &product, &unit_id);

    let ttc = freeze(&db, &cp, 200.0);
    assert_eq!(ttc, 218.0, "9% of 200 = 18");

    let row = db
        .executor()
        .contracts()
        .get_contract_product(&cp)
        .unwrap()
        .unwrap();
    assert_eq!(row.tva_rate, Some(9.0));
    assert_eq!(row.tva_amount, Some(18.0));
    assert_eq!(row.price_ttc, Some(218.0));
}

#[test]
fn nineteen_percent_tva_exact() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let unit_id = Uuid::new_v4().to_string();
    let product = insert_configured_product(&ex, "ProdNineteen", 2);
    let supplier = seed_supplier(&ex);
    seed_unit_and_fiscal(&ex, &unit_id);
    let contract = seed_contract(&ex, &unit_id, &supplier);
    let cp = seed_product_line(&db, &contract, &product, &unit_id);

    let ttc = freeze(&db, &cp, 100.0);
    assert_eq!(ttc, 119.0, "19% of 100 = 19");

    let row = db
        .executor()
        .contracts()
        .get_contract_product(&cp)
        .unwrap()
        .unwrap();
    assert_eq!(row.tva_rate, Some(19.0));
    assert_eq!(row.tva_amount, Some(19.0));
    assert_eq!(row.price_ttc, Some(119.0));
}

#[test]
fn half_cent_tva_rounds_to_cent() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let unit_id = Uuid::new_v4().to_string();
    let product = insert_configured_product(&ex, "ProdHalfCent", 2);
    let supplier = seed_supplier(&ex);
    seed_unit_and_fiscal(&ex, &unit_id);
    let contract = seed_contract(&ex, &unit_id, &supplier);
    let cp = seed_product_line(&db, &contract, &product, &unit_id);

    // 199.97 * 19 / 100 = 37.9943 -> round_2dp -> 37.99; TTC = 237.96.
    let ttc = freeze(&db, &cp, 199.97);
    assert_eq!(ttc, 237.96, "rounding must be exact to the cent");

    let row = db
        .executor()
        .contracts()
        .get_contract_product(&cp)
        .unwrap()
        .unwrap();
    assert_eq!(row.tva_amount, Some(37.99));
    assert_eq!(row.price_ttc, Some(237.96));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot persistence & immutability
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_row_persists_full_price_boundary_fields() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let unit_id = Uuid::new_v4().to_string();
    let product = insert_configured_product(&ex, "ProdPersist", 2);
    let supplier = seed_supplier(&ex);
    seed_unit_and_fiscal(&ex, &unit_id);
    let contract = seed_contract(&ex, &unit_id, &supplier);
    let cp = seed_product_line(&db, &contract, &product, &unit_id);

    freeze(&db, &cp, 250.0);
    let row = db
        .executor()
        .contracts()
        .get_contract_product(&cp)
        .unwrap()
        .unwrap();
    assert_eq!(
        row.agreed_price_ht,
        Some(250.0),
        "agreed_price_ht = explicit HT at agreement"
    );
    assert_eq!(row.tva_classification, Some(2));
    assert_eq!(row.tva_rate, Some(19.0));
    assert_eq!(row.tva_amount, Some(47.5));
    assert_eq!(row.price_ttc, Some(297.5));
    assert_eq!(row.purchase_unit, Some(1));
    assert_eq!(row.consumption_unit, Some(1));
    assert_eq!(row.conversion_factor, Some(1));
}

#[test]
fn snapshot_is_historical_and_independent_of_product_master() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let unit_id = Uuid::new_v4().to_string();
    let product = insert_configured_product(&ex, "ProdMaster", 2);
    let supplier = seed_supplier(&ex);
    seed_unit_and_fiscal(&ex, &unit_id);
    let contract = seed_contract(&ex, &unit_id, &supplier);
    let cp = seed_product_line(&db, &contract, &product, &unit_id);

    freeze(&db, &cp, 100.0); // TTC 119 @ 19%

    // A later product-master reclassification must NOT rewrite the snapshot.
    db.get_connection()
        .execute(
            "UPDATE products SET tva_classification = 0 WHERE id = ?1",
            params![product],
        )
        .unwrap();

    let row = db
        .executor()
        .contracts()
        .get_contract_product(&cp)
        .unwrap()
        .unwrap();
    assert_eq!(row.tva_classification, Some(2), "snapshot keeps agreed TVA");
    assert_eq!(row.tva_rate, Some(19.0));
    assert_eq!(row.price_ttc, Some(119.0));

    // A new order still resolves from the historical snapshot.
    let (_, total) = OrderService::new(db.executor())
        .create_supplier_order(
            &CreateOrderRequest {
                reference_number: Some("SNAP-INDEP".to_string()),
                items: vec![OrderItemInput {
                    product_id: product.clone(),
                    quantity: 1.0,
                }],
            },
            &unit_id,
            FY,
        )
        .expect("order resolves from snapshot");
    assert_eq!(total, 119.0);
}

#[test]
fn freeze_after_acceptance_is_rejected_and_leaves_snapshot_unchanged() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let unit_id = Uuid::new_v4().to_string();
    let product = insert_configured_product(&ex, "ProdFrozen", 2);
    let supplier = seed_supplier(&ex);
    seed_unit_and_fiscal(&ex, &unit_id);
    let contract = seed_contract(&ex, &unit_id, &supplier);
    let cp = seed_product_line(&db, &contract, &product, &unit_id);

    freeze(&db, &cp, 100.0);
    let accepted = ContractService::new(db.executor())
        .accept_contract(&contract, &Utc::now().to_rfc3339())
        .expect("accept with full snapshot");
    assert_eq!(accepted.status, ContractStatus::Accepted);

    // Price is immutable after acceptance.
    let err = ContractService::new(db.executor())
        .set_agreed_price_ht(&SetAgreedPriceHtRequest {
            contract_product_id: cp.clone(),
            agreed_price_ht: 999.0,
        })
        .expect_err("freeze blocked after acceptance");
    assert!(matches!(err, grpc_lib::errors::AppError::BusinessLogic(_)));

    let row = db
        .executor()
        .contracts()
        .get_contract_product(&cp)
        .unwrap()
        .unwrap();
    assert_eq!(row.price_ttc, Some(119.0), "snapshot must be unchanged");
    assert_eq!(row.agreed_price_ht, Some(100.0));
}

// ─────────────────────────────────────────────────────────────────────────────
// Resolver authority
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn resolver_uses_price_ttc_as_sole_authority() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let unit_id = Uuid::new_v4().to_string();
    let product = insert_configured_product(&ex, "ProdTtc", 2);
    let supplier = seed_supplier(&ex);
    seed_unit_and_fiscal(&ex, &unit_id);
    let contract = seed_contract(&ex, &unit_id, &supplier);
    let cp = seed_product_line(&db, &contract, &product, &unit_id);

    freeze(&db, &cp, 100.0); // price_ttc = 119.0

    let (order, total) = OrderService::new(db.executor())
        .create_supplier_order(
            &CreateOrderRequest {
                reference_number: Some("SNAP-TTC".to_string()),
                items: vec![OrderItemInput {
                    product_id: product.clone(),
                    quantity: 2.0,
                }],
            },
            &unit_id,
            FY,
        )
        .expect("order resolves from price_ttc");
    assert_eq!(total, 238.0, "2 x price_ttc 119 (sole authority)");

    let items = db
        .executor()
        .orders()
        .get_supplier_order_items(&order)
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].unit_price, 119.0,
        "order item unit_price = price_ttc snapshot"
    );
}

#[test]
fn agreed_ht_without_frozen_snapshot_is_not_orderable() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let unit_id = Uuid::new_v4().to_string();
    let product = insert_configured_product(&ex, "ProdNoSnapshot", 2);
    let supplier = seed_supplier(&ex);
    seed_unit_and_fiscal(&ex, &unit_id);
    let contract = seed_contract(&ex, &unit_id, &supplier);
    // A row carrying `agreed_price_ht` but NOT the frozen pricing snapshot: no
    // `price_ttc` is ever computed because `freeze_contract_price` was never
    // invoked. The resolver must fail closed — `agreed_price_ht` is NOT an
    // operational TTC source.
    {
        let now = Utc::now().to_rfc3339();
        let cp_id = Uuid::new_v4().to_string();
        let alloc_id = Uuid::new_v4().to_string();
        ex.contracts()
            .insert_contract_product(
                &cp_id,
                &AddContractProductRequest {
                    contract_id: contract.clone(),
                    product_id: product.clone(),
                    proposed_price_ht: 150.0,
                    agreed_price_ht: Some(41.0),
                    contracted_quantity: 1000.0,
                },
                &now,
            )
            .unwrap();
        ex.contracts()
            .insert_allocation(
                &alloc_id, &contract, &cp_id, &unit_id, &product, FY, 1000.0, &now,
            )
            .unwrap();
        cp_id
    };

    // `agreed_price_ht` is not an operational price: resolution reads strictly
    // `cp.price_ttc`, which is NULL here -> fail closed.
    let err = OrderService::new(db.executor())
        .create_supplier_order(
            &CreateOrderRequest {
                reference_number: Some("SNAP-NO-FREEZE".to_string()),
                items: vec![OrderItemInput {
                    product_id: product.clone(),
                    quantity: 1.0,
                }],
            },
            &unit_id,
            FY,
        )
        .expect_err("row with only agreed_price_ht must not resolve");
    assert!(
        matches!(err, grpc_lib::errors::AppError::BusinessLogic(_)),
        "expected resolution failure, got {err:?}"
    );

    // Acceptance is equally impossible: the line lacks a frozen price snapshot.
    let accept_err = ContractService::new(db.executor())
        .accept_contract(&contract, &Utc::now().to_rfc3339())
        .expect_err("accept must fail without price_ttc");
    assert!(matches!(
        accept_err,
        grpc_lib::errors::AppError::BusinessLogic(_)
    ));
    let c = db
        .executor()
        .contracts()
        .get_contract(&contract)
        .unwrap()
        .unwrap();
    assert_eq!(c.status, ContractStatus::Proposed, "no partial acceptance");
}

#[test]
fn snapshots_are_built_from_explicit_agreed_ht_only() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let unit_id = Uuid::new_v4().to_string();
    let product = insert_configured_product(&ex, "ProdSnapshotHt", 2);
    let supplier = seed_supplier(&ex);
    seed_unit_and_fiscal(&ex, &unit_id);
    let contract = seed_contract(&ex, &unit_id, &supplier);
    let cp = seed_product_line(&db, &contract, &product, &unit_id);

    // Pre-set the persisted `agreed_price_ht` to a different value than the
    // freeze request: the snapshot MUST be derived from the explicit request
    // HT, not from whatever the column currently holds.
    db.get_connection()
        .execute(
            "UPDATE contract_products SET agreed_price_ht = 4100 WHERE id = ?1",
            params![cp],
        )
        .unwrap();

    // Explicit HT 100.00 @ 19% -> TTC 119.00 regardless of the pre-set value.
    let ttc = freeze(&db, &cp, 100.0);
    assert_eq!(ttc, 119.0);

    let row = db
        .executor()
        .contracts()
        .get_contract_product(&cp)
        .unwrap()
        .unwrap();
    assert_eq!(
        row.agreed_price_ht,
        Some(100.0),
        "HT comes from the explicit request"
    );
    assert_eq!(row.price_ttc, Some(119.0));
}

// ─────────────────────────────────────────────────────────────────────────────
// Fail-closed configuration & atomic acceptance
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn missing_tva_classification_rejects_freezing() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let unit_id = Uuid::new_v4().to_string();

    // Product with units but NO TVA classification.
    let now = Utc::now().to_rfc3339();
    let product = Uuid::new_v4().to_string();
    ex.products()
        .insert_raw_product(
            &grpc_lib::models::Product {
                id: product.clone(),
                name: "NoTva".to_string(),
                base_price: 100.0,
                year: FY,
                created_at: Utc::now(),
            },
            &now,
        )
        .unwrap();
    db.get_connection()
        .execute(
            "UPDATE products SET purchase_unit = 1, consumption_unit = 1, conversion_factor = 1 WHERE id = ?1",
            params![product],
        )
        .unwrap();

    let supplier = seed_supplier(&ex);
    seed_unit_and_fiscal(&ex, &unit_id);
    let contract = seed_contract(&ex, &unit_id, &supplier);
    let cp = seed_product_line(&db, &contract, &product, &unit_id);

    let err = ContractService::new(db.executor())
        .set_agreed_price_ht(&SetAgreedPriceHtRequest {
            contract_product_id: cp,
            agreed_price_ht: 100.0,
        })
        .expect_err("missing tva_classification must fail closed");
    assert!(
        matches!(
            err,
            grpc_lib::errors::AppError::BusinessLogic(_)
                | grpc_lib::errors::AppError::Validation(_)
        ),
        "expected fail-closed rejection, got {err:?}"
    );
}

#[test]
fn missing_unit_configuration_rejects_freezing() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let unit_id = Uuid::new_v4().to_string();

    // Product with a TVA classification but NO unit configuration.
    let now = Utc::now().to_rfc3339();
    let product = Uuid::new_v4().to_string();
    ex.products()
        .insert_raw_product(
            &grpc_lib::models::Product {
                id: product.clone(),
                name: "NoUnits".to_string(),
                base_price: 100.0,
                year: FY,
                created_at: Utc::now(),
            },
            &now,
        )
        .unwrap();
    db.get_connection()
        .execute(
            "UPDATE products SET tva_classification = 2 WHERE id = ?1",
            params![product],
        )
        .unwrap();

    let supplier = seed_supplier(&ex);
    seed_unit_and_fiscal(&ex, &unit_id);
    let contract = seed_contract(&ex, &unit_id, &supplier);
    let cp = seed_product_line(&db, &contract, &product, &unit_id);

    let err = ContractService::new(db.executor())
        .set_agreed_price_ht(&SetAgreedPriceHtRequest {
            contract_product_id: cp,
            agreed_price_ht: 100.0,
        })
        .expect_err("incomplete unit config must fail closed");
    assert!(
        matches!(
            err,
            grpc_lib::errors::AppError::BusinessLogic(_)
                | grpc_lib::errors::AppError::Validation(_)
        ),
        "expected fail-closed rejection, got {err:?}"
    );
}

#[test]
fn acceptance_requires_every_line_snapshot_and_is_atomic() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let unit_id = Uuid::new_v4().to_string();
    let product_a = insert_configured_product(&ex, "LineA", 2);
    let product_b = insert_configured_product(&ex, "LineB", 2);
    let supplier = seed_supplier(&ex);
    seed_unit_and_fiscal(&ex, &unit_id);
    let contract = seed_contract(&ex, &unit_id, &supplier);

    // Line A is fully frozen; line B is not.
    let cp_a = seed_product_line(&db, &contract, &product_a, &unit_id);
    freeze(&db, &cp_a, 100.0);
    let _cp_b = seed_product_line(&db, &contract, &product_b, &unit_id);

    let err = ContractService::new(db.executor())
        .accept_contract(&contract, &Utc::now().to_rfc3339())
        .expect_err("accept must fail while a line lacks price_ttc");
    assert!(matches!(err, grpc_lib::errors::AppError::BusinessLogic(_)));

    // No partial transition happened: still proposed, no accepted_at stamp.
    let c = db
        .executor()
        .contracts()
        .get_contract(&contract)
        .unwrap()
        .unwrap();
    assert_eq!(c.status, ContractStatus::Proposed);
    assert!(c.accepted_at.is_none());

    // Completing the last line makes acceptance succeed in a single step.
    freeze(&db, &_cp_b, 100.0);
    let accepted = ContractService::new(db.executor())
        .accept_contract(&contract, &Utc::now().to_rfc3339())
        .expect("accept once every line is snapshotted");
    assert_eq!(accepted.status, ContractStatus::Accepted);
}

// ─────────────────────────────────────────────────────────────────────────────
// Authorization (price-agreement stays WILAYA-admin scoped)
// ─────────────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
mod common;

fn wilaya_configured_state() -> AppState {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'WILAYA', configured = 1, unit_name = NULL, wilaya_code = '16', wilaya_name = 'Alger' WHERE id = 1",
                [],
            )
            .expect("settings");
    }
    state
}

fn unit_configured_state() -> AppState {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'UNIT', configured = 1, unit_name = 'unit-scope-id', wilaya_code = '16', wilaya_name = NULL WHERE id = 1",
                [],
            )
            .expect("settings");
    }
    state
}

fn set_session(state: &AppState, user_id: &str, role: &str) {
    let mut session = common::create_test_session(user_id, "admin", role);
    session.user_role = UserRole::from(role.to_string());
    common::insert_test_user(state, user_id, "admin", role);
    *state.current_session.lock().expect("session mutex") = Some(session);
}

#[test]
fn price_agreement_remains_wilaya_admin_only() {
    use grpc_lib::application::authz::AuthorizationError;
    use grpc_lib::errors::AppError;

    // WILAYA admin is authorized for both the price-agreement action and the
    // contract management family.
    let state = wilaya_configured_state();
    set_session(&state, "w-admin", "Admin");
    authorize_command(&state, Action::ApproveContractPrice, None).expect("allow");
    authorize_command(&state, Action::ManageContracts, None).expect("allow");

    // WILAYA non-admin is denied.
    let state = wilaya_configured_state();
    set_session(&state, "w-user", "User");
    let err = authorize_command(&state, Action::ApproveContractPrice, None).expect_err("deny");
    assert!(matches!(err, AppError::Authorization(_)));

    // UNIT admin is denied the WILAYA-only pricing actions.
    let state = unit_configured_state();
    set_session(&state, "u-admin", "Admin");
    let err = authorize_command(&state, Action::ApproveContractPrice, None).expect_err("deny");
    assert!(
        matches!(
            err,
            AppError::Authorization(AuthorizationError::RequiresWilayaNode)
        ) || matches!(
            err,
            AppError::Authorization(AuthorizationError::InsufficientPermissions)
        ),
        "UNIT admin must be denied ApproveContractPrice: {err:?}"
    );
}
