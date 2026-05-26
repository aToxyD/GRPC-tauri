//! FIFO Meal Cost Integrity Tests
//!
//! Tests the integrity of the FIFO consumption chain:
//! FIFO consume → inventory_layer_consumptions → stock_movements → daily_report_meal_items
//!
//! Each scenario verifies that the total cost recorded in daily_report_meal_items
//! matches the total cost recorded in inventory_layer_consumptions for the same
//! stock movement.

use chrono::{Datelike, NaiveDate, Utc};
use grpc_lib::application::services::{AuditTxService, DailyReportService, UserContext};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::audit::AuditAction;
use grpc_lib::models::{ConsumptionItemInput, DailyReportInput, MealSectionInput, MealType};
use grpc_lib::repositories::{executor::DbExecutor, FifoLayerRepository};
use uuid::Uuid;

// ── Helper Functions ───────────────────────────────────────────────────────────

/// Seed a unit in the database
fn seed_unit(ex: DbExecutor<'_>, id: &str, now: &str) {
    ex.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U01','Test Unit','01',?2)",
        rusqlite::params![id, now],
    ).expect("insert unit");
}

/// Seed a product and its inventory_stocks row
fn seed_product_and_stock(
    ex: DbExecutor<'_>,
    product_id: &str,
    name: &str,
    unit_cost: f64,
    now: &str,
    year: i32,
) {
    ex.execute(
        "INSERT INTO products (id, name, base_price, tva, year, created_at) VALUES (?1,?2,?3,0.0,?4,?5)",
        rusqlite::params![product_id, name, unit_cost, year, now],
    ).expect("insert product");

    ex.execute(
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, last_updated, updated_at) VALUES (?1,?2,0.0,'unit',?3,?3)",
        rusqlite::params![format!("stock-{}", product_id), product_id, now],
    ).expect("insert inventory_stocks");
}

/// Add a FIFO layer for a unit and product
fn add_layer(
    fifo: &FifoLayerRepository,
    unit_id: &str,
    product_id: &str,
    qty: f64,
    unit_cost: f64,
    received_at: &str,
) -> String {
    fifo.create_layer(
        unit_id,
        product_id,
        "ORDER",
        None,
        unit_cost,
        qty,
        received_at,
        "system",
    )
    .expect("create_layer failed")
}

/// Create a meal section input
fn meal(meal_type: MealType, items: Vec<(&str, f64)>) -> MealSectionInput {
    MealSectionInput {
        meal_type,
        staff_24h_count: 10,
        staff_8h_count: 5,
        reservation_count: 0,
        mission_count: 0,
        guest_count: 0,
        items: items
            .into_iter()
            .map(|(product_id, qty)| ConsumptionItemInput {
                product_id: product_id.to_string(),
                quantity: qty,
            })
            .collect(),
    }
}

/// Get the sum of total_cost from inventory_layer_consumptions for a movement
fn get_layer_consumption_sum(ex: DbExecutor<'_>, movement_id: &str) -> f64 {
    ex.query_row(
        "SELECT COALESCE(SUM(total_cost), 0.0) FROM inventory_layer_consumptions WHERE movement_id = ?1",
        rusqlite::params![movement_id],
        |row| row.get(0),
    )
    .unwrap_or(0.0)
}

/// Get the sum of total_cost from inventory_layer_consumptions across ALL movements
/// for a given product within a report (meal-level FIFO creates multiple movements).
fn get_layer_consumption_sum_for_report(
    ex: DbExecutor<'_>,
    report_id: &str,
    product_id: &str,
) -> f64 {
    ex.query_row(
        "SELECT COALESCE(SUM(ilc.total_cost), 0.0)
         FROM inventory_layer_consumptions ilc
         JOIN stock_movements sm ON ilc.movement_id = sm.id
         WHERE sm.reference_id = ?1 AND sm.product_id = ?2",
        rusqlite::params![report_id, product_id],
        |row| row.get(0),
    )
    .unwrap_or(0.0)
}

