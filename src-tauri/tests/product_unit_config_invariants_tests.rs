//! SEC-087 Phase 6C — product unit/TVA configuration invariants.
//!
//! Pins the finalized invariants end-to-end:
//!   - schema-level: NOT NULL and CHECK constraints reject any Product row that
//!     omits or invalidates the config (`purchase_unit`, `consumption_unit`,
//!     `conversion_factor`, `tva_classification`), and any `inventory_stocks`
//!     row without a `consumption_unit` key;
//!   - application-level: every create path rejects a missing config field
//!     fail-closed BEFORE any mutation (zero products / zero stock persisted);
//!   - update cannot touch the config (no config fields in `UpdateProductRequest`)
//!     and leaves the persisted config + keyed stock identity intact;
//!   - a valid create persists the four codes as-is and creates the keyed
//!     initial stock row on the configured consumption unit.

use chrono::Utc;
use grpc_lib::application::services::ProductService;
use grpc_lib::db::ConnectionFactory;
use grpc_lib::models::{CreateProductRequest, UpdateProductRequest};
use rusqlite::params;
use uuid::Uuid;

// ─── 1. Schema-level: incomplete Product rows are rejected by NOT NULL ───────

#[test]
fn products_insert_without_config_columns_is_rejected() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();

    let res = db.get_connection().execute(
        "INSERT INTO products (id, name, base_price, year, created_at)
         VALUES (?1, 'No Config', 10000, 2026, ?2)",
        params![Uuid::new_v4().to_string(), now],
    );
    assert!(
        res.is_err(),
        "a product row omitting the SEC-087 config columns must be rejected"
    );
}

#[test]
fn products_insert_with_null_tva_is_rejected() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();

    let res = db.get_connection().execute(
        "INSERT INTO products (id, name, base_price, year, purchase_unit, consumption_unit, conversion_factor, tva_classification, created_at)
         VALUES (?1, 'Null TVA', 10000, 2026, 1, 1, 1, NULL, ?2)",
        params![Uuid::new_v4().to_string(), now],
    );
    assert!(res.is_err(), "NULL tva_classification must be rejected");
}

#[test]
fn products_insert_with_invalid_tva_code_is_rejected() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();

    let res = db.get_connection().execute(
        "INSERT INTO products (id, name, base_price, year, purchase_unit, consumption_unit, conversion_factor, tva_classification, created_at)
         VALUES (?1, 'Bad TVA', 10000, 2026, 1, 1, 1, 5, ?2)",
        params![Uuid::new_v4().to_string(), now],
    );
    assert!(
        res.is_err(),
        "tva_classification outside 0..=2 must be rejected"
    );
}

#[test]
fn products_insert_with_matching_units_and_factor_not_one_is_rejected() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();

    let res = db.get_connection().execute(
        "INSERT INTO products (id, name, base_price, year, purchase_unit, consumption_unit, conversion_factor, tva_classification, created_at)
         VALUES (?1, 'Bad Factor', 10000, 2026, 1, 1, 2, 0, ?2)",
        params![Uuid::new_v4().to_string(), now],
    );
    assert!(
        res.is_err(),
        "identical purchase/consumption units with factor != 1 must be rejected"
    );
}

#[test]
fn inventory_stocks_insert_without_consumption_unit_is_rejected() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let now = Utc::now().to_rfc3339();

    let res = db.get_connection().execute(
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, last_updated)
         VALUES (?1, 'no-such-product', 0, 'kg', ?2)",
        params![Uuid::new_v4().to_string(), now],
    );
    assert!(
        res.is_err(),
        "a stock row without a consumption-unit key must be rejected"
    );
}

// ─── 2. Application-level: create rejects a missing config field, pre-mutation ─

