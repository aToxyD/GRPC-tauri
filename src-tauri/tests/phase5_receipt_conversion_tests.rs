//! SEC-087 Phase 5 — supplier-order receipt / FIFO conversion integration.
//!
//! Pins receipt-side behavior of the multi-unit model:
//!   - the purchase→consumption unit snapshot is captured at order creation
//!     (all-or-nothing) and is immutable against later contract changes;
//!   - confirmation converts purchase quantity → consumption quantity and
//!     price_ttc → consumption-unit TTC cost (rounded once, Option A);
//!   - FIFO layers, IN stock movements and inventory stock are written in
//!     CONSUMPTION units, keyed by `(product_id, consumption_unit)`;
//!   - valuation uses the snapshot TTC (never `base_price`);
//!   - legacy (snapshot-NULL) order items still convert 1:1;
//!   - a partial/inconsistent persisted snapshot fails closed;
//!   - failures part-way through confirmation roll back every mutation.

use chrono::Utc;
use grpc_lib::application::services::{ContractService, OrderService};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::models::{
    AddContractProductRequest, CreateContractRequest, CreateOrderRequest, CreateProductRequest,
    CreateSupplierRequest, OrderItemInput, OrderStatus, SetAgreedPriceHtRequest,
    StockMovementQuery,
};
use grpc_lib::repositories::RepositoryProvider;

fn open_fiscal_year(db: &grpc_lib::db::Database, year: i32) {
    db.executor()
        .execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1, 'open', ?2)",
            rusqlite::params![year, Utc::now().to_rfc3339()],
        )
        .unwrap();
}

fn setup_unit(db: &grpc_lib::db::Database) -> String {
    let unit_id = format!("unit-{}", uuid::Uuid::new_v4());
    db.executor()
        .units()
        .upsert_raw_unit(
            &unit_id,
            "PHASE5_UNIT",
            "Phase 5 Unit",
            "05",
            &Utc::now().to_rfc3339(),
        )
        .unwrap();
    unit_id
}

fn setup_supplier(db: &grpc_lib::db::Database, name: &str) -> String {
    let supplier_id = format!("sup-{}", uuid::Uuid::new_v4());
    db.executor()
        .suppliers()
        .insert_supplier(
            &supplier_id,
            &CreateSupplierRequest {
                name: name.to_string(),
                contact_info: None,
            },
            &Utc::now().to_rfc3339(),
        )
        .unwrap();
    supplier_id
}

#[allow(clippy::too_many_arguments)]
fn setup_product(
    db: &grpc_lib::db::Database,
    name: &str,
    purchase_unit: i32,
    consumption_unit: i32,
    conversion_factor: i32,
) -> String {
    let product_id = format!("prod-{}", uuid::Uuid::new_v4());
    let config = grpc_lib::domain::validation::validate_product_units(
        Some(purchase_unit),
        Some(consumption_unit),
        Some(conversion_factor),
        Some(0),
    )
    .expect("valid product config");
    db.executor()
        .products()
        .insert_product(
            &product_id,
            &CreateProductRequest {
                name: name.to_string(),
                base_price: 999.0, // deliberately ≠ purchase TTC → valuation must ignore it
                purchase_unit: Some(purchase_unit),
                consumption_unit: Some(consumption_unit),
                conversion_factor: Some(conversion_factor),
                tva_classification: Some(0),
            },
            2025,
            &config,
            &Utc::now().to_rfc3339(),
        )
        .unwrap();
    product_id
}

fn setup_contract(
    db: &grpc_lib::db::Database,
    unit_id: &str,
    supplier_id: &str,
    fy: i32,
) -> String {
    let contract_id = format!("ctr-{}-{}", fy, uuid::Uuid::new_v4());
    db.executor()
        .contracts()
        .insert_contract(
            &contract_id,
            &CreateContractRequest {
                unit_id: unit_id.to_string(),
                supplier_id: supplier_id.to_string(),
                fiscal_year: fy,
                contract_reference: format!("CTR-P5-{fy}"),
                notes: None,
            },
            &Utc::now().to_rfc3339(),
        )
        .unwrap();
    contract_id
}