/// Get the sum of total_cost from daily_report_meal_items for a report and product
fn get_meal_item_sum(ex: DbExecutor<'_>, report_id: &str, product_id: &str) -> f64 {
    ex.query_row(
        "SELECT COALESCE(SUM(drmi.total_cost), 0.0) 
         FROM daily_report_meal_items drmi
         INNER JOIN daily_report_meals drm ON drmi.meal_id = drm.id
         WHERE drm.daily_report_id = ?1 AND drmi.product_id = ?2",
        rusqlite::params![report_id, product_id],
        |row| row.get(0),
    )
    .unwrap_or(0.0)
}

/// Get movement_id for a product from stock_movements by reference_id
fn get_movement_id_for_product(ex: DbExecutor<'_>, reference_id: &str, product_id: &str) -> String {
    ex.query_row(
        "SELECT id FROM stock_movements WHERE reference_id = ?1 AND product_id = ?2",
        rusqlite::params![reference_id, product_id],
        |row| row.get(0),
    )
    .expect("movement_id not found")
}

fn current_test_date_and_year() -> (NaiveDate, i32) {
    let date = Utc::now().date_naive();
    let year = date.year();
    (date, year)
}

/// Assert that inventory_stocks.quantity matches SUM(fifo_stock_layers.qty_remaining)
/// for every given product. Uses tolerance 1e-6.
fn assert_inventory_fifo_consistency(ex: DbExecutor<'_>, unit_id: &str, products: &[&str]) {
    for &product_id in products {
        let fifo_qty: f64 = ex
            .query_row(
                "SELECT COALESCE(SUM(qty_remaining), 0.0)
                 FROM fifo_stock_layers
                 WHERE unit_id = ?1 AND product_id = ?2",
                rusqlite::params![unit_id, product_id],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        let stock_qty: f64 = ex
            .query_row(
                "SELECT COALESCE(quantity, 0.0) FROM inventory_stocks WHERE product_id = ?1",
                rusqlite::params![product_id],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        let diff = (fifo_qty - stock_qty).abs();
        assert!(
            diff < 1e-6,
            "Inventory-FIFO mismatch for product {}: fifo_qty_remaining={}, stock_qty={}, diff={}",
            product_id,
            fifo_qty,
            stock_qty,
            diff
        );
    }
}

/// Sync inventory_stocks.quantity to match FIFO layer quantities before a test.
/// This ensures both tracking systems start consistent.
fn sync_inventory_from_fifo(ex: DbExecutor<'_>, unit_id: &str, product_id: &str) {
    let fifo_qty: f64 = ex
        .query_row(
            "SELECT COALESCE(SUM(qty_remaining), 0.0)
             FROM fifo_stock_layers
             WHERE unit_id = ?1 AND product_id = ?2",
            rusqlite::params![unit_id, product_id],
            |row| row.get(0),
        )
        .unwrap_or(0.0);
    ex.execute(
        "UPDATE inventory_stocks SET quantity = ?1 WHERE product_id = ?2",
        rusqlite::params![fifo_qty, product_id],
    )
    .expect("sync inventory failed");
}

// ── Test Scenarios ───────────────────────────────────────────────────────────

/// Scenario A: Single layer, full consumption
/// P1: 100@500, Breakfast: P1=100
#[test]
fn test_scenario_a_single_layer_full_consumption() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, current_year) = current_test_date_and_year();

    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // Setup
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![current_year, now],
        ).expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Product A", 500.0, &now, current_year);

        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            100.0,
            500.0,
            "2024-01-10T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    // Execute daily report creation
    let input = DailyReportInput {
        date,
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 100.0)])],
    };

    let user_ctx = UserContext::new("system", "system", None);
    let report_id = AuditTxService::execute_with_audit(
        &mut db,
        AuditAction::CreateDailyReport,
        &user_ctx,
        |tx| {
            DailyReportService::new(tx.executor).create_daily_report(
                &input,
                Some(&unit_id),
                "system",
                "system",
            )
        },
    )
    .expect("create_daily_report failed");

    // Verify
    let ex = db.executor();
    let movement_id = get_movement_id_for_product(ex, &report_id, &product_id);
    let layer_sum = get_layer_consumption_sum(ex, &movement_id);
    let meal_sum = get_meal_item_sum(ex, &report_id, &product_id);

    let diff = (layer_sum - meal_sum).abs();
    assert!(
        diff < 1e-9,
        "Cost mismatch: layer_sum={}, meal_sum={}, diff={}",
        layer_sum,
        meal_sum,
        diff
    );

    // Expected: 100 * 500 = 50000
    assert!(
        (layer_sum - 50000.0).abs() < 0.01,
        "Expected layer_sum 50000, got {}",
        layer_sum
    );
    assert!(
        (meal_sum - 50000.0).abs() < 0.01,
        "Expected meal_sum 50000, got {}",
        meal_sum
    );
    assert_inventory_fifo_consistency(ex, &unit_id, &[&product_id]);
}

