//! SEC-087 Phase 5 correction — inventory read-path reconciliation with keyed
//! stock identity `(product_id, consumption_unit)`.
//!
//! Pins the corrected read/write semantics required by the pre-commit
//! clarification audit:
//!   - availability, `get_stock`, `get_stock_summary` and `stock_exists_for_product`
//!     address the row keyed by the product's AUTHORITATIVE consumption unit
//!     (backend `products.consumption_unit`), never a guessed or legacy row;
//!   - a configured product never falls back to its NULL-keyed legacy row;
//!   - an OUT movement deducts only the keyed row;
//!   - initial stock for a configured product is created with its keyed unit,
//!     while legacy/unconfigured products keep their NULL-keyed row;
//!   - two distinct keyed rows for one product are isolated — only the
//!     configured unit is readable;
//!   - the summary projection yields exactly one row per product with no
//!     duplicate when NULL and keyed rows coexist.

use chrono::Utc;
use grpc_lib::application::services::{ProductService, StockLevelService, StockMovementService};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::models::{
    ConsumptionItemInput, CreateProductRequest, NewStockMovement, StockMovementType,
};
use grpc_lib::repositories::RepositoryProvider;
use uuid::Uuid;

/// Open fiscal year 2026 and make it the settings current year so stock
/// movements can be recorded (the migration already seeds 2026 open, but this
/// keeps the helper deterministic and independent of wall-clock drift).
fn open_fiscal_2026(db: &grpc_lib::db::Database) {
    db.executor()
        .execute(
            "UPDATE settings SET current_year = 2026, configured = 1 WHERE id = 1",
            [],
        )
        .unwrap();
    db.executor()
        .execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at)
             VALUES (2026, 'open', ?1)",
            rusqlite::params![Utc::now().to_rfc3339()],
        )
        .unwrap();
}

/// Insert a fully configured product (same unit for purchase/consumption,
/// factor 1) WITHOUT creating initial stock. Returns its id.
fn insert_configured_product(db: &grpc_lib::db::Database, name: &str, unit: i32) -> String {
    let product_id = format!("prod-{}", Uuid::new_v4());
    let config = grpc_lib::domain::validation::validate_product_units(
        Some(unit),
        Some(unit),
        Some(1),
        Some(0),
    )
    .expect("valid product config");
    db.executor()
        .products()
        .insert_product(
            &product_id,
            &CreateProductRequest {
                name: name.to_string(),
                base_price: 100.0,
                purchase_unit: Some(unit),
                consumption_unit: Some(unit),
                conversion_factor: Some(1),
                tva_classification: Some(0),
            },
            2026,
            &config,
            &Utc::now().to_rfc3339(),
        )
        .expect("configured product inserted");
    product_id
}

/// Insert an unconfigured product (no unit codes) WITHOUT initial stock.
fn insert_unconfigured_product(db: &grpc_lib::db::Database, name: &str) -> String {
    let product_id = format!("prod-{}", Uuid::new_v4());
    db.executor()
        .products()
        .insert_raw_product(
            &grpc_lib::models::Product {
                id: product_id.clone(),
                name: name.to_string(),
                base_price: 100.0,
                year: 2026,
                created_at: Utc::now(),
            },
            &Utc::now().to_rfc3339(),
        )
        .expect("unconfigured product inserted");
    product_id
}

/// Insert a stock row with an explicit consumption-unit key. The quantity is
/// written in scale-3 INTEGER form (×1000), mirroring the accounting schema.
fn insert_stock_row(
    db: &grpc_lib::db::Database,
    product_id: &str,
    consumption_unit: Option<i32>,
    quantity: f64,
) {
    db.executor()
        .execute(
            "INSERT INTO inventory_stocks (id, product_id, quantity, unit, last_updated, consumption_unit)
             VALUES (?1, ?2, ?3, 'unit', ?4, ?5)",
            rusqlite::params![
                format!("stock-{}", Uuid::new_v4()),
                product_id,
                (quantity * 1000.0) as i64,
                Utc::now().to_rfc3339(),
                consumption_unit,
            ],
        )
        .expect("stock row inserted");
}

// ─── Scenario 1: configured availability reads the keyed row ────────────────
#[test]
fn configured_availability_reads_keyed_row_ignoring_legacy_null_row() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let product_id = insert_configured_product(&db, "Semoule", 1);
    insert_stock_row(&db, &product_id, None, 0.0);
    insert_stock_row(&db, &product_id, Some(1), 5.0);

    let results = StockLevelService::new(db.executor())
        .check_stock_availability(vec![
            ConsumptionItemInput {
                product_id: product_id.clone(),
                quantity: 4.0,
            },
            ConsumptionItemInput {
                product_id: product_id.clone(),
                quantity: 6.0,
            },
        ])
        .unwrap();

    let ok = results.iter().find(|r| r.requested == 4.0).unwrap();
    assert!(ok.available, "4.0 must be available from the 5.0 keyed row");
    assert_eq!(ok.available_stock, 5.0);
    assert_eq!(ok.deficit, 0.0);
    assert_eq!(ok.product_name, "Semoule");

    let ko = results.iter().find(|r| r.requested == 6.0).unwrap();
    assert!(!ko.available, "6.0 must exceed the 5.0 keyed stock");
    assert_eq!(ko.available_stock, 5.0, "must not read the NULL row (0.0)");
    assert_eq!(ko.deficit, 1.0);
}