#[allow(clippy::too_many_arguments)]
fn add_product_to_contract(
    db: &grpc_lib::db::Database,
    unit_id: &str,
    contract_id: &str,
    product_id: &str,
    fy: i32,
    allocation_id: &str,
    contracted: f64,
    price: f64,
) -> String {
    let contract_product_id = format!("cp-{allocation_id}");
    db.executor()
        .contracts()
        .insert_contract_product(
            &contract_product_id,
            &AddContractProductRequest {
                contract_id: contract_id.to_string(),
                product_id: product_id.to_string(),
                proposed_price_ht: 999.0,
                agreed_price_ht: Some(price),
                contracted_quantity: contracted,
            },
            &Utc::now().to_rfc3339(),
        )
        .unwrap();
    let frozen = ContractService::new(db.executor())
        .set_agreed_price_ht(&SetAgreedPriceHtRequest {
            contract_product_id: contract_product_id.clone(),
            agreed_price_ht: price,
        })
        .expect("freeze agreed price snapshot");
    assert!(
        (frozen - price).abs() < f64::EPSILON,
        "price_ttc must equal frozen agreed price (TVA 0%)"
    );
    db.executor()
        .contracts()
        .insert_allocation(
            allocation_id,
            contract_id,
            &contract_product_id,
            unit_id,
            product_id,
            fy,
            contracted,
            "2027-01-01T00:00:00Z",
        )
        .unwrap();
    allocation_id.to_string()
}

fn create(
    db: &grpc_lib::db::Database,
    unit_id: &str,
    fy: i32,
    req: &CreateOrderRequest,
) -> Vec<(String, f64)> {
    OrderService::new(db.executor())
        .create_supplier_orders(req, unit_id, fy)
        .expect("plan and create")
}

fn confirm(db: &grpc_lib::db::Database, order_id: &str) -> Result<(), grpc_lib::errors::AppError> {
    OrderService::new(db.executor()).confirm_order_atomic(order_id, "phase5", "phase5")
}

fn items(db: &grpc_lib::db::Database, order_id: &str) -> Vec<grpc_lib::models::SupplierOrderItem> {
    db.executor()
        .orders()
        .get_supplier_order_items(order_id)
        .expect("read items")
}

fn layers(
    db: &grpc_lib::db::Database,
    unit_id: &str,
    product_id: &str,
) -> Vec<grpc_lib::models::FifoStockLayer> {
    db.executor()
        .fifo_layers()
        .get_layers_for_product(unit_id, product_id)
        .expect("read layers")
}

fn movements(
    db: &grpc_lib::db::Database,
    order_id: &str,
) -> Vec<grpc_lib::models::StockMovementDbRow> {
    db.executor()
        .stock_movements()
        .fetch_stock_movements(&StockMovementQuery {
            reference_id: Some(order_id.to_string()),
            limit: 100,
            ..Default::default()
        })
        .expect("read movements")
}

fn allocation(
    db: &grpc_lib::db::Database,
    allocation_id: &str,
) -> grpc_lib::models::ContractAllocation {
    db.executor()
        .contracts()
        .get_allocation(allocation_id)
        .expect("read allocation")
        .expect("allocation present")
}

fn order(db: &grpc_lib::db::Database, order_id: &str) -> grpc_lib::models::SupplierOrder {
    db.executor()
        .orders()
        .get_supplier_order(order_id)
        .expect("read order")
        .expect("order present")
}

fn assert_eq_w(qty: Option<f64>, expected: f64) {
    assert!(qty.is_some(), "expected Some quantity");
    assert!(
        (qty.unwrap() - expected).abs() < f64::EPSILON,
        "quantity {} != expected {}",
        qty.unwrap(),
        expected
    );
}

// --- 1. same unit (kg→kg, factor 1): receipt is identity -------------------

