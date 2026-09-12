use chrono::{NaiveDate, Utc};
use grpc_lib::application::services::{
    AuditTxService, DailyReportService, FiscalClosingService, UserContext,
};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::audit::AuditAction;
use grpc_lib::models::{ConsumptionItemInput, DailyReportInput, MealSectionInput, MealType};
use grpc_lib::repositories::{executor::DbExecutor, FifoLayerRepository, RepositoryProvider};
use uuid::Uuid;

const YEAR_N: i32 = 2025;
const YEAR_N1: i32 = 2026;

fn parse_date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

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
        YEAR_N,
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
            "SELECT COALESCE(SUM(total_cost), 0) FROM inventory_layer_consumptions WHERE movement_id = ?1",
            rusqlite::params![movement_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    sum as f64 / 100.0
}

fn get_meal_item_sum(ex: DbExecutor<'_>, report_id: &str, product_id: &str) -> f64 {
    let sum: i64 = ex
        .query_row(
            "SELECT COALESCE(SUM(drmi.total_cost), 0)
         FROM daily_report_meal_items drmi
         INNER JOIN daily_report_meals drm ON drmi.meal_id = drm.id
         WHERE drm.daily_report_id = ?1 AND drmi.product_id = ?2",
            rusqlite::params![report_id, product_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    sum as f64 / 100.0
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

fn assert_inventory_fifo_consistency(ex: DbExecutor<'_>, unit_id: &str, products: &[&str]) {
    for &product_id in products {
        let fifo_qty: f64 = ex.query_row(
            "SELECT COALESCE(SUM(qty_remaining), 0.0) FROM fifo_stock_layers WHERE unit_id = ?1 AND product_id = ?2",
            rusqlite::params![unit_id, product_id],
            |row| row.get(0),
        ).unwrap_or(0.0);
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
            "FIFO-inventory mismatch for product {}: fifo={}, stock={}, diff={}",
            product_id,
            fifo_qty,
            stock_qty,
            diff
        );
    }
}

// ── Scenario 1: Single layer carry-forward ────────────────────────────────

#[test]
fn test_e2e_single_layer_carry_forward() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // Setup
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_fiscal_year(ex, YEAR_N);
        seed_product_and_stock(ex, &product_id, "Rice", &now, YEAR_N);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            100.0,
            50.0,
            "2025-06-01T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    // Phase 1: consume 30kg @ 50 = 1500 in YEAR_N
    let input = DailyReportInput {
        date: parse_date("2025-11-01"),
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
    .expect("daily report YEAR_N failed");

    let ex = db.executor();
    let mid = get_movement_id_for_product(ex, &report_id, &product_id);
    let cost_n = get_layer_consumption_sum(ex, &mid);
    assert!(
        (cost_n - 1500.0).abs() < 0.01,
        "YEAR_N cost: expected 1500, got {}",
        cost_n
    );

    // Phase 2: close YEAR_N, open YEAR_N1
    let uid = admin_id(&db);
    db.with_transaction(|ex| {
        FiscalClosingService::new(ex).close_year(YEAR_N, YEAR_N1, &uid, "system", None)
    })
    .expect("close_year failed");

    // Phase 3: verify opening balance snapshot
    let ex = db.executor();
    let snaps = ex
        .opening_balances()
        .get_by_year(YEAR_N1)
        .expect("get snapshots");
    let snap = snaps
        .iter()
        .find(|s| s.product_id == product_id)
        .expect("snapshot for product");
    assert!(
        (snap.opening_quantity - 70.0).abs() < 1e-9,
        "snapshot qty: expected 70, got {}",
        snap.opening_quantity
    );
    assert!(
        (snap.unit_cost - 50.0).abs() < 1e-9,
        "snapshot unit_cost: expected 50, got {}",
        snap.unit_cost
    );
    assert!(
        (snap.total_value - 3500.0).abs() < 1e-9,
        "snapshot value: expected 3500, got {}",
        snap.total_value
    );
    assert_eq!(snap.snapshot_reason, "year_close");
    assert_eq!(snap.carried_from_year, Some(YEAR_N));

    // Phase 4: consume 20kg @ 50 = 1000 in YEAR_N1
    let input2 = DailyReportInput {
        date: parse_date("2026-01-15"),
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 20.0)])],
    };
    let report_id2 = AuditTxService::execute_with_audit(
        &mut db,
        AuditAction::CreateDailyReport,
        &user_ctx,
        |tx| {
            DailyReportService::new(tx.executor).create_daily_report(
                &input2,
                Some(&unit_id),
                "system",
                "system",
            )
        },
    )
    .expect("daily report YEAR_N1 failed");

    let ex = db.executor();
    let mid2 = get_movement_id_for_product(ex, &report_id2, &product_id);
    let cost_n1 = get_layer_consumption_sum(ex, &mid2);
    // 20kg from the same 50/kg layer that carried over
    assert!(
        (cost_n1 - 1000.0).abs() < 0.01,
        "YEAR_N1 cost: expected 1000, got {}",
        cost_n1
    );

    let meal_sum = get_meal_item_sum(ex, &report_id2, &product_id);
    assert!(
        (meal_sum - cost_n1).abs() < 1e-9,
        "meal_items cost mismatch: {} vs {}",
        meal_sum,
        cost_n1
    );

    assert_inventory_fifo_consistency(ex, &unit_id, &[&product_id]);
}

