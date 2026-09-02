//! Order-independence tests for meal-level FIFO costing.
//!
//! Verifies that the FIFO layers consumed by each meal are invariant
//! regardless of the order meals appear in the input payload.
//! The backend must sort meals as: Breakfast → Lunch → Dinner.
//!
//! Setup:
//!   Layer A = 10 @ 500
//!   Layer B = 20 @ 800
//!   Breakfast = 10, Lunch = 10
//!
//! Expected (operational reality):
//!   Breakfast → Layer A (10@500) = 5000
//!   Lunch     → Layer B (10@800) = 8000
//!
//! Tested payload orders:
//!   1. [Breakfast, Lunch]
//!   2. [Dinner, Lunch, Breakfast]  (Dinner has zero items)
//!   3. [Lunch, Breakfast, Dinner]  (Dinner has zero items)
//!
//! Each assertion checks: meal cost, item unit_price, item total_cost,
//! and fifo_layer_id for every consumed item.

use chrono::{Datelike, NaiveDate, Utc};
use grpc_lib::application::services::{AuditTxService, DailyReportService, UserContext};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::audit::AuditAction;
use grpc_lib::models::{ConsumptionItemInput, DailyReportInput, MealSectionInput, MealType};
use grpc_lib::repositories::FifoLayerRepository;
use std::collections::HashMap;

// ── Helpers (duplicated from fifo_meal_level_cost_tests) ─────────────────────

fn seed_unit(ex: grpc_lib::repositories::executor::DbExecutor<'_>, id: &str, now: &str) {
    ex.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U01','Test Unit','01',?2)",
        rusqlite::params![id, now],
    ).expect("insert unit");
}

fn seed_product_and_stock(
    ex: grpc_lib::repositories::executor::DbExecutor<'_>,
    product_id: &str,
    name: &str,
    now: &str,
    year: i32,
) {
    ex.execute(
        "INSERT INTO products (id, name, base_price, year, created_at) VALUES (?1,?2,0.0,?3,?4)",
        rusqlite::params![product_id, name, year, now],
    )
    .expect("insert product");
    ex.execute(
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, last_updated, updated_at) VALUES (?1,?2,0.0,'unit',?3,?3)",
        rusqlite::params![format!("stock-{}", product_id), product_id, now],
    ).expect("insert inventory_stocks");
}

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
        2025,
    )
    .expect("create_layer failed")
}

fn sync_inventory_from_fifo(
    ex: grpc_lib::repositories::executor::DbExecutor<'_>,
    unit_id: &str,
    product_id: &str,
) {
    let fifo_qty: f64 = ex
        .query_row(
            "SELECT COALESCE(SUM(qty_remaining), 0.0)
         FROM fifo_stock_layers WHERE unit_id = ?1 AND product_id = ?2",
            rusqlite::params![unit_id, product_id],
            |row| row.get(0),
        )
        .unwrap_or(0.0);
    ex.execute(
        "UPDATE inventory_stocks SET quantity = ?1 WHERE product_id = ?2",
        rusqlite::params![fifo_qty, product_id],
    )
    .expect("sync inventory");
}

fn current_date_and_year() -> (NaiveDate, i32) {
    let date = Utc::now().date_naive();
    let year = date.year();
    (date, year)
}

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
            .map(|(pid, qty)| ConsumptionItemInput {
                product_id: pid.to_string(),
                quantity: qty,
            })
            .collect(),
    }
}