#[test]
fn same_unit_receipt_is_identity() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_id = setup_supplier(&db, "Supplier A");
    let product_id = setup_product(&db, "Flour", 1, 1, 1);
    let contract_id = setup_contract(&db, &unit_id, &supplier_id, 2027);
    add_product_to_contract(
        &db,
        &unit_id,
        &contract_id,
        &product_id,
        2027,
        "alloc-same-unit",
        100.0,
        40.00,
    );

    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("ID-1".to_string()),
            items: vec![OrderItemInput {
                product_id: product_id.clone(),
                quantity: 10.0,
            }],
        },
    );
    let order_id = created[0].0.clone();
    let item = &items(&db, &order_id)[0];
    assert_eq!(item.purchase_unit, Some(1));
    assert_eq!(item.consumption_unit, Some(1));
    assert_eq!(item.conversion_factor, Some(1));
    assert_eq_w(item.consumption_quantity, 10.0);

    confirm(&db, &order_id).expect("confirm same-unit receipt");

    let layer = &layers(&db, &unit_id, &product_id)[0];
    assert_eq!(layer.qty_remaining, 10.0);
    assert_eq!(layer.unit_cost, 40.00);
    assert_eq_w(layer.purchase_quantity, 10.0);
    assert_eq_w(layer.purchase_unit_cost, 40.00);
    assert_eq!(layer.purchase_unit, Some(1));
    assert_eq!(layer.consumption_unit, Some(1));
    assert_eq!(layer.conversion_factor, Some(1));

    let mov = &movements(&db, &order_id)[0];
    assert_eq!(mov.movement_type, "IN");
    assert_eq!(mov.quantity, 10.0);
    assert_eq!(mov.unit_cost.unwrap(), 40.00);
    assert_eq!(order(&db, &order_id).status, OrderStatus::Confirmed);
}

// --- 2. integer conversion (box→kg, factor 10) is exact --------------------

#[test]
fn integer_conversion_is_received_in_consumption_units() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_id = setup_supplier(&db, "Supplier A");
    let product_id = setup_product(&db, "Oil Box", 8, 1, 10); // 1 box = 10 kg
    let contract_id = setup_contract(&db, &unit_id, &supplier_id, 2027);
    add_product_to_contract(
        &db,
        &unit_id,
        &contract_id,
        &product_id,
        2027,
        "alloc-integer",
        100.0,
        120.00,
    );

    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("ID-2".to_string()),
            items: vec![OrderItemInput {
                product_id: product_id.clone(),
                quantity: 1.0, // 1 box
            }],
        },
    );
    let order_id = created[0].0.clone();

    let item = &items(&db, &order_id)[0];
    assert_eq!(item.purchase_unit, Some(8));
    assert_eq!(item.consumption_unit, Some(1));
    assert_eq!(item.conversion_factor, Some(10));
    assert_eq_w(item.consumption_quantity, 10.0); // 1 box × 10 = 10 kg

    confirm(&db, &order_id).expect("confirm integer conversion");

    let layer = &layers(&db, &unit_id, &product_id)[0];
    assert_eq!(layer.qty_remaining, 10.0); // consumption units
    assert_eq!(layer.unit_cost, 12.00); // 120 / 10, exact
    assert_eq_w(layer.purchase_quantity, 1.0);
    assert_eq_w(layer.purchase_unit_cost, 120.00);

    let mov = &movements(&db, &order_id)[0];
    assert_eq!(mov.quantity, 10.0); // IN in consumption units
    assert_eq!(mov.unit_cost.unwrap(), 12.00);
}

// --- 3. non-terminating conversion rounds exactly once (Option A) ----------

#[test]
fn non_terminating_conversion_rounds_once_to_cent() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_id = setup_supplier(&db, "Supplier A");
    let product_id = setup_product(&db, "Pack", 6, 1, 3); // 1 pack (can) = 3 kg
    let contract_id = setup_contract(&db, &unit_id, &supplier_id, 2027);
    add_product_to_contract(
        &db,
        &unit_id,
        &contract_id,
        &product_id,
        2027,
        "alloc-7div3",
        100.0,
        7.00,
    );

    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("ID-3".to_string()),
            items: vec![OrderItemInput {
                product_id: product_id.clone(),
                quantity: 1.0,
            }],
        },
    );
    let order_id = created[0].0.clone();
    confirm(&db, &order_id).expect("confirm 7/3 conversion");

    let layer = &layers(&db, &unit_id, &product_id)[0];
    assert_eq!(layer.qty_remaining, 3.0);
    assert_eq!(layer.unit_cost, 2.33); // rounded once to the cent

    // Valuation 3 × 2.33 = 6.99 mirrors purchase 7.00 with bounded residual.
    let (qty, value) = db
        .executor()
        .fifo_layers()
        .get_global_quantity_and_value_for_product(&product_id)
        .unwrap();
    assert_eq!(qty, 3.0);
    assert_eq!(value, 6.99);
}

// --- 4. multiple receipts stack as distinct consumption-unit layers --------

