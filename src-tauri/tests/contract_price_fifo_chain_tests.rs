//! Stage 8 verification — SEC-087-F contract price drives FIFO, never the
//! catalog reference price (ADR-0055).
//!
//! Chain under test:
//!   ContractAgreedPrice → allocation agreed_price → resolution unit_price
//!   → supplier_order_items.unit_price → (confirm) FIFO layer unit_cost
//!   → consumption/valuation cost.
//!
//! products.base_price is a reference/price-list value only and must never
//! influence FIFO layer cost, COGS, or inventory valuation.

use chrono::Utc;
use grpc_lib::application::services::{OrderService, StockMovementService};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::models::{
    AddContractProductRequest, CreateContractRequest, CreateOrderRequest, CreateSupplierRequest,
    OrderItemInput,
};
use grpc_lib::repositories::RepositoryProvider;
use uuid::Uuid;

fn setup_contract_chain(db: &grpc_lib::db::Database) -> (String, String, String) {
    let ex = db.executor();
    let now = Utc::now().to_rfc3339();

    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    let supplier_id = Uuid::new_v4().to_string();
    let contract_id = Uuid::new_v4().to_string();
    let contract_product_id = Uuid::new_v4().to_string();
    let allocation_id = Uuid::new_v4().to_string();
    let user_id = Uuid::new_v4().to_string();

    ex.units()
        .upsert_raw_unit(&unit_id, "UNIT_P", "Price Unit", "01", &now)
        .unwrap();
    ex.execute(
        "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (2024, 'open', ?1)",
        rusqlite::params![now],
    )
    .unwrap();
    ex.products()
        .insert_raw_product(
            &grpc_lib::models::Product {
                id: product_id.clone(),
                name: "Fearina".to_string(),
                base_price: 999.0,
                year: 2024,
                created_at: Utc::now(),
            },
            &now,
        )
        .unwrap();
    ex.users()
        .insert_raw_user(
            &user_id,
            "price_admin",
            "Password123",
            "Admin",
            "WILAYA",
            &now,
        )
        .unwrap();
    ex.suppliers()
        .insert_supplier(
            &supplier_id,
            &CreateSupplierRequest {
                name: "Price Supplier".to_string(),
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
                contract_reference: "CTR-FIFO-2024".to_string(),
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
                proposed_price: 999.0,
                agreed_price: Some(40.0),
                contracted_quantity: 1000.0,
            },
            &now,
        )
        .unwrap();
    ex.contracts()
        .set_agreed_price(&contract_product_id, 40.0)
        .unwrap();
    ex.contracts()
        .insert_allocation(
            &allocation_id,
            &contract_id,
            &contract_product_id,
            &unit_id,
            &product_id,
            2024,
            1000.0,
            &now,
        )
        .unwrap();

    (unit_id, product_id, user_id)
}

#[test]
fn agreed_price_reaches_order_item_and_fifo_layer() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let (unit_id, product_id, _user_id) = setup_contract_chain(&db);

    let (order_id, total) = OrderService::new(db.executor())
        .create_supplier_order(
            &CreateOrderRequest {
                reference_number: Some("PRI-001".to_string()),
                items: vec![OrderItemInput {
                    product_id: product_id.clone(),
                    quantity: 50.0,
                }],
            },
            &unit_id,
            2024,
        )
        .expect("create order resolves negotiated price");

    // Contract price (40.0) is authoritative, not the catalog base price (999.0).
    assert!((total - 2000.0).abs() < f64::EPSILON, "total {:?}", total);

    let items = db
        .executor()
        .orders()
        .get_supplier_order_items(&order_id)
        .unwrap();
    assert_eq!(items.len(), 1);
    assert!(
        (items[0].unit_price - 40.0).abs() < f64::EPSILON,
        "item unit_price must be the agreed price"
    );

    OrderService::new(db.executor())
        .confirm_order_atomic(&order_id, "price_admin", "price_admin")
        .expect("confirm order");

    let layers = db
        .executor()
        .fifo_layers()
        .get_layers_for_product(&unit_id, &product_id)
        .unwrap();
    assert_eq!(layers.len(), 1);
    assert!(
        (layers[0].unit_cost - 40.0).abs() < f64::EPSILON,
        "FIFO layer unit_cost {} must come from agreed price",
        layers[0].unit_cost
    );
    assert_eq!(layers[0].source_type, "ORDER");
    assert!((layers[0].qty_remaining - 50.0).abs() < f64::EPSILON);
}