/// Scenario B: Single layer, partial consumption
/// P1: 100@500, Breakfast: P1=30
#[test]
fn test_scenario_b_single_layer_partial_consumption() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, current_year) = current_test_date_and_year();

    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // Setup
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![current_year, now],
        ).expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Product B", 500.0, &now, current_year);

        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            100.0,
            500.0,
            "2024-01-10T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    // Execute daily report creation
    let input = DailyReportInput {
        date,
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 30.0)])],
    };

    let user_ctx = UserContext::new("system", "system", None);
    let report_id = AuditTxService::execute_with_audit(
        &mut db,
        AuditAction::CreateDailyReport,
        &user_ctx,
        |tx| {
            DailyReportService::new(tx.executor).create_daily_report(
                &input,
                Some(&unit_id),
                "system",
                "system",
            )
        },
    )
    .expect("create_daily_report failed");

    // Verify
    let ex = db.executor();
    let movement_id = get_movement_id_for_product(ex, &report_id, &product_id);
    let layer_sum = get_layer_consumption_sum(ex, &movement_id);
    let meal_sum = get_meal_item_sum(ex, &report_id, &product_id);

    let diff = (layer_sum - meal_sum).abs();
    assert!(
        diff < 1e-9,
        "Cost mismatch: layer_sum={}, meal_sum={}, diff={}",
        layer_sum,
        meal_sum,
        diff
    );

    // Expected: 30 * 500 = 15000
    assert!(
        (layer_sum - 15000.0).abs() < 0.01,
        "Expected layer_sum 15000, got {}",
        layer_sum
    );
    assert!(
        (meal_sum - 15000.0).abs() < 0.01,
        "Expected meal_sum 15000, got {}",
        meal_sum
    );
    assert_inventory_fifo_consistency(ex, &unit_id, &[&product_id]);
}

