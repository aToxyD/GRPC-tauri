//! Phase 5 post-integer conformance regression tests.
//!
//! Pins the residual accounting-defect fixes:
//!   1. FIFO consumption-cost integrity accepts the writer's boundary-rounded
//!      centime value `(quantity*unit_cost + 500) / 1000` (otherwise every
//!      sub-cent product is a false `INVALID_CONSUMPTION_COST`).
//!   2. A daily report whose FIFO meal costs carry sub-cent fractions must
//!      persist a header equal to the strict integer sum of the stored meal
//!      values, so `DAILY_REPORT_TOTAL_MISMATCH` never false-positives.
//!   3. `fetch_stock_movements_iter` reads quantity/balances at scale-3 and
//!      unit_cost at scale-2 (not raw unscaled INTEGERs).

use chrono::{Datelike, Utc};
use grpc_lib::application::services::{
    AuditTxService, DailyReportService, IntegrityService, UserContext,
};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::audit::AuditAction;
use grpc_lib::models::{ConsumptionItemInput, DailyReportInput, MealSectionInput, MealType};
use grpc_lib::repositories::{executor::DbExecutor, IntegrityRepository};
use uuid::Uuid;

fn seed_unit(ex: DbExecutor<'_>, id: &str, now: &str) {
    ex.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U01','Test Unit','01',?2)",
        rusqlite::params![id, now],
    )
    .expect("insert unit");
}

fn seed_product(ex: DbExecutor<'_>, product_id: &str, now: &str) {
    ex.execute(
        "INSERT INTO products (id, name, base_price, year, created_at, purchase_unit, consumption_unit, conversion_factor, tva_classification) VALUES (?1,'P',0.0,?2,?3,1,1,1,0)",
        rusqlite::params![product_id, Utc::now().year(), now],
    )
    .expect("insert product");
}

fn seed_product_and_stock(ex: DbExecutor<'_>, product_id: &str, name: &str, now: &str, year: i32) {
    ex.execute(
        "INSERT INTO products (id, name, base_price, year, created_at, purchase_unit, consumption_unit, conversion_factor, tva_classification) VALUES (?1,?2,0.0,?3,?4,1,1,1,0)",
        rusqlite::params![product_id, name, year, now],
    )
    .expect("insert product");
    ex.execute(
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, consumption_unit, last_updated, updated_at)
         VALUES (?1,?2,0.0,'unit',1,?3,?3)",
        rusqlite::params![format!("stock-{}", product_id), product_id, now],
    )
    .expect("insert inventory_stocks");
}