#[test]
fn catalog_base_price_never_drives_consumption_or_valuation() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let (unit_id, product_id, _user_id) = setup_contract_chain(&db);

    let (order_id, _) = OrderService::new(db.executor())
        .create_supplier_order(
            &CreateOrderRequest {
                reference_number: Some("PRI-002".to_string()),
                items: vec![OrderItemInput {
                    product_id: product_id.clone(),
                    quantity: 50.0,
                }],
            },
            &unit_id,
            2024,
        )
        .unwrap();
    OrderService::new(db.executor())
        .confirm_order_atomic(&order_id, "price_admin", "price_admin")
        .unwrap();

    // Inventory valuation uses the agreed-price layer before the change.
    let value_before = db
        .executor()
        .fifo_layers()
        .get_inventory_value_fifo(&unit_id)
        .unwrap();
    assert!((value_before - 2000.0).abs() < f64::EPSILON);

    // Radically change the catalog reference price AFTER receipt.
    db.executor()
        .execute(
            "UPDATE products SET base_price = 3.0 WHERE id = ?1",
            rusqlite::params![product_id],
        )
        .unwrap();

    let value_after = db
        .executor()
        .fifo_layers()
        .get_inventory_value_fifo(&unit_id)
        .unwrap();
    assert!(
        (value_after - 2000.0).abs() < f64::EPSILON,
        "valuation must stay on agreed price, got {:?}",
        value_after
    );

    // Consumption (goods issue) also stays on the historical layer cost.
    let portions = db
        .executor()
        .fifo_layers()
        .preview_consume_fifo(&unit_id, &product_id, 4.0)
        .unwrap();
    assert_eq!(portions.len(), 1);
    assert!(
        (portions[0].unit_cost - 40.0).abs() < f64::EPSILON,
        "consumption unit_cost must stay 40.0 (agreed), got {:?}",
        portions[0].unit_cost
    );
    assert!(
        (portions[0].total_cost - 160.0).abs() < f64::EPSILON,
        "consumption total must be 160.0, got {:?}",
        portions[0].total_cost
    );

    // Stock movement recorded at the agreed price as well.
    let movements = StockMovementService::new(db.executor()).get_stock_movements(
        &grpc_lib::models::StockMovementFilters {
            product_id: Some(product_id),
            unit_id: Some(unit_id),
            reference_type: Some("Order".to_string()),
            ..Default::default()
        },
        0,
        10,
    );
    let movements = movements.expect("list movements");
    assert!(!movements.movements.is_empty(), "no movements recorded");
    for m in &movements.movements {
        assert!(
            (m.unit_cost.expect("movement cost") - 40.0).abs() < f64::EPSILON,
            "movement unit_cost must be agreed price, got {:?}",
            m.unit_cost
        );
    }
}

