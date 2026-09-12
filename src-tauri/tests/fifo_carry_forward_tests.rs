//! Integration tests for fiscal-year carry-forward (FIFO).
//!
//! Covers:
//!   1. close_year reclassifies ORDER to OPENING
//!   2. FIFO consumption works after reclassification
//!   3. OPENING layers are consumed before new ORDER layers
//!   4. Partial consumption reflected correctly after reclassification
//!   5. origin_fiscal_year preserved after reclassification
//!   6. No duplicate OPENING layers across multiple year closes

use chrono::Utc;
use grpc_lib::application::services::{
    AuditTxService, DailyReportService, FiscalClosingService, UserContext,
};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::audit::AuditAction;
use grpc_lib::models::{ConsumptionItemInput, DailyReportInput, MealSectionInput, MealType};
use grpc_lib::repositories::{executor::DbExecutor, FifoLayerRepository};
use uuid::Uuid;

const YEAR_N: i32 = 2025;
const YEAR_N1: i32 = 2026;

fn admin_id(db: &grpc_lib::db::Database) -> String {
    db.get_connection()
        .query_row(
            "SELECT id FROM users WHERE username = 'admin' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

fn seed_unit(ex: DbExecutor<'_>, id: &str, now: &str) {
    ex.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U01','Test Unit','01',?2)",
        rusqlite::params![id, now],
    ).expect("insert unit");
}

fn seed_product_and_stock(ex: DbExecutor<'_>, product_id: &str, name: &str, now: &str, year: i32) {
    ex.execute(
        "INSERT INTO products (id, name, base_price, year, created_at, purchase_unit, consumption_unit, conversion_factor, tva_classification) VALUES (?1,?2,0.0,?3,?4,1,1,1,0)",
        rusqlite::params![product_id, name, year, now],
    )
    .expect("insert product");
    ex.execute(
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, consumption_unit, last_updated, updated_at) VALUES (?1,?2,0.0,'unit',1,?3,?3)",
        rusqlite::params![format!("stock-{}", product_id), product_id, now],
    ).expect("insert inventory_stocks");
}

fn clear_fiscal_status(ex: DbExecutor<'_>) {
    ex.execute("DELETE FROM fiscal_year_status", [])
        .expect("clear fiscal status");
}

fn seed_fiscal_year(ex: DbExecutor<'_>, year: i32) {
    let now = Utc::now().to_rfc3339();
    clear_fiscal_status(ex);
    ex.execute(
        "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
        rusqlite::params![year, now],
    )
    .expect("seed fiscal year");
    ex.execute(
        "UPDATE settings SET current_year = ?1 WHERE id = 1",
        rusqlite::params![year],
    )
    .expect("set current year");
}

fn sync_inventory_from_fifo(ex: DbExecutor<'_>, unit_id: &str, product_id: &str) {
    let fifo_qty: f64 = ex.query_row(
        "SELECT COALESCE(SUM(qty_remaining), 0.0) FROM fifo_stock_layers WHERE unit_id = ?1 AND product_id = ?2",
        rusqlite::params![unit_id, product_id],
        |row| row.get(0),
    ).unwrap_or(0.0);
    ex.execute(
        "UPDATE inventory_stocks SET quantity = ?1 WHERE product_id = ?2",
        rusqlite::params![fifo_qty, product_id],
    )
    .expect("sync inventory failed");
}

fn add_layer(
    fifo: &FifoLayerRepository,
    unit_id: &str,
    product_id: &str,
    qty: f64,
    unit_cost: f64,
    received_at: &str,
    origin_fy: i32,
) {
    fifo.create_layer(
        unit_id,
        product_id,
        "ORDER",
        None,
        unit_cost,
        qty,
        received_at,
        "system",
        origin_fy,
    )
    .expect("create_layer failed");
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
            .map(|(product_id, qty)| ConsumptionItemInput {
                product_id: product_id.to_string(),
                quantity: qty,
            })
            .collect(),
    }
}

fn get_movement_id_for_product(ex: DbExecutor<'_>, reference_id: &str, product_id: &str) -> String {
    ex.query_row(
        "SELECT id FROM stock_movements WHERE reference_id = ?1 AND product_id = ?2",
        rusqlite::params![reference_id, product_id],
        |row| row.get(0),
    )
    .expect("movement_id not found")
}

