//! SEC-087 Phase 4B (ADR-0056) — deterministic confirmation item ordering.
//!
//! The confirmation item-loading query MUST return rows ordered by the resolver's
//! allocation priority key (`fiscal_year ASC, created_at ASC, id ASC`) with
//! `SupplierOrderItem.id` as the final tie-break, and MUST NOT depend on insertion,
//! rowid, index, join, or query-plan order.
//!
//! Confirmation correctness is order-independent under the greedy-drain allocation
//! invariant and Phase 4A own-reservation netting (ADR-0056 §Confirmation Correctness).
//! These tests pin BOTH the explicit query contract and an end-to-end confirmation of
//! a same-supplier multi-portion order whose items were inserted in REVERSED order
//! (the reverse is deliberately the opposite of any "insertion order" reliance).
//!
//! Phase 4B auto-splitting creation is NOT implemented at this baseline, so the
//! multi-portion shape is built at the repository level against the existing schema,
//! exactly as the confirmation path consumes it.

use chrono::Utc;
use grpc_lib::application::services::{ContractService, OrderService};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::models::{
    AddContractProductRequest, CreateContractRequest, CreateProductRequest, CreateSupplierRequest,
    OrderItemInput, OrderStatus, SetAgreedPriceHtRequest, StockMovementQuery,
};
use grpc_lib::repositories::RepositoryProvider;

struct PortionContract {
    allocation_id: String,
}

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
            "ORDER_UNIT",
            "Ordering Unit",
            "01",
            &Utc::now().to_rfc3339(),
        )
        .unwrap();
    unit_id
}

