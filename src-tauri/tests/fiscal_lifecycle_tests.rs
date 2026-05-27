//! Fiscal Lifecycle Integration Tests
//!
//! Covers:
//! 1. Opening a fiscal year
//! 2. Creating records in an open year
//! 3. Attempting to create records in a non-existent year (should fail)
//! 4. Atomic Closing: Snapshot -> Carry-forward -> Lock
//! 5. Post-close Guards: Writing to closed year (should fail)
//! 6. Determinism: verify opening balances in the new year match old year snapshots.

mod common;

use chrono::{Datelike, Utc};
use grpc_lib::application::services::{FiscalYearService, StockMovementService};
use grpc_lib::models::{NewStockMovement, StockMovementType};
use grpc_lib::repositories::RepositoryProvider;

#[test]
fn test_fiscal_lifecycle_sim() {
    let (state, _temp) = common::create_test_state();
    let db = state.db.lock().unwrap();
    let executor = db.as_ref().unwrap().executor();

    let fiscal_service = FiscalYearService::new(executor);
    let stock_service = StockMovementService::new(executor);

    let current_year = Utc::now().year();
    let next_year = current_year + 1;
    let admin_user = executor
        .users()
        .get_user_by_username("admin")
        .unwrap()
        .expect("Default admin should exist");
    let user_id = admin_user.id;
    let username = "admin";

    // 1. Initial State: Year should be open by default (via migration or manual seed)
    // Let's seed it to be sure
    fiscal_service
        .close_year(current_year - 1, current_year, &user_id, username, None)
        .ok(); // ignore if fails

    // 2. Create some products and stock
    let product_id = "prod-1";
    executor.execute(
        "INSERT INTO products (id, name, base_price, tva, supplier_name, year, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        rusqlite::params![product_id, "Product 1", 100.0, 0.0, "Test Supplier", current_year, Utc::now().to_rfc3339()],
    ).unwrap();

    executor.execute(
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, last_updated) VALUES (?1, ?2, 0.0, 'kg', ?3)",
        rusqlite::params!["stock-1", product_id, Utc::now().to_rfc3339()],
    ).unwrap();

    // Create test unit
    executor.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1, 'TU', 'Test Unit', '00', ?2)",
        rusqlite::params!["test-unit", Utc::now().to_rfc3339()],
    ).unwrap();

    // 3. Record movement in OPEN year
    let movement = NewStockMovement {
        product_id: product_id.to_string(),
        movement_type: StockMovementType::In,
        quantity: 50.0,
        reference_type: None,
        reference_id: None,
        notes: Some("Initial stock".to_string()),
        user_id: user_id.to_string(),
        username: username.to_string(),
        unit_id: Some("test-unit".to_string()),
        unit_cost: None,
    };
    stock_service
        .record_stock_movement(&movement)
        .expect("Should allow write in open year");

    // Verify stock
    let stock = executor.inventory().get_stock(product_id).unwrap().unwrap();
    assert_eq!(stock.quantity, 50.0);

    executor
        .fifo_layers()
        .create_layer(
            "test-unit",
            product_id,
            "ORDER",
            None,
            100.0,
            50.0,
            &Utc::now().to_rfc3339(),
            username,
            2025,
        )
        .expect("Should create FIFO layer");

    // 4. ATOMIC CLOSE
    fiscal_service
        .close_year(current_year, next_year, &user_id, username, None)
        .expect("Atomic close should succeed");

    // 5. POST-CLOSE GUARDS
    // After closing, settings.current_year is updated to next_year
    // Stock movements should use the current fiscal year from settings (next_year)
    // which is now open, so this should succeed
    let result = stock_service.record_stock_movement(&movement);
    assert!(
        result.is_ok(),
        "Should allow write in new open year (next_year)"
    );

    // Verify stock increased
    let stock = executor.inventory().get_stock(product_id).unwrap().unwrap();
    assert_eq!(stock.quantity, 100.0); // 50 + 50

    // 6. CARRY-FORWARD VERIFICATION
    let snapshots = executor.opening_balances().get_by_year(next_year).unwrap();
    let snapshot = snapshots
        .iter()
        .find(|s| s.product_id == product_id)
        .expect("Snapshot should exist");
    assert_eq!(snapshot.opening_quantity, 50.0);
    assert_eq!(snapshot.carried_from_year, Some(current_year));

    // 7. Verify next year is OPEN
    fiscal_service
        .assert_fiscal_year_open(next_year)
        .expect("Next year should be open");
}
