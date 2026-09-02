use grpc_lib::db::ConnectionFactory;
use uuid::Uuid;

fn init_db() -> grpc_lib::db::Database {
    let db = ConnectionFactory::new_for_test().expect("test db init failed");
    let ex = db.executor();

    // Setup settings for UNIT node
    ex.execute(
        "UPDATE settings SET node_type='UNIT', unit_name='test_unit', wilaya_code='99', configured=1 WHERE id=1",
        [],
    ).unwrap();

    // Insert a unit row (fifo_stock_layers.unit_id FK → units.id)
    ex.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES ('test_unit', 'TU', 'Test Unit', '99', '2025-01-01T00:00:00Z')",
        [],
    ).unwrap_or_else(|e| panic!("Failed to insert unit: {}", e));

    // Insert products
    let now = "2025-01-01T00:00:00Z";
    ex.execute(
        "INSERT OR IGNORE INTO products (id, name, base_price, year, created_at) VALUES ('p1', 'Farine', 100.0, 2025, ?1)",
        rusqlite::params![now],
    ).unwrap();
    ex.execute(
        "INSERT OR IGNORE INTO products (id, name, base_price, year, created_at) VALUES ('p2', 'Huile', 200.0, 2025, ?1)",
        rusqlite::params![now],
    ).unwrap();
    ex.execute(
        "INSERT OR IGNORE INTO products (id, name, base_price, year, created_at) VALUES ('p3', 'Sucre', 150.0, 2025, ?1)",
        rusqlite::params![now],
    ).unwrap();

    db
}

/// Helper: create a FIFO layer.
fn create_layer(
    db: &grpc_lib::db::Database,
    product_id: &str,
    unit_cost: f64,
    qty: f64,
    source_type: &str,
    days_ago: i32,
) {
    let id = Uuid::new_v4().to_string();
    let received_at = chrono::Utc::now() - chrono::Duration::days(days_ago as i64);
    let received_at_str = received_at.to_rfc3339();

    db.executor()
        .execute(
            "INSERT INTO fifo_stock_layers (id, unit_id, product_id, source_type, source_id, unit_cost, qty_original, qty_remaining, received_at, created_by, origin_fiscal_year)
             VALUES (?1, 'test_unit', ?2, ?3, NULL, ?4, ?5, ?5, ?6, 'test_user', ?7)",
            rusqlite::params![id, product_id, source_type, unit_cost, qty, received_at_str, 2025],
        )
        .unwrap_or_else(|e| panic!("insert layer failed: {}", e));
}

// ─── Test 1: single_product_single_layer ───────────────────────────────────

#[test]
fn single_product_single_layer() {
    let db = init_db();
    create_layer(&db, "p1", 500.0, 100.0, "ORDER", 10);

    let view = grpc_lib::application::services::StockLevelService::new(db.executor())
        .get_inventory_fifo_view("test_unit")
        .unwrap();

    assert_eq!(view.total_products, 1);
    assert_eq!(view.total_active_layers, 1);
    assert_eq!(view.total_inventory_value, 50000.0); // 100 * 500

    let p = &view.products[0];
    assert_eq!(p.product_id, "p1");
    assert_eq!(p.total_quantity, 100.0);
    assert_eq!(p.total_value, 50000.0);
    assert_eq!(p.layer_count, 1);
    assert_eq!(p.layers.len(), 1);

    let l = &p.layers[0];
    assert_eq!(l.qty_remaining, 100.0);
    assert_eq!(l.unit_cost, 500.0);
    assert_eq!(l.layer_value, 50000.0);
}

// ─── Test 2: single_product_multi_layer ────────────────────────────────────