fn get_layer_consumption_sum(ex: DbExecutor<'_>, movement_id: &str) -> f64 {
    let sum: i64 = ex
        .query_row(
            "SELECT COALESCE(SUM(quantity), 0) FROM inventory_layer_consumptions WHERE movement_id = ?1",
            rusqlite::params![movement_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    sum as f64 / 1000.0
}

fn get_layer_consumption_cost(ex: DbExecutor<'_>, movement_id: &str) -> f64 {
    let sum: i64 = ex
        .query_row(
            "SELECT COALESCE(SUM(total_cost), 0) FROM inventory_layer_consumptions WHERE movement_id = ?1",
            rusqlite::params![movement_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    sum as f64 / 100.0
}

fn close_year(db: &mut grpc_lib::db::Database, year: i32, next_year: i32) {
    let uid = admin_id(db);
    db.with_transaction(|tx| {
        FiscalClosingService::new(tx).close_year(year, next_year, &uid, "system", None)
    })
    .expect("close_year failed");
}

fn get_layer_source_type(ex: DbExecutor<'_>, layer_id: &str) -> String {
    ex.query_row(
        "SELECT source_type FROM fifo_stock_layers WHERE id = ?1",
        rusqlite::params![layer_id],
        |row| row.get(0),
    )
    .expect("get source_type")
}

fn get_layer_origin_fy(ex: DbExecutor<'_>, layer_id: &str) -> i32 {
    ex.query_row(
        "SELECT origin_fiscal_year FROM fifo_stock_layers WHERE id = ?1",
        rusqlite::params![layer_id],
        |row| row.get(0),
    )
    .expect("get origin_fiscal_year")
}

fn get_layer_qty(ex: DbExecutor<'_>, layer_id: &str) -> f64 {
    let qty: i64 = ex
        .query_row(
            "SELECT qty_remaining FROM fifo_stock_layers WHERE id = ?1",
            rusqlite::params![layer_id],
            |row| row.get(0),
        )
        .expect("get qty_remaining");
    qty as f64 / 1000.0
}

fn count_active_layers(ex: DbExecutor<'_>, product_id: &str) -> usize {
    let count: i64 = ex
        .query_row(
            "SELECT COUNT(*) FROM fifo_stock_layers WHERE product_id = ?1 AND qty_remaining > 0",
            rusqlite::params![product_id],
            |row| row.get(0),
        )
        .expect("count active layers");
    count as usize
}

fn count_opening_layers(ex: DbExecutor<'_>, product_id: &str) -> usize {
    let count: i64 = ex.query_row(
        "SELECT COUNT(*) FROM fifo_stock_layers WHERE product_id = ?1 AND source_type = 'OPENING' AND qty_remaining > 0",
        rusqlite::params![product_id],
        |row| row.get(0),
    )
    .expect("count OPENING layers");
    count as usize
}

// ── Test 1: close_year reclassifies ORDER to OPENING ───────────────────────

#[test]
fn close_year_reclassifies_order_to_opening() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // Setup
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_fiscal_year(ex, YEAR_N);
        seed_product_and_stock(ex, &product_id, "Rice", &now, YEAR_N);
        add_layer(
            &FifoLayerRepository::new(ex),
            &unit_id,
            &product_id,
            100.0,
            50.0,
            "2025-06-01T08:00:00Z",
            YEAR_N,
        );
        sync_inventory_from_fifo(ex, &unit_id, &product_id);
    }

    // Get layer id before close
    let layer_id: String = db
        .executor()
        .query_row(
            "SELECT id FROM fifo_stock_layers WHERE product_id = ?1",
            rusqlite::params![product_id],
            |row| row.get(0),
        )
        .expect("get layer id");

    // Close year
    close_year(&mut db, YEAR_N, YEAR_N1);

    // Verify: same layer, source_type now OPENING, origin_fiscal_year preserved
    let ex = db.executor();
    let source_type = get_layer_source_type(ex, &layer_id);
    assert_eq!(
        source_type, "OPENING",
        "ORDER should be reclassified to OPENING"
    );

    let origin_fy = get_layer_origin_fy(ex, &layer_id);
    assert_eq!(origin_fy, YEAR_N, "origin_fiscal_year must remain YEAR_N");

    let qty = get_layer_qty(ex, &layer_id);
    assert!((qty - 100.0).abs() < 1e-9, "qty should remain 100");
}

// ── Test 2: FIFO consumption works after reclassification ──────────────────

#[test]
fn fifo_consumption_works_after_reclassification() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // Setup
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_fiscal_year(ex, YEAR_N);
        seed_product_and_stock(ex, &product_id, "Flour", &now, YEAR_N);
        add_layer(
            &FifoLayerRepository::new(ex),
            &unit_id,
            &product_id,
            100.0,
            50.0,
            "2025-06-01T08:00:00Z",
            YEAR_N,
        );
        sync_inventory_from_fifo(ex, &unit_id, &product_id);
    }

    // Close year → reclassifies to OPENING
    close_year(&mut db, YEAR_N, YEAR_N1);

    // Consume 50 in YEAR_N1
    let input = DailyReportInput {
        date: chrono::NaiveDate::parse_from_str("2026-01-15", "%Y-%m-%d").unwrap(),
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 50.0)])],
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
    .expect("daily report YEAR_N1 failed");

    let ex = db.executor();
    let mid = get_movement_id_for_product(ex, &report_id, &product_id);
    let consumed_qty = get_layer_consumption_sum(ex, &mid);
    let consumed_cost = get_layer_consumption_cost(ex, &mid);
    assert!(
        (consumed_qty - 50.0).abs() < 1e-9,
        "consumed qty should be 50, got {}",
        consumed_qty
    );
    assert!(
        (consumed_cost - 2500.0).abs() < 0.01,
        "consumed cost should be 2500, got {}",
        consumed_cost
    );

    // product_id is not a layer_id — we need the layer id
    let layer_id: String = ex
        .query_row(
            "SELECT id FROM fifo_stock_layers WHERE product_id = ?1 AND qty_remaining > 0",
            rusqlite::params![product_id],
            |row| row.get(0),
        )
        .expect("get remaining layer id");
    let qty = get_layer_qty(ex, &layer_id);
    assert!(
        (qty - 50.0).abs() < 1e-9,
        "remaining qty should be 50, got {}",
        qty
    );
}

