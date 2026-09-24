//! SEC-087 Phase 4B (ADR-0056) — multi-supplier auto-splitting creation.
//!
//! The create path now plans the whole request (greedy-drain, oldest-first)
//! BEFORE any write, then materializes exactly one SupplierOrder per supplier
//! inside a single transactional audit unit. These tests pin:
//!   - multi-supplier request => N SupplierOrders in one transaction, no
//!     partial writes on failure;
//!   - same-supplier multi-portions => ONE SupplierOrder with one
//!     SupplierOrderItem per portion and one allocation leg per item;
//!   - `reference_number` copied to every resulting order;
//!   - reservation-ledger correctness after creation;
//!   - confirmation of every resulting order remains order-independent and
//!     converts each reservation to fulfillment (existing invariants intact).

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
            "PHASE4B_UNIT",
            "Phase 4B Unit",
            "01",
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

fn setup_product(db: &grpc_lib::db::Database, name: &str) -> String {
    let product_id = format!("prod-{}", uuid::Uuid::new_v4());
    let config =
        grpc_lib::domain::validation::validate_product_units(Some(1), Some(1), Some(1), Some(0))
            .expect("valid product config");
    db.executor()
        .products()
        .insert_product(
            &product_id,
            &CreateProductRequest {
                name: name.to_string(),
                base_price: 999.0,
                purchase_unit: Some(1),
                consumption_unit: Some(1),
                conversion_factor: Some(1),
                tva_classification: Some(0),
            },
            2024,
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
                contract_reference: format!("CTR-P4B-{fy}"),
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
    allocation_created_at: &str,
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
        "price_ttc must equal frozen agreed price (TVA 0%), got {frozen}"
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
            allocation_created_at,
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

fn order(db: &grpc_lib::db::Database, order_id: &str) -> grpc_lib::models::SupplierOrder {
    db.executor()
        .orders()
        .get_supplier_order(order_id)
        .expect("read order")
        .expect("order present")
}

fn items(db: &grpc_lib::db::Database, order_id: &str) -> Vec<grpc_lib::models::SupplierOrderItem> {
    db.executor()
        .orders()
        .get_supplier_order_items(order_id)
        .expect("read items")
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

fn stock_movements(db: &grpc_lib::db::Database, order_id: &str) -> i64 {
    db.executor()
        .stock_movements()
        .count_stock_movements(&StockMovementQuery {
            reference_id: Some(order_id.to_string()),
            ..Default::default()
        })
        .expect("count movements")
}

fn fifo_layers(db: &grpc_lib::db::Database, unit_id: &str, product_id: &str) -> usize {
    db.executor()
        .fifo_layers()
        .get_layers_for_product(unit_id, product_id)
        .expect("read layers")
        .len()
}

fn order_count(db: &grpc_lib::db::Database) -> usize {
    db.executor()
        .orders()
        .list_supplier_orders(None)
        .expect("list orders")
        .len()
}

/// A single product spanning two suppliers across fiscal years must split into
/// two SupplierOrders (obligations first), each fully reserved, atomically.
#[test]
fn product_spanning_suppliers_splits_into_two_orders_in_one_transaction() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_x = setup_supplier(&db, "Supplier X");
    let supplier_y = setup_supplier(&db, "Supplier Y");
    let product_id = setup_product(&db, "Shared Product");

    // X owns the older obligation (FY2026, 200 @ 100); Y owns the current
    // entitlement (FY2027, 300 @ 120).
    let contract_x = setup_contract(&db, &unit_id, &supplier_x, 2026);
    let alloc_x = add_product_to_contract(
        &db,
        &unit_id,
        &contract_x,
        &product_id,
        2026,
        "alloc-x-2026",
        "2026-01-01T00:00:00Z",
        200.0,
        100.0,
    );
    let contract_y = setup_contract(&db, &unit_id, &supplier_y, 2027);
    let alloc_y = add_product_to_contract(
        &db,
        &unit_id,
        &contract_y,
        &product_id,
        2027,
        "alloc-y-2027",
        "2027-01-01T00:00:00Z",
        300.0,
        120.0,
    );

    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("REF-SPLIT".to_string()),
            items: vec![OrderItemInput {
                product_id: product_id.clone(),
                quantity: 250.0,
            }],
        },
    );

    assert_eq!(created.len(), 2, "one order per supplier");
    let id_x = created[0].0.clone();
    let id_y = created[1].0.clone();
    assert_ne!(id_x, id_y);

    let order_x = order(&db, &id_x);
    let order_y = order(&db, &id_y);
    assert_eq!(
        order_x.supplier_id, supplier_x,
        "X order carries supplier X"
    );
    assert_eq!(
        order_y.supplier_id, supplier_y,
        "Y order carries supplier Y"
    );
    assert_eq!(
        order_x.fiscal_year,
        Some(2027),
        "I10 anchor on every header"
    );
    assert_eq!(order_y.fiscal_year, Some(2027));
    assert_eq!(order_x.reference_number.as_deref(), Some("REF-SPLIT"));
    assert_eq!(order_y.reference_number.as_deref(), Some("REF-SPLIT"));
    assert_eq!(order_x.status, OrderStatus::Draft);
    assert_eq!(order_y.status, OrderStatus::Draft);

    // X total = 200 × 100 = 20_000; Y total = 50 × 120 = 6_000.
    assert_eq!(order_x.total_amount, Some(20_000.0));
    assert_eq!(order_y.total_amount, Some(6_000.0));

    // One SupplierOrderItem per portion, one allocation leg per item.
    let items_x = items(&db, &id_x);
    let items_y = items(&db, &id_y);
    assert_eq!(items_x.len(), 1);
    assert_eq!(items_y.len(), 1);
    assert_eq!(items_x[0].product_id, product_id);
    assert_eq!(items_x[0].quantity, 200.0);
    assert_eq!(items_y[0].product_id, product_id);
    assert_eq!(items_y[0].quantity, 50.0);

    // Reservation ledger exact.
    assert_eq!(allocation(&db, &alloc_x).reserved_quantity, 200.0);
    assert_eq!(allocation(&db, &alloc_y).reserved_quantity, 50.0);
    assert_eq!(allocation(&db, &alloc_x).fulfilled_quantity, 0.0);
    assert_eq!(allocation(&db, &alloc_y).fulfilled_quantity, 0.0);
}