#[test]
fn per_line_agreed_prices_isolate_fifo_layers() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let now = Utc::now().to_rfc3339();
    let ex = db.executor();

    let unit_id = Uuid::new_v4().to_string();
    let user_id = Uuid::new_v4().to_string();
    let supplier_id = Uuid::new_v4().to_string();
    let product1 = Uuid::new_v4().to_string();
    let product2 = Uuid::new_v4().to_string();

    ex.units()
        .upsert_raw_unit(&unit_id, "UNIT_B", "Bulk Unit", "01", &now)
        .unwrap();
    ex.execute(
        "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (2024, 'open', ?1)",
        rusqlite::params![now],
    )
    .unwrap();
    ex.products()
        .insert_raw_product(
            &grpc_lib::models::Product {
                id: product1.clone(),
                name: "Fearina".to_string(),
                base_price: 999.0,
                year: 2024,
                created_at: Utc::now(),
            },
            &now,
        )
        .unwrap();
    ex.products()
        .insert_raw_product(
            &grpc_lib::models::Product {
                id: product2.clone(),
                name: "Second".to_string(),
                base_price: 1.0,
                year: 2024,
                created_at: Utc::now(),
            },
            &now,
        )
        .unwrap();
    ex.users()
        .insert_raw_user(
            &user_id,
            "price_admin",
            "Password123",
            "Admin",
            "WILAYA",
            &now,
        )
        .unwrap();
    ex.suppliers()
        .insert_supplier(
            &supplier_id,
            &CreateSupplierRequest {
                name: "Bulk Supplier".to_string(),
                contact_info: None,
            },
            &now,
        )
        .unwrap();
    let contract_id = Uuid::new_v4().to_string();
    ex.contracts()
        .insert_contract(
            &contract_id,
            &CreateContractRequest {
                unit_id: unit_id.clone(),
                supplier_id: supplier_id.clone(),
                fiscal_year: 2024,
                contract_reference: "CTR-LANES".to_string(),
                notes: None,
            },
            &now,
        )
        .unwrap();
    // One contract, two product lines with different agreed prices.
    for (product_id, price) in [(&product1, 40.0), (&product2, 25.0)] {
        let cp_id = Uuid::new_v4().to_string();
        ex.contracts()
            .insert_contract_product(
                &cp_id,
                &AddContractProductRequest {
                    contract_id: contract_id.clone(),
                    product_id: product_id.clone(),
                    proposed_price: price,
                    agreed_price: Some(price),
                    contracted_quantity: 1000.0,
                },
                &now,
            )
            .unwrap();
        ex.contracts().set_agreed_price(&cp_id, price).unwrap();
        ex.contracts()
            .insert_allocation(
                &Uuid::new_v4().to_string(),
                &contract_id,
                &cp_id,
                &unit_id,
                product_id,
                2024,
                1000.0,
                &now,
            )
            .unwrap();
    }

    // Order 1: product1 at 40.0; Order 2: product2 at 25.0.
    let (o1, t1) = OrderService::new(db.executor())
        .create_supplier_order(
            &CreateOrderRequest {
                reference_number: Some("LANE-1".to_string()),
                items: vec![OrderItemInput {
                    product_id: product1.clone(),
                    quantity: 10.0,
                }],
            },
            &unit_id,
            2024,
        )
        .unwrap();
    let (o2, t2) = OrderService::new(db.executor())
        .create_supplier_order(
            &CreateOrderRequest {
                reference_number: Some("LANE-2".to_string()),
                items: vec![OrderItemInput {
                    product_id: product2.clone(),
                    quantity: 10.0,
                }],
            },
            &unit_id,
            2024,
        )
        .unwrap();
    assert!((t1 - 400.0).abs() < f64::EPSILON);
    assert!((t2 - 250.0).abs() < f64::EPSILON);

    OrderService::new(db.executor())
        .confirm_order_atomic(&o1, "price_admin", "price_admin")
        .unwrap();
    OrderService::new(db.executor())
        .confirm_order_atomic(&o2, "price_admin", "price_admin")
        .unwrap();

    let layers1 = db
        .executor()
        .fifo_layers()
        .get_layers_for_product(&unit_id, &product1)
        .unwrap();
    assert!((layers1[0].unit_cost - 40.0).abs() < f64::EPSILON);

    let layers2 = db
        .executor()
        .fifo_layers()
        .get_layers_for_product(&unit_id, &product2)
        .unwrap();
    assert!((layers2[0].unit_cost - 25.0).abs() < f64::EPSILON);
    assert!(
        (db.executor()
            .fifo_layers()
            .get_inventory_value_fifo(&unit_id)
            .unwrap()
            - (400.0 + 250.0))
            .abs()
            < f64::EPSILON
    );
}

#[test]
fn contract_agreed_price_domain_type_roundtrips() {
    use grpc_lib::domain::accounting::cost::ContractAgreedPrice;
    use grpc_lib::domain::numeric::Money;

    let p = ContractAgreedPrice::new(Money::from_centimes(4250).unwrap());
    assert_eq!(p.value(), Money::from_centimes(4250).unwrap());
}
