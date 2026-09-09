//! SEC-087 Phase 4A — order-confirmation boundary regression tests.
//!
//! The confirmed defect: confirmation re-resolution and the conversion guard
//! both counted the confirming order's OWN reservation against itself, so any
//! order that reserved more than half of a negotiated allocation (including
//! exact exhaustion) could never be confirmed.
//!
//! These tests pin the corrected behavior: an order that already holds its own
//! reservation must confirm up to its recorded quantity (including 100%
//! exhaustion), while every authoritative guard (single supplier, allocation
//! stability, price_ttc stability, guarded conversion, transactional rollback,
//! stock/FIFO exactly-once) remains intact.

use chrono::Utc;
use grpc_lib::application::services::{ContractService, OrderService};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::models::{
    AddContractProductRequest, CreateContractRequest, CreateOrderRequest, CreateProductRequest,
    CreateSupplierRequest, OrderItemInput, OrderStatus, SetAgreedPriceHtRequest,
    StockMovementQuery,
};
use grpc_lib::repositories::RepositoryProvider;
use uuid::Uuid;

struct Entitlement {
    unit_id: String,
    product_id: String,
    contract_id: String,
    contract_product_id: String,
    allocation_id: String,
}

/// One unit + one product + one supplier + one contract + one negotiated
/// contract product + one allocation with the given contracted quantity.
/// Agreed price is frozen at 40.0 (so `total = 40.0 * quantity`).
fn setup_entitlement(db: &grpc_lib::db::Database, contracted: f64) -> Entitlement {
    let ex = db.executor();
    let now = Utc::now().to_rfc3339();

    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    let supplier_id = Uuid::new_v4().to_string();
    let contract_id = Uuid::new_v4().to_string();
    let contract_product_id = Uuid::new_v4().to_string();
    let allocation_id = Uuid::new_v4().to_string();

    ex.units()
        .upsert_raw_unit(&unit_id, "UNIT_CA", "Confirmation Unit", "01", &now)
        .unwrap();
    ex.execute(
        "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (2024, 'open', ?1)",
        rusqlite::params![now],
    )
    .unwrap();

    let config =
        grpc_lib::domain::validation::validate_product_units(Some(1), Some(1), Some(1), Some(0))
            .expect("valid product config");
    ex.products()
        .insert_product(
            &product_id,
            &CreateProductRequest {
                name: "Entitlement Boundary Product".to_string(),
                base_price: 999.0,
                purchase_unit: Some(1),
                consumption_unit: Some(1),
                conversion_factor: Some(1),
                tva_classification: Some(0),
            },
            2024,
            &config,
            &now,
        )
        .unwrap();

    ex.suppliers()
        .insert_supplier(
            &supplier_id,
            &CreateSupplierRequest {
                name: "Boundary Supplier".to_string(),
                contact_info: None,
            },
            &now,
        )
        .unwrap();
    ex.contracts()
        .insert_contract(
            &contract_id,
            &CreateContractRequest {
                unit_id: unit_id.clone(),
                supplier_id: supplier_id.clone(),
                fiscal_year: 2024,
                contract_reference: "CTR-BOUNDARY-2024".to_string(),
                notes: None,
            },
            &now,
        )
        .unwrap();
    ex.contracts()
        .insert_contract_product(
            &contract_product_id,
            &AddContractProductRequest {
                contract_id: contract_id.clone(),
                product_id: product_id.clone(),
                proposed_price_ht: 999.0,
                agreed_price_ht: Some(40.0),
                contracted_quantity: contracted,
            },
            &now,
        )
        .unwrap();
    ContractService::new(db.executor())
        .set_agreed_price_ht(&SetAgreedPriceHtRequest {
            contract_product_id: contract_product_id.clone(),
            agreed_price_ht: 40.0,
        })
        .expect("freeze agreed HT price snapshot");
    ex.contracts()
        .insert_allocation(
            &allocation_id,
            &contract_id,
            &contract_product_id,
            &unit_id,
            &product_id,
            2024,
            contracted,
            &now,
        )
        .unwrap();

    Entitlement {
        unit_id,
        product_id,
        contract_id,
        contract_product_id,
        allocation_id,
    }
}