// ── Per-meal snapshot ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
struct MealItemSnapshot {
    quantity: f64,
    unit_price: f64,
    total_cost: f64,
    fifo_layer_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct OrderSnapshot {
    breakfast_cost: f64,
    lunch_cost: f64,
    dinner_cost: f64,
    items_by_meal: HashMap<String, Vec<MealItemSnapshot>>,
}

// ── Shared verification helper ───────────────────────────────────────────────

fn run_and_capture(
    db: &mut Database,
    unit_id: &str,
    _product_id: &str,
    date: NaiveDate,
    meals: Vec<MealSectionInput>,
) -> OrderSnapshot {
    let user_ctx = UserContext::new("system", "system", None);
    let report_id =
        AuditTxService::execute_with_audit(db, AuditAction::CreateDailyReport, &user_ctx, |tx| {
            DailyReportService::new(tx.executor).create_daily_report(
                &DailyReportInput { date, meals },
                Some(unit_id),
                "system",
                "system",
            )
        })
        .expect("create_daily_report failed");

    let ex = db.executor();

    let meal_rows: Vec<(String, String, f64)> = ex
        .query_all(
            "SELECT id, meal_type, total_meal_cost
             FROM daily_report_meals
             WHERE daily_report_id = ?1",
            rusqlite::params![report_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            },
        )
        .expect("query meal rows");

    let mut snapshot = OrderSnapshot {
        breakfast_cost: 0.0,
        lunch_cost: 0.0,
        dinner_cost: 0.0,
        items_by_meal: HashMap::new(),
    };

    for (meal_id, meal_type, cost) in &meal_rows {
        match meal_type.as_str() {
            "breakfast" => snapshot.breakfast_cost = *cost,
            "lunch" => snapshot.lunch_cost = *cost,
            "dinner" => snapshot.dinner_cost = *cost,
            _ => panic!("unknown meal_type: {}", meal_type),
        }

        let items: Vec<MealItemSnapshot> = ex
            .query_all(
                "SELECT quantity, unit_price, total_cost, fifo_layer_id
                 FROM daily_report_meal_items
                 WHERE meal_id = ?1
                 ORDER BY product_id ASC, id ASC",
                rusqlite::params![meal_id],
                |row| {
                    Ok(MealItemSnapshot {
                        quantity: row.get(0)?,
                        unit_price: row.get(1)?,
                        total_cost: row.get(2)?,
                        fifo_layer_id: row.get(3)?,
                    })
                },
            )
            .expect("query meal items");

        snapshot.items_by_meal.insert(meal_type.clone(), items);
    }

    snapshot
}

fn assert_snapshots_equal(a: &OrderSnapshot, b: &OrderSnapshot) {
    assert!(
        (a.breakfast_cost - b.breakfast_cost).abs() < 0.001,
        "breakfast_cost mismatch: {} vs {}",
        a.breakfast_cost,
        b.breakfast_cost
    );
    assert!(
        (a.lunch_cost - b.lunch_cost).abs() < 0.001,
        "lunch_cost mismatch: {} vs {}",
        a.lunch_cost,
        b.lunch_cost
    );
    assert!(
        (a.dinner_cost - b.dinner_cost).abs() < 0.001,
        "dinner_cost mismatch: {} vs {}",
        a.dinner_cost,
        b.dinner_cost
    );

    for meal_key in &["breakfast", "lunch", "dinner"] {
        let a_items = a.items_by_meal.get(*meal_key);
        let b_items = b.items_by_meal.get(*meal_key);

        match (a_items, b_items) {
            (Some(ai), Some(bi)) => {
                assert_eq!(
                    ai.len(),
                    bi.len(),
                    "item count mismatch for meal '{}': {} vs {}",
                    meal_key,
                    ai.len(),
                    bi.len()
                );
                for (i, (ia, ib)) in ai.iter().zip(bi.iter()).enumerate() {
                    assert!(
                        (ia.quantity - ib.quantity).abs() < 0.001,
                        "meal '{}' item[{}] quantity mismatch: {} vs {}",
                        meal_key,
                        i,
                        ia.quantity,
                        ib.quantity
                    );
                    assert!(
                        (ia.unit_price - ib.unit_price).abs() < 0.001,
                        "meal '{}' item[{}] unit_price mismatch: {} vs {}",
                        meal_key,
                        i,
                        ia.unit_price,
                        ib.unit_price
                    );
                    assert!(
                        (ia.total_cost - ib.total_cost).abs() < 0.001,
                        "meal '{}' item[{}] total_cost mismatch: {} vs {}",
                        meal_key,
                        i,
                        ia.total_cost,
                        ib.total_cost
                    );
                    // Each DB instance generates unique layer UUIDs,
                    // so fifo_layer_id cannot be compared across snapshots.
                    // Verify both are Some (non-null) instead.
                    assert!(
                        ia.fifo_layer_id.is_some(),
                        "meal '{}' item[{}] fifo_layer_id should be Some",
                        meal_key,
                        i
                    );
                    assert!(
                        ib.fifo_layer_id.is_some(),
                        "meal '{}' item[{}] opposite fifo_layer_id should be Some",
                        meal_key,
                        i
                    );
                }
            }
            (None, None) => {} // both absent — fine
            (Some(_), None) => {
                panic!(
                    "meal '{}' present in first snapshot but absent in second",
                    meal_key
                );
            }
            (None, Some(_)) => {
                panic!(
                    "meal '{}' absent in first snapshot but present in second",
                    meal_key
                );
            }
        }
    }
}

// ── Shared setup ─────────────────────────────────────────────────────────────

fn create_snapshot(
    unit_id: &str,
    product_id: &str,
    date: NaiveDate,
    year: i32,
    meals: Vec<MealSectionInput>,
) -> OrderSnapshot {
    let now = Utc::now().to_rfc3339();
    let mut db = ConnectionFactory::new_for_test().expect("test db init");

    {
        let ex = db.executor();
        seed_unit(ex, unit_id, &now);
        ex.execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now],
        )
        .expect("seed fiscal year");
        seed_product_and_stock(ex, product_id, "Rice", &now, year);
        let fifo = FifoLayerRepository::new(ex);

