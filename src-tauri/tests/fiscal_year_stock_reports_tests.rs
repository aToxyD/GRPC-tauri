use chrono::Utc;
use grpc_lib::db::ConnectionFactory;
use grpc_lib::repositories::{executor::DbExecutor, ReportRepository};
use uuid::Uuid;

fn init_db() -> grpc_lib::db::Database {
    let db = ConnectionFactory::new_for_test().expect("test db init failed");
    let ex = db.executor();

    // Setup settings for UNIT node
    ex.execute(
        "UPDATE settings SET node_type='UNIT', unit_name='test_unit', wilaya_code='99', configured=1 WHERE id=1",
        [],
    ).unwrap();

    // Insert a unit row
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

    // Insert inventory_stocks rows
    let stock_id1 = Uuid::new_v4().to_string();
    ex.execute(
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, last_updated, updated_at) VALUES (?1, 'p1', 50.0, 'kg', ?2, ?2)",
        rusqlite::params![stock_id1, now],
    ).unwrap_or_else(|e| panic!("insert stock p1: {}", e));
    let stock_id2 = Uuid::new_v4().to_string();
    ex.execute(
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, last_updated, updated_at) VALUES (?1, 'p2', 30.0, 'L', ?2, ?2)",
        rusqlite::params![stock_id2, now],
    ).unwrap_or_else(|e| panic!("insert stock p2: {}", e));

    db
}

fn insert_movement(ex: DbExecutor<'_>, product_id: &str, mtype: &str, qty: f64, year: i32) {
    let id = Uuid::new_v4().to_string();
    let ts = Utc::now().to_rfc3339();
    ex.execute(
        "INSERT INTO stock_movements (id, product_id, movement_type, quantity, balance_before, balance_after, timestamp, user_id, username, unit_id, updated_at, fiscal_year, unit_cost)
         VALUES (?1, ?2, ?3, ?4, 0.0, ?4, ?5, 'test_user', 'test_user', 'test_unit', ?5, ?6, 100.0)",
        rusqlite::params![id, product_id, mtype, qty, ts, year],
    ).unwrap_or_else(|e| panic!("insert movement: {}", e));
}

fn insert_daily_report(ex: DbExecutor<'_>, date: &str, year: i32, unit_id: &str) -> String {
    let id = Uuid::new_v4().to_string();
    let ts = Utc::now().to_rfc3339();
    ex.execute(
        "INSERT INTO daily_reports (id, date, unit_id, total_daily_cost, total_daily_average, total_daily_beneficiaries, created_at, updated_at, fiscal_year)
         VALUES (?1, ?2, ?3, 1000.0, 200.0, 50, ?4, ?4, ?5)",
        rusqlite::params![id, date, unit_id, ts, year],
    ).unwrap_or_else(|e| panic!("insert daily report: {}", e));
    id
}

// ─── Test 1: stock_summary_no_filter ────────────────────────────────────────
#[test]
fn stock_summary_no_filter() {
    let db = init_db();
    let ex = db.executor();
    // movements in two different fiscal years
    insert_movement(ex, "p1", "IN", 100.0, 2024);
    insert_movement(ex, "p1", "IN", 200.0, 2025);
    insert_movement(ex, "p2", "OUT", 10.0, 2024);

    let service = grpc_lib::application::services::StockLevelService::new(db.executor());
    // None = no filter → all movements included
    let summary = service.get_stock_summary(None).unwrap();

    let p1 = summary.iter().find(|s| s.product_id == "p1").unwrap();
    let p2 = summary.iter().find(|s| s.product_id == "p2").unwrap();

    assert_eq!(p1.current_quantity, 50.0, "real stock unfiltered");
    assert_eq!(p1.total_in, 300.0, "all IN across years");
    assert_eq!(p1.total_out, 0.0);
    assert_eq!(p1.movement_count, 2);

    assert_eq!(p2.total_out, 10.0);
    assert_eq!(p2.movement_count, 1);
}