/// Add a SECOND product + allocation to the same contract (same supplier),
/// so a two-item order can exercise mid-loop drift atomicity.
fn add_second_product_to_contract(
    db: &grpc_lib::db::Database,
    unit_id: &str,
    contract_id: &str,
    contracted: f64,
) -> (String, String) {
    let ex = db.executor();
    let now = Utc::now().to_rfc3339();
    let product_id = Uuid::new_v4().to_string();
    let contract_product_id = Uuid::new_v4().to_string();
    let allocation_id = Uuid::new_v4().to_string();

    let config =
        grpc_lib::domain::validation::validate_product_units(Some(1), Some(1), Some(1), Some(0))
            .expect("valid product config");
    ex.products()
        .insert_product(
            &product_id,
            &CreateProductRequest {
                name: "Second Boundary Product".to_string(),
                base_price: 999.0,
                purchase_unit: Some(1),
                consumption_unit: Some(1),
                conversion_factor: Some(1),
                tva_classification: Some(0),
            },
            2024,
            &config,
            &now,
        )
        .unwrap();
    ex.contracts()
        .insert_contract_product(
            &contract_product_id,
            &AddContractProductRequest {
                contract_id: contract_id.to_string(),
                product_id: product_id.clone(),
                proposed_price_ht: 999.0,
                agreed_price_ht: Some(40.0),
                contracted_quantity: contracted,
            },
            &now,
        )
        .unwrap();
    ContractService::new(db.executor())
        .set_agreed_price_ht(&SetAgreedPriceHtRequest {
            contract_product_id: contract_product_id.clone(),
            agreed_price_ht: 40.0,
        })
        .expect("freeze agreed HT price snapshot");
    ex.contracts()
        .insert_allocation(
            &allocation_id,
            contract_id,
            &contract_product_id,
            unit_id,
            &product_id,
            2024,
            contracted,
            &now,
        )
        .unwrap();

    (product_id, allocation_id)
}

fn create_order(
    db: &grpc_lib::db::Database,
    unit_id: &str,
    reference: &str,
    items: Vec<OrderItemInput>,
) -> String {
    let (order_id, _total) = OrderService::new(db.executor())
        .create_supplier_order(
            &CreateOrderRequest {
                reference_number: Some(reference.to_string()),
                items,
            },
            unit_id,
            2024,
        )
        .expect("create order");
    order_id
}

fn confirm_order(
    db: &grpc_lib::db::Database,
    order_id: &str,
) -> Result<(), grpc_lib::errors::AppError> {
    OrderService::new(db.executor()).confirm_order_atomic(order_id, "confirmation", "confirmation")
}

fn order_status(db: &grpc_lib::db::Database, order_id: &str) -> OrderStatus {
    db.executor()
        .orders()
        .get_supplier_order(order_id)
        .expect("order exists")
        .expect("order present")
        .status
}

fn read_allocation(
    db: &grpc_lib::db::Database,
    allocation_id: &str,
) -> grpc_lib::models::ContractAllocation {
    db.executor()
        .contracts()
        .get_allocation(allocation_id)
        .expect("read allocation")
        .expect("allocation present")
}

fn fifo_layer_count(db: &grpc_lib::db::Database, unit_id: &str, product_id: &str) -> usize {
    db.executor()
        .fifo_layers()
        .get_layers_for_product(unit_id, product_id)
        .expect("read layers")
        .len()
}

fn stock_movement_count_for_order(db: &grpc_lib::db::Database, order_id: &str) -> i64 {
    db.executor()
        .stock_movements()
        .count_stock_movements(&StockMovementQuery {
            reference_id: Some(order_id.to_string()),
            ..Default::default()
        })
        .expect("count movements")
}

fn assert_close(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() < f64::EPSILON,
        "{label}: expected {expected}, got {actual}"
    );
}

// Letting an order confirm exactly 50% of its negotiated allocation is ordinary
// behavior (this was ALREADY possible pre-fix; it stays possible).
#[test]
fn exactly_half_entitlement_confirms() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ent = setup_entitlement(&db, 100.0);

    let order_id = create_order(
        &db,
        &ent.unit_id,
        "BOUND-50PCT",
        vec![OrderItemInput {
            product_id: ent.product_id.clone(),
            quantity: 50.0,
        }],
    );
    confirm_order(&db, &order_id).expect("50% order must confirm");

    let a = read_allocation(&db, &ent.allocation_id);
    assert_close(a.fulfilled_quantity, 50.0, "fulfilled");
    assert_close(a.reserved_quantity, 0.0, "reserved");
    assert_eq!(order_status(&db, &order_id), OrderStatus::Confirmed);
}

