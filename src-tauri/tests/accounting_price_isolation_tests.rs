//! Accounting integrity: catalog prices must not affect FIFO valuation, COGS, or previews.

mod common;

use common::{create_test_product, create_test_state, seed_fiscal_year_open, set_test_stock};
use grpc_lib::application::services::{DailyReportService, FifoPreviewService, FiscalYearService};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::models::{ConsumptionItemInput, DailyReportInput, MealSectionInput, MealType};
use grpc_lib::repositories::{FifoLayerRepository, RepositoryProvider};
use uuid::Uuid;

fn setup_unit_product_layers(
    db: &grpc_lib::db::Database,
    unit_cost: f64,
    qty: f64,
) -> (String, String) {
    let ex = db.executor();
    let now = chrono::Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    ex.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U','U','01',?2)",
        rusqlite::params![unit_id, now],
    )
    .unwrap();
    ex.execute(
        "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (2025,'open',?1)",
        rusqlite::params![now],
    )
    .unwrap();
    ex.execute(
        "INSERT INTO products (id, name, base_price, tva, year, created_at) VALUES (?1,'P',9999.0,0.0,2025,?2)",
        rusqlite::params![product_id, now],
    )
    .unwrap();
    ex.execute(
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, last_updated, updated_at)
         VALUES (?1, ?2, ?3, 'unit', ?4, ?4)",
        rusqlite::params![format!("s-{}", product_id), product_id, qty, now],
    )
    .unwrap();
    FifoLayerRepository::new(ex)
        .create_layer(
            &unit_id,
            &product_id,
            "ORDER",
            None,
            unit_cost,
            qty,
            &now,
            "system",
        )
        .unwrap();

    (unit_id, product_id)
}

#[test]
fn product_price_change_does_not_affect_fifo_preview() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let (unit_id, product_id) = setup_unit_product_layers(&db, 50.0, 20.0);

    let preview = FifoPreviewService::new(db.executor())
        .preview_items(
            &unit_id,
            &[ConsumptionItemInput {
                product_id: product_id.clone(),
                quantity: 4.0,
            }],
        )
        .unwrap();
    assert!((preview.predicted_fifo_cost - 200.0).abs() < 1e-9);

    db.executor()
        .execute(
            "UPDATE products SET base_price = 1.0 WHERE id = ?1",
            rusqlite::params![product_id],
        )
        .unwrap();

    let preview2 = FifoPreviewService::new(db.executor())
        .preview_items(
            &unit_id,
            &[ConsumptionItemInput {
                product_id,
                quantity: 4.0,
            }],
        )
        .unwrap();
    assert!((preview2.predicted_fifo_cost - 200.0).abs() < 1e-9);
}

#[test]
fn fiscal_close_zero_stock_never_uses_base_price() {
    let (state, _dir) = create_test_state();
    common::clear_fiscal_status(&state);
    seed_fiscal_year_open(&state, 2025);

    let pid = create_test_product(&state, "ZeroStock", 500.0, 2025);

    let mut guard = state.db.lock().unwrap();
    let db = guard.as_mut().unwrap();
    db.with_transaction(|tx| {
        FiscalYearService::new(tx).close_year(2025, 2026, "system", "system", None)
    })
    .unwrap();

    let snaps = db.executor().opening_balances().get_by_year(2026).unwrap();
    let snap = snaps.iter().find(|s| s.product_id == pid).unwrap();
    assert_eq!(snap.opening_quantity, 0.0);
    assert_eq!(snap.unit_cost, 0.0);
    assert_eq!(snap.total_value, 0.0);
}