/// Same supplier, two allocations (older + current): ONE SupplierOrder with
/// multiple item rows, greedy-drain full-then-partial.
#[test]
fn same_supplier_multi_portion_is_one_order_with_multiple_item_rows() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier = setup_supplier(&db, "Sole Supplier");
    let product_id = setup_product(&db, "Portioned Product");

    let contract_old = setup_contract(&db, &unit_id, &supplier, 2026);
    let alloc_old = add_product_to_contract(
        &db,
        &unit_id,
        &contract_old,
        &product_id,
        2026,
        "alloc-old-2026",
        "2026-01-01T00:00:00Z",
        200.0,
        100.0,
    );
    let contract_cur = setup_contract(&db, &unit_id, &supplier, 2027);
    let alloc_cur = add_product_to_contract(
        &db,
        &unit_id,
        &contract_cur,
        &product_id,
        2027,
        "alloc-cur-2027",
        "2027-01-01T00:00:00Z",
        300.0,
        120.0,
    );

    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("REF-ONE".to_string()),
            items: vec![OrderItemInput {
                product_id: product_id.clone(),
                quantity: 250.0,
            }],
        },
    );

    assert_eq!(created.len(), 1, "same supplier => one order");
    let order_id = created[0].0.clone();
    let order = order(&db, &order_id);
    assert_eq!(order.supplier_id, supplier);
    assert_eq!(order.total_amount, Some(26_000.0), "200×100 + 50×120");

    let rows = items(&db, &order_id);
    assert_eq!(rows.len(), 2, "one SupplierOrderItem per portion");
    assert_eq!(rows[0].quantity, 200.0, "older allocation fully drained");
    assert_eq!(rows[0].unit_price, 100.0);
    assert_eq!(rows[1].quantity, 50.0, "only final portion is partial");
    assert_eq!(rows[1].unit_price, 120.0);
    assert_eq!(rows[0].product_id, product_id);
    assert_eq!(rows[1].product_id, product_id);

    // One leg per item.
    assert_eq!(
        db.executor()
            .order_allocations()
            .list_for_item(&rows[0].id)
            .expect("legs")
            .len(),
        1
    );
    assert_eq!(
        db.executor()
            .order_allocations()
            .list_for_item(&rows[1].id)
            .expect("legs")
            .len(),
        1
    );

    assert_eq!(allocation(&db, &alloc_old).reserved_quantity, 200.0);
    assert_eq!(allocation(&db, &alloc_cur).reserved_quantity, 50.0);
}

