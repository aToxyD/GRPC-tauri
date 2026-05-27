//! Meal-level FIFO cost authenticity tests.
//!
//! Verifies that FIFO consumption happens per meal item (not aggregated),
//! and that each meal's cost reflects authentic FIFO layer portions.

use chrono::{Datelike, NaiveDate, Utc};
use grpc_lib::application::services::{AuditTxService, DailyReportService, UserContext};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::audit::AuditAction;
use grpc_lib::models::{ConsumptionItemInput, DailyReportInput, MealSectionInput, MealType};
use grpc_lib::repositories::{executor::DbExecutor, FifoLayerRepository};

fn seed_unit(ex: DbExecutor<'_>, id: &str, now: &str) {
    ex.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U01','Test Unit','01',?2)",
        rusqlite::params![id, now],
    ).expect("insert unit");
}

fn seed_product_and_stock(ex: DbExecutor<'_>, product_id: &str, name: &str, now: &str, year: i32) {
    ex.execute(
        "INSERT INTO products (id, name, base_price, tva, year, created_at) VALUES (?1,?2,0.0,0.0,?3,?4)",
        rusqlite::params![product_id, name, year, now],
    ).expect("insert product");
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

fn sync_inventory_from_fifo(ex: DbExecutor<'_>, unit_id: &str, product_id: &str) {
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

// ── Scenario 1: Single layer, breakfast only ───────────────────────────────
// Layer: 100@500
// Breakfast: 10
// Expected: Breakfast cost = 10 * 500 = 5000

#[test]
fn test_scenario_1_single_layer_breakfast_only() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, year) = current_date_and_year();
    let unit_id = uuid::Uuid::new_v4().to_string();
    let product_id = uuid::Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute("INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now]).expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Rice", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            100.0,
            500.0,
            "2024-01-01T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    let input = DailyReportInput {
        date,
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 10.0)])],
    };
    let user_ctx = UserContext::new("system", "system", None);
    AuditTxService::execute_with_audit(&mut db, AuditAction::CreateDailyReport, &user_ctx, |tx| {
        DailyReportService::new(tx.executor).create_daily_report(
            &input,
            Some(&unit_id),
            "system",
            "system",
        )
    })
    .expect("create daily report");

    // Verify breakfast cost = 10 * 500 = 5000
    let ex = db.executor();
    let breakfast_cost: f64 = ex
        .query_row(
            "SELECT total_meal_cost FROM daily_report_meals WHERE meal_type = 'breakfast'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(-1.0);
    assert!(
        (breakfast_cost - 5000.0).abs() < 0.01,
        "Expected breakfast cost 5000, got {}",
        breakfast_cost
    );
}

// ── Scenario 2: Breakfast + Lunch, two FIFO layers ────────────────────────
// Layer1: 10@500 (older), Layer2: 20@800 (newer)
// Breakfast: 10, Lunch: 10
// Expected:
//   Breakfast gets 10 from Layer1 @500 = 5000
//   Lunch gets 10 from Layer2 @800 = 8000