// ── Scenario 2: Multi-layer carry-forward ─────────────────────────────────

#[test]
fn test_e2e_multi_layer_carry_forward() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    // Setup: L1=100@50, L2=200@80
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_fiscal_year(ex, YEAR_N);
        seed_product_and_stock(ex, &product_id, "Flour", &now, YEAR_N);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            100.0,
            50.0,
            "2025-06-01T08:00:00Z",
        );
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            200.0,
            80.0,
            "2025-07-01T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    // Phase 1: consume 150 — consumes L1 fully (100@50=5000) + 50@80=4000 from L2 = 9000
    let input = DailyReportInput {
        date: parse_date("2025-11-01"),
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 150.0)])],
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
    .expect("daily report YEAR_N failed");

    let ex = db.executor();
    let mid = get_movement_id_for_product(ex, &report_id, &product_id);
    let cost_n = get_layer_consumption_sum(ex, &mid);
    assert!(
        (cost_n - 9000.0).abs() < 0.01,
        "YEAR_N cost: expected 9000, got {}",
        cost_n
    );

    // Phase 2: close_year
    let uid = admin_id(&db);
    db.with_transaction(|ex| {
        FiscalClosingService::new(ex).close_year(YEAR_N, YEAR_N1, &uid, "system", None)
    })
    .expect("close_year failed");

    // Phase 3: verify snapshot — remaining L2: 150@80 = 12000
    let ex = db.executor();
    let snaps = ex
        .opening_balances()
        .get_by_year(YEAR_N1)
        .expect("get snapshots");
    let snap = snaps
        .iter()
        .find(|s| s.product_id == product_id)
        .expect("snapshot for product");
    assert!(
        (snap.opening_quantity - 150.0).abs() < 1e-9,
        "snapshot qty: expected 150, got {}",
        snap.opening_quantity
    );
    assert!(
        (snap.unit_cost - 80.0).abs() < 1e-9,
        "snapshot unit_cost: expected 80, got {}",
        snap.unit_cost
    );
    assert!(
        (snap.total_value - 12000.0).abs() < 1e-9,
        "snapshot value: expected 12000, got {}",
        snap.total_value
    );

    // Phase 4: consume 50kg in YEAR_N1 — should take from L2's 150@80 = 4000
    let input2 = DailyReportInput {
        date: parse_date("2026-01-15"),
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 50.0)])],
    };
    let report_id2 = AuditTxService::execute_with_audit(
        &mut db,
        AuditAction::CreateDailyReport,
        &user_ctx,
        |tx| {
            DailyReportService::new(tx.executor).create_daily_report(
                &input2,
                Some(&unit_id),
                "system",
                "system",
            )
        },
    )
    .expect("daily report YEAR_N1 failed");

    let ex = db.executor();
    let mid2 = get_movement_id_for_product(ex, &report_id2, &product_id);
    let cost_n1 = get_layer_consumption_sum(ex, &mid2);
    assert!(
        (cost_n1 - 4000.0).abs() < 0.01,
        "YEAR_N1 cost: expected 4000, got {}",
        cost_n1
    );

    let meal_sum = get_meal_item_sum(ex, &report_id2, &product_id);
    assert!(
        (meal_sum - cost_n1).abs() < 1e-9,
        "meal_items cost mismatch: {} vs {}",
        meal_sum,
        cost_n1
    );

    assert_inventory_fifo_consistency(ex, &unit_id, &[&product_id]);
}

// ── Scenario 3: Multi-product carry-forward ───────────────────────────────

