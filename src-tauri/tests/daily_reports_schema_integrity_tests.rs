//! Q2/Q3 database-integrity schema tests (ADR-0065, C7).
//!
//! Pins the fresh-install baseline:
//! - `daily_reports.unit_id` NOT NULL + `REFERENCES units(id) ON DELETE RESTRICT`
//! - `daily_report_meal_items.fifo_layer_id`
//!   `REFERENCES fifo_stock_layers(id) ON DELETE RESTRICT` (nullable)
//! - `supplier_orders.unit_id` / `supplier_order_items.unit_id` NOT NULL
//!   (the unit FK with `ON DELETE RESTRICT` pre-existed; only nullability
//!   changed)
//! - `idx_daily_reports_date_unit_unique` as an ordinary two-column unique
//!   index on `(date, unit_id)`
//!
//! Tests assert **distinct SQLite failure classes** — `NOT NULL constraint
//! failed` vs `FOREIGN KEY constraint failed` vs `UNIQUE constraint failed`
//! — plus the service-level unit gates (C2/C3/C4), all of which fail closed
//! with zero writes.

use chrono::Utc;
use grpc_lib::application::services::{
    DailyReportService, OrderService, SyncImportExecutionService,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::errors::{AppError, BusinessLogicError};
use grpc_lib::models::{
    ConsumptionItemInput, CreateOrderRequest, DailyConsumptionSummary, DailyReport,
    DailyReportInput, DailyReportMeal, DailyReportResult, MealSectionInput, MealSectionResult,
    MealType, OrderItemInput,
};
use grpc_lib::repositories::RepositoryProvider;
use tempfile::tempdir;
use uuid::Uuid;

const NOW: &str = "2025-01-01T00:00:00Z";

fn fresh_db(test_name: &str) -> (tempfile::TempDir, Database) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join(format!("{test_name}.db"));
    let db = ConnectionFactory::new_with_path(&path).expect("db init");
    (dir, db)
}

fn seed_unit(conn: &rusqlite::Connection, id: &str) {
    conn.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) \
         VALUES (?1, ?2, ?3, '16', ?4)",
        rusqlite::params![id, id, id, NOW],
    )
    .unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// Schema level — failure-class discrimination.
// ─────────────────────────────────────────────────────────────────────────────