#[test]
fn more_than_half_entitlement_confirms() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ent = setup_entitlement(&db, 100.0);

    let order_id = create_order(
        &db,
        &ent.unit_id,
        "BOUND-75PCT",
        vec![OrderItemInput {
            product_id: ent.product_id.clone(),
            quantity: 75.0,
        }],
    );
    confirm_order(&db, &order_id).expect("75% order must confirm");

    let a = read_allocation(&db, &ent.allocation_id);
    assert_close(a.fulfilled_quantity, 75.0, "fulfilled");
    assert_close(a.reserved_quantity, 0.0, "reserved");
    assert_close(a.effective_remaining(), 25.0, "remaining");
    assert_eq!(order_status(&db, &order_id), OrderStatus::Confirmed);
}

#[test]
fn exact_exhaustion_confirms_and_zeroes_counters() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ent = setup_entitlement(&db, 100.0);

    let order_id = create_order(
        &db,
        &ent.unit_id,
        "BOUND-100PCT",
        vec![OrderItemInput {
            product_id: ent.product_id.clone(),
            quantity: 100.0,
        }],
    );
    confirm_order(&db, &order_id).expect("100% exhaustion order must confirm");

    let a = read_allocation(&db, &ent.allocation_id);
    assert_close(a.fulfilled_quantity, 100.0, "fulfilled");
    assert_close(a.reserved_quantity, 0.0, "reserved");
    assert_close(a.released_quantity, 0.0, "released");
    assert_close(a.contracted_quantity, 100.0, "contracted");
    assert_close(a.effective_remaining(), 0.0, "remaining");
    assert_eq!(order_status(&db, &order_id), OrderStatus::Confirmed);
}

#[test]
fn small_reservation_still_confirms() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ent = setup_entitlement(&db, 100.0);

    let order_id = create_order(
        &db,
        &ent.unit_id,
        "BOUND-SMALL",
        vec![OrderItemInput {
            product_id: ent.product_id.clone(),
            quantity: 5.0,
        }],
    );
    confirm_order(&db, &order_id).expect("small reservation must confirm");

    let a = read_allocation(&db, &ent.allocation_id);
    assert_close(a.fulfilled_quantity, 5.0, "fulfilled");
    assert_close(a.reserved_quantity, 0.0, "reserved");
}

#[test]
fn multiple_orders_reserve_without_oversubscription_and_each_confirms() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ent = setup_entitlement(&db, 100.0);

    let a_order = create_order(
        &db,
        &ent.unit_id,
        "OVR-A",
        vec![OrderItemInput {
            product_id: ent.product_id.clone(),
            quantity: 60.0,
        }],
    );

    // Second order attempting 60 of the 40 left must be rejected at creation
    // (third-party reservation stays fully counted).
    let oversubscribe = OrderService::new(db.executor()).create_supplier_order(
        &CreateOrderRequest {
            reference_number: Some("OVR-BAD".to_string()),
            items: vec![OrderItemInput {
                product_id: ent.product_id.clone(),
                quantity: 60.0,
            }],
        },
        &ent.unit_id,
        2024,
    );
    assert!(oversubscribe.is_err(), "oversubscription must be rejected");

    let b_order = create_order(
        &db,
        &ent.unit_id,
        "OVR-B",
        vec![OrderItemInput {
            product_id: ent.product_id.clone(),
            quantity: 40.0,
        }],
    );

    // A's own reservation must not block A; B's reservation must remain
    // invisible to A as well as to B.
    confirm_order(&db, &a_order).expect("A must confirm its 60");
    let a = read_allocation(&db, &ent.allocation_id);
    assert_close(a.fulfilled_quantity, 60.0, "fulfilled after A");
    assert_close(a.reserved_quantity, 40.0, "reserved after A");

    confirm_order(&db, &b_order).expect("B must confirm its 40");
    let b = read_allocation(&db, &ent.allocation_id);
    assert_close(b.fulfilled_quantity, 100.0, "fulfilled final");
    assert_close(b.reserved_quantity, 0.0, "reserved final");
    assert_close(b.effective_remaining(), 0.0, "remaining final");
    assert_eq!(order_status(&db, &a_order), OrderStatus::Confirmed);
    assert_eq!(order_status(&db, &b_order), OrderStatus::Confirmed);
}