fn setup_supplier(db: &grpc_lib::db::Database) -> String {
    let supplier_id = format!("sup-{}", uuid::Uuid::new_v4());
    db.executor()
        .suppliers()
        .insert_supplier(
            &supplier_id,
            &CreateSupplierRequest {
                name: "Order Determinism Supplier".to_string(),
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
    let contract_id = format!("ctr-{fy}-{}", uuid::Uuid::new_v4());
    db.executor()
        .contracts()
        .insert_contract(
            &contract_id,
            &CreateContractRequest {
                unit_id: unit_id.to_string(),
                supplier_id: supplier_id.to_string(),
                fiscal_year: fy,
                contract_reference: format!("CTR-DET-{fy}"),
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
) -> PortionContract {
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
    PortionContract {
        allocation_id: allocation_id.to_string(),
    }
}

fn insert_order_header(
    db: &grpc_lib::db::Database,
    order_id: &str,
    supplier_id: &str,
    unit_id: &str,
    fy: i32,
    total: f64,
) {
    db.executor()
        .orders()
        .create_supplier_order_header(
            order_id,
            &None,
            supplier_id,
            "Order Determinism Supplier",
            unit_id,
            fy,
            total,
            &Utc::now().date_naive().to_string(),
            &Utc::now().to_rfc3339(),
        )
        .unwrap();
}

#[allow(clippy::too_many_arguments)]
fn insert_item_and_leg(
    db: &grpc_lib::db::Database,
    order_id: &str,
    item_id: &str,
    product_id: &str,
    quantity: f64,
    unit_price: f64,
    unit_id: &str,
    fy: i32,
    allocation_id: &str,
) {
    let ex = db.executor();
    ex.orders()
        .insert_order_item(
            item_id,
            order_id,
            &OrderItemInput {
                product_id: product_id.to_string(),
                quantity,
            },
            unit_price,
            quantity * unit_price,
            unit_id,
            fy,
        )
        .unwrap();
    ex.order_allocations()
        .insert(
            &format!("{item_id}-leg"),
            item_id,
            allocation_id,
            quantity,
            unit_price,
            quantity * unit_price,
            &Utc::now().to_rfc3339(),
        )
        .unwrap();
}

fn confirmation_rows(
    db: &grpc_lib::db::Database,
    order_id: &str,
) -> Vec<(String, f64, String, f64, String)> {
    db.executor()
        .orders()
        .get_order_items_for_confirmation(order_id)
        .expect("load confirmation items")
}

fn allocation_fulfilled(db: &grpc_lib::db::Database, allocation_id: &str) -> f64 {
    db.executor()
        .contracts()
        .get_allocation(allocation_id)
        .expect("read allocation")
        .expect("allocation present")
        .fulfilled_quantity
}

fn allocation_reserved(db: &grpc_lib::db::Database, allocation_id: &str) -> f64 {
    db.executor()
        .contracts()
        .get_allocation(allocation_id)
        .expect("read allocation")
        .expect("allocation present")
        .reserved_quantity
}

fn leg_count(db: &grpc_lib::db::Database, item_id: &str) -> usize {
    db.executor()
        .order_allocations()
        .list_for_item(item_id)
        .expect("read legs")
        .len()
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

/// Same-supplier two-allocation shape across fiscal years (the ADR-0056 locked
/// example): supplier X, allocation A (FY2026, rem 200, price 100) and allocation
/// B (FY2027, rem 300, price 120). Returns the two portion contracts and the unit.
struct TwoPortionFixture {
    unit_id: String,
    product_id: String,
    a: PortionContract,
    b: PortionContract,
    order_id: String,
}

fn setup_two_portion_fixture(
    db: &grpc_lib::db::Database,
    insert_reversed: bool,
) -> TwoPortionFixture {
    open_fiscal_year(db, 2027);
    let unit_id = setup_unit(db);
    let supplier_id = setup_supplier(db);
    let product_id = setup_product(db, "Portioned Product");

    let contract_2026 = setup_contract(db, &unit_id, &supplier_id, 2026);
    let a = add_product_to_contract(
        db,
        &unit_id,
        &contract_2026,
        &product_id,
        2026,
        "alloc-a-2026",
        "2026-01-01T00:00:00Z",
        200.0,
        100.0,
    );
    let contract_2027 = setup_contract(db, &unit_id, &supplier_id, 2027);
    let b = add_product_to_contract(
        db,
        &unit_id,
        &contract_2027,
        &product_id,
        2027,
        "alloc-b-2027",
        "2027-01-01T00:00:00Z",
        300.0,
        120.0,
    );

    let order_id = "order-two-portion-x".to_string();
    insert_order_header(db, &order_id, &supplier_id, &unit_id, 2027, 32_000.0);

    let item_a = (
        "item-a-2026".to_string(),
        200.0,
        100.0,
        a.allocation_id.clone(),
    );
    let item_b = (
        "item-b-2027".to_string(),
        100.0,
        120.0,
        b.allocation_id.clone(),
    );
    let ordered: Vec<(String, f64, f64, String)> = if insert_reversed {
        vec![item_b, item_a]
    } else {
        vec![item_a, item_b]
    };
    for (item_id, quantity, unit_price, allocation_id) in ordered {
        insert_item_and_leg(
            db,
            &order_id,
            &item_id,
            &product_id,
            quantity,
            unit_price,
            &unit_id,
            2027,
            &allocation_id,
        );
    }

    TwoPortionFixture {
        unit_id,
        product_id,
        a,
        b,
        order_id,
    }
}

#[test]
fn confirmation_items_ordered_by_allocation_priority_across_fiscal_years() {
    let db = ConnectionFactory::new_for_test().unwrap();

    // Items are inserted REVERSED (B first, then A). If the query relied on
    // insertion/rowid order, the reverse would be returned; the explicit ORDER BY
    // must return resolver priority (FY2026 before FY2027).
    let f = setup_two_portion_fixture(&db, true);

    let rows = confirmation_rows(&db, &f.order_id);
    assert_eq!(rows.len(), 2, "two portions -> two rows");
    assert_eq!(
        rows[0].4, f.a.allocation_id,
        "older fiscal year must sort first"
    );
    assert_eq!(rows[0].1, 200.0, "portion A quantity");
    assert_eq!(rows[0].3, 100.0, "portion A price");
    assert_eq!(rows[1].4, f.b.allocation_id, "younger fiscal year second");
    assert_eq!(rows[1].1, 100.0, "portion B quantity");
    assert_eq!(rows[1].3, 120.0, "portion B price");
}

#[test]
fn confirmation_items_ordered_by_created_at_then_allocation_id_within_fiscal_year() {
    let db = ConnectionFactory::new_for_test().unwrap();
    open_fiscal_year(&db, 2027);
    let unit_id = setup_unit(&db);
    let supplier_id = setup_supplier(&db);
    let contract_2027 = setup_contract(&db, &unit_id, &supplier_id, 2027);

    // Same fiscal year, distinct created_at and id: order must be created_at ASC,
    // then id ASC among equal created_at.
    let p1 = setup_product(&db, "Product One");
    let c1 = add_product_to_contract(
        &db,
        &unit_id,
        &contract_2027,
        &p1,
        2027,
        "alloc-z-later",
        "2026-01-01T00:00:00Z",
        100.0,
        10.0,
    );
    let p2 = setup_product(&db, "Product Two");
    let c2 = add_product_to_contract(
        &db,
        &unit_id,
        &contract_2027,
        &p2,
        2027,
        "alloc-m-earlier",
        "2025-01-01T00:00:00Z",
        100.0,
        10.0,
    );
    let p3 = setup_product(&db, "Product Three");
    let c3 = add_product_to_contract(
        &db,
        &unit_id,
        &contract_2027,
        &p3,
        2027,
        "alloc-n-tie",
        "2025-01-01T00:00:00Z",
        100.0,
        10.0,
    );

    let order_id = "order-tie-break".to_string();
    insert_order_header(&db, &order_id, &supplier_id, &unit_id, 2027, 3000.0);

    // Scrambled insertion order: P1, P3, P2.
    let inserts = [
        ("item-c1", &p1, 100.0, &c1.allocation_id),
        ("item-c3", &p3, 100.0, &c3.allocation_id),
        ("item-c2", &p2, 100.0, &c2.allocation_id),
    ];
    for (item_id, product_id, qty, allocation_id) in inserts {
        insert_item_and_leg(
            &db,
            &order_id,
            item_id,
            product_id,
            qty,
            10.0,
            &unit_id,
            2027,
            allocation_id,
        );
    }

    let rows = confirmation_rows(&db, &order_id);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].4, c2.allocation_id, "earlier created_at first");
    assert_eq!(
        rows[1].4, c3.allocation_id,
        "equal created_at -> id ASC ('m' < 'n')"
    );
    assert_eq!(rows[2].4, c1.allocation_id, "later created_at last");
}

#[test]
fn same_supplier_multi_portion_shape_has_one_row_per_portion_and_one_leg_per_item() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let f = setup_two_portion_fixture(&db, false);

    let rows = confirmation_rows(&db, &f.order_id);
    assert_eq!(
        rows.len(),
        2,
        "one SupplierOrderItem per allocation portion"
    );
    assert_eq!(rows[0].0, f.product_id);
    assert_eq!(rows[1].0, f.product_id);
    assert_eq!(rows[0].1, 200.0);
    assert_eq!(rows[0].3, 100.0);
    assert_eq!(rows[0].4, f.a.allocation_id);
    assert_eq!(rows[1].1, 100.0);
    assert_eq!(rows[1].3, 120.0);
    assert_eq!(rows[1].4, f.b.allocation_id);

    assert_eq!(
        leg_count(&db, "item-a-2026"),
        1,
        "Item A has exactly one leg"
    );
    assert_eq!(
        leg_count(&db, "item-b-2027"),
        1,
        "Item B has exactly one leg"
    );

    let order = db
        .executor()
        .orders()
        .get_supplier_order(&f.order_id)
        .expect("read order")
        .expect("order present");
    assert_eq!(order.total_amount, Some(32_000.0), "header total is exact");
}

#[test]
fn same_supplier_multi_portion_confirms_end_to_end_despite_reversed_insertion() {
    let db = ConnectionFactory::new_for_test().unwrap();

    // Build the multi-portion order with items inserted REVERSED (B before A).
    let f = setup_two_portion_fixture(&db, true);

    // Simulate creation-time reservations exactly as the plan would have laid them.
    let ex = db.executor();
    assert_eq!(
        ex.contracts()
            .try_increment_reserved(&f.a.allocation_id, 200.0)
            .expect("reserve A"),
        1
    );
    assert_eq!(
        ex.contracts()
            .try_increment_reserved(&f.b.allocation_id, 100.0)
            .expect("reserve B"),
        1
    );

    // Confirmation must succeed because correctness is order-independent under
    // greedy-drain + own-reservation netting — NOT because of any row ordering.
    OrderService::new(db.executor())
        .confirm_order_atomic(&f.order_id, "determinism", "determinism")
        .expect("multi-portion order must confirm regardless of insertion order");

    let status = db
        .executor()
        .orders()
        .get_supplier_order(&f.order_id)
        .expect("read order")
        .expect("order present")
        .status;
    assert_eq!(status, OrderStatus::Confirmed);

    assert_eq!(allocation_fulfilled(&db, &f.a.allocation_id), 200.0);
    assert_eq!(allocation_fulfilled(&db, &f.b.allocation_id), 100.0);
    assert_eq!(allocation_reserved(&db, &f.a.allocation_id), 0.0);
    assert_eq!(allocation_reserved(&db, &f.b.allocation_id), 0.0);

    assert_eq!(stock_movement_count_for_order(&db, &f.order_id), 2);
    assert_eq!(fifo_layer_count(&db, &f.unit_id, &f.product_id), 2);
}