/// Duplicate product rows are aggregated at the boundary: two rows of 150 each
/// behave exactly like one row of 300.
#[test]
fn duplicate_product_rows_are_aggregated_before_planning() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier = setup_supplier(&db, "Sole Supplier");
    let product_id = setup_product(&db, "Aggregated Product");

    let contract_old = setup_contract(&db, &unit_id, &supplier, 2026);
    let alloc_old = add_product_to_contract(
        &db,
        &unit_id,
        &contract_old,
        &product_id,
        2026,
        "alloc-aggregate-old",
        "2026-01-01T00:00:00Z",
        200.0,
        100.0,
    );
    let contract_cur = setup_contract(&db, &unit_id, &supplier, 2027);
    let alloc_cur = add_product_to_contract(
        &db,
        &unit_id,
        &contract_cur,
        &product_id,
        2027,
        "alloc-aggregate-cur",
        "2027-01-01T00:00:00Z",
        300.0,
        120.0,
    );

    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: None,
            items: vec![
                OrderItemInput {
                    product_id: product_id.clone(),
                    quantity: 150.0,
                },
                OrderItemInput {
                    product_id: product_id.clone(),
                    quantity: 150.0,
                },
            ],
        },
    );

    assert_eq!(created.len(), 1);
    let rows = items(&db, &created[0].0.clone());
    assert_eq!(rows.len(), 2, "aggregated 300 spans both allocations");
    assert_eq!(rows[0].quantity, 200.0);
    assert_eq!(rows[1].quantity, 100.0);
    assert_eq!(allocation(&db, &alloc_old).reserved_quantity, 200.0);
    assert_eq!(allocation(&db, &alloc_cur).reserved_quantity, 100.0);
}

/// Over-request must reject the WHOLE request before any write: no orders and
/// no reservations may be created.
#[test]
fn over_request_rejects_whole_request_without_partial_writes() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_x = setup_supplier(&db, "Supplier X");
    let supplier_y = setup_supplier(&db, "Supplier Y");

    let product_a = setup_product(&db, "Product A");
    let product_b = setup_product(&db, "Product B");
    // One live contract per (unit, fiscal_year): X owns 2026, Y owns 2027.
    let contract_x = setup_contract(&db, &unit_id, &supplier_x, 2026);
    let alloc_x = add_product_to_contract(
        &db,
        &unit_id,
        &contract_x,
        &product_a,
        2026,
        "alloc-over-a",
        "2026-01-01T00:00:00Z",
        100.0,
        10.0,
    );
    let contract_y = setup_contract(&db, &unit_id, &supplier_y, 2027);
    let alloc_y = add_product_to_contract(
        &db,
        &unit_id,
        &contract_y,
        &product_b,
        2027,
        "alloc-over-b",
        "2027-01-01T00:00:00Z",
        100.0,
        10.0,
    );

    // 150 > 100 on product A: the FIRST product is in order and fully plan-able,
    // but the request as a whole must be rejected with no writes at all.
    let err = OrderService::new(db.executor())
        .create_supplier_orders(
            &CreateOrderRequest {
                reference_number: Some("REF-BAD".to_string()),
                items: vec![
                    OrderItemInput {
                        product_id: product_a.clone(),
                        quantity: 150.0,
                    },
                    OrderItemInput {
                        product_id: product_b.clone(),
                        quantity: 10.0,
                    },
                ],
            },
            &unit_id,
            2027,
        )
        .expect_err("over-request must reject the entire request");

    assert!(
        err.to_string().contains("تتجاوز"),
        "explanatory error: {err}"
    );
    assert_eq!(order_count(&db), 0, "no partial orders written");
    assert_eq!(allocation(&db, &alloc_x).reserved_quantity, 0.0);
    assert_eq!(allocation(&db, &alloc_y).reserved_quantity, 0.0);
}