#[test]
fn create_rejects_each_missing_config_field_before_any_mutation() {
    let db = ConnectionFactory::new_for_test().expect("test db init");

    let cases: Vec<(String, CreateProductRequest)> = vec![
        (
            "purchase_unit".to_string(),
            CreateProductRequest {
                name: "M1".to_string(),
                base_price: 100.0,
                purchase_unit: None,
                consumption_unit: Some(1),
                conversion_factor: Some(1),
                tva_classification: Some(0),
            },
        ),
        (
            "consumption_unit".to_string(),
            CreateProductRequest {
                name: "M2".to_string(),
                base_price: 100.0,
                purchase_unit: Some(1),
                consumption_unit: None,
                conversion_factor: Some(1),
                tva_classification: Some(0),
            },
        ),
        (
            "conversion_factor".to_string(),
            CreateProductRequest {
                name: "M3".to_string(),
                base_price: 100.0,
                purchase_unit: Some(1),
                consumption_unit: Some(1),
                conversion_factor: None,
                tva_classification: Some(0),
            },
        ),
        (
            "tva_classification".to_string(),
            CreateProductRequest {
                name: "M4".to_string(),
                base_price: 100.0,
                purchase_unit: Some(1),
                consumption_unit: Some(1),
                conversion_factor: Some(1),
                tva_classification: None,
            },
        ),
    ];

    for (field, req) in cases {
        let err = ProductService::new(db.executor())
            .create_product(&req, 2026)
            .expect_err(&format!("create with missing {field} must fail closed"));
        assert!(
            err.to_string().contains(&field),
            "error must name the missing field `{field}`: {err}"
        );
    }

    // Fail-closed means ZERO rows reached the DB.
    let products: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM products", [], |row| row.get(0))
        .expect("count products");
    let stocks: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM inventory_stocks", [], |row| {
            row.get(0)
        })
        .expect("count stocks");
    assert_eq!(products, 0, "no product row persisted by a rejected create");
    assert_eq!(stocks, 0, "no stock row persisted by a rejected create");
}

// ─── 3. Update cannot clear or invalidate the persisted config ───────────────

#[test]
fn update_preserves_config_and_keyed_stock_identity() {
    let db = ConnectionFactory::new_for_test().expect("test db init");

    let product_id = ProductService::new(db.executor())
        .create_product(
            &CreateProductRequest {
                name: "Stable".to_string(),
                base_price: 200.0,
                purchase_unit: Some(1),
                consumption_unit: Some(1),
                conversion_factor: Some(1),
                tva_classification: Some(0),
            },
            2026,
        )
        .expect("product created");

    // UpdateProductRequest has NO unit/TVA/factor fields — the config can
    // neither be cleared nor invalidated through the update path. The price is
    // left unchanged so no fiscal-year price lock gate applies.
    ProductService::new(db.executor())
        .update_product(&UpdateProductRequest {
            id: product_id.clone(),
            name: "Stable Renamed".to_string(),
            base_price: 200.0,
        })
        .expect("update ok");

    let (purchase, consumption, factor, tva): (i32, i32, i32, i32) = db
        .get_connection()
        .query_row(
            "SELECT purchase_unit, consumption_unit, conversion_factor, tva_classification
             FROM products WHERE id = ?1",
            [&product_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("read config");
    assert_eq!((purchase, consumption, factor, tva), (1, 1, 1, 0));

    // Scale-3 INTEGER quantity: initial stock is 0 → 0.
    let (scaled_qty, key, name): (i64, i32, String) = db
        .get_connection()
        .query_row(
            "SELECT s.quantity, s.consumption_unit, p.name FROM inventory_stocks s
             JOIN products p ON p.id = s.product_id
             WHERE s.product_id = ?1",
            [&product_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read keyed stock");
    assert_eq!(scaled_qty, 0, "keyed initial row intact after update");
    assert_eq!(key, 1, "keyed identity unchanged after update");
    assert_eq!(name, "Stable Renamed");
}

// ─── 4. Valid create persists the config verbatim + keyed initial stock ──────

#[test]
fn valid_create_persists_config_and_keyed_initial_stock() {
    let db = ConnectionFactory::new_for_test().expect("test db init");

    // Sucres cross-unit: purchase unit 1 (Kg), consumption unit 3 (Piece),
    // factor 4, TVA 1 — a non-trivial configuration persisted verbatim.
    let product_id = ProductService::new(db.executor())
        .create_product(
            &CreateProductRequest {
                name: "Sucres Cross".to_string(),
                base_price: 300.0,
                purchase_unit: Some(1),
                consumption_unit: Some(3),
                conversion_factor: Some(4),
                tva_classification: Some(1),
            },
            2026,
        )
        .expect("product created");

    let (purchase, consumption, factor, tva): (i32, i32, i32, i32) = db
        .get_connection()
        .query_row(
            "SELECT purchase_unit, consumption_unit, conversion_factor, tva_classification
             FROM products WHERE id = ?1",
            [&product_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("read config");
    assert_eq!(
        (purchase, consumption, factor, tva),
        (1, 3, 4, 1),
        "config persisted verbatim, unnormalized"
    );

    let (scaled_qty, consumed_key, total_stock): (i64, i32, i64) = db
        .get_connection()
        .query_row(
            "SELECT MIN(quantity), MAX(consumption_unit), COUNT(*) FROM inventory_stocks WHERE product_id = ?1",
            [&product_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read keyed stock");
    assert_eq!(scaled_qty, 0, "initial stock is empty");
    assert_eq!(
        consumed_key, 3,
        "initial stock keyed on the configured consumption unit"
    );
    assert_eq!(total_stock, 1, "exactly one keyed stock row per product");
}