// ADR-0065 C1/TA1 — missing unit (NOT NULL class).
#[test]
fn daily_report_without_unit_id_rejected_not_null() {
    let (_dir, db) = fresh_db("ta1_null");
    let err = db
        .get_connection()
        .execute(
            "INSERT INTO daily_reports (id, date, unit_id, total_daily_cost, total_daily_average, \
             total_daily_beneficiaries, created_at, updated_at, fiscal_year) \
             VALUES ('dr-x', '2025-01-01', NULL, 0, 0, 0, ?1, ?1, 2025)",
            [NOW],
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("NOT NULL constraint failed"),
        "expected NOT NULL violation, got: {err}"
    );
    assert_eq!(
        db.get_connection()
            .query_row("SELECT COUNT(*) FROM daily_reports", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

// ADR-0065 C1/TA1 — nonexistent unit (FOREIGN KEY class).
#[test]
fn daily_report_with_nonexistent_unit_rejected_fk() {
    let (_dir, db) = fresh_db("ta1_fk");
    let err = db
        .get_connection()
        .execute(
            "INSERT INTO daily_reports (id, date, unit_id, total_daily_cost, total_daily_average, \
             total_daily_beneficiaries, created_at, updated_at, fiscal_year) \
             VALUES ('dr-x', '2025-01-01', 'u-ghost', 0, 0, 0, ?1, ?1, 2025)",
            [NOW],
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("FOREIGN KEY constraint failed"),
        "expected FK violation, got: {err}"
    );
    assert_eq!(
        db.get_connection()
            .query_row("SELECT COUNT(*) FROM daily_reports", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

// ADR-0065 C1/G7 — duplicate (date, unit_id) is rejected (UNIQUE class) while
// another unit on the same date stays legal (per-unit uniqueness preserved).
#[test]
fn duplicate_date_unit_rejected_unique() {
    let (_dir, db) = fresh_db("g7_unique");
    let conn = db.get_connection();
    seed_unit(conn, "u-uniq");
    conn.execute(
        "INSERT INTO daily_reports (id, date, unit_id, total_daily_cost, total_daily_average, \
         total_daily_beneficiaries, created_at, updated_at, fiscal_year) \
         VALUES ('dr-1', '2025-01-01', 'u-uniq', 0, 0, 0, ?1, ?1, 2025)",
        [NOW],
    )
    .unwrap();
    let err = conn
        .execute(
            "INSERT INTO daily_reports (id, date, unit_id, total_daily_cost, total_daily_average, \
             total_daily_beneficiaries, created_at, updated_at, fiscal_year) \
             VALUES ('dr-2', '2025-01-01', 'u-uniq', 0, 0, 0, ?1, ?1, 2025)",
            [NOW],
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("UNIQUE constraint failed"),
        "expected UNIQUE violation, got: {err}"
    );

    seed_unit(conn, "u-uniq2");
    conn.execute(
        "INSERT INTO daily_reports (id, date, unit_id, total_daily_cost, total_daily_average, \
         total_daily_beneficiaries, created_at, updated_at, fiscal_year) \
         VALUES ('dr-3', '2025-01-01', 'u-uniq2', 0, 0, 0, ?1, ?1, 2025)",
        [NOW],
    )
    .unwrap();
}

// ADR-0065 C1/G1 — meal item referencing a nonexistent FIFO layer (FK class);
// a NULL fifo_layer_id stays legal (nullable preserved) and an existing layer
// reference is accepted and protected by ON DELETE RESTRICT.
#[test]
fn meal_item_fifo_layer_fk_classes_and_restrict() {
    let (_dir, db) = fresh_db("g1_fifo");
    let conn = db.get_connection();
    seed_unit(conn, "u-fi");
    conn.execute(
        "INSERT INTO products (id, name, base_price, year, created_at, purchase_unit, \
         consumption_unit, conversion_factor, tva_classification) \
         VALUES ('p-fi', 'Prod', 100, 2025, ?1, 1, 1, 1, 0)",
        [NOW],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO daily_reports (id, date, unit_id, total_daily_cost, total_daily_average, \
         total_daily_beneficiaries, created_at, updated_at, fiscal_year) \
         VALUES ('dr-fi', '2025-01-01', 'u-fi', 0, 0, 0, ?1, ?1, 2025)",
        [NOW],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO daily_report_meals (id, daily_report_id, meal_type) \
         VALUES ('m-fi', 'dr-fi', 'breakfast')",
        [],
    )
    .unwrap();

    // Nonexistent layer → FOREIGN KEY class.
    let err = conn
        .execute(
            "INSERT INTO daily_report_meal_items (id, meal_id, product_id, quantity, unit_price, \
             total_cost, fifo_layer_id) \
             VALUES ('mi-bad', 'm-fi', 'p-fi', 1, 10, 10, 'layer-ghost')",
            [],
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("FOREIGN KEY constraint failed"),
        "expected FK violation, got: {err}"
    );

    // NULL layer stays legal (nullable preserved).
    conn.execute(
        "INSERT INTO daily_report_meal_items (id, meal_id, product_id, quantity, unit_price, \
         total_cost, fifo_layer_id) VALUES ('mi-null', 'm-fi', 'p-fi', 1, 10, 10, NULL)",
        [],
    )
    .unwrap();

    // Existing layer is accepted...
    conn.execute(
        "INSERT INTO fifo_stock_layers (id, unit_id, product_id, source_type, unit_cost, \
         qty_original, qty_remaining, received_at, created_by, origin_fiscal_year) \
         VALUES ('layer-real', 'u-fi', 'p-fi', 'ORDER', 10, 1, 1, ?1, 'tester', 2025)",
        [NOW],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO daily_report_meal_items (id, meal_id, product_id, quantity, unit_price, \
         total_cost, fifo_layer_id) VALUES ('mi-good', 'm-fi', 'p-fi', 1, 10, 10, 'layer-real')",
        [],
    )
    .unwrap();

    // ...and cannot be deleted while referenced (ON DELETE RESTRICT).
    let err = conn
        .execute("DELETE FROM fifo_stock_layers WHERE id = 'layer-real'", [])
        .unwrap_err();
    assert!(
        err.to_string().contains("FOREIGN KEY constraint failed"),
        "expected RESTRICT violation, got: {err}"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM fifo_stock_layers WHERE id = 'layer-real'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

// ADR-0065 — deleting a unit that still owns reports is rejected
// (unit_id REFERENCES units(id) ON DELETE RESTRICT).
#[test]
fn deleting_unit_with_reports_rejected_by_restrict() {
    let (_dir, db) = fresh_db("g3_restrict_units");
    let conn = db.get_connection();
    seed_unit(conn, "u-res");
    conn.execute(
        "INSERT INTO daily_reports (id, date, unit_id, total_daily_cost, total_daily_average, \
         total_daily_beneficiaries, created_at, updated_at, fiscal_year) \
         VALUES ('dr-res', '2025-01-01', 'u-res', 0, 0, 0, ?1, ?1, 2025)",
        [NOW],
    )
    .unwrap();
    let err = conn
        .execute("DELETE FROM units WHERE id = 'u-res'", [])
        .unwrap_err();
    assert!(
        err.to_string().contains("FOREIGN KEY constraint failed"),
        "expected RESTRICT violation, got: {err}"
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM units WHERE id = 'u-res'", [], |r| r.get::<_, i64>(0))
        .unwrap(),
        1
    );
}

// ADR-0065 C1 — supplier_orders.unit_id NOT NULL + FK-guarded (NOT NULL and
// FOREIGN KEY classes; the RESTRICT purge protection is exercised by the
// unit_deletion suite).
#[test]
fn supplier_order_missing_unit_rejected_and_fk_guarded() {
    let (_dir, db) = fresh_db("ta2_supplier");
    let conn = db.get_connection();
    conn.execute(
        "INSERT INTO suppliers (id, name, created_at) VALUES ('s-tn', 'Sup', ?1)",
        [NOW],
    )
    .unwrap();
    let err = conn
        .execute(
            "INSERT INTO supplier_orders (id, order_date, supplier_id, supplier_name, created_at, \
             unit_id, fiscal_year) VALUES ('o-x', '2025-01-01', 's-tn', 'Sup', ?1, NULL, 2025)",
            [NOW],
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("NOT NULL constraint failed"),
        "expected NOT NULL violation, got: {err}"
    );
    let err = conn
        .execute(
            "INSERT INTO supplier_orders (id, order_date, supplier_id, supplier_name, created_at, \
             unit_id, fiscal_year) VALUES ('o-x2', '2025-01-01', 's-tn', 'Sup', ?1, 'u-ghost', 2025)",
            [NOW],
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("FOREIGN KEY constraint failed"),
        "expected FK violation, got: {err}"
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM supplier_orders", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Service level — C2/C3/C4 unit gates, zero writes.
// ─────────────────────────────────────────────────────────────────────────────

/// Mirror of the case_b fiscal seeding (immutable_historical_mutation_tests):
/// a fully open imported year must still pass `validate_fiscal` so this suite
/// reaches the unit gates with them active.
fn seed_open_fiscal_year(db: &Database) {
    db.executor()
        .fiscal_year_status()
        .seed_year(2026, "open", "2026-01-01")
        .unwrap();
    db.executor().settings().set_current_year(2026).unwrap();
}

fn dummy_report(unit_id: Option<&str>) -> DailyReportResult {
    DailyReportResult {
        report: DailyReport {
            id: Uuid::new_v4().to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            unit_id: unit_id.map(|s| s.to_string()),
            total_daily_cost: 0.0,
            total_daily_average: 0.0,
            total_daily_beneficiaries: 2,
            created_at: Utc::now(),
            fiscal_year: 2026,
        },
        daily_summary: DailyConsumptionSummary::default(),
        meals: vec![MealSectionResult {
            meal: DailyReportMeal {
                id: "m-imp".into(),
                daily_report_id: "r-imp".into(),
                meal_type: MealType::Breakfast,
                staff_24h_count: 1,
                staff_8h_count: 0,
                reservation_count: 0,
                mission_count: 0,
                guest_count: 1,
                total_beneficiaries: 2,
                total_meal_cost: 0.0,
                meal_average: 0.0,
            },
            items: vec![],
        }],
    }
}

fn asserted_unit_not_found(err: AppError) {
    match err {
        AppError::BusinessLogic(BusinessLogicError::ResourceNotFound { resource, id }) => {
            assert_eq!(resource, "unit");
            assert_eq!(id, "u-unknown");
        }
        other => panic!("expected ResourceNotFound(unit), got {other:?}"),
    }
}

// ADR-0065 C2 (TA2) — missing unit on an OPEN fiscal year: the fiscal gate
// passes, the unit gate rejects, and no row is written (owner decision
// 2026-10-23: FiscalYearArchived keeps precedence — covered by case_b).
#[test]
fn import_daily_report_missing_unit_rejected_before_any_write() {
    let dir = tempdir().unwrap();
    let db = ConnectionFactory::new_with_path(&dir.path().join("ta2_missing.db")).unwrap();
    seed_open_fiscal_year(&db);

    let err = SyncImportExecutionService::new(db.executor())
        .import_daily_reports(vec![dummy_report(None)])
        .unwrap_err();
    assert!(
        matches!(
            &err,
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
        ),
        "got {err:?}"
    );
    assert_eq!(
        db.get_connection()
            .query_row("SELECT COUNT(*) FROM daily_reports", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0,
        "import must write nothing before the unit gate"
    );
}

// ADR-0065 C2 (TB2) — existing-but-unknown unit on an OPEN fiscal year.
#[test]
fn import_daily_report_unknown_unit_rejected_as_not_found() {
    let dir = tempdir().unwrap();
    let db = ConnectionFactory::new_with_path(&dir.path().join("ta2_unknown.db")).unwrap();
    seed_open_fiscal_year(&db);

    let err = SyncImportExecutionService::new(db.executor())
        .import_daily_reports(vec![dummy_report(Some("u-unknown"))])
        .unwrap_err();
    asserted_unit_not_found(err);
    assert_eq!(
        db.get_connection()
            .query_row("SELECT COUNT(*) FROM daily_reports", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

// ADR-0065 C3 (TA3) — creation path rejects a nonexistent unit with zero writes.
#[test]
fn create_daily_report_unknown_unit_rejected_before_any_write() {
    let dir = tempdir().unwrap();
    let db = ConnectionFactory::new_with_path(&dir.path().join("ta3_unknown.db")).unwrap();

    let input = DailyReportInput {
        date: Utc::now().date_naive(),
        meals: vec![MealSectionInput {
            meal_type: MealType::Breakfast,
            staff_24h_count: 10,
            staff_8h_count: 5,
            reservation_count: 0,
            mission_count: 0,
            guest_count: 0,
            items: vec![ConsumptionItemInput {
                product_id: "p-any".to_string(),
                quantity: 1.0,
            }],
        }],
    };

    let err = DailyReportService::new(db.executor())
        .create_daily_report(&input, Some("u-unknown"), "system", "system")
        .unwrap_err();
    asserted_unit_not_found(err);

    let conn = db.get_connection();
    for table in [
        "daily_reports",
        "daily_report_meals",
        "daily_report_meal_items",
        "fifo_stock_layers",
        "stock_movements",
    ] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0))
            .unwrap(),
            0,
            "{table} must stay empty"
        );
    }
}

// ADR-0065 C4 (TB4) — supplier-order path rejects a nonexistent unit with zero
// writes; per-item units structurally inherit the header unit (no duplicated
// item-level check — TA4/TA6/TA7).
#[test]
fn create_supplier_order_unknown_unit_rejected_before_any_write() {
    let dir = tempdir().unwrap();
    let db = ConnectionFactory::new_with_path(&dir.path().join("tb4_unknown.db")).unwrap();

    let req = CreateOrderRequest {
        reference_number: Some("TAU-001".to_string()),
        items: vec![OrderItemInput {
            product_id: "p-any".to_string(),
            quantity: 2.0,
        }],
    };

    let err = OrderService::new(db.executor())
        .create_supplier_order(&req, "u-unknown", 2025)
        .unwrap_err();
    asserted_unit_not_found(err);

    let conn = db.get_connection();
    for table in [
        "supplier_orders",
        "supplier_order_items",
        "fifo_stock_layers",
        "stock_movements",
    ] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0))
            .unwrap(),
            0,
            "{table} must stay empty"
        );
    }
}