#[test]
fn test_scenario_2_breakfast_gets_oldest_layer() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, year) = current_date_and_year();
    let unit_id = uuid::Uuid::new_v4().to_string();
    let product_id = uuid::Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute("INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now]).expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Rice", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            10.0,
            500.0,
            "2024-01-01T08:00:00Z",
        );
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            20.0,
            800.0,
            "2024-01-02T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    let input = DailyReportInput {
        date,
        meals: vec![
            meal(MealType::Breakfast, vec![(&product_id, 10.0)]),
            meal(MealType::Lunch, vec![(&product_id, 10.0)]),
        ],
    };
    let user_ctx = UserContext::new("system", "system", None);
    AuditTxService::execute_with_audit(&mut db, AuditAction::CreateDailyReport, &user_ctx, |tx| {
        DailyReportService::new(tx.executor).create_daily_report(
            &input,
            Some(&unit_id),
            "system",
            "system",
        )
    })
    .expect("create daily report");

    let ex = db.executor();
    let breakfast_cost: f64 = ex
        .query_row(
            "SELECT total_meal_cost FROM daily_report_meals WHERE meal_type = 'breakfast'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(-1.0);
    let lunch_cost: f64 = ex
        .query_row(
            "SELECT total_meal_cost FROM daily_report_meals WHERE meal_type = 'lunch'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(-1.0);

    assert!(
        (breakfast_cost - 5000.0).abs() < 0.01,
        "Breakfast should cost 5000 (10@500), got {}",
        breakfast_cost
    );
    assert!(
        (lunch_cost - 8000.0).abs() < 0.01,
        "Lunch should cost 8000 (10@800), got {}",
        lunch_cost
    );
}

// ── Scenario 3: Breakfast + Lunch + Dinner, three layers ──────────────────
// Layer1: 10@400, Layer2: 20@500, Layer3: 30@600
// Breakfast: 10, Lunch: 20, Dinner: 10
// Expected:
//   Breakfast: 10@400 = 4000 (consumes Layer1 entirely)
//   Lunch: 20@500 = 10000 (consumes Layer2 entirely)
//   Dinner: 10@600 = 6000 (consumes 10 from Layer3)

#[test]
fn test_scenario_3_three_meals_three_layers() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, year) = current_date_and_year();
    let unit_id = uuid::Uuid::new_v4().to_string();
    let product_id = uuid::Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute("INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now]).expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Rice", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            10.0,
            400.0,
            "2024-01-01T08:00:00Z",
        );
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            20.0,
            500.0,
            "2024-01-02T08:00:00Z",
        );
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            30.0,
            600.0,
            "2024-01-03T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    let input = DailyReportInput {
        date,
        meals: vec![
            meal(MealType::Breakfast, vec![(&product_id, 10.0)]),
            meal(MealType::Lunch, vec![(&product_id, 20.0)]),
            meal(MealType::Dinner, vec![(&product_id, 10.0)]),
        ],
    };
    let user_ctx = UserContext::new("system", "system", None);
    AuditTxService::execute_with_audit(&mut db, AuditAction::CreateDailyReport, &user_ctx, |tx| {
        DailyReportService::new(tx.executor).create_daily_report(
            &input,
            Some(&unit_id),
            "system",
            "system",
        )
    })
    .expect("create daily report");

    let ex = db.executor();
    let breakfast_cost: f64 = ex
        .query_row(
            "SELECT total_meal_cost FROM daily_report_meals WHERE meal_type = 'breakfast'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(-1.0);
    let lunch_cost: f64 = ex
        .query_row(
            "SELECT total_meal_cost FROM daily_report_meals WHERE meal_type = 'lunch'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(-1.0);
    let dinner_cost: f64 = ex
        .query_row(
            "SELECT total_meal_cost FROM daily_report_meals WHERE meal_type = 'dinner'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(-1.0);

    assert!(
        (breakfast_cost - 4000.0).abs() < 0.01,
        "Breakfast should cost 4000 (10@400), got {}",
        breakfast_cost
    );
    assert!(
        (lunch_cost - 10000.0).abs() < 0.01,
        "Lunch should cost 10000 (20@500), got {}",
        lunch_cost
    );
    assert!(
        (dinner_cost - 6000.0).abs() < 0.01,
        "Dinner should cost 6000 (10@600), got {}",
        dinner_cost
    );
}

// ── Scenario 4: Multiple products, FIFO independent per product ──────────
// Product A: Layer1: 10@500
// Product B: Layer1: 20@300
// Breakfast: A=5, B=10
// Expected:
//   Breakfast(A) = 5*500 = 2500
//   Breakfast(B) = 10*300 = 3000

#[test]
fn test_scenario_4_multiple_products_independent() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, year) = current_date_and_year();
    let unit_id = uuid::Uuid::new_v4().to_string();
    let pa = uuid::Uuid::new_v4().to_string();
    let pb = uuid::Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute("INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now]).expect("seed fiscal year");
        seed_product_and_stock(ex, &pa, "ProductA", &now, year);
        seed_product_and_stock(ex, &pb, "ProductB", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(&fifo, &unit_id, &pa, 10.0, 500.0, "2024-01-01T08:00:00Z");
        add_layer(&fifo, &unit_id, &pb, 20.0, 300.0, "2024-01-01T08:00:00Z");
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &pa);
    sync_inventory_from_fifo(db.executor(), &unit_id, &pb);

    let input = DailyReportInput {
        date,
        meals: vec![meal(MealType::Breakfast, vec![(&pa, 5.0), (&pb, 10.0)])],
    };
    let user_ctx = UserContext::new("system", "system", None);
    AuditTxService::execute_with_audit(&mut db, AuditAction::CreateDailyReport, &user_ctx, |tx| {
        DailyReportService::new(tx.executor).create_daily_report(
            &input,
            Some(&unit_id),
            "system",
            "system",
        )
    })
    .expect("create daily report");

    // Total breakfast cost = 5*500 + 10*300 = 2500 + 3000 = 5500
    let ex = db.executor();
    let breakfast_cost: f64 = ex
        .query_row(
            "SELECT total_meal_cost FROM daily_report_meals WHERE meal_type = 'breakfast'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(-1.0);
    assert!(
        (breakfast_cost - 5500.0).abs() < 0.01,
        "Expected breakfast cost 5500, got {}",
        breakfast_cost
    );
}

// ── Scenario 5: Fractional quantities ────────────────────────────────────
// Layer: 100@500
// Breakfast: 10.333, Lunch: 20.667
// Expected:
//   Breakfast = 10.333 * 500 = 5166.50
//   Lunch = 20.667 * 500 = 10333.50

#[test]
fn test_scenario_5_fractional_quantities() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, year) = current_date_and_year();
    let unit_id = uuid::Uuid::new_v4().to_string();
    let product_id = uuid::Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute("INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now]).expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Rice", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            100.0,
            500.0,
            "2024-01-01T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    let input = DailyReportInput {
        date,
        meals: vec![
            meal(MealType::Breakfast, vec![(&product_id, 10.333)]),
            meal(MealType::Lunch, vec![(&product_id, 20.667)]),
        ],
    };
    let user_ctx = UserContext::new("system", "system", None);
    AuditTxService::execute_with_audit(&mut db, AuditAction::CreateDailyReport, &user_ctx, |tx| {
        DailyReportService::new(tx.executor).create_daily_report(
            &input,
            Some(&unit_id),
            "system",
            "system",
        )
    })
    .expect("create daily report");

    let ex = db.executor();
    let breakfast_cost: f64 = ex
        .query_row(
            "SELECT total_meal_cost FROM daily_report_meals WHERE meal_type = 'breakfast'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(-1.0);
    let lunch_cost: f64 = ex
        .query_row(
            "SELECT total_meal_cost FROM daily_report_meals WHERE meal_type = 'lunch'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(-1.0);

    assert!(
        (breakfast_cost - 5166.50).abs() < 1.0,
        "Breakfast should cost ~5166.50 (10.333*500), got {}",
        breakfast_cost
    );
    assert!(
        (lunch_cost - 10333.50).abs() < 1.0,
        "Lunch should cost ~10333.50 (20.667*500), got {}",
        lunch_cost
    );
}

// ── Scenario 6: sum(meal item costs) == daily_report.total_daily_cost ────
// Layer: 100@500
// Breakfast: 10, Lunch: 20
// total_daily_cost should equal breakfast_cost + lunch_cost

#[test]
fn test_scenario_6_meal_sum_equals_daily_total() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, year) = current_date_and_year();
    let unit_id = uuid::Uuid::new_v4().to_string();
    let product_id = uuid::Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute("INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now]).expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Rice", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            100.0,
            500.0,
            "2024-01-01T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

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
    .expect("create daily report");

    let ex = db.executor();
    let total_daily_cost: f64 = ex
        .query_row(
            "SELECT total_daily_cost FROM daily_reports WHERE id = ?1",
            rusqlite::params![report_id],
            |row| row.get(0),
        )
        .unwrap_or(-1.0);
    let sum_meal_costs: f64 = ex.query_row(
        "SELECT COALESCE(SUM(total_meal_cost), 0.0) FROM daily_report_meals WHERE daily_report_id = ?1",
        rusqlite::params![report_id], |row| row.get(0),
    ).unwrap_or(-1.0);
    let sum_item_costs: f64 = ex
        .query_row(
            "SELECT COALESCE(SUM(drmi.total_cost), 0.0)
         FROM daily_report_meal_items drmi
         JOIN daily_report_meals drm ON drmi.meal_id = drm.id
         WHERE drm.daily_report_id = ?1",
            rusqlite::params![report_id],
            |row| row.get(0),
        )
        .unwrap_or(-1.0);

    assert!(
        (total_daily_cost - 15000.0).abs() < 0.01,
        "total_daily_cost expected 15000, got {}",
        total_daily_cost
    );
    assert!(
        (sum_meal_costs - total_daily_cost).abs() < 0.01,
        "sum(meal_costs)={} != total_daily_cost={}",
        sum_meal_costs,
        total_daily_cost
    );
    assert!(
        (sum_item_costs - total_daily_cost).abs() < 0.01,
        "sum(item_costs)={} != total_daily_cost={}",
        sum_item_costs,
        total_daily_cost
    );
}

// ── Scenario 7: sum(qty_remaining in FIFO) == inventory_stocks.quantity ──
// after report creation
// Layer: 100@500
// Breakfast: 10, Lunch: 20
// Remaining: 100-10-20 = 70

#[test]
fn test_scenario_7_inventory_fifo_consistency_after_report() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, year) = current_date_and_year();
    let unit_id = uuid::Uuid::new_v4().to_string();
    let product_id = uuid::Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute("INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now]).expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Rice", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            100.0,
            500.0,
            "2024-01-01T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    let input = DailyReportInput {
        date,
        meals: vec![
            meal(MealType::Breakfast, vec![(&product_id, 10.0)]),
            meal(MealType::Lunch, vec![(&product_id, 20.0)]),
        ],
    };
    let user_ctx = UserContext::new("system", "system", None);
    AuditTxService::execute_with_audit(&mut db, AuditAction::CreateDailyReport, &user_ctx, |tx| {
        DailyReportService::new(tx.executor).create_daily_report(
            &input,
            Some(&unit_id),
            "system",
            "system",
        )
    })
    .expect("create daily report");

    let ex = db.executor();
    let fifo_qty: f64 = ex
        .query_row(
            "SELECT COALESCE(SUM(qty_remaining), 0.0)
         FROM fifo_stock_layers
         WHERE unit_id = ?1 AND product_id = ?2",
            rusqlite::params![unit_id, product_id],
            |row| row.get(0),
        )
        .unwrap_or(-1.0);
    let stock_qty: f64 = ex
        .query_row(
            "SELECT COALESCE(quantity, 0.0) FROM inventory_stocks WHERE product_id = ?1",
            rusqlite::params![product_id],
            |row| row.get(0),
        )
        .unwrap_or(-1.0);

    assert!(
        (fifo_qty - 70.0).abs() < 0.01,
        "FIFO remaining expected 70, got {}",
        fifo_qty
    );
    assert!(
        (stock_qty - 70.0).abs() < 0.01,
        "Inventory stock expected 70, got {}",
        stock_qty
    );
    assert!(
        (fifo_qty - stock_qty).abs() < 1e-6,
        "FIFO qty={} != inventory stock={}",
        fifo_qty,
        stock_qty
    );
}