#[test]
fn test_e2e_multi_product_carry_forward() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let rice_id = Uuid::new_v4().to_string();
    let oil_id = Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_fiscal_year(ex, YEAR_N);
        seed_product_and_stock(ex, &rice_id, "Rice", &now, YEAR_N);
        seed_product_and_stock(ex, &oil_id, "Oil", &now, YEAR_N);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &rice_id,
            100.0,
            50.0,
            "2025-06-01T08:00:00Z",
        );
        add_layer(
            &fifo,
            &unit_id,
            &oil_id,
            50.0,
            200.0,
            "2025-06-01T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &rice_id);
    sync_inventory_from_fifo(db.executor(), &unit_id, &oil_id);

    // Phase 1: consume Rice 30@50=1500, Oil 10@200=2000
    let input = DailyReportInput {
        date: parse_date("2025-11-01"),
        meals: vec![meal(
            MealType::Breakfast,
            vec![(&rice_id, 30.0), (&oil_id, 10.0)],
        )],
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
    .expect("daily report YEAR_N failed");

    let ex = db.executor();
    let mid_r = get_movement_id_for_product(ex, &report_id, &rice_id);
    let mid_o = get_movement_id_for_product(ex, &report_id, &oil_id);
    let cost_r = get_layer_consumption_sum(ex, &mid_r);
    let cost_o = get_layer_consumption_sum(ex, &mid_o);
    assert!(
        (cost_r - 1500.0).abs() < 0.01,
        "Rice cost: expected 1500, got {}",
        cost_r
    );
    assert!(
        (cost_o - 2000.0).abs() < 0.01,
        "Oil cost: expected 2000, got {}",
        cost_o
    );

    // Phase 2: close_year
    let uid = admin_id(&db);
    db.with_transaction(|ex| {
        FiscalClosingService::new(ex).close_year(YEAR_N, YEAR_N1, &uid, "system", None)
    })
    .expect("close_year failed");

    // Phase 3: verify snapshots — Rice 70@50=3500, Oil 40@200=8000
    let ex = db.executor();
    let snaps = ex
        .opening_balances()
        .get_by_year(YEAR_N1)
        .expect("get snapshots");
    let snap_r = snaps
        .iter()
        .find(|s| s.product_id == rice_id)
        .expect("snapshot for Rice");
    let snap_o = snaps
        .iter()
        .find(|s| s.product_id == oil_id)
        .expect("snapshot for Oil");
    assert!(
        (snap_r.opening_quantity - 70.0).abs() < 1e-9,
        "Rice qty: expected 70, got {}",
        snap_r.opening_quantity
    );
    assert!(
        (snap_r.total_value - 3500.0).abs() < 1e-9,
        "Rice value: expected 3500, got {}",
        snap_r.total_value
    );
    assert!(
        (snap_o.opening_quantity - 40.0).abs() < 1e-9,
        "Oil qty: expected 40, got {}",
        snap_o.opening_quantity
    );
    assert!(
        (snap_o.total_value - 8000.0).abs() < 1e-9,
        "Oil value: expected 8000, got {}",
        snap_o.total_value
    );

    // Phase 4: consume in YEAR_N1 — Rice 20@50=1000, Oil 15@200=3000
    let input2 = DailyReportInput {
        date: parse_date("2026-01-15"),
        meals: vec![meal(
            MealType::Breakfast,
            vec![(&rice_id, 20.0), (&oil_id, 15.0)],
        )],
    };
    let report_id2 = AuditTxService::execute_with_audit(
        &mut db,
        AuditAction::CreateDailyReport,
        &user_ctx,
        |tx| {
            DailyReportService::new(tx.executor).create_daily_report(
                &input2,
                Some(&unit_id),
                "system",
                "system",
            )
        },
    )
    .expect("daily report YEAR_N1 failed");

    let ex = db.executor();
    let mid2_r = get_movement_id_for_product(ex, &report_id2, &rice_id);
    let mid2_o = get_movement_id_for_product(ex, &report_id2, &oil_id);
    let cost2_r = get_layer_consumption_sum(ex, &mid2_r);
    let cost2_o = get_layer_consumption_sum(ex, &mid2_o);
    assert!(
        (cost2_r - 1000.0).abs() < 0.01,
        "Rice YEAR_N1 cost: expected 1000, got {}",
        cost2_r
    );
    assert!(
        (cost2_o - 3000.0).abs() < 0.01,
        "Oil YEAR_N1 cost: expected 3000, got {}",
        cost2_o
    );

    assert_inventory_fifo_consistency(ex, &unit_id, &[&rice_id, &oil_id]);
}