// ─── Scenario 2: OUT deducts only the keyed row; availability matches ───────
#[test]
fn out_movement_deducts_only_keyed_row_and_matches_availability() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    open_fiscal_2026(&db);
    let product_id = insert_configured_product(&db, "Huile", 1);
    insert_stock_row(&db, &product_id, None, 0.0);
    insert_stock_row(&db, &product_id, Some(1), 5.0);

    StockMovementService::new(db.executor())
        .record_stock_movement(&NewStockMovement {
            product_id: product_id.clone(),
            movement_type: StockMovementType::Out,
            quantity: 3.0,
            reference_type: None,
            reference_id: None,
            notes: Some("scenario 2 out".to_string()),
            user_id: "test_user".to_string(),
            username: "test_user".to_string(),
            unit_id: None,
            unit_cost: None,
        })
        .expect("OUT movement must succeed");

    let keyed = db
        .executor()
        .inventory()
        .get_stock_typed(&product_id, Some(1))
        .unwrap()
        .expect("keyed row must exist");
    assert_eq!(keyed.quantity, 2.0, "keyed row deducted 5.0 -> 2.0");

    let legacy = db
        .executor()
        .inventory()
        .get_stock_typed(&product_id, None)
        .unwrap()
        .expect("legacy NULL row untouched");
    assert_eq!(
        legacy.quantity, 0.0,
        "NULL row must remain untouched by OUT"
    );

    let results = StockLevelService::new(db.executor())
        .check_stock_availability(vec![ConsumptionItemInput {
            product_id: product_id.clone(),
            quantity: 2.0,
        }])
        .unwrap();
    assert!(results[0].available);
    assert_eq!(
        results[0].available_stock, 2.0,
        "availability reads keyed row"
    );

    let results = StockLevelService::new(db.executor())
        .check_stock_availability(vec![ConsumptionItemInput {
            product_id: product_id.clone(),
            quantity: 3.0,
        }])
        .unwrap();
    assert!(!results[0].available);
    assert_eq!(results[0].available_stock, 2.0);
    assert_eq!(results[0].deficit, 1.0);
}

// ─── Scenario 3: summary yields one row per product, no duplicate ───────────
#[test]
fn summary_has_single_row_when_null_and_keyed_rows_coexist() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let configured = insert_configured_product(&db, "Lait", 1);
    insert_stock_row(&db, &configured, None, 0.0);
    insert_stock_row(&db, &configured, Some(1), 5.0);
    let unconfigured = insert_unconfigured_product(&db, "Farine");
    insert_stock_row(&db, &unconfigured, None, 3.0);

    let summary = StockLevelService::new(db.executor())
        .get_stock_summary(None)
        .unwrap();

    let cfg_rows: Vec<_> = summary
        .iter()
        .filter(|s| s.product_id == configured)
        .collect();
    assert_eq!(
        cfg_rows.len(),
        1,
        "exactly one summary row for the configured product"
    );
    assert_eq!(
        cfg_rows[0].current_quantity, 5.0,
        "configured product reads keyed row"
    );

    let legacy_rows: Vec<_> = summary
        .iter()
        .filter(|s| s.product_id == unconfigured)
        .collect();
    assert_eq!(
        legacy_rows.len(),
        1,
        "exactly one summary row for the unconfigured product"
    );
    assert_eq!(
        legacy_rows[0].current_quantity, 3.0,
        "unconfigured product reads NULL row"
    );
}

// ─── Scenario 4: service get_stock returns the keyed identity ───────────────
#[test]
fn service_get_stock_returns_keyed_identity() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let product_id = insert_configured_product(&db, "Tomate", 1);
    insert_stock_row(&db, &product_id, None, 7.0);
    insert_stock_row(&db, &product_id, Some(1), 5.0);

    let stock = StockLevelService::new(db.executor())
        .get_stock(&product_id)
        .unwrap()
        .expect("configured product resolves its keyed row");
    assert_eq!(stock.quantity, 5.0, "never the legacy NULL row (7.0)");
    assert_eq!(stock.consumption_unit, Some(1));
}

// ─── Scenario 5: unconfigured legacy product keeps NULL semantics ───────────
#[test]
fn unconfigured_legacy_product_preserved() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let product_id = insert_unconfigured_product(&db, "Legacy");
    insert_stock_row(&db, &product_id, None, 3.0);

    let stock = StockLevelService::new(db.executor())
        .get_stock(&product_id)
        .unwrap()
        .expect("unconfigured product reads its NULL-keyed row");
    assert_eq!(stock.quantity, 3.0);
    assert_eq!(stock.consumption_unit, None);

    let results = StockLevelService::new(db.executor())
        .check_stock_availability(vec![ConsumptionItemInput {
            product_id: product_id.clone(),
            quantity: 2.0,
        }])
        .unwrap();
    assert!(results[0].available);
    assert_eq!(results[0].available_stock, 3.0);
}

