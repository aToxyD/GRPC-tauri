//! SEC-087 Phase 6C — inventory read/write path with the finalized keyed stock
//! identity `(product_id, consumption_unit)`.
//!
//! Pins the finalized semantics required by the Phase 6C finalization:
//!   - every `inventory_stocks` row carries a REQUIRED non-NULL consumption-unit
//!     key (NOT NULL schema); there is no legacy NULL-keyed row and no fallback;
//!   - availability, `get_stock`, `get_stock_summary` and `stock_exists_for_product`
//!     address the row keyed by the product's AUTHORITATIVE consumption unit
//!     (backend `products.consumption_unit`), never a guessed key;
//!   - an OUT movement deducts only the keyed row;
//!   - initial stock for a product is created keyed by its configured unit;
//!   - two distinct keyed rows for one product are isolated — only the
//!     configured unit is readable; a row on any other unit is invisible;
//!   - the summary projection yields exactly one row per product with no
//!     duplicate and no cross-unit leakage;
//!   - a missing product / unresolved config resolves to "no stock" — a hard
//!     miss, never a guessed or legacy row.

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

/// Insert a stock row with an explicit consumption-unit key (i32 — the schema
/// is NOT NULL, a NULL-keyed row cannot exist). The quantity is written in
/// scale-3 INTEGER form (×1000), mirroring the accounting schema.
fn insert_stock_row(
    db: &grpc_lib::db::Database,
    product_id: &str,
    consumption_unit: i32,
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
fn configured_availability_reads_keyed_row() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let product_id = insert_configured_product(&db, "Semoule", 1);
    insert_stock_row(&db, &product_id, 1, 5.0);

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
    assert_eq!(ko.available_stock, 5.0);
    assert_eq!(ko.deficit, 1.0);
}

// ─── Scenario 2: OUT deducts only the keyed row; availability matches ───────
#[test]
fn out_movement_deducts_only_keyed_row_and_matches_availability() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    open_fiscal_2026(&db);
    let product_id = insert_configured_product(&db, "Huile", 1);
    insert_stock_row(&db, &product_id, 1, 5.0);

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
        .get_stock_typed(&product_id, 1)
        .unwrap()
        .expect("keyed row must exist");
    assert_eq!(keyed.quantity, 2.0, "keyed row deducted 5.0 -> 2.0");
    assert_eq!(keyed.consumption_unit, 1);

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

// ─── Scenario 3: summary yields one row per product, no cross-unit leakage ──
#[test]
fn summary_has_single_row_and_no_cross_unit_leakage() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let configured = insert_configured_product(&db, "Lait", 1);
    insert_stock_row(&db, &configured, 1, 5.0);
    insert_stock_row(&db, &configured, 2, 99.0);
    let other = insert_configured_product(&db, "Farine", 1);
    insert_stock_row(&db, &other, 1, 3.0);

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
        "configured product reads its keyed row only (not the unit=2 row)"
    );

    let other_rows: Vec<_> = summary.iter().filter(|s| s.product_id == other).collect();
    assert_eq!(other_rows.len(), 1, "exactly one summary row per product");
    assert_eq!(other_rows[0].current_quantity, 3.0);
}

// ─── Scenario 4: service get_stock returns the configured key identity ──────
#[test]
fn service_get_stock_returns_configured_key_identity() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let product_id = insert_configured_product(&db, "Tomate", 1);
    insert_stock_row(&db, &product_id, 1, 5.0);

    let stock = StockLevelService::new(db.executor())
        .get_stock(&product_id)
        .unwrap()
        .expect("configured product resolves its keyed row");
    assert_eq!(stock.quantity, 5.0);
    assert_eq!(stock.consumption_unit, 1);
}

// ─── Scenario 5: unresolved config (unknown/deleted product) = no stock ─────
#[test]
fn unresolvable_product_is_a_hard_miss() {
    let db = ConnectionFactory::new_for_test().expect("test db init");

    assert!(
        StockLevelService::new(db.executor())
            .get_stock("does-not-exist")
            .unwrap()
            .is_none(),
        "unknown product resolves to no stock — never a guessed identity"
    );

    let results = StockLevelService::new(db.executor())
        .check_stock_availability(vec![ConsumptionItemInput {
            product_id: "does-not-exist".to_string(),
            quantity: 1.0,
        }])
        .unwrap();
    assert!(!results[0].available);
    assert_eq!(results[0].available_stock, 0.0);
    assert_eq!(results[0].deficit, 1.0);
}

