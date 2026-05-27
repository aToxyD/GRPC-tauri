use chrono::{Datelike, Utc};
use grpc_lib::application::services::{
    AuditTxService, DailyReportService, FiscalYearService, IntegrityService, IntegrityStatus,
    UserContext,
};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::audit::AuditAction;
use grpc_lib::models::{ConsumptionItemInput, DailyReportInput, MealSectionInput, MealType};
use grpc_lib::repositories::{executor::DbExecutor, FifoLayerRepository};
use uuid::Uuid;

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

fn parse_date(s: &str) -> chrono::NaiveDate {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

// ── Scenario 1: All checks pass on a clean DB ────────────────────────────

#[test]
fn test_clean_db_all_checks_pass() {
    let db = ConnectionFactory::new_for_test().expect("test db init failed");
    let report = IntegrityService::new(db.executor())
        .run()
        .expect("integrity check");
    assert_eq!(
        report.status,
        IntegrityStatus::Ok,
        "clean DB should be Ok, got {:?} with {} findings",
        report.status,
        report.findings.len()
    );
}

// ── Scenario 2: INV_FIFO_MISMATCH detected ───────────────────────────────

#[test]
fn test_detects_fifo_inventory_mismatch() {
    let db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    let year = Utc::now().year();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_product_and_stock(ex, &product_id, "Test", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(&fifo, &unit_id, &product_id, 100.0, 50.0, &now);
    }

    // inventory_stocks.quantity = 0 (from seed_product_and_stock), but
    // SUM(fifo.qty_remaining) = 100 → mismatch
    let report = IntegrityService::new(db.executor())
        .run()
        .expect("integrity check");
    assert!(
        report.status == IntegrityStatus::Critical || report.status == IntegrityStatus::Warning,
        "expected non-Ok status, got {:?}",
        report.status
    );

    let fifo_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code == "INV_FIFO_MISMATCH")
        .collect();
    assert!(
        !fifo_findings.is_empty(),
        "expected INV_FIFO_MISMATCH finding"
    );
}

// ── Scenario 3: NEGATIVE_FIFO_LAYER detected ─────────────────────────────

#[test]
fn test_detects_negative_layer() {
    let db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute(
            "INSERT INTO products (id, name, base_price, tva, year, created_at) VALUES (?1,'Neg',0.0,0.0,?2,?3)",
            rusqlite::params![product_id, Utc::now().year(), now],
        ).expect("insert product");
        ex.execute("PRAGMA ignore_check_constraints = ON", [])
            .expect("pragma");
        ex.execute(
            "INSERT INTO fifo_stock_layers (id, unit_id, product_id, source_type, unit_cost, qty_original, qty_remaining, received_at, created_by)
             VALUES (?1,?2,?3,'ORDER',10.0,5.0,-1.0,?4,'system')",
            rusqlite::params![Uuid::new_v4().to_string(), unit_id, product_id, now],
        ).expect("insert negative layer");
        ex.execute("PRAGMA ignore_check_constraints = OFF", [])
            .expect("pragma");
    }

    let report = IntegrityService::new(db.executor())
        .run()
        .expect("integrity check");
    let neg_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code == "NEGATIVE_FIFO_LAYER")
        .collect();
    assert!(
        !neg_findings.is_empty(),
        "expected NEGATIVE_FIFO_LAYER finding"
    );
}

// ── Scenario 4: No orphan false-positive on clean data ───────────────────
// (Positive detection requires FK OFF; the ORPHAN check exists as
//  defense-in-depth for data imported from external sources.)

#[test]
fn test_no_false_orphan_on_clean_data() {
    let db = ConnectionFactory::new_for_test().expect("test db init failed");
    let report = IntegrityService::new(db.executor())
        .run()
        .expect("integrity check");
    let orphan_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code == "ORPHAN_LAYER_CONSUMPTION")
        .collect();
    assert!(
        orphan_findings.is_empty(),
        "should be no orphan findings on clean DB"
    );
}

// ── Scenario 5: INVALID_CONSUMPTION_COST detected ─────────────────────────

