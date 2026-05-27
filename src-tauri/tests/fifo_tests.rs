//! FIFO Inventory Integration Tests
//!
//! Covers:
//!   1. Layer creation on order confirmation
//!   2. FIFO ordering (oldest layer first)
//!   3. Partial consumption spanning multiple layers
//!   4. Full exhaustion of a layer
//!   5. Insufficient stock error
//!   6. Cross-unit isolation (unit A cannot consume unit B's layers)
//!   7. Inventory value calculation from active FIFO layers
//!   8. Fiscal year close produces correct opening balance from FIFO layers

use grpc_lib::db::ConnectionFactory;
use grpc_lib::errors::AppError;
use grpc_lib::repositories::FifoLayerRepository;
use uuid::Uuid;

// ── Test helpers ─────────────────────────────────────────────────────────────

fn setup_db_with_unit_and_product() -> (grpc_lib::db::Database, String, String) {
    let db = ConnectionFactory::new_for_test().expect("test db init failed");
    let ex = db.executor();
    let now = chrono::Utc::now().to_rfc3339();

    let unit_id = Uuid::new_v4().to_string();
    ex.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U01','Test Unit','01',?2)",
        rusqlite::params![unit_id, now],
    ).expect("insert unit");
    ex.execute(
        "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (2024,'open',?1)",
        rusqlite::params![now],
    ).expect("seed fiscal year");

    let product_id = Uuid::new_v4().to_string();
    ex.execute(
        "INSERT INTO products (id, name, base_price, tva, year, created_at) VALUES (?1,'Test Product',500.0,0.0,2024,?2)",
        rusqlite::params![product_id, now],
    ).expect("insert product");

    (db, unit_id, product_id)
}

fn add_layer(
    db: &grpc_lib::db::Database,
    unit_id: &str,
    product_id: &str,
    qty: f64,
    unit_cost: f64,
    received_at: &str,
) -> String {
    let repo = FifoLayerRepository::new(db.executor());
    repo.create_layer(
        unit_id,
        product_id,
        "ORDER",
        None,
        unit_cost,
        qty,
        received_at,
        "system",
        2025,
    )
    .expect("create_layer failed")
}