// ─── Scenario 6: initial stock is created keyed by the config unit ──────────
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
        .get_stock_typed(&product_id, 1)
        .unwrap()
        .expect("configured product must get a keyed initial row");
    assert_eq!(keyed.quantity, 0.0);
    assert_eq!(keyed.consumption_unit, 1);

    // A direct keyed initial row is created explicitly with the config key.
    let legacy_id = insert_configured_product(&db, "Legacy Initial", 1);
    db.executor()
        .inventory()
        .create_initial_stock_for_product(
            &format!("stock-{}", Uuid::new_v4()),
            &legacy_id,
            1,
            &Utc::now().to_rfc3339(),
        )
        .unwrap();
    let keyed = db
        .executor()
        .inventory()
        .get_stock_typed(&legacy_id, 1)
        .unwrap()
        .expect("keyed initial row present");
    assert_eq!(keyed.quantity, 0.0);
}

// ─── Scenario 7: stock_exists_for_product aligns with the config key ────────
#[test]
fn stock_exists_for_product_aligned_with_config_key() {
    let db = ConnectionFactory::new_for_test().expect("test db init");

    let keyed = insert_configured_product(&db, "Pates", 1);
    insert_stock_row(&db, &keyed, 1, 1.0);
    assert!(
        db.executor()
            .inventory()
            .stock_exists_for_product(&keyed)
            .unwrap(),
        "row keyed by the configured unit counts as existing"
    );

    let cross_keyed = insert_configured_product(&db, "Couscous", 1);
    insert_stock_row(&db, &cross_keyed, 2, 1.0);
    assert!(
        !db.executor()
            .inventory()
            .stock_exists_for_product(&cross_keyed)
            .unwrap(),
        "a row on a different unit does not satisfy the configured key"
    );
}

// ─── Scenario 8: two keyed rows are isolated; config unit is authoritative ──
#[test]
fn cross_unit_isolation_config_unit_authoritative() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let product_id = insert_configured_product(&db, "Sucres", 2);
    insert_stock_row(&db, &product_id, 1, 99.0);
    insert_stock_row(&db, &product_id, 2, 5.0);

    let stock = StockLevelService::new(db.executor())
        .get_stock(&product_id)
        .unwrap()
        .expect("config unit row resolves");
    assert_eq!(
        stock.quantity, 5.0,
        "must read the configured unit=2 row, not unit=1"
    );
    assert_eq!(stock.consumption_unit, 2);

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

// ─── Scenario 9: a row on the wrong key is a hard miss, no fallback ─────────
#[test]
fn missing_configured_key_row_does_not_fall_back_to_other_key() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let product_id = insert_configured_product(&db, "Pomme", 1);
    insert_stock_row(&db, &product_id, 2, 3.0);

    assert!(
        StockLevelService::new(db.executor())
            .get_stock(&product_id)
            .unwrap()
            .is_none(),
        "configured product with only a wrong-key row must resolve to no stock"
    );

    let results = StockLevelService::new(db.executor())
        .check_stock_availability(vec![ConsumptionItemInput {
            product_id: product_id.clone(),
            quantity: 1.0,
        }])
        .unwrap();
    assert!(!results[0].available);
    assert_eq!(results[0].available_stock, 0.0);
    assert_eq!(results[0].deficit, 1.0);

    let summary = StockLevelService::new(db.executor())
        .get_stock_summary(None)
        .unwrap();
    let row = summary.iter().find(|s| s.product_id == product_id).unwrap();
    assert_eq!(
        row.current_quantity, 0.0,
        "summary must not fall back to a row on another unit"
    );
}

// ─── Scenario 10: existing Phase-5 regression suite stays green ─────────────
// Covered by running the full `cargo test` gate (phase5_receipt_conversion_tests,
// fiscal_year_stock_reports_tests, fiscal_lifecycle_tests, ...).