#[test]
fn preview_matches_actual_consumption_cost() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let (unit_id, product_id) = setup_unit_product_layers(&db, 30.0, 10.0);
    let now = chrono::Utc::now().to_rfc3339();

    let input = DailyReportInput {
        date: chrono::NaiveDate::from_ymd_opt(2025, 6, 15).unwrap(),
        meals: vec![MealSectionInput {
            meal_type: MealType::Lunch,
            staff_24h_count: 5,
            staff_8h_count: 0,
            reservation_count: 0,
            mission_count: 0,
            guest_count: 0,
            items: vec![ConsumptionItemInput {
                product_id: product_id.clone(),
                quantity: 3.0,
            }],
        }],
    };

    let preview_cost = FifoPreviewService::new(db.executor())
        .preview_daily_report(&input, &unit_id)
        .unwrap()
        .predicted_fifo_cost;

    // Simulate actual consumption (same engine path as daily report phase 2)
    let portions = FifoLayerRepository::new(db.executor())
        .consume_fifo(&unit_id, &product_id, 3.0)
        .unwrap();
    let actual_cost: f64 = portions.iter().map(|p| p.total_cost).sum();

    assert!((preview_cost - actual_cost).abs() < 1e-9);
    assert!((actual_cost - 90.0).abs() < 1e-9);

    let _ = now;
}

#[test]
fn inventory_valuation_is_sum_of_remaining_layers() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let (unit_id, product_id) = setup_unit_product_layers(&db, 25.0, 8.0);

    let value = FifoLayerRepository::new(db.executor())
        .get_inventory_value_fifo(&unit_id)
        .unwrap();
    assert!((value - 200.0).abs() < 1e-9);

    FifoLayerRepository::new(db.executor())
        .consume_fifo(&unit_id, &product_id, 3.0)
        .unwrap();

    let after = FifoLayerRepository::new(db.executor())
        .get_inventory_value_fifo(&unit_id)
        .unwrap();
    assert!((after - 125.0).abs() < 1e-9);
}

#[test]
fn fifo_consumes_oldest_layer_first_by_received_at_then_id() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let ex = db.executor();
    let now = chrono::Utc::now().to_rfc3339();
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();

    ex.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U','U','01',?2)",
        rusqlite::params![unit_id, now],
    )
    .unwrap();
    ex.execute(
        "INSERT INTO products (id, name, base_price, tva, year, created_at) VALUES (?1,'P',1.0,0.0,2025,?2)",
        rusqlite::params![product_id, now],
    )
    .unwrap();

    let repo = FifoLayerRepository::new(ex);
    let older = repo
        .create_layer(
            &unit_id,
            &product_id,
            "ORDER",
            None,
            10.0,
            5.0,
            "2024-01-01T00:00:00Z",
            "system",
        )
        .unwrap();
    repo.create_layer(
        &unit_id,
        &product_id,
        "ORDER",
        None,
        20.0,
        5.0,
        "2024-02-01T00:00:00Z",
        "system",
    )
    .unwrap();

    let portions = repo.consume_fifo(&unit_id, &product_id, 3.0).unwrap();
    assert_eq!(portions.len(), 1);
    assert_eq!(portions[0].layer_id, older);
    assert!((portions[0].unit_cost - 10.0).abs() < 1e-9);
}

#[test]
fn closed_fiscal_year_rejects_daily_report() {
    let (state, _dir) = create_test_state();
    common::clear_fiscal_status(&state);
    seed_fiscal_year_open(&state, 2025);

    let pid = create_test_product(&state, "Item", 10.0, 2025);
    set_test_stock(&state, &pid, 10.0);

    {
        let mut guard = state.db.lock().unwrap();
        let db = guard.as_mut().unwrap();
        db.with_transaction(|tx| {
            FiscalYearService::new(tx).close_year(2025, 2026, "system", "system", None)
        })
        .unwrap();
    }

    seed_fiscal_year_open(&state, 2026);

    let input = DailyReportInput {
        date: chrono::NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
        meals: vec![MealSectionInput {
            meal_type: MealType::Breakfast,
            staff_24h_count: 1,
            staff_8h_count: 0,
            reservation_count: 0,
            mission_count: 0,
            guest_count: 0,
            items: vec![ConsumptionItemInput {
                product_id: pid,
                quantity: 1.0,
            }],
        }],
    };

    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let err = DailyReportService::new(db.executor()).create_daily_report(
        &input,
        Some("test-unit"),
        "system",
        "system",
    );
    assert!(err.is_err());
}
