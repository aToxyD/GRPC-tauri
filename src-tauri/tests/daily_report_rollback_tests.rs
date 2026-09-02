use chrono::{Datelike, Utc};
use grpc_lib::application::services::{AuditTxService, DailyReportService, UserContext};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::audit::AuditAction;
use grpc_lib::models::{ConsumptionItemInput, DailyReportInput, MealSectionInput, MealType};
use grpc_lib::repositories::FifoLayerRepository;
use uuid::Uuid;

fn seed_unit(ex: grpc_lib::repositories::DbExecutor<'_>, id: &str, now: &str) {
    ex.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U01','Test Unit','01',?2)",
        rusqlite::params![id, now],
    ).expect("insert unit");
}

fn seed_fiscal_year(ex: grpc_lib::repositories::DbExecutor<'_>, year: i32) {
    let now = Utc::now().to_rfc3339();
    ex.execute(
        "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
        rusqlite::params![year, now],
    )
    .expect("seed fiscal year");
}

fn seed_product_and_stock(
    ex: grpc_lib::repositories::DbExecutor<'_>,
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
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, last_updated, updated_at) VALUES (?1,?2,100.0,'unit',?3,?3)",
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
        2025,
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

#[test]
fn test_daily_report_failure_rolls_back_fifo_and_inventory() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let date = Utc::now().date_naive();
    let year = date.year();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // ── Setup ──────────────────────────────────────────────────────────────
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_fiscal_year(ex, year);
        seed_product_and_stock(ex, &product_id, "Test Product", &now, year);

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

    // ── Inject failure trigger on stock_movements ──────────────────────────
    db.get_connection()
        .execute_batch(
            r#"
            CREATE TRIGGER IF NOT EXISTS trg_stock_movement_fail
            BEFORE INSERT ON stock_movements
            FOR EACH ROW
            BEGIN
                SELECT RAISE(ABORT, 'injected stock_movement failure');
            END;
        "#,
        )
        .expect("create trigger");

    // ── Capture state BEFORE ──────────────────────────────────────────────
    let ex = db.executor();
    let fifo_qty_before: f64 = ex
        .query_row(
            "SELECT COALESCE(SUM(qty_remaining), 0.0) FROM fifo_stock_layers WHERE unit_id = ?1 AND product_id = ?2",
            rusqlite::params![unit_id, product_id],
            |row| row.get(0),
        )
        .unwrap_or(0.0);
    let stock_qty_before: f64 = ex
        .query_row(
            "SELECT COALESCE(quantity, 0.0) FROM inventory_stocks WHERE product_id = ?1",
            rusqlite::params![product_id],
            |row| row.get(0),
        )
        .unwrap_or(0.0);
    let reports_before: i64 = ex
        .query_row("SELECT COUNT(*) FROM daily_reports", [], |row| row.get(0))
        .unwrap_or(0);
    let meals_before: i64 = ex
        .query_row("SELECT COUNT(*) FROM daily_report_meals", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);
    let items_before: i64 = ex
        .query_row("SELECT COUNT(*) FROM daily_report_meal_items", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);
    let consumptions_before: i64 = ex
        .query_row(
            "SELECT COUNT(*) FROM inventory_layer_consumptions",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let movements_before: i64 = ex
        .query_row("SELECT COUNT(*) FROM stock_movements", [], |row| row.get(0))
        .unwrap_or(0);
    let audit_before: i64 = ex
        .query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))
        .unwrap_or(0);
    let fifo_layers_before: i64 = ex
        .query_row(
            "SELECT COUNT(*) FROM fifo_stock_layers WHERE unit_id = ?1 AND product_id = ?2",
            rusqlite::params![unit_id, product_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // ── Execute (expected to fail) ─────────────────────────────────────────
    let input = DailyReportInput {
        date,
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 30.0)])],
    };

    let user_ctx = UserContext::new("system", "system", None);
    let result = AuditTxService::execute_with_audit(
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
    );
    assert!(
        result.is_err(),
        "expected operation to fail, got: {:?}",
        result
    );

    // ── Verify state AFTER unchanged ───────────────────────────────────────
    let ex = db.executor();

    let fifo_qty_after: f64 = ex
        .query_row(
            "SELECT COALESCE(SUM(qty_remaining), 0.0) FROM fifo_stock_layers WHERE unit_id = ?1 AND product_id = ?2",
            rusqlite::params![unit_id, product_id],
            |row| row.get(0),
        )
        .unwrap_or(0.0);
    assert!(
        (fifo_qty_after - fifo_qty_before).abs() < 1e-9,
        "FIFO qty_remaining changed: before={}, after={}",
        fifo_qty_before,
        fifo_qty_after
    );

    let stock_qty_after: f64 = ex
        .query_row(
            "SELECT COALESCE(quantity, 0.0) FROM inventory_stocks WHERE product_id = ?1",
            rusqlite::params![product_id],
            |row| row.get(0),
        )
        .unwrap_or(0.0);
    assert!(
        (stock_qty_after - stock_qty_before).abs() < 1e-9,
        "Inventory quantity changed: before={}, after={}",
        stock_qty_before,
        stock_qty_after
    );

    let reports_after: i64 = ex
        .query_row("SELECT COUNT(*) FROM daily_reports", [], |row| row.get(0))
        .unwrap_or(0);
    assert_eq!(reports_after, reports_before, "daily_reports count changed");

    let meals_after: i64 = ex
        .query_row("SELECT COUNT(*) FROM daily_report_meals", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);
    assert_eq!(
        meals_after, meals_before,
        "daily_report_meals count changed"
    );

    let items_after: i64 = ex
        .query_row("SELECT COUNT(*) FROM daily_report_meal_items", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);
    assert_eq!(
        items_after, items_before,
        "daily_report_meal_items count changed"
    );

    let consumptions_after: i64 = ex
        .query_row(
            "SELECT COUNT(*) FROM inventory_layer_consumptions",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    assert_eq!(
        consumptions_after, consumptions_before,
        "inventory_layer_consumptions count changed"
    );

    let movements_after: i64 = ex
        .query_row("SELECT COUNT(*) FROM stock_movements", [], |row| row.get(0))
        .unwrap_or(0);
    assert_eq!(
        movements_after, movements_before,
        "stock_movements count changed"
    );

    let audit_after: i64 = ex
        .query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))
        .unwrap_or(0);
    assert_eq!(audit_after, audit_before, "audit_log count changed");

    let fifo_layers_after: i64 = ex
        .query_row(
            "SELECT COUNT(*) FROM fifo_stock_layers WHERE unit_id = ?1 AND product_id = ?2",
            rusqlite::params![unit_id, product_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    assert_eq!(
        fifo_layers_after, fifo_layers_before,
        "number of FIFO layer rows changed"
    );
}