/// Scenario C: Two layers, one meal
/// P1: 20@500, 50@600, Breakfast: P1=30
#[test]
fn test_scenario_c_two_layers_one_meal() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, current_year) = current_test_date_and_year();

    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // Setup
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![current_year, now],
        ).expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Product C", 500.0, &now, current_year);

        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            20.0,
            500.0,
            "2024-01-10T08:00:00Z",
        );
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            50.0,
            600.0,
            "2024-01-11T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    // Execute daily report creation
    let input = DailyReportInput {
        date,
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 30.0)])],
    };

    let user_ctx = UserContext::new("system", "system", None);
    let report_id = AuditTxService::execute_with_audit(
        &mut db,
        AuditAction::CreateDailyReport,
        &user_ctx,
        |tx| {
            DailyReportService::new(tx.executor).create_daily_report(
                &input,
                Some(&unit_id),
                "system",
                "system",
            )
        },
    )
    .expect("create_daily_report failed");

    // Verify
    let ex = db.executor();
    let movement_id = get_movement_id_for_product(ex, &report_id, &product_id);
    let layer_sum = get_layer_consumption_sum(ex, &movement_id);
    let meal_sum = get_meal_item_sum(ex, &report_id, &product_id);

    let diff = (layer_sum - meal_sum).abs();
    assert!(
        diff < 1e-9,
        "Cost mismatch: layer_sum={}, meal_sum={}, diff={}",
        layer_sum,
        meal_sum,
        diff
    );

    // Expected: 20*500 + 10*600 = 10000 + 6000 = 16000
    assert!(
        (layer_sum - 16000.0).abs() < 0.01,
        "Expected layer_sum 16000, got {}",
        layer_sum
    );
    assert!(
        (meal_sum - 16000.0).abs() < 0.01,
        "Expected meal_sum 16000, got {}",
        meal_sum
    );
    assert_inventory_fifo_consistency(ex, &unit_id, &[&product_id]);
}

/// Scenario D: Two layers, two meals
/// P1: 20@500, 50@600, Breakfast: P1=10, Lunch: P1=20
#[test]
fn test_scenario_d_two_layers_two_meals() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, current_year) = current_test_date_and_year();

    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // Setup
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![current_year, now],
        ).expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Product D", 500.0, &now, current_year);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            20.0,
            500.0,
            "2024-01-10T08:00:00Z",
        );
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            50.0,
            600.0,
            "2024-01-11T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    // Execute daily report creation
    let input = DailyReportInput {
        date,
        meals: vec![
            meal(MealType::Breakfast, vec![(&product_id, 10.0)]),
            meal(MealType::Lunch, vec![(&product_id, 20.0)]),
        ],
    };

    let user_ctx = UserContext::new("system", "system", None);
    let report_id = AuditTxService::execute_with_audit(
        &mut db,
        AuditAction::CreateDailyReport,
        &user_ctx,
        |tx| {
            DailyReportService::new(tx.executor).create_daily_report(
                &input,
                Some(&unit_id),
                "system",
                "system",
            )
        },
    )
    .expect("create_daily_report failed");

    // Verify — meal-level FIFO creates one movement per meal item (two movements).
    let ex = db.executor();
    let layer_sum = get_layer_consumption_sum_for_report(ex, &report_id, &product_id);
    let meal_sum = get_meal_item_sum(ex, &report_id, &product_id);

    let diff = (layer_sum - meal_sum).abs();
    assert!(
        diff < 1e-9,
        "Cost mismatch: layer_sum={}, meal_sum={}, diff={}",
        layer_sum,
        meal_sum,
        diff
    );

    // Total across both meals: 10*500 + 10*500 + 10*600 = 5000 + 5000 + 6000 = 16000
    assert!(
        (layer_sum - 16000.0).abs() < 0.01,
        "Expected layer_sum 16000, got {}",
        layer_sum
    );
    assert!(
        (meal_sum - 16000.0).abs() < 0.01,
        "Expected meal_sum 16000, got {}",
        meal_sum
    );
    assert_inventory_fifo_consistency(ex, &unit_id, &[&product_id]);
}