#[test]
fn test_detects_invalid_consumption_cost() {
    let db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    let movement_id = Uuid::new_v4().to_string();
    let layer_id = Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute(
            "INSERT INTO products (id, name, base_price, tva, year, created_at) VALUES (?1,'Cost',0.0,0.0,?2,?3)",
            rusqlite::params![product_id, Utc::now().year(), now],
        ).expect("insert product");
        ex.execute(
            "INSERT INTO inventory_stocks (id, product_id, quantity, unit, last_updated, updated_at) VALUES (?1,?2,0.0,'unit',?3,?3)",
            rusqlite::params![format!("stock-{}", product_id), product_id, now],
        ).expect("insert inventory_stocks");
        ex.execute(
            "INSERT INTO fifo_stock_layers (id, unit_id, product_id, source_type, unit_cost, qty_original, qty_remaining, received_at, created_by)
             VALUES (?1,?2,?3,'ORDER',10.0,5.0,5.0,?4,'system')",
            rusqlite::params![layer_id, unit_id, product_id, now],
        ).expect("insert layer");
        ex.execute(
            "INSERT INTO stock_movements (id, product_id, movement_type, quantity, balance_before, balance_after, timestamp, user_id, username, unit_id)
             VALUES (?1,?2,'OUT',3.0,5.0,2.0,?3,'system','system',?4)",
            rusqlite::params![movement_id, product_id, now, unit_id],
        ).expect("insert movement");
        // qty=3, unit_cost=10 → expected total=30, but store 25 → invalid
        ex.execute(
            "INSERT INTO inventory_layer_consumptions (id, unit_id, movement_id, layer_id, quantity, unit_cost, total_cost, consumed_at)
             VALUES (?1,?2,?3,?4,3.0,10.0,25.0,?5)",
            rusqlite::params![Uuid::new_v4().to_string(), unit_id, movement_id, layer_id, now],
        ).expect("insert bad cost consumption");
    }

    let report = IntegrityService::new(db.executor())
        .run()
        .expect("integrity check");
    let cost_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code == "INVALID_CONSUMPTION_COST")
        .collect();
    assert!(
        !cost_findings.is_empty(),
        "expected INVALID_CONSUMPTION_COST finding"
    );
}

// ── Scenario 6: DAILY_REPORT_TOTAL_MISMATCH detected ──────────────────────

#[test]
fn test_detects_daily_report_total_mismatch() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    let year = Utc::now().year();
    let date = parse_date(&format!("{}-05-01", year));

    // Setup: clean fiscal year (delete default, seed current)
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute("DELETE FROM fiscal_year_status", [])
            .expect("clear fiscal status");
        ex.execute(
            "INSERT INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now],
        )
        .expect("seed fiscal year");
        ex.execute(
            "UPDATE settings SET current_year = ?1 WHERE id = 1",
            rusqlite::params![year],
        )
        .expect("set current year");
        seed_product_and_stock(ex, &product_id, "Report", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(&fifo, &unit_id, &product_id, 100.0, 50.0, &now);
    }
    // Sync inventory to match FIFO
    {
        let fifo_qty: f64 = db.executor().query_row(
            "SELECT COALESCE(SUM(qty_remaining), 0.0) FROM fifo_stock_layers WHERE unit_id = ?1 AND product_id = ?2",
            rusqlite::params![unit_id, product_id],
            |row| row.get(0),
        ).unwrap_or(0.0);
        db.executor()
            .execute(
                "UPDATE inventory_stocks SET quantity = ?1 WHERE product_id = ?2",
                rusqlite::params![fifo_qty, product_id],
            )
            .expect("sync inventory");
    }

    // Create a valid daily report
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
    .expect("daily report");

    // Now corrupt the report total directly
    let conn = db.get_connection();
    conn.execute(
        "UPDATE daily_reports SET total_daily_cost = 999.0 WHERE total_daily_cost != 999.0",
        [],
    )
    .expect("corrupt report total");

    let report = IntegrityService::new(db.executor())
        .run()
        .expect("integrity check");
    let total_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code == "DAILY_REPORT_TOTAL_MISMATCH")
        .collect();
    assert!(
        !total_findings.is_empty(),
        "expected DAILY_REPORT_TOTAL_MISMATCH finding, got {:?}",
        report.findings
    );
}

