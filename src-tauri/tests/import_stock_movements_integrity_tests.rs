use chrono::{Datelike, Utc};
use grpc_lib::application::services::SyncImportExecutionService;
use grpc_lib::db::ConnectionFactory;
use grpc_lib::models::inventory::{StockMovement, StockMovementType};
use uuid::Uuid;

fn seed_fiscal_year(ex: grpc_lib::repositories::DbExecutor<'_>, year: i32) {
    let now = Utc::now().to_rfc3339();
    ex.execute(
        "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1, 'open', ?2)",
        rusqlite::params![year, now],
    )
    .expect("seed fiscal year");
    ex.execute(
        "UPDATE settings SET current_year = ?1 WHERE id = 1",
        rusqlite::params![year],
    )
    .expect("set current year");
}

fn seed_unit(ex: grpc_lib::repositories::DbExecutor<'_>, id: &str, now: &str) {
    ex.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U01','Test Unit','01',?2)",
        rusqlite::params![id, now],
    ).expect("insert unit");
}

fn seed_product(
    ex: grpc_lib::repositories::DbExecutor<'_>,
    product_id: &str,
    name: &str,
    year: i32,
) {
    let now = Utc::now().to_rfc3339();
    ex.execute(
        "INSERT INTO products (id, name, base_price, year, created_at, purchase_unit, consumption_unit, conversion_factor, tva_classification) VALUES (?1,?2,0.0,?3,?4,1,1,1,0)",
        rusqlite::params![product_id, name, year, now],
    )
    .expect("insert product");
    ex.execute(
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, consumption_unit, last_updated, updated_at) VALUES (?1,?2,100.0,'unit',1,?3,?3)",
        rusqlite::params![format!("stock-{}", product_id), product_id, now],
    )
    .expect("insert inventory_stocks");
}

#[allow(clippy::too_many_arguments)]
fn make_movement(
    id: &str,
    product_id: &str,
    mtype: StockMovementType,
    qty: f64,
    balance_before: f64,
    balance_after: f64,
    unit_id: Option<&str>,
    now: &str,
    year: i32,
) -> StockMovement {
    StockMovement {
        id: id.to_string(),
        product_id: product_id.to_string(),
        product_name: None,
        movement_type: mtype,
        quantity: qty,
        balance_before,
        balance_after,
        reference_type: None,
        reference_id: None,
        notes: None,
        timestamp: now.to_string(),
        user_id: "system".to_string(),
        username: "system".to_string(),
        unit_id: unit_id.map(|s| s.to_string()),
        fiscal_year: Some(year),
        unit_cost: None,
    }
}

#[test]
fn test_imported_movements_do_not_create_fifo_layers() {
    let db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let year = Utc::now().year();
    let product_id = Uuid::new_v4().to_string();
    let unit_id = Uuid::new_v4().to_string();
    let movement_id = Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_fiscal_year(ex, year);
        seed_product(ex, &product_id, "Test Product", year);
    }

    let movement = make_movement(
        &movement_id,
        &product_id,
        StockMovementType::In,
        50.0,
        100.0,
        150.0,
        Some(&unit_id),
        &now,
        year,
    );

    let result =
        SyncImportExecutionService::new(db.executor()).import_stock_movements(vec![movement], None);
    assert!(
        result.is_ok(),
        "import_stock_movements failed: {:?}",
        result
    );
    assert_eq!(result.unwrap(), 1, "expected 1 movement imported");

    let ex = db.executor();
    let layer_count: i64 = ex
        .query_row(
            "SELECT COUNT(*) FROM fifo_stock_layers WHERE product_id = ?1",
            rusqlite::params![product_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    assert_eq!(
        layer_count, 0,
        "import_stock_movements should NOT create FIFO layers"
    );

    let consumption_count: i64 = ex
        .query_row(
            "SELECT COUNT(*) FROM inventory_layer_consumptions",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    assert_eq!(
        consumption_count, 0,
        "import_stock_movements should NOT create layer consumptions"
    );
}

#[test]
fn test_imported_movements_do_not_update_inventory() {
    let db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let year = Utc::now().year();
    let product_id = Uuid::new_v4().to_string();
    let unit_id = Uuid::new_v4().to_string();
    let movement_id = Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        seed_fiscal_year(ex, year);
        seed_product(ex, &product_id, "Test Product", year);
    }

    let ex = db.executor();
    let before_qty: f64 = ex
        .query_row(
            "SELECT quantity FROM inventory_stocks WHERE product_id = ?1",
            rusqlite::params![product_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(before_qty, 100.0, "initial inventory should be 100");

    let movement = make_movement(
        &movement_id,
        &product_id,
        StockMovementType::In,
        50.0,
        100.0,
        150.0,
        Some(&unit_id),
        &now,
        year,
    );

    let result =
        SyncImportExecutionService::new(db.executor()).import_stock_movements(vec![movement], None);
    assert!(
        result.is_ok(),
        "import_stock_movements failed: {:?}",
        result
    );

    let after_qty: f64 = ex
        .query_row(
            "SELECT quantity FROM inventory_stocks WHERE product_id = ?1",
            rusqlite::params![product_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        after_qty, 100.0,
        "import_stock_movements should NOT update inventory_stocks"
    );

    let movement_exists: i64 = ex
        .query_row(
            "SELECT COUNT(*) FROM stock_movements WHERE id = ?1",
            rusqlite::params![movement_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    assert_eq!(movement_exists, 1, "stock movement should be recorded");
}