/// Confirmation of the order(s) produced by the split path converts each
/// reservation into fulfillment and stocks FIFO layers; a same-supplier
/// multi-portion order confirms order-independently.
#[test]
fn split_created_orders_confirm_end_to_end_and_fulfill_exactly() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_x = setup_supplier(&db, "Conf X");
    let supplier_y = setup_supplier(&db, "Conf Y");

    let product_a = setup_product(&db, "Conf Product A");
    let product_b = setup_product(&db, "Conf Product B");

    // X: same product across two past fiscal years (multi-portion single order).
    let contract_x_old = setup_contract(&db, &unit_id, &supplier_x, 2025);
    let alloc_x_old = add_product_to_contract(
        &db,
        &unit_id,
        &contract_x_old,
        &product_a,
        2025,
        "alloc-conf-2025",
        "2025-01-01T00:00:00Z",
        200.0,
        100.0,
    );
    let contract_x_mid = setup_contract(&db, &unit_id, &supplier_x, 2026);
    let alloc_x_cur = add_product_to_contract(
        &db,
        &unit_id,
        &contract_x_mid,
        &product_a,
        2026,
        "alloc-conf-2026",
        "2026-01-01T00:00:00Z",
        300.0,
        120.0,
    );

    // Y: another product, the current year (one live contract per unit-year).
    let contract_y = setup_contract(&db, &unit_id, &supplier_y, 2027);
    let alloc_y = add_product_to_contract(
        &db,
        &unit_id,
        &contract_y,
        &product_b,
        2027,
        "alloc-conf-y",
        "2027-01-01T00:00:00Z",
        100.0,
        50.0,
    );

    // Request: A 250 (spans X 2025/2026) + B 100 (Y 2027).
    let created = create(
        &db,
        &unit_id,
        2027,
        &CreateOrderRequest {
            reference_number: Some("REF-CONF".to_string()),
            items: vec![
                OrderItemInput {
                    product_id: product_a.clone(),
                    quantity: 250.0,
                },
                OrderItemInput {
                    product_id: product_b.clone(),
                    quantity: 100.0,
                },
            ],
        },
    );
    assert_eq!(created.len(), 2, "X order + Y order");

    // Confirm X's multi-portion order (order-independent by construction) and
    // then Y's order.
    let x_order_id = created[0].0.clone();
    let y_order_id = created[1].0.clone();
    OrderService::new(db.executor())
        .confirm_order_atomic(&x_order_id, "phase4b", "phase4b")
        .expect("confirm multi-portion X order");
    OrderService::new(db.executor())
        .confirm_order_atomic(&y_order_id, "phase4b", "phase4b")
        .expect("confirm Y order");

    assert_eq!(order(&db, &x_order_id).status, OrderStatus::Confirmed);
    assert_eq!(order(&db, &y_order_id).status, OrderStatus::Confirmed);

    // Exact fulfillment: X 2025 = 200 (drained fully), X 2026 = 50 (the residual
    // 50 of the 250 request), Y = 100.
    assert_eq!(allocation(&db, &alloc_x_old).fulfilled_quantity, 200.0);
    assert_eq!(allocation(&db, &alloc_x_cur).fulfilled_quantity, 50.0);
    assert_eq!(allocation(&db, &alloc_y).fulfilled_quantity, 100.0);
    assert_eq!(allocation(&db, &alloc_x_old).reserved_quantity, 0.0);
    assert_eq!(allocation(&db, &alloc_x_cur).reserved_quantity, 0.0);
    assert_eq!(allocation(&db, &alloc_y).reserved_quantity, 0.0);

    // Stock + FIFO per item.
    assert_eq!(stock_movements(&db, &x_order_id), 2);
    assert_eq!(stock_movements(&db, &y_order_id), 1);
    assert_eq!(fifo_layers(&db, &unit_id, &product_a), 2);
    assert_eq!(fifo_layers(&db, &unit_id, &product_b), 1);

    // Re-confirming any order fails closed.
    let err =
        OrderService::new(db.executor()).confirm_order_atomic(&x_order_id, "phase4b", "phase4b");
    assert!(err.is_err(), "already-confirmed order must be rejected");
}