// ── Scenario 7: INVALID_CURRENT_FISCAL_YEAR detected ─────────────────────

#[test]
fn test_detects_invalid_current_fiscal_year() {
    let db = ConnectionFactory::new_for_test().expect("test db init failed");

    // Delete all fiscal years — current_year in settings points to year
    // that either doesn't exist or isn't open
    let ex = db.executor();
    ex.execute("DELETE FROM fiscal_year_status", [])
        .expect("clear fiscal status");

    let report = IntegrityService::new(db.executor())
        .run()
        .expect("integrity check");
    let fy_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code == "INVALID_CURRENT_FISCAL_YEAR")
        .collect();
    assert!(
        !fy_findings.is_empty(),
        "expected INVALID_CURRENT_FISCAL_YEAR finding"
    );
}

// ── Scenario 8: FIFO-inventory mismatch after daily report (should NOT fire) ─

#[test]
fn test_no_fifo_mismatch_after_clean_report() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    let year = Utc::now().year();
    let date = parse_date(&format!("{}-05-01", year));

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute("DELETE FROM fiscal_year_status", [])
            .expect("clear fiscal status");
        ex.execute(
            "INSERT INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now],
        )
        .expect("seed fiscal year");
        ex.execute(
            "UPDATE settings SET current_year = ?1 WHERE id = 1",
            rusqlite::params![year],
        )
        .expect("set current year");
        seed_product_and_stock(ex, &product_id, "CleanTest", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(&fifo, &unit_id, &product_id, 100.0, 50.0, &now);
    }

    // Sync inventory to match FIFO
    let fifo_qty: f64 = db.executor().query_row(
        "SELECT COALESCE(SUM(qty_remaining), 0.0) FROM fifo_stock_layers WHERE unit_id = ?1 AND product_id = ?2",
        rusqlite::params![unit_id, product_id],
        |row| row.get(0),
    ).unwrap_or(0.0);
    db.executor()
        .execute(
            "UPDATE inventory_stocks SET quantity = ?1 WHERE product_id = ?2",
            rusqlite::params![fifo_qty, product_id],
        )
        .expect("sync inventory");

    // Report should not trigger mismatch
    let report_before = IntegrityService::new(db.executor())
        .run()
        .expect("integrity check");
    let inv_findings_before: Vec<_> = report_before
        .findings
        .iter()
        .filter(|f| f.code == "INV_FIFO_MISMATCH")
        .collect();
    assert!(
        inv_findings_before.is_empty(),
        "should be no FIFO mismatch before report: {:?}",
        inv_findings_before
    );

    // Create daily report
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
    .expect("daily report");

    // Report should still be clean
    let report_after = IntegrityService::new(db.executor())
        .run()
        .expect("integrity check");
    let inv_findings_after: Vec<_> = report_after
        .findings
        .iter()
        .filter(|f| f.code == "INV_FIFO_MISMATCH")
        .collect();
    assert!(
        inv_findings_after.is_empty(),
        "should be no FIFO mismatch after clean report: {:?}",
        inv_findings_after
    );
}

// ── Scenario 9: Persist on clean DB → attempt PASS, no findings logged ────