#[test]
fn single_product_multi_layer() {
    let db = init_db();
    create_layer(&db, "p1", 500.0, 100.0, "ORDER", 20);
    create_layer(&db, "p1", 800.0, 50.0, "ORDER", 10);

    let view = grpc_lib::application::services::StockLevelService::new(db.executor())
        .get_inventory_fifo_view("test_unit")
        .unwrap();

    assert_eq!(view.total_products, 1);
    assert_eq!(view.total_active_layers, 2);
    assert_eq!(view.total_inventory_value, 90000.0); // 100*500 + 50*800

    let p = &view.products[0];
    assert_eq!(p.total_quantity, 150.0);
    assert_eq!(p.total_value, 90000.0);
    assert_eq!(p.layer_count, 2);
    assert_eq!(p.layers.len(), 2);

    // Layers ordered by received_at ASC
    assert_eq!(p.layers[0].qty_remaining, 100.0);
    assert_eq!(p.layers[0].unit_cost, 500.0);
    assert_eq!(p.layers[1].qty_remaining, 50.0);
    assert_eq!(p.layers[1].unit_cost, 800.0);
}

// ─── Test 3: multi_product ─────────────────────────────────────────────────

#[test]
fn multi_product() {
    let db = init_db();
    create_layer(&db, "p1", 500.0, 100.0, "ORDER", 10);
    create_layer(&db, "p2", 2000.0, 20.0, "ORDER", 5);

    let view = grpc_lib::application::services::StockLevelService::new(db.executor())
        .get_inventory_fifo_view("test_unit")
        .unwrap();

    assert_eq!(view.total_products, 2);
    assert_eq!(view.total_active_layers, 2);
    assert_eq!(view.total_inventory_value, 90000.0); // 100*500 + 20*2000

    // Products sorted by name ASC
    assert_eq!(view.products[0].product_name, "Farine");
    assert_eq!(view.products[1].product_name, "Huile");
}

// ─── Test 4: opening_balance_layer ─────────────────────────────────────────

#[test]
fn opening_balance_layer() {
    let db = init_db();
    create_layer(&db, "p1", 500.0, 100.0, "OPENING", 365);

    let view = grpc_lib::application::services::StockLevelService::new(db.executor())
        .get_inventory_fifo_view("test_unit")
        .unwrap();

    assert_eq!(view.total_products, 1);
    let p = &view.products[0];
    assert_eq!(p.layer_count, 1);
    assert_eq!(p.layers[0].source_type.as_deref(), Some("OPENING"));
    assert_eq!(p.layers[0].qty_remaining, 100.0);
    assert_eq!(p.layers[0].unit_cost, 500.0);
}

// ─── Test 5: global_totals ─────────────────────────────────────────────────

#[test]
fn global_totals() {
    let db = init_db();
    create_layer(&db, "p1", 500.0, 100.0, "ORDER", 10);
    create_layer(&db, "p1", 800.0, 50.0, "ORDER", 5);
    create_layer(&db, "p2", 2000.0, 20.0, "ORDER", 3);
    create_layer(&db, "p3", 1000.0, 30.0, "OPENING", 400);

    let view = grpc_lib::application::services::StockLevelService::new(db.executor())
        .get_inventory_fifo_view("test_unit")
        .unwrap();

    assert_eq!(view.total_products, 3);
    assert_eq!(view.total_active_layers, 4);

    // p1: 100*500 + 50*800 = 90000
    // p2: 20*2000 = 40000
    // p3: 30*1000 = 30000
    assert_eq!(view.total_inventory_value, 160000.0);

    // Products sorted by name: Farine, Huile, Sucre
    assert_eq!(view.products[0].product_name, "Farine");
    assert_eq!(view.products[0].layer_count, 2);
    assert_eq!(view.products[1].product_name, "Huile");
    assert_eq!(view.products[1].layer_count, 1);
    assert_eq!(view.products[2].product_name, "Sucre");
    assert_eq!(view.products[2].layer_count, 1);
}

// ─── Test 6: empty_inventory ───────────────────────────────────────────────

#[test]
fn empty_inventory() {
    let db = init_db();

    let view = grpc_lib::application::services::StockLevelService::new(db.executor())
        .get_inventory_fifo_view("test_unit")
        .unwrap();

    assert_eq!(view.total_products, 0);
    assert_eq!(view.total_active_layers, 0);
    assert_eq!(view.total_inventory_value, 0.0);
    assert_eq!(view.products.len(), 0);
}