/// Scenario E: Three layers, one meal
/// P1: 30@400, 40@500, 50@600, Breakfast: P1=90
#[test]
fn test_scenario_e_three_layers_one_meal() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, current_year) = current_test_date_and_year();

    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // Setup
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![current_year, now],
        ).expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Product E", 500.0, &now, current_year);

        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            30.0,
            400.0,
            "2024-01-08T08:00:00Z",
        );
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            40.0,
            500.0,
            "2024-01-09T08:00:00Z",
        );
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            50.0,
            600.0,
            "2024-01-10T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    // Execute daily report creation
    let input = DailyReportInput {
        date,
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 90.0)])],
    };

    let user_ctx = UserContext::new("system", "system", None);
    let report_id = AuditTxService::execute_with_audit(
        &mut db,
        AuditAction::CreateDailyReport,
        &user_ctx,
        |tx| {
            DailyReportService::new(tx.executor).create_daily_report(
                &input,
                Some(&unit_id),
                "system",
                "system",
            )
        },
    )
    .expect("create_daily_report failed");

    // Verify
    let ex = db.executor();
    let movement_id = get_movement_id_for_product(ex, &report_id, &product_id);
    let layer_sum = get_layer_consumption_sum(ex, &movement_id);
    let meal_sum = get_meal_item_sum(ex, &report_id, &product_id);

    let diff = (layer_sum - meal_sum).abs();
    assert!(
        diff < 1e-9,
        "Cost mismatch: layer_sum={}, meal_sum={}, diff={}",
        layer_sum,
        meal_sum,
        diff
    );

    // Expected: 30*400 + 40*500 + 20*600 = 12000 + 20000 + 12000 = 44000
    assert!(
        (layer_sum - 44000.0).abs() < 0.01,
        "Expected layer_sum 44000, got {}",
        layer_sum
    );
    assert!(
        (meal_sum - 44000.0).abs() < 0.01,
        "Expected meal_sum 44000, got {}",
        meal_sum
    );
    assert_inventory_fifo_consistency(ex, &unit_id, &[&product_id]);
}

/// Scenario F: Two products with multiple layers across 3 meals
/// P1: 20@500, 50@600; P2: 100@300
/// Breakfast: P1=10,P2=20; Lunch: P1=20,P2=30; Dinner: P1=5
#[test]
fn test_scenario_f_two_products_multiple_layers_three_meals() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, current_year) = current_test_date_and_year();

    let unit_id = Uuid::new_v4().to_string();
    let product1_id = Uuid::new_v4().to_string();
    let product2_id = Uuid::new_v4().to_string();

    // Setup
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![current_year, now],
        ).expect("seed fiscal year");
        seed_product_and_stock(ex, &product1_id, "Product F1", 500.0, &now, current_year);
        seed_product_and_stock(ex, &product2_id, "Product F2", 300.0, &now, current_year);

        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product1_id,
            20.0,
            500.0,
            "2024-01-10T08:00:00Z",
        );
        add_layer(
            &fifo,
            &unit_id,
            &product1_id,
            50.0,
            600.0,
            "2024-01-11T08:00:00Z",
        );
        add_layer(
            &fifo,
            &unit_id,
            &product2_id,
            100.0,
            300.0,
            "2024-01-10T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product1_id);
    sync_inventory_from_fifo(db.executor(), &unit_id, &product2_id);

    // Execute daily report creation
    let input = DailyReportInput {
        date,
        meals: vec![
            meal(
                MealType::Breakfast,
                vec![(&product1_id, 10.0), (&product2_id, 20.0)],
            ),
            meal(
                MealType::Lunch,
                vec![(&product1_id, 20.0), (&product2_id, 30.0)],
            ),
            meal(MealType::Dinner, vec![(&product1_id, 5.0)]),
        ],
    };

    let user_ctx = UserContext::new("system", "system", None);
    let report_id = AuditTxService::execute_with_audit(
        &mut db,
        AuditAction::CreateDailyReport,
        &user_ctx,
        |tx| {
            DailyReportService::new(tx.executor).create_daily_report(
                &input,
                Some(&unit_id),
                "system",
                "system",
            )
        },
    )
    .expect("create_daily_report failed");

    // Verify Product 1 (meal-level FIFO — 3 movements for P1 across meals)
    let ex = db.executor();
    let layer1_sum = get_layer_consumption_sum_for_report(ex, &report_id, &product1_id);
    let meal1_sum = get_meal_item_sum(ex, &report_id, &product1_id);

    let diff1 = (layer1_sum - meal1_sum).abs();
    assert!(
        diff1 < 1e-9,
        "Product 1 cost mismatch: layer_sum={}, meal_sum={}, diff={}",
        layer1_sum,
        meal1_sum,
        diff1
    );

    // Expected P1: 10*500 + 10*500 + 10*600 + 5*600 = 5000 + 5000 + 6000 + 3000 = 19000
    // Breakfast=10@500, Lunch=10@500+10@600, Dinner=5@600
    assert!(
        (layer1_sum - 19000.0).abs() < 0.01,
        "Expected P1 layer_sum 19000, got {}",
        layer1_sum
    );
    assert!(
        (meal1_sum - 19000.0).abs() < 0.01,
        "Expected P1 meal_sum 19000, got {}",
        meal1_sum
    );

    // Verify Product 2 (2 movements for P2 across meals)
    let layer2_sum = get_layer_consumption_sum_for_report(ex, &report_id, &product2_id);
    let meal2_sum = get_meal_item_sum(ex, &report_id, &product2_id);

    let diff2 = (layer2_sum - meal2_sum).abs();
    assert!(
        diff2 < 1e-9,
        "Product 2 cost mismatch: layer_sum={}, meal_sum={}, diff={}",
        layer2_sum,
        meal2_sum,
        diff2
    );

    // Expected P2: 20*300 + 30*300 = 6000 + 9000 = 15000
    assert!(
        (layer2_sum - 15000.0).abs() < 0.01,
        "Expected P2 layer_sum 15000, got {}",
        layer2_sum
    );
    assert!(
        (meal2_sum - 15000.0).abs() < 0.01,
        "Expected P2 meal_sum 15000, got {}",
        meal2_sum
    );
    assert_inventory_fifo_consistency(ex, &unit_id, &[&product1_id, &product2_id]);
}