// ─── Test 2: stock_summary_fiscal_year_filter ───────────────────────────────
#[test]
fn stock_summary_fiscal_year_filter() {
    let db = init_db();
    let ex = db.executor();
    insert_movement(ex, "p1", "IN", 100.0, 2024);
    insert_movement(ex, "p1", "IN", 200.0, 2025);
    insert_movement(ex, "p2", "OUT", 10.0, 2024);
    insert_movement(ex, "p2", "OUT", 5.0, 2025);

    let service = grpc_lib::application::services::StockLevelService::new(db.executor());
    // Filter to fiscal year 2025 only
    let summary = service.get_stock_summary(Some(2025)).unwrap();

    let p1 = summary.iter().find(|s| s.product_id == "p1").unwrap();
    let p2 = summary.iter().find(|s| s.product_id == "p2").unwrap();

    assert_eq!(p1.current_quantity, 50.0, "real stock unchanged");
    assert_eq!(p1.total_in, 200.0, "only 2025 IN");
    assert_eq!(p1.total_out, 0.0);
    assert_eq!(p1.movement_count, 1);

    assert_eq!(p2.total_out, 5.0, "only 2025 OUT");
    assert_eq!(p2.movement_count, 1);
}

// ─── Test 3: list_reports_by_fiscal_year ────────────────────────────────────
#[test]
fn list_reports_by_fiscal_year() {
    let db = init_db();
    let ex = db.executor();
    insert_daily_report(ex, "2024-06-15", 2024, "test_unit");
    insert_daily_report(ex, "2025-01-10", 2025, "test_unit");
    insert_daily_report(ex, "2025-12-20", 2025, "test_unit");

    let repo = ReportRepository::new(db.executor());
    // All reports for fiscal year 2025
    let reports = repo
        .list_daily_reports_by_fiscal_year(Some(2025), None, "test_unit")
        .unwrap();

    assert_eq!(reports.len(), 2, "only 2025 reports");
    assert!(reports.iter().all(|r| r.fiscal_year == 2025));
}

// ─── Test 4: list_reports_by_fiscal_year_and_month ──────────────────────────
#[test]
fn list_reports_by_fiscal_year_and_month() {
    let db = init_db();
    let ex = db.executor();
    insert_daily_report(ex, "2025-01-10", 2025, "test_unit");
    insert_daily_report(ex, "2025-02-05", 2025, "test_unit");
    insert_daily_report(ex, "2025-02-20", 2025, "test_unit");

    let repo = ReportRepository::new(db.executor());
    // Month = 2 (February)
    let reports = repo
        .list_daily_reports_by_fiscal_year(Some(2025), Some(2), "test_unit")
        .unwrap();

    assert_eq!(reports.len(), 2, "two reports in February");
    assert!(reports.iter().all(|r| r.fiscal_year == 2025));
}

// ─── Test 5: list_reports_invalid_month ────────────────────────────────────
#[test]
fn list_reports_invalid_month() {
    let db = init_db();
    let repo = ReportRepository::new(db.executor());

    let result = repo.list_daily_reports_by_fiscal_year(Some(2025), Some(13), "test_unit");
    assert!(result.is_err(), "month 13 should be rejected");

    let err_str = format!("{}", result.unwrap_err());
    assert!(err_str.contains("month"), "error mentions month field");
}

// ─── Test 6: list_available_fiscal_years ────────────────────────────────────
#[test]
fn list_available_fiscal_years() {
    let db = init_db();
    let ex = db.executor();

    // Clear seed data first for deterministic test
    ex.execute("DELETE FROM fiscal_year_status", []).unwrap();
    let ts = Utc::now().to_rfc3339();
    ex.execute(
        "INSERT INTO fiscal_year_status (year, status, opened_at) VALUES (2023, 'closed', ?1)",
        rusqlite::params![ts],
    )
    .unwrap();
    ex.execute(
        "INSERT INTO fiscal_year_status (year, status, opened_at) VALUES (2025, 'open', ?1)",
        rusqlite::params![ts],
    )
    .unwrap();

    insert_daily_report(ex, "2024-06-01", 2024, "test_unit");
    insert_daily_report(ex, "2025-01-10", 2025, "test_unit");

    let repo = ReportRepository::new(db.executor());
    let years = repo.list_available_fiscal_years().unwrap();

    // Expect [2025, 2024, 2023] — sorted descending, distinct
    assert_eq!(years, vec![2025, 2024, 2023], "years sorted desc");
}