fn add_layer(
    fifo: &grpc_lib::repositories::FifoLayerRepository,
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

fn sync_stock_to_fifo(db: &grpc_lib::db::Database) {
    let total: i64 = db
        .executor()
        .query_row(
            "SELECT COALESCE(SUM(qty_remaining),0) FROM fifo_stock_layers",
            [],
            |r| r.get(0),
        )
        .expect("fifo sum");
    db.executor()
        .execute(
            "UPDATE inventory_stocks SET quantity = ?1",
            rusqlite::params![total],
        )
        .expect("sync inventory");
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

fn open_fiscal_year(ex: DbExecutor<'_>, year: i32, now: &str) {
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
}

// ── 1. consumption-cost integrity uses the writer's cent rounding ──────────

#[test]
fn sub_cent_consumption_cost_rounding_is_not_flagged() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    let layer_id = {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_product(ex, &product_id, &now);
        grpc_lib::repositories::FifoLayerRepository::new(ex)
            .create_layer(
                &unit_id,
                &product_id,
                "ORDER",
                None,
                6.67,
                100.0,
                &now,
                "phase5",
                2025,
            )
            .expect("create layer")
    };

    let movement_id = Uuid::new_v4().to_string();
    let consumption_id = Uuid::new_v4().to_string();
    db.executor()
        .execute(
            "INSERT INTO stock_movements (id, product_id, movement_type, quantity, balance_before, balance_after, timestamp, user_id, username, unit_id, updated_at, fiscal_year, unit_cost)
             VALUES (?1, ?2, 'IN', 100000, 0, 100000, ?3, 'u', 'u', ?4, ?3, 2025, 667)",
            rusqlite::params![&movement_id, &product_id, &now, &unit_id],
        )
        .expect("insert movement");
    // qty=1.001 (scaled 1001) × unit_cost=6.67 (scaled 667) = 667.667 centimes;
    // the writer persists round(667.667) = 668 centimes.
    db.executor()
        .execute(
            "INSERT INTO inventory_layer_consumptions (id, unit_id, movement_id, layer_id, quantity, unit_cost, total_cost, consumed_at)
             VALUES (?1,?2,?3,?4,1001,667,668,?5)",
            rusqlite::params![consumption_id, &unit_id, &movement_id, &layer_id, &now],
        )
        .expect("insert consumption");

    let integrity = IntegrityRepository::new(db.executor());
    let bad = integrity
        .fetch_invalid_consumption_costs()
        .expect("integrity check");
    assert!(
        bad.is_empty(),
        "boundary-rounded consumption cost must be clean: {bad:?}"
    );

    // A genuine one-cent deviation IS a mismatch.
    db.executor()
        .execute(
            "UPDATE inventory_layer_consumptions SET total_cost = 667 WHERE id = ?1",
            rusqlite::params![&consumption_id],
        )
        .expect("corrupt");
    let bad = integrity
        .fetch_invalid_consumption_costs()
        .expect("integrity check");
    assert_eq!(bad.len(), 1, "one-cent cost deviation must be reported");
}

// ── 2. daily report end-to-end: sub-cent FIFO meal costs stay exact ───────

#[test]
fn sub_cent_fifo_meal_costs_pass_consumption_and_daily_integrity() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    let year = Utc::now().year();
    let date = chrono::NaiveDate::from_ymd_opt(year, 5, 1).expect("date");

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        open_fiscal_year(ex, year, &now);
        seed_product_and_stock(ex, &product_id, "FIFO", &now, year);
        let fifo = grpc_lib::repositories::FifoLayerRepository::new(ex);
        // 6.67 DA unit cost (scaled 667) → 1.501 × 6.67 = 10.01167 DA carries
        // a sub-cent fraction after meal-level rounding.
        add_layer(&fifo, &unit_id, &product_id, 100.0, 6.67, &now);
    }
    sync_stock_to_fifo(&db);

    let input = DailyReportInput {
        date,
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 1.501)])],
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
    sync_stock_to_fifo(&db);

    let report = IntegrityService::new(db.executor())
        .run()
        .expect("integrity check");
    let consumption = report
        .findings
        .iter()
        .filter(|f| f.code == "INVALID_CONSUMPTION_COST")
        .count();
    let daily_mismatch = report
        .findings
        .iter()
        .filter(|f| f.code == "DAILY_REPORT_TOTAL_MISMATCH")
        .count();
    assert_eq!(
        (consumption, daily_mismatch),
        (0, 0),
        "sub-cent FIFO meal costs must not produce false integrity findings: {:?}",
        report.findings
    );
}

// ── 3. stock movement iter reads INTEGERS with scale ──────────────────────

#[test]
fn stock_movement_iter_reads_scaled_integers() {
    use grpc_lib::repositories::StockMovementRepository;

    let db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    let movement_id = Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        ex.execute(
            "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U01','U','01',?2)",
            rusqlite::params![unit_id, now],
        )
        .expect("unit");
        ex.execute(
            "INSERT INTO products (id, name, base_price, year, created_at, purchase_unit, consumption_unit, conversion_factor, tva_classification) VALUES (?1,?2,0.0,?3,?4,1,1,1,0)",
            rusqlite::params![product_id, "P", Utc::now().year(), now],
        )
        .expect("product");
        // quantity 5500 = 5.500, balances scaled, unit_cost 1234 = 12.34.
        ex.execute(
            "INSERT INTO stock_movements (id, product_id, movement_type, quantity, balance_before, balance_after, timestamp, user_id, username, unit_id, fiscal_year, unit_cost)
             VALUES (?1,?2,'IN',5500,0,5500,?3,'system','system',?4,?5,1234)",
            rusqlite::params![movement_id, product_id, now, unit_id, Utc::now().year()],
        )
        .expect("movement");
    }

    let repo = StockMovementRepository::new(db.executor());
    let mut rows = Vec::new();
    repo.fetch_stock_movements_iter(&Default::default(), |row| {
        rows.push(row);
        Ok(())
    })
    .expect("iter fetch");

    assert_eq!(rows.len(), 1, "expected exactly one movement row");
    assert!((rows[0].quantity - 5.5).abs() < f64::EPSILON);
    assert!((rows[0].balance_after - 5.5).abs() < f64::EPSILON);
    assert!((rows[0].balance_before).abs() < f64::EPSILON);
    assert_eq!(rows[0].unit_cost, Some(12.34));
}