#[test]
fn multiple_receipts_stack_as_distinct_layers() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_id = setup_supplier(&db, "Supplier A");
    let product_id = setup_product(&db, "Rice", 1, 1, 1);
    let contract_id = setup_contract(&db, &unit_id, &supplier_id, 2027);
    add_product_to_contract(
        &db,
        &unit_id,
        &contract_id,
        &product_id,
        2027,
        "alloc-multi-a",
        1000.0,
        40.00,
    );

    let o1 = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("ID-4A".to_string()),
            items: vec![OrderItemInput {
                product_id: product_id.clone(),
                quantity: 100.0,
            }],
        },
    )[0]
    .0
    .clone();
    let o2 = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("ID-4B".to_string()),
            items: vec![OrderItemInput {
                product_id: product_id.clone(),
                quantity: 50.0,
            }],
        },
    )[0]
    .0
    .clone();

    confirm(&db, &o1).expect("confirm receipt 1");
    confirm(&db, &o2).expect("confirm receipt 2");

    let layers = layers(&db, &unit_id, &product_id);
    assert_eq!(layers.len(), 2);
    let total: f64 = layers.iter().map(|l| l.qty_remaining).sum();
    assert_eq!(total, 150.0);
    let total_value: f64 = layers.iter().map(|l| l.unit_cost * l.qty_remaining).sum();
    assert_eq!(total_value, 6000.0, "100×40 + 50×40");
}

// --- 5. valuation uses snapshot TTC, never base_price ----------------------

#[test]
fn valuation_uses_snapshot_ttc_not_base_price() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_id = setup_supplier(&db, "Supplier A");
    // base_price = 999.0 on purpose; purchase TTC = 120.00/box → 12.00/kg.
    let product_id = setup_product(&db, "Sugar Box", 8, 1, 10);
    let contract_id = setup_contract(&db, &unit_id, &supplier_id, 2027);
    add_product_to_contract(
        &db,
        &unit_id,
        &contract_id,
        &product_id,
        2027,
        "alloc-ttc",
        100.0,
        120.00,
    );

    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("ID-5".to_string()),
            items: vec![OrderItemInput {
                product_id: product_id.clone(),
                quantity: 1.0,
            }],
        },
    );
    confirm(&db, &created[0].0.clone()).expect("confirm");

    let (qty, value) = db
        .executor()
        .fifo_layers()
        .get_global_quantity_and_value_for_product(&product_id)
        .unwrap();
    assert_eq!(qty, 10.0);
    assert_eq!(value, 120.00); // 12.00/kg × 10 kg  (NOT base_price 999)
}

// --- 6. inventory stock identity is (product_id, consumption_unit) ---------

#[test]
fn inventory_stock_is_keyed_by_product_and_consumption_unit() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_id = setup_supplier(&db, "Supplier A");
    let product_id = setup_product(&db, "Tomato Paste", 8, 1, 10);
    let contract_id = setup_contract(&db, &unit_id, &supplier_id, 2027);
    add_product_to_contract(
        &db,
        &unit_id,
        &contract_id,
        &product_id,
        2027,
        "alloc-stock-key",
        100.0,
        30.00,
    );

    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("ID-6".to_string()),
            items: vec![OrderItemInput {
                product_id: product_id.clone(),
                quantity: 2.0, // 2 boxes → 20 kg
            }],
        },
    );
    confirm(&db, &created[0].0.clone()).expect("confirm");

    let stock = db
        .executor()
        .inventory()
        .get_stock_typed(&product_id, Some(1))
        .expect("read keyed stock")
        .expect("keyed stock present");
    assert_eq!(stock.quantity, 20.0);
    assert_eq!(stock.consumption_unit, Some(1));

    // A legacy NULL-key row can coexist without clobbering the keyed row.
    db.executor()
        .inventory()
        .update_stock_typed(&product_id, None, 0.0)
        .expect("seed legacy row");
    let keyed = db
        .executor()
        .inventory()
        .get_stock_typed(&product_id, Some(1))
        .expect("read keyed stock again")
        .expect("keyed stock still present");
    assert_eq!(keyed.quantity, 20.0, "keyed row unaffected by legacy row");
}

// --- 7. persisted snapshot is immutable against later contract changes -----