// ── Test 3: OPENING consumed before new ORDER ──────────────────────────────

#[test]
fn fifo_orders_opening_before_new_order() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // Setup: 100@500 in YEAR_N
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_fiscal_year(ex, YEAR_N);
        seed_product_and_stock(ex, &product_id, "Sugar", &now, YEAR_N);
        add_layer(
            &FifoLayerRepository::new(ex),
            &unit_id,
            &product_id,
            100.0,
            500.0,
            "2025-06-01T08:00:00Z",
            YEAR_N,
        );
        sync_inventory_from_fifo(ex, &unit_id, &product_id);
    }

    // Close → becomes OPENING 100@500
    close_year(&mut db, YEAR_N, YEAR_N1);

    // Add new ORDER 100@800 in YEAR_N1
    let ex = db.executor();
    let fifo = FifoLayerRepository::new(ex);
    add_layer(
        &fifo,
        &unit_id,
        &product_id,
        100.0,
        800.0,
        "2026-01-10T08:00:00Z",
        YEAR_N1,
    );
    sync_inventory_from_fifo(ex, &unit_id, &product_id);

    // Consume 70 — should take from OPENING (cheaper) first
    let input = DailyReportInput {
        date: chrono::NaiveDate::parse_from_str("2026-01-15", "%Y-%m-%d").unwrap(),
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 70.0)])],
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
    .expect("daily report failed");

    let ex = db.executor();
    let mid = get_movement_id_for_product(ex, &report_id, &product_id);
    let consumed_cost = get_layer_consumption_cost(ex, &mid);

    // All 70 should come from OPENING 500/kg = 35000
    // If it came from ORDER 800/kg = 56000
    assert!(
        (consumed_cost - 35000.0).abs() < 0.01,
        "70 should be consumed from OPENING 500/kg: expected 35000, got {}",
        consumed_cost
    );
}

// ── Test 4: Partial consumption before close ───────────────────────────────

#[test]
fn partial_consumption_respected() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // Setup: Layer 100@500
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_fiscal_year(ex, YEAR_N);
        seed_product_and_stock(ex, &product_id, "Oil", &now, YEAR_N);
        add_layer(
            &FifoLayerRepository::new(ex),
            &unit_id,
            &product_id,
            100.0,
            500.0,
            "2025-06-01T08:00:00Z",
            YEAR_N,
        );
        sync_inventory_from_fifo(ex, &unit_id, &product_id);
    }

    // Consume 30 in YEAR_N
    let input = DailyReportInput {
        date: chrono::NaiveDate::parse_from_str("2025-11-01", "%Y-%m-%d").unwrap(),
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 30.0)])],
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
    .expect("daily report YEAR_N failed");

    // Get remaining layer id
    let layer_id: String = db
        .executor()
        .query_row(
            "SELECT id FROM fifo_stock_layers WHERE product_id = ?1",
            rusqlite::params![product_id],
            |row| row.get(0),
        )
        .expect("get layer id");

    // Verify 70 remaining before close
    assert!(
        (get_layer_qty(db.executor(), &layer_id) - 70.0).abs() < 1e-9,
        "70 should remain after consuming 30 from 100"
    );

    // Close year
    close_year(&mut db, YEAR_N, YEAR_N1);

    // Verify: same layer, qty_remaining=70, source_type='OPENING'
    let ex = db.executor();
    let source_type = get_layer_source_type(ex, &layer_id);
    assert_eq!(
        source_type, "OPENING",
        "layer should be reclassified to OPENING, got {}",
        source_type
    );

    let qty = get_layer_qty(ex, &layer_id);
    assert!(
        (qty - 70.0).abs() < 1e-9,
        "qty must be 70 (not 100), got {}",
        qty
    );

    let origin_fy = get_layer_origin_fy(ex, &layer_id);
    assert_eq!(
        origin_fy, YEAR_N,
        "origin_fiscal_year unchanged after reclassification"
    );
}