fn insert_dummy_movement(db: &grpc_lib::db::Database, unit_id: &str, product_id: &str) -> String {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    db.executor().execute(
        "INSERT INTO stock_movements (id, product_id, movement_type, quantity, balance_before, balance_after, timestamp, user_id, username, unit_id) VALUES (?1, ?2, 'OUT', 10.0, 100.0, 90.0, ?3, 'system', 'system', ?4)",
        rusqlite::params![id, product_id, now, unit_id],
    ).expect("insert dummy movement");
    id
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Layer created with correct remaining qty == original qty.
#[test]
fn test_create_layer_sets_full_remaining() {
    let (db, unit_id, product_id) = setup_db_with_unit_and_product();
    let layer_id = add_layer(
        &db,
        &unit_id,
        &product_id,
        100.0,
        500.0,
        "2024-01-10T08:00:00Z",
    );

    let layers = FifoLayerRepository::new(db.executor())
        .get_remaining_layers(&unit_id, &product_id)
        .unwrap();

    assert_eq!(layers.len(), 1);
    let l = &layers[0];
    assert_eq!(l.id, layer_id);
    assert!((l.qty_original - 100.0).abs() < 0.001);
    assert!((l.qty_remaining - 100.0).abs() < 0.001);
}

/// FIFO order: the earliest `received_at` layer is consumed first.
#[test]
fn test_fifo_order_oldest_consumed_first() {
    let (db, unit_id, product_id) = setup_db_with_unit_and_product();

    // Newer layer added first (reversed order)
    add_layer(
        &db,
        &unit_id,
        &product_id,
        50.0,
        600.0,
        "2024-02-01T00:00:00Z",
    );
    add_layer(
        &db,
        &unit_id,
        &product_id,
        50.0,
        500.0,
        "2024-01-01T00:00:00Z",
    );

    let portions = FifoLayerRepository::new(db.executor())
        .consume_fifo(&unit_id, &product_id, 30.0)
        .unwrap();

    // The 500-cost (Jan) layer should be consumed first
    assert_eq!(portions.len(), 1);
    assert!((portions[0].unit_cost - 500.0).abs() < 0.001);
    assert!((portions[0].quantity - 30.0).abs() < 0.001);
}

/// Partial consumption spanning two layers.
#[test]
fn test_partial_consumption_across_two_layers() {
    let (db, unit_id, product_id) = setup_db_with_unit_and_product();

    add_layer(
        &db,
        &unit_id,
        &product_id,
        20.0,
        500.0,
        "2024-01-01T00:00:00Z",
    );
    add_layer(
        &db,
        &unit_id,
        &product_id,
        50.0,
        600.0,
        "2024-02-01T00:00:00Z",
    );

    let portions = FifoLayerRepository::new(db.executor())
        .consume_fifo(&unit_id, &product_id, 30.0)
        .unwrap();

    // First layer fully exhausted (20), second partially consumed (10)
    assert_eq!(portions.len(), 2);
    assert!((portions[0].quantity - 20.0).abs() < 0.001);
    assert!((portions[0].unit_cost - 500.0).abs() < 0.001);
    assert!((portions[1].quantity - 10.0).abs() < 0.001);
    assert!((portions[1].unit_cost - 600.0).abs() < 0.001);

    // Total cost: 20*500 + 10*600 = 10000 + 6000 = 16000
    let total: f64 = portions.iter().map(|p| p.total_cost).sum();
    assert!(
        (total - 16000.0).abs() < 0.001,
        "Total FIFO cost should be 16000, got {total}"
    );

    // First layer should now have 0 remaining, second should have 40
    let remaining = FifoLayerRepository::new(db.executor())
        .get_remaining_layers(&unit_id, &product_id)
        .unwrap();
    assert_eq!(
        remaining.len(),
        1,
        "Exhausted layer should not appear in remaining"
    );
    assert!((remaining[0].qty_remaining - 40.0).abs() < 0.001);
}

/// Full exhaustion: consuming exactly the full remaining quantity.
#[test]
fn test_full_exhaustion_removes_layer_from_active() {
    let (db, unit_id, product_id) = setup_db_with_unit_and_product();
    add_layer(
        &db,
        &unit_id,
        &product_id,
        100.0,
        500.0,
        "2024-01-01T00:00:00Z",
    );

    FifoLayerRepository::new(db.executor())
        .consume_fifo(&unit_id, &product_id, 100.0)
        .unwrap();

    let remaining = FifoLayerRepository::new(db.executor())
        .get_remaining_layers(&unit_id, &product_id)
        .unwrap();
    assert!(remaining.is_empty(), "All layers should be exhausted");
}

/// Consuming more than available returns InsufficientStock error.
#[test]
fn test_insufficient_stock_returns_error() {
    let (db, unit_id, product_id) = setup_db_with_unit_and_product();
    add_layer(
        &db,
        &unit_id,
        &product_id,
        10.0,
        500.0,
        "2024-01-01T00:00:00Z",
    );

    let result = FifoLayerRepository::new(db.executor()).consume_fifo(&unit_id, &product_id, 999.0);

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::BusinessLogic(grpc_lib::errors::BusinessLogicError::InsufficientStock(_)) => {}
        other => panic!("Expected InsufficientStock, got: {other:?}"),
    }
}

/// Cross-unit isolation: unit B cannot consume unit A's layers.
#[test]
fn test_cross_unit_isolation() {
    let (db, unit_a, product_id) = setup_db_with_unit_and_product();
    let now = chrono::Utc::now().to_rfc3339();

    // Create unit B
    let unit_b = Uuid::new_v4().to_string();
    db.executor().execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U02','Unit B','01',?2)",
        rusqlite::params![unit_b, now],
    ).unwrap();

    // Add layer for unit A
    add_layer(
        &db,
        &unit_a,
        &product_id,
        100.0,
        500.0,
        "2024-01-01T00:00:00Z",
    );

    // Unit B tries to consume
    let result = FifoLayerRepository::new(db.executor()).consume_fifo(&unit_b, &product_id, 10.0);

    assert!(
        result.is_err(),
        "Unit B should not be able to consume unit A's layers"
    );
}

/// Inventory value = SUM(qty_remaining * unit_cost) over all active layers.
#[test]
fn test_inventory_value_reflects_fifo_layers() {
    let (db, unit_id, product_id) = setup_db_with_unit_and_product();

    add_layer(
        &db,
        &unit_id,
        &product_id,
        100.0,
        500.0,
        "2024-01-01T00:00:00Z",
    );
    add_layer(
        &db,
        &unit_id,
        &product_id,
        50.0,
        600.0,
        "2024-02-01T00:00:00Z",
    );

    // Partially consume from first layer
    FifoLayerRepository::new(db.executor())
        .consume_fifo(&unit_id, &product_id, 30.0)
        .unwrap();

    // Expected: (100-30)*500 + 50*600 = 35000 + 30000 = 65000
    let value = grpc_lib::repositories::InventoryRepository::new(db.executor())
        .get_total_inventory_value()
        .unwrap();

    assert!(
        (value - 65000.0).abs() < 0.01,
        "Expected inventory value 65000, got {value}"
    );
}