#[test]
fn unit_snapshot_is_immutable_against_contract_changes() {
    let mut db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_id = setup_supplier(&db, "Supplier A");
    let product_id = setup_product(&db, "Milk Carton", 8, 1, 10);
    let contract_id = setup_contract(&db, &unit_id, &supplier_id, 2027);
    add_product_to_contract(
        &db,
        &unit_id,
        &contract_id,
        &product_id,
        2027,
        "alloc-immutable",
        100.0,
        45.00,
    );

    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("ID-7".to_string()),
            items: vec![OrderItemInput {
                product_id: product_id.clone(),
                quantity: 1.0,
            }],
        },
    );
    let order_id = created[0].0.clone();

    // Snapshot captured at creation is the exact creation-time config.
    let item = &items(&db, &order_id)[0];
    assert_eq!(item.purchase_unit, Some(8));
    assert_eq!(item.consumption_unit, Some(1));
    assert_eq!(item.conversion_factor, Some(10));

    // Change the contract unit WITHOUT touching the price: confirmation must
    // reject (immutability), because the receipt would otherwise convert with
    // stale semantics.
    db.get_connection()
        .execute(
            "UPDATE contract_products SET conversion_factor = 5 WHERE product_id = ?1",
            rusqlite::params![product_id],
        )
        .expect("simulate unit drift after creation");

    let result = db.with_transaction(|tx| {
        OrderService::new(tx).confirm_order_atomic(&order_id, "phase5", "phase5")
    });
    assert!(result.is_err(), "unit drift must reject confirmation");
    assert_eq!(order(&db, &order_id).status, OrderStatus::Draft);
    assert_eq!(layers(&db, &unit_id, &product_id).len(), 0);
}

// --- 8. partial persisted snapshot fails closed ----------------------------

#[test]
fn partial_persisted_snapshot_fails_closed() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_id = setup_supplier(&db, "Supplier A");
    let product_id = setup_product(&db, "Cheese", 1, 1, 1);
    let contract_id = setup_contract(&db, &unit_id, &supplier_id, 2027);
    add_product_to_contract(
        &db,
        &unit_id,
        &contract_id,
        &product_id,
        2027,
        "alloc-partial",
        100.0,
        50.00,
    );

    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("ID-8".to_string()),
            items: vec![OrderItemInput {
                product_id: product_id.clone(),
                quantity: 5.0,
            }],
        },
    );
    let order_id = created[0].0.clone();

    // Corrupt the persisted snapshot by clearing one field only.
    db.get_connection()
        .execute(
            "UPDATE supplier_order_items SET consumption_unit = NULL WHERE order_id = ?1",
            rusqlite::params![order_id],
        )
        .expect("corrupt snapshot");

    let err = confirm(&db, &order_id).expect_err("partial snapshot must fail closed");
    assert!(
        err.to_string().contains("غير مكتمل"),
        "unexpected error: {err}"
    );
    assert_eq!(order(&db, &order_id).status, OrderStatus::Draft);
    assert_eq!(layers(&db, &unit_id, &product_id).len(), 0);
}

// --- 9. atomic rollback when the SECOND item fails after the first converts --

#[test]
fn snapshot_drift_mid_confirm_rolls_back_everything() {
    let mut db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_id = setup_supplier(&db, "Supplier A");
    let product1 = setup_product(&db, "Product One", 1, 1, 1);
    let product2 = setup_product(&db, "Product Two", 1, 1, 1);
    let contract_id = setup_contract(&db, &unit_id, &supplier_id, 2027);
    let allocation1 = add_product_to_contract(
        &db,
        &unit_id,
        &contract_id,
        &product1,
        2027,
        "alloc-rollback-a",
        100.0,
        40.00,
    );
    let allocation2 = add_product_to_contract(
        &db,
        &unit_id,
        &contract_id,
        &product2,
        2027,
        "alloc-rollback-b",
        100.0,
        40.00,
    );

    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("ID-9".to_string()),
            items: vec![
                OrderItemInput {
                    product_id: product1.clone(),
                    quantity: 10.0,
                },
                OrderItemInput {
                    product_id: product2.clone(),
                    quantity: 10.0,
                },
            ],
        },
    );
    let order_id = created[0].0.clone();

    // Drift the SECOND product's unit snapshot only: item 1 would fully convert
    // before item 2 trips the immutability guard.
    db.get_connection()
        .execute(
            "UPDATE contract_products SET purchase_unit = 2, conversion_factor = 1 WHERE product_id = ?1",
            rusqlite::params![product2],
        )
        .expect("simulate second-product unit drift");

    let result = db.with_transaction(|tx| {
        OrderService::new(tx).confirm_order_atomic(&order_id, "phase5", "phase5")
    });
    assert!(
        result.is_err(),
        "second-item drift must reject confirmation"
    );

    // Item 1's reservation→fulfillment conversion must be rolled back, and the
    // whole order must keep its prior state.
    assert_eq!(allocation(&db, &allocation1).fulfilled_quantity, 0.0);
    assert_eq!(allocation(&db, &allocation1).reserved_quantity, 10.0);
    assert_eq!(allocation(&db, &allocation2).fulfilled_quantity, 0.0);
    assert_eq!(allocation(&db, &allocation2).reserved_quantity, 10.0);
    assert_eq!(order(&db, &order_id).status, OrderStatus::Draft);
    assert_eq!(movements(&db, &order_id).len(), 0);
    assert_eq!(layers(&db, &unit_id, &product1).len(), 0);
    assert_eq!(layers(&db, &unit_id, &product2).len(), 0);
}