// ── Test 5: origin_fiscal_year preserved after reclassification ────────────

#[test]
fn origin_fiscal_year_preserved_after_reclassification() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // Setup with origin_fiscal_year = 2024 (older year)
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_fiscal_year(ex, YEAR_N);
        seed_product_and_stock(ex, &product_id, "Pasta", &now, YEAR_N);
        add_layer(
            &FifoLayerRepository::new(ex),
            &unit_id,
            &product_id,
            50.0,
            200.0,
            "2024-01-01T08:00:00Z",
            2024,
        );
        sync_inventory_from_fifo(ex, &unit_id, &product_id);
    }

    let layer_id: String = db
        .executor()
        .query_row(
            "SELECT id FROM fifo_stock_layers WHERE product_id = ?1",
            rusqlite::params![product_id],
            |row| row.get(0),
        )
        .expect("get layer id");

    // origin_fiscal_year = 2024 (not YEAR_N)
    assert_eq!(get_layer_origin_fy(db.executor(), &layer_id), 2024);

    // Close YEAR_N → only reclassifies source_type, not origin_fiscal_year
    close_year(&mut db, YEAR_N, YEAR_N1);

    let ex = db.executor();
    assert_eq!(
        get_layer_origin_fy(ex, &layer_id),
        2024,
        "origin_fiscal_year must NOT change after close_year"
    );
    assert_eq!(
        get_layer_source_type(ex, &layer_id),
        "OPENING",
        "source_type must be OPENING after close_year"
    );
}

// ── Test 6: No duplicate OPENING layers across multiple year closes ─────────

#[test]
fn multi_year_no_layer_duplication() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // 2025: ORDER 100@500
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_fiscal_year(ex, 2025);
        seed_product_and_stock(ex, &product_id, "Meat", &now, 2025);
        add_layer(
            &FifoLayerRepository::new(ex),
            &unit_id,
            &product_id,
            100.0,
            500.0,
            "2025-06-01T08:00:00Z",
            2025,
        );
        sync_inventory_from_fifo(ex, &unit_id, &product_id);
    }

    // Close 2025 → 2026: ORDER reclassified to OPENING
    close_year(&mut db, 2025, 2026);

    // Verify: 1 active layer, source_type = OPENING
    let ex = db.executor();
    assert_eq!(
        count_active_layers(ex, &product_id),
        1,
        "1 active layer after first close"
    );
    assert_eq!(
        count_opening_layers(ex, &product_id),
        1,
        "1 OPENING layer after first close"
    );

    // Close 2026 → 2027: OPENING should NOT be reclassified again
    // First, seed fiscal year 2026 as open (close_year does this, but we need it in the right state)
    // Actually, close_year already opened 2026. Let's verify.

    // Sync current_year to 2026 since close_year already set it
    // But we need 2026 to be the "open" year. close_year from 2025→2026 already did this.

    // Now close 2026 → 2027
    close_year(&mut db, 2026, 2027);

    // Verify: still 1 active layer, still 1 OPENING layer
    let ex = db.executor();
    assert_eq!(
        count_active_layers(ex, &product_id),
        1,
        "must still have exactly 1 active layer (no duplication) after second close"
    );
    assert_eq!(
        count_opening_layers(ex, &product_id),
        1,
        "must still have exactly 1 OPENING layer (no duplication) after second close"
    );

    // qty still 100
    let layer_id: String = ex
        .query_row(
            "SELECT id FROM fifo_stock_layers WHERE product_id = ?1",
            rusqlite::params![product_id],
            |row| row.get(0),
        )
        .expect("get layer id");
    assert!(
        (get_layer_qty(ex, &layer_id) - 100.0).abs() < 1e-9,
        "qty must remain 100, got {}",
        get_layer_qty(ex, &layer_id)
    );
}