#[test]
fn price_drift_still_rejects_confirmation() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ent = setup_entitlement(&db, 100.0);

    let order_id = create_order(
        &db,
        &ent.unit_id,
        "DRIFT-PRICE",
        vec![OrderItemInput {
            product_id: ent.product_id.clone(),
            quantity: 50.0,
        }],
    );
    db.get_connection()
        .execute(
            "UPDATE contract_products SET price_ttc = 999999 WHERE id = ?1",
            rusqlite::params![ent.contract_product_id],
        )
        .expect("simulate price change after creation");

    let result = confirm_order(&db, &order_id);
    assert!(result.is_err(), "price drift must reject confirmation");

    let a = read_allocation(&db, &ent.allocation_id);
    assert_close(a.fulfilled_quantity, 0.0, "no fulfillment on drift");
    assert_close(a.reserved_quantity, 50.0, "reservation intact");
    assert_eq!(order_status(&db, &order_id), OrderStatus::Draft);
    assert_eq!(stock_movement_count_for_order(&db, &order_id), 0);
    assert_eq!(fifo_layer_count(&db, &ent.unit_id, &ent.product_id), 0);
}

#[test]
fn allocation_drift_still_rejects_confirmation() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ent = setup_entitlement(&db, 100.0);

    let order_id = create_order(
        &db,
        &ent.unit_id,
        "DRIFT-ALLOC",
        vec![OrderItemInput {
            product_id: ent.product_id.clone(),
            quantity: 50.0,
        }],
    );
    // The recorded allocation stops being the authoritative resolve target.
    db.get_connection()
        .execute(
            "UPDATE contract_allocations SET entitlement_state = 'CANCELLED' WHERE id = ?1",
            rusqlite::params![ent.allocation_id],
        )
        .expect("invalidate recorded allocation");

    let result = confirm_order(&db, &order_id);
    assert!(result.is_err(), "allocation drift must reject confirmation");

    let a = read_allocation(&db, &ent.allocation_id);
    assert_close(a.fulfilled_quantity, 0.0, "no fulfillment on drift");
    assert_close(a.reserved_quantity, 50.0, "reservation intact");
    assert_eq!(order_status(&db, &order_id), OrderStatus::Draft);
    assert_eq!(stock_movement_count_for_order(&db, &order_id), 0);
    assert_eq!(fifo_layer_count(&db, &ent.unit_id, &ent.product_id), 0);
}

#[test]
fn supplier_drift_still_rejects_confirmation() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ent = setup_entitlement(&db, 100.0);

    let order_id = create_order(
        &db,
        &ent.unit_id,
        "DRIFT-SUPPLIER",
        vec![OrderItemInput {
            product_id: ent.product_id.clone(),
            quantity: 50.0,
        }],
    );

    // Point the contract at a different supplier after creation.
    let other_supplier_id = Uuid::new_v4().to_string();
    db.executor()
        .suppliers()
        .insert_supplier(
            &other_supplier_id,
            &CreateSupplierRequest {
                name: "Other Supplier".to_string(),
                contact_info: None,
            },
            &Utc::now().to_rfc3339(),
        )
        .unwrap();
    db.get_connection()
        .execute(
            "UPDATE contracts SET supplier_id = ?1 WHERE id = ?2",
            rusqlite::params![other_supplier_id, ent.contract_id],
        )
        .expect("simulate supplier change after creation");

    let result = confirm_order(&db, &order_id);
    assert!(result.is_err(), "supplier drift must reject confirmation");

    assert_eq!(order_status(&db, &order_id), OrderStatus::Draft);
    assert_eq!(stock_movement_count_for_order(&db, &order_id), 0);
    assert_eq!(fifo_layer_count(&db, &ent.unit_id, &ent.product_id), 0);
}