// --- 10. confirmation is a zero-sum reservation → fulfillment transfer -----

#[test]
fn confirmation_is_zero_sum_reservation_to_fulfillment() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_id = setup_supplier(&db, "Supplier A");
    let product_id = setup_product(&db, "Couscous", 1, 1, 1);
    let contract_id = setup_contract(&db, &unit_id, &supplier_id, 2027);
    let allocation_id = add_product_to_contract(
        &db,
        &unit_id,
        &contract_id,
        &product_id,
        2027,
        "alloc-zerosum",
        100.0,
        25.00,
    );

    let a_before = allocation(&db, &allocation_id);
    assert_eq!(a_before.reserved_quantity, 0.0);
    assert_eq!(a_before.fulfilled_quantity, 0.0);

    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("ID-10".to_string()),
            items: vec![OrderItemInput {
                product_id: product_id.clone(),
                quantity: 30.0,
            }],
        },
    );
    let order_id = created[0].0.clone();
    let reserved = allocation(&db, &allocation_id).reserved_quantity;
    assert_eq!(reserved, 30.0);

    confirm(&db, &order_id).expect("confirm zero-sum");

    let a = allocation(&db, &allocation_id);
    assert_eq!(a.fulfilled_quantity, 30.0);
    assert_eq!(a.reserved_quantity, 0.0);
    // Inventory, FIFO and movement all land at consumption quantity 30.0.
    let stock = db
        .executor()
        .inventory()
        .get_stock_typed(&product_id, Some(1))
        .expect("read stock")
        .expect("stock present");
    assert_eq!(stock.quantity, 30.0);
    assert_eq!(layers(&db, &unit_id, &product_id)[0].qty_remaining, 30.0);
    assert_eq!(movements(&db, &order_id)[0].quantity, 30.0);
}

// --- 11. legacy order items (NULL snapshot) convert 1:1 --------------------

#[test]
fn legacy_order_item_without_snapshot_converts_11() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_id = setup_supplier(&db, "Supplier A");
    let product_id = setup_product(&db, "Bread", 1, 1, 1);
    let contract_id = setup_contract(&db, &unit_id, &supplier_id, 2027);
    add_product_to_contract(
        &db,
        &unit_id,
        &contract_id,
        &product_id,
        2027,
        "alloc-legacy",
        100.0,
        10.00,
    );

    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("ID-11".to_string()),
            items: vec![OrderItemInput {
                product_id: product_id.clone(),
                quantity: 4.0,
            }],
        },
    );
    let order_id = created[0].0.clone();

    // Simulate a pre-Phase-5 order by nulling the snapshot columns.
    db.get_connection()
        .execute(
            "UPDATE supplier_order_items SET purchase_unit = NULL, consumption_unit = NULL, conversion_factor = NULL, consumption_quantity = NULL WHERE order_id = ?1",
            rusqlite::params![order_id],
        )
        .expect("simulate legacy item");

    confirm(&db, &order_id).expect("legacy order confirms under factor 1");

    let layer = &layers(&db, &unit_id, &product_id)[0];
    assert_eq!(layer.qty_remaining, 4.0); // 1:1
    assert_eq!(layer.unit_cost, 10.00);
    // Factor-1 identity: purchase quantities equal the consumption ones, and the
    // snapshot unit codes stay NULL (legacy item keeps its purchase semantics).
    assert_eq_w(layer.purchase_quantity, 4.0);
    assert_eq_w(layer.purchase_unit_cost, 10.00);
    assert_eq!(layer.purchase_unit, None);
    assert_eq!(layer.consumption_unit, None);
    assert_eq!(layer.conversion_factor, None);
}