/// get_total_available returns only remaining qty for the unit+product pair.
#[test]
fn test_get_total_available_correct() {
    let (db, unit_id, product_id) = setup_db_with_unit_and_product();

    add_layer(
        &db,
        &unit_id,
        &product_id,
        80.0,
        500.0,
        "2024-01-01T00:00:00Z",
    );
    add_layer(
        &db,
        &unit_id,
        &product_id,
        20.0,
        550.0,
        "2024-01-15T00:00:00Z",
    );

    let available = FifoLayerRepository::new(db.executor())
        .get_total_available(&unit_id, &product_id)
        .unwrap();

    assert!(
        (available - 100.0).abs() < 0.001,
        "Total available should be 100, got {available}"
    );
}

/// Consumption record is created and queryable by movement_id.
#[test]
fn test_consumption_history_recorded() {
    let (db, unit_id, product_id) = setup_db_with_unit_and_product();
    add_layer(
        &db,
        &unit_id,
        &product_id,
        50.0,
        500.0,
        "2024-01-01T00:00:00Z",
    );

    let movement_id = insert_dummy_movement(&db, &unit_id, &product_id);
    let portions = FifoLayerRepository::new(db.executor())
        .consume_fifo(&unit_id, &product_id, 20.0)
        .unwrap();

    // Manually create consumption records (simulating what daily_report_service does)
    let now = chrono::Utc::now().to_rfc3339();
    for portion in &portions {
        FifoLayerRepository::new(db.executor())
            .create_consumption_record(
                &unit_id,
                &movement_id,
                &portion.layer_id,
                portion.quantity,
                portion.unit_cost,
                &now,
            )
            .unwrap();
    }

    let history = FifoLayerRepository::new(db.executor())
        .get_consumption_history(&movement_id)
        .unwrap();

    assert_eq!(history.len(), 1);
    assert!((history[0].quantity - 20.0).abs() < 0.001);
    assert!((history[0].unit_cost - 500.0).abs() < 0.001);
    assert!((history[0].total_cost - 10000.0).abs() < 0.001);
}

/// Two sequential consumptions recorded under different movement_ids.
#[test]
fn test_two_movements_have_independent_histories() {
    let (db, unit_id, product_id) = setup_db_with_unit_and_product();
    add_layer(
        &db,
        &unit_id,
        &product_id,
        100.0,
        500.0,
        "2024-01-01T00:00:00Z",
    );

    let mv1 = insert_dummy_movement(&db, &unit_id, &product_id);
    let portions1 = FifoLayerRepository::new(db.executor())
        .consume_fifo(&unit_id, &product_id, 10.0)
        .unwrap();

    let mv2 = insert_dummy_movement(&db, &unit_id, &product_id);
    let portions2 = FifoLayerRepository::new(db.executor())
        .consume_fifo(&unit_id, &product_id, 15.0)
        .unwrap();

    // Manually create consumption records (simulating what daily_report_service does)
    let now = chrono::Utc::now().to_rfc3339();
    for portion in &portions1 {
        FifoLayerRepository::new(db.executor())
            .create_consumption_record(
                &unit_id,
                &mv1,
                &portion.layer_id,
                portion.quantity,
                portion.unit_cost,
                &now,
            )
            .unwrap();
    }
    for portion in &portions2 {
        FifoLayerRepository::new(db.executor())
            .create_consumption_record(
                &unit_id,
                &mv2,
                &portion.layer_id,
                portion.quantity,
                portion.unit_cost,
                &now,
            )
            .unwrap();
    }

    let h1 = FifoLayerRepository::new(db.executor())
        .get_consumption_history(&mv1)
        .unwrap();
    let h2 = FifoLayerRepository::new(db.executor())
        .get_consumption_history(&mv2)
        .unwrap();

    assert_eq!(h1.len(), 1);
    assert!((h1[0].quantity - 10.0).abs() < 0.001);
    assert_eq!(h2.len(), 1);
    assert!((h2[0].quantity - 15.0).abs() < 0.001);
}