#[test]
fn test_persist_clean_db_records_pass() {
    let db = ConnectionFactory::new_for_test().expect("test db init failed");
    let report = IntegrityService::new(db.executor())
        .run()
        .expect("integrity check");
    IntegrityService::new(db.executor())
        .persist(&report)
        .expect("persist");

    let pass_count: i64 = db
        .executor()
        .query_row(
            "SELECT COUNT(*) FROM integrity_verification_attempts WHERE outcome = 'PASS'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pass_count, 1, "expected 1 PASS attempt");

    let findings_count: i64 = db
        .executor()
        .query_row("SELECT COUNT(*) FROM operational_findings_log", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(findings_count, 0, "expected no findings on clean DB");
}

// ── Scenario 10: Persist on corrupt DB → attempt FAIL + findings logged ───

#[test]
fn test_persist_corrupt_db_records_fail_and_findings() {
    let db = ConnectionFactory::new_for_test().expect("test db init failed");
    // Delete fiscal years to trigger INVALID_CURRENT_FISCAL_YEAR (CRITICAL)
    db.executor()
        .execute("DELETE FROM fiscal_year_status", [])
        .expect("clear fiscal status");

    let report = IntegrityService::new(db.executor())
        .run()
        .expect("integrity check");
    assert!(
        report.status != IntegrityStatus::Ok,
        "expected non-Ok after corrupting fiscal status"
    );
    IntegrityService::new(db.executor())
        .persist(&report)
        .expect("persist");

    let fail_count: i64 = db
        .executor()
        .query_row(
            "SELECT COUNT(*) FROM integrity_verification_attempts WHERE outcome = 'FAIL'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fail_count, 1, "expected 1 FAIL attempt");

    let critical_count: i64 = db.executor().query_row(
        "SELECT COUNT(*) FROM operational_findings_log WHERE severity = 'CRITICAL' AND category = 'INTEGRITY_VERIFICATION'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert!(
        critical_count > 0,
        "expected at least one CRITICAL finding in operational_findings_log"
    );
}

// ── Scenario 11: Integrity gate blocks close_year with Critical findings ───

#[test]
fn test_integrity_gate_blocks_close_year_on_critical() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let year = Utc::now().year();
    let next_year = year + 1;

    // Delete all fiscal years → INVALID_CURRENT_FISCAL_YEAR is Critical
    db.executor()
        .execute("DELETE FROM fiscal_year_status", [])
        .expect("clear fiscal status");

    // close_year must fail
    let result = db.with_transaction(|tx| {
        FiscalYearService::new(tx).close_year(year, next_year, "system", "admin", None)
    });
    assert!(
        result.is_err(),
        "expected BusinessLogic error from close_year with critical integrity issues"
    );
    let err_msg = format!("{:?}", result.err().unwrap());
    assert!(
        err_msg.contains("OperationNotPermitted") || err_msg.contains("لا يمكن إغلاق"),
        "error should indicate integrity gate blocked close: {}",
        err_msg,
    );
}

// ── Scenario 12: Integrity gate allows close_year when clean ──────────────

#[test]
fn test_integrity_gate_allows_close_year_when_clean() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let year = Utc::now().year();
    let next_year = year + 1;
    let product_id = Uuid::new_v4().to_string();
    let unit_id = Uuid::new_v4().to_string();

    // Setup: one open year, proper fiscal state
    {
        let ex = db.executor();
        ex.execute("DELETE FROM fiscal_year_status", [])
            .expect("clear");
        ex.execute(
            "INSERT INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now],
        )
        .expect("seed open year");
        ex.execute(
            "UPDATE settings SET current_year = ?1 WHERE id = 1",
            rusqlite::params![year],
        )
        .expect("set current year");
        seed_unit(ex, &unit_id, &now);
        seed_product_and_stock(ex, &product_id, "GateTest", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(&fifo, &unit_id, &product_id, 100.0, 50.0, &now);
    }

    // Sync inventory to match FIFO
    let fifo_qty: f64 = db.executor().query_row(
        "SELECT COALESCE(SUM(qty_remaining), 0.0) FROM fifo_stock_layers WHERE unit_id = ?1 AND product_id = ?2",
        rusqlite::params![unit_id, product_id],
        |row| row.get(0),
    ).unwrap_or(0.0);
    db.executor()
        .execute(
            "UPDATE inventory_stocks SET quantity = ?1 WHERE product_id = ?2",
            rusqlite::params![fifo_qty, product_id],
        )
        .expect("sync inventory");

    // close_year should succeed
    let result = db.with_transaction(|tx| {
        FiscalYearService::new(tx).close_year(year, next_year, "system", "admin", Some(&unit_id))
    });
    assert!(
        result.is_ok(),
        "close_year should succeed on clean DB: {:?}",
        result.err()
    );
}