// ─── Scenario 6: initial stock is keyed for configured products ─────────────
#[test]
fn initial_stock_created_with_product_config_key() {
    let db = ConnectionFactory::new_for_test().expect("test db init");

    let service = ProductService::new(db.executor());
    let product_id = service
        .create_product(
            &CreateProductRequest {
                name: "Riz 50".to_string(),
                base_price: 500.0,
                purchase_unit: Some(1),
                consumption_unit: Some(1),
                conversion_factor: Some(1),
                tva_classification: Some(0),
            },
            2026,
        )
        .expect("product created");

    let keyed = db
        .executor()
        .inventory()
        .get_stock_typed(&product_id, Some(1))
        .unwrap()
        .expect("configured product must get a keyed initial row");
    assert_eq!(keyed.quantity, 0.0);
    assert_eq!(keyed.consumption_unit, Some(1));
    assert!(
        db.executor()
            .inventory()
            .get_stock_typed(&product_id, None)
            .unwrap()
            .is_none(),
        "no legacy NULL initial row for a configured product"
    );

    // Legacy path still untouched: unconfigured product → NULL initial row.
    let legacy_id = insert_unconfigured_product(&db, "Legacy Initial");
    db.executor()
        .inventory()
        .create_initial_stock_for_product(
            &format!("stock-{}", Uuid::new_v4()),
            &legacy_id,
            None,
            &Utc::now().to_rfc3339(),
        )
        .unwrap();
    assert!(
        db.executor()
            .inventory()
            .get_stock_typed(&legacy_id, None)
            .unwrap()
            .is_some(),
        "unconfigured product must get a legacy NULL initial row"
    );
}

// ─── Scenario 7: stock_exists_for_product aligns with the config key ────────
#[test]
fn stock_exists_for_product_aligned_with_config_key() {
    let db = ConnectionFactory::new_for_test().expect("test db init");

    let keyed_only = insert_configured_product(&db, "Pates", 1);
    insert_stock_row(&db, &keyed_only, Some(1), 1.0);
    assert!(
        db.executor()
            .inventory()
            .stock_exists_for_product(&keyed_only)
            .unwrap(),
        "keyed-only stock row counts as existing"
    );

    let legacy_only = insert_configured_product(&db, "Couscous", 1);
    insert_stock_row(&db, &legacy_only, None, 1.0);
    assert!(
        !db.executor()
            .inventory()
            .stock_exists_for_product(&legacy_only)
            .unwrap(),
        "a NULL-only legacy row does not satisfy the configured key"
    );
}

// ─── Scenario 9: two keyed rows are isolated; config unit is authoritative ───
#[test]
fn cross_unit_isolation_config_unit_authoritative() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let product_id = insert_configured_product(&db, "Sucres", 2);
    insert_stock_row(&db, &product_id, Some(1), 99.0);
    insert_stock_row(&db, &product_id, Some(2), 5.0);

    let stock = StockLevelService::new(db.executor())
        .get_stock(&product_id)
        .unwrap()
        .expect("config unit row resolves");
    assert_eq!(
        stock.quantity, 5.0,
        "must read the configured unit=2 row, not unit=1"
    );
    assert_eq!(stock.consumption_unit, Some(2));

    let results = StockLevelService::new(db.executor())
        .check_stock_availability(vec![ConsumptionItemInput {
            product_id: product_id.clone(),
            quantity: 10.0,
        }])
        .unwrap();
    assert!(
        !results[0].available,
        "10.0 exceeds the 5.0 configured-unit stock"
    );
    assert_eq!(
        results[0].available_stock, 5.0,
        "stock from other unit rows must never be aggregated"
    );
}

// ─── Scenario 10: a missing keyed row is a hard miss, no legacy fallback ────
#[test]
fn missing_keyed_row_does_not_fall_back_to_legacy() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let product_id = insert_configured_product(&db, "Pomme", 1);
    insert_stock_row(&db, &product_id, None, 3.0);

    assert!(
        StockLevelService::new(db.executor())
            .get_stock(&product_id)
            .unwrap()
            .is_none(),
        "configured product with only a NULL row must resolve to no stock"
    );

    let results = StockLevelService::new(db.executor())
        .check_stock_availability(vec![ConsumptionItemInput {
            product_id: product_id.clone(),
            quantity: 1.0,
        }])
        .unwrap();
    assert!(!results[0].available);
    assert_eq!(results[0].available_stock, 0.0);

    let summary = StockLevelService::new(db.executor())
        .get_stock_summary(None)
        .unwrap();
    let row = summary.iter().find(|s| s.product_id == product_id).unwrap();
    assert_eq!(
        row.current_quantity, 0.0,
        "summary must not fall back to the NULL row"
    );
}

// ─── Scenario 8: existing Phase-5 regression suite stays green ──────────────
// Covered by running the full `cargo test` gate (phase5_receipt_conversion_tests,
// fiscal_year_stock_reports_tests, fiscal_lifecycle_tests, ...) unchanged.