/// Scenario G: Fractional quantities
/// P1: 100@500, Breakfast: P1=10.333, Lunch: P1=20.667
#[test]
fn test_scenario_g_fractional_quantities() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, current_year) = current_test_date_and_year();

    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // Setup
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![current_year, now],
        ).expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Product G", 500.0, &now, current_year);

        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            100.0,
            500.0,
            "2024-01-10T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    // Execute daily report creation
    let input = DailyReportInput {
        date,
        meals: vec![
            meal(MealType::Breakfast, vec![(&product_id, 10.333)]),
            meal(MealType::Lunch, vec![(&product_id, 20.667)]),
        ],
    };

    let user_ctx = UserContext::new("system", "system", None);
    let report_id = AuditTxService::execute_with_audit(
        &mut db,
        AuditAction::CreateDailyReport,
        &user_ctx,
        |tx| {
            DailyReportService::new(tx.executor).create_daily_report(
                &input,
                Some(&unit_id),
                "system",
                "system",
            )
        },
    )
    .expect("create_daily_report failed");

    // Verify (meal-level FIFO — 2 movements for P1 across meals)
    let ex = db.executor();
    let layer_sum = get_layer_consumption_sum_for_report(ex, &report_id, &product_id);
    let meal_sum = get_meal_item_sum(ex, &report_id, &product_id);

    let diff = (layer_sum - meal_sum).abs();
    assert!(
        diff < 1e-9,
        "Cost mismatch: layer_sum={}, meal_sum={}, diff={}",
        layer_sum,
        meal_sum,
        diff
    );

    // Expected: 31.0 * 500 = 15500
    let expected = 31.0 * 500.0;
    assert!(
        (layer_sum - expected).abs() < 0.01,
        "Expected layer_sum {}, got {}",
        expected,
        layer_sum
    );
    assert!(
        (meal_sum - expected).abs() < 0.01,
        "Expected meal_sum {}, got {}",
        expected,
        meal_sum
    );
    assert_inventory_fifo_consistency(ex, &unit_id, &[&product_id]);
}