// A failure part-way through the confirmation loop must roll back every earlier
// mutation (conversion, stock movement, FIFO, order status) inside the same tx.
#[test]
fn failed_confirmation_rolls_back_all_mutations() {
    let mut db = ConnectionFactory::new_for_test().unwrap();
    let ent = setup_entitlement(&db, 100.0);
    let (product2, allocation2) =
        add_second_product_to_contract(&db, &ent.unit_id, &ent.contract_id, 100.0);
    let product1 = ent.product_id.clone();

    let order_id = create_order(
        &db,
        &ent.unit_id,
        "ATOMIC-TWO-ITEM",
        vec![
            OrderItemInput {
                product_id: product1.clone(),
                quantity: 75.0,
            },
            OrderItemInput {
                product_id: product2.clone(),
                quantity: 75.0,
            },
        ],
    );

    // Drift ONLY the second product's price: item 1 would convert before item 2
    // trips the drift guard.
    db.get_connection()
        .execute(
            "UPDATE contract_products SET price_ttc = 1234567 WHERE contract_id = ?1 AND product_id = ?2",
            rusqlite::params![ent.contract_id, product2],
        )
        .expect("simulate price change on second product");

    let result = db.with_transaction(|tx| {
        OrderService::new(tx).confirm_order_atomic(&order_id, "atomicity", "atomicity")
    });
    assert!(
        result.is_err(),
        "second-item drift must reject confirmation"
    );

    // Item 1's conversion must have been rolled back.
    let a1 = read_allocation(&db, &ent.allocation_id);
    assert_close(
        a1.fulfilled_quantity,
        0.0,
        "allocation1 fulfilled rolled back",
    );
    assert_close(a1.reserved_quantity, 75.0, "allocation1 reservation intact");
    let a2 = read_allocation(&db, &allocation2);
    assert_close(
        a2.fulfilled_quantity,
        0.0,
        "allocation2 fulfilled rolled back",
    );
    assert_close(a2.reserved_quantity, 75.0, "allocation2 reservation intact");
    assert_eq!(order_status(&db, &order_id), OrderStatus::Draft);
    assert_eq!(stock_movement_count_for_order(&db, &order_id), 0);
    assert_eq!(fifo_layer_count(&db, &ent.unit_id, &product1), 0);
    assert_eq!(fifo_layer_count(&db, &ent.unit_id, &product2), 0);
}

#[test]
fn fifo_and_stock_are_created_exactly_once() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ent = setup_entitlement(&db, 100.0);

    let order_id = create_order(
        &db,
        &ent.unit_id,
        "ONCE-ONLY",
        vec![OrderItemInput {
            product_id: ent.product_id.clone(),
            quantity: 50.0,
        }],
    );
    confirm_order(&db, &order_id).expect("confirm");

    assert_eq!(stock_movement_count_for_order(&db, &order_id), 1);
    assert_eq!(fifo_layer_count(&db, &ent.unit_id, &ent.product_id), 1);
    let a = read_allocation(&db, &ent.allocation_id);
    assert_close(a.fulfilled_quantity, 50.0, "fulfilled");
    assert_close(a.reserved_quantity, 0.0, "reserved");
}

// Repository guard (DB level): converting more than the recorded reservation
// must fail closed with zero affected rows.
#[test]
fn conversion_greater_than_recorded_reservation_fails_closed() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ent = setup_entitlement(&db, 100.0);

    let _order_id = create_order(
        &db,
        &ent.unit_id,
        "CONV-GUARD",
        vec![OrderItemInput {
            product_id: ent.product_id.clone(),
            quantity: 5.0,
        }],
    );

    let contracts = db.executor().contracts();
    let blocked = contracts
        .try_convert_reserved_to_fulfilled(&ent.allocation_id, 10.0)
        .expect("repo call");
    assert_eq!(blocked, 0, "conversion beyond reservation must fail closed");

    let accepted = contracts
        .try_convert_reserved_to_fulfilled(&ent.allocation_id, 5.0)
        .expect("repo call");
    assert_eq!(accepted, 1, "conversion up to reservation must succeed");

    let a = read_allocation(&db, &ent.allocation_id);
    assert_close(a.fulfilled_quantity, 5.0, "fulfilled");
    assert_close(a.reserved_quantity, 0.0, "reserved");
}