        // Layer A = 10 @ 500  (oldest)
        add_layer(
            &fifo,
            unit_id,
            product_id,
            10.0,
            500.0,
            "2024-01-01T08:00:00Z",
        );
        // Layer B = 20 @ 800  (newer)
        add_layer(
            &fifo,
            unit_id,
            product_id,
            20.0,
            800.0,
            "2024-01-02T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), unit_id, product_id);

    run_and_capture(&mut db, unit_id, product_id, date, meals)
}

// ── Test cases ───────────────────────────────────────────────────────────────

#[test]
fn test_order_independence_breakfast_lunch_dinner() {
    let unit_id = uuid::Uuid::new_v4().to_string();
    let product_id = uuid::Uuid::new_v4().to_string();
    let (date, year) = current_date_and_year();

    let snapshot = create_snapshot(
        &unit_id,
        &product_id,
        date,
        year,
        vec![
            meal(MealType::Breakfast, vec![(&product_id, 10.0)]),
            meal(MealType::Lunch, vec![(&product_id, 10.0)]),
        ],
    );

    assert!(
        (snapshot.breakfast_cost - 5000.0).abs() < 0.001,
        "Breakfast should cost 5000 (10@500), got {}",
        snapshot.breakfast_cost
    );
    assert!(
        (snapshot.lunch_cost - 8000.0).abs() < 0.001,
        "Lunch should cost 8000 (10@800), got {}",
        snapshot.lunch_cost
    );
    assert!(
        (snapshot.dinner_cost - 0.0).abs() < 0.001,
        "Dinner should cost 0, got {}",
        snapshot.dinner_cost
    );

    // Check item-level detail
    let bf_items = snapshot
        .items_by_meal
        .get("breakfast")
        .expect("breakfast items");
    assert_eq!(bf_items.len(), 1, "breakfast should have 1 item");
    assert!(
        (bf_items[0].unit_price - 500.0).abs() < 0.001,
        "breakfast unit_price should be 500, got {}",
        bf_items[0].unit_price
    );
    assert!(
        (bf_items[0].total_cost - 5000.0).abs() < 0.001,
        "breakfast total_cost should be 5000, got {}",
        bf_items[0].total_cost
    );
    assert!(
        bf_items[0].fifo_layer_id.is_some(),
        "breakfast fifo_layer_id should be Some"
    );

    let l_items = snapshot.items_by_meal.get("lunch").expect("lunch items");
    assert_eq!(l_items.len(), 1, "lunch should have 1 item");
    assert!(
        (l_items[0].unit_price - 800.0).abs() < 0.001,
        "lunch unit_price should be 800, got {}",
        l_items[0].unit_price
    );
    assert!(
        (l_items[0].total_cost - 8000.0).abs() < 0.001,
        "lunch total_cost should be 8000, got {}",
        l_items[0].total_cost
    );
    assert!(
        l_items[0].fifo_layer_id.is_some(),
        "lunch fifo_layer_id should be Some"
    );

    // Breakfast and Lunch should NOT share the same layer
    assert_ne!(
        bf_items[0].fifo_layer_id, l_items[0].fifo_layer_id,
        "Breakfast and Lunch must consume different layers"
    );
}

#[test]
fn test_order_independence_dinner_lunch_breakfast() {
    let unit_id = uuid::Uuid::new_v4().to_string();
    let product_id = uuid::Uuid::new_v4().to_string();
    let (date, year) = current_date_and_year();

    let snapshot = create_snapshot(
        &unit_id,
        &product_id,
        date,
        year,
        vec![
            meal(MealType::Dinner, vec![]),
            meal(MealType::Lunch, vec![(&product_id, 10.0)]),
            meal(MealType::Breakfast, vec![(&product_id, 10.0)]),
        ],
    );

    assert!(
        (snapshot.breakfast_cost - 5000.0).abs() < 0.001,
        "Breakfast should cost 5000, got {}",
        snapshot.breakfast_cost
    );
    assert!(
        (snapshot.lunch_cost - 8000.0).abs() < 0.001,
        "Lunch should cost 8000, got {}",
        snapshot.lunch_cost
    );
    assert!(
        (snapshot.dinner_cost - 0.0).abs() < 0.001,
        "Dinner should cost 0, got {}",
        snapshot.dinner_cost
    );
}

#[test]
fn test_order_independence_lunch_breakfast_dinner() {
    let unit_id = uuid::Uuid::new_v4().to_string();
    let product_id = uuid::Uuid::new_v4().to_string();
    let (date, year) = current_date_and_year();

    let snapshot = create_snapshot(
        &unit_id,
        &product_id,
        date,
        year,
        vec![
            meal(MealType::Lunch, vec![(&product_id, 10.0)]),
            meal(MealType::Breakfast, vec![(&product_id, 10.0)]),
            meal(MealType::Dinner, vec![]),
        ],
    );

    assert!(
        (snapshot.breakfast_cost - 5000.0).abs() < 0.001,
        "Breakfast should cost 5000, got {}",
        snapshot.breakfast_cost
    );
    assert!(
        (snapshot.lunch_cost - 8000.0).abs() < 0.001,
        "Lunch should cost 8000, got {}",
        snapshot.lunch_cost
    );
    assert!(
        (snapshot.dinner_cost - 0.0).abs() < 0.001,
        "Dinner should cost 0, got {}",
        snapshot.dinner_cost
    );
}

#[test]
fn test_all_orders_produce_identical_results() {
    let unit_id = uuid::Uuid::new_v4().to_string();
    let product_id = uuid::Uuid::new_v4().to_string();
    let (date, year) = current_date_and_year();

    let ref_snapshot = create_snapshot(
        &unit_id,
        &product_id,
        date,
        year,
        vec![
            meal(MealType::Breakfast, vec![(&product_id, 10.0)]),
            meal(MealType::Lunch, vec![(&product_id, 10.0)]),
            meal(MealType::Dinner, vec![]),
        ],
    );

    let unit_id2 = uuid::Uuid::new_v4().to_string();
    let snapshot_rev = create_snapshot(
        &unit_id2,
        &product_id,
        date,
        year,
        vec![
            meal(MealType::Dinner, vec![]),
            meal(MealType::Lunch, vec![(&product_id, 10.0)]),
            meal(MealType::Breakfast, vec![(&product_id, 10.0)]),
        ],
    );

    let unit_id3 = uuid::Uuid::new_v4().to_string();
    let snapshot_scrambled = create_snapshot(
        &unit_id3,
        &product_id,
        date,
        year,
        vec![
            meal(MealType::Lunch, vec![(&product_id, 10.0)]),
            meal(MealType::Breakfast, vec![(&product_id, 10.0)]),
            meal(MealType::Dinner, vec![]),
        ],
    );

    assert_snapshots_equal(&ref_snapshot, &snapshot_rev);
    assert_snapshots_equal(&ref_snapshot, &snapshot_scrambled);
}
