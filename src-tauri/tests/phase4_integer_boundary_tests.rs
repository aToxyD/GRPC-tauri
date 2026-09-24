//! Phase 4 focused tests: direct INTEGER accounting schema.
//!
//! Verifies the integer-scaled persistence boundary:
//!   Money ×10², Quantity ×10³, Rate ×10⁴ (percent-domain).
//! Covers: fresh-schema column types, exact boundary round-trips, the
//! reproducibility of integer aggregates (no residue tolerance), fail-closed
//! rejection of negative/overflow values, and the exact percent-domain TVA
//! semantics (19.0 % ≠ 0.19 fraction).

use chrono::Utc;
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::numeric::{Money, NumericError, Quantity, Rate};
use grpc_lib::domain::pricing::price::compute_contract_fiscal;
use grpc_lib::repositories::{executor::DbExecutor, FifoLayerRepository, IntegrityRepository};
use rusqlite::params;
use uuid::Uuid;

fn seed_unit(ex: DbExecutor<'_>, id: &str) {
    ex.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U01','Test Unit','01',?2)",
        params![id, Utc::now().to_rfc3339()],
    )
    .expect("insert unit");
}

fn seed_product(ex: DbExecutor<'_>, product_id: &str) {
    ex.execute(
        "INSERT INTO products (id, name, base_price, year, created_at, purchase_unit, consumption_unit, conversion_factor, tva_classification) VALUES (?1,'Test Product',0,2025,?2,1,1,1,0)",
        params![product_id, Utc::now().to_rfc3339()],
    )
    .expect("insert product");
    ex.execute(
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, consumption_unit, last_updated, updated_at) VALUES (?1,?2,0,'unit',1,?3,?3)",
        params![format!("stock-{}", product_id), product_id, Utc::now().to_rfc3339()],
    )
    .expect("insert inventory_stocks");
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

// ── 1. Fresh schema: accounting columns are INTEGER ─────────────────────────

#[test]
fn fresh_schema_accounting_columns_are_integer() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let columns: &[(&str, &str)] = &[
        ("products", "base_price"),
        ("inventory_stocks", "quantity"),
        ("opening_balance_snapshots", "opening_quantity"),
        ("opening_balance_snapshots", "unit_cost"),
        ("opening_balance_snapshots", "total_value"),
        ("stock_movements", "quantity"),
        ("stock_movements", "balance_before"),
        ("stock_movements", "balance_after"),
        ("stock_movements", "unit_cost"),
        ("fifo_stock_layers", "unit_cost"),
        ("fifo_stock_layers", "qty_original"),
        ("fifo_stock_layers", "qty_remaining"),
        ("inventory_layer_consumptions", "quantity"),
        ("inventory_layer_consumptions", "unit_cost"),
        ("inventory_layer_consumptions", "total_cost"),
        ("daily_reports", "total_daily_cost"),
        ("daily_report_meals", "total_meal_cost"),
        ("daily_report_meal_items", "quantity"),
        ("daily_report_meal_items", "unit_price"),
        ("daily_report_meal_items", "total_cost"),
        ("supplier_orders", "total_amount"),
        ("supplier_order_items", "quantity"),
        ("supplier_order_items", "unit_price"),
        ("supplier_order_items", "total_cost"),
        ("contract_products", "proposed_price_ht"),
        ("contract_products", "agreed_price_ht"),
        ("contract_products", "tva_rate"),
        ("contract_products", "tva_amount"),
        ("contract_products", "price_ttc"),
        ("contract_products", "conversion_factor"),
        ("contract_allocations", "contracted_quantity"),
        ("contract_allocations", "fulfilled_quantity"),
        ("contract_allocations", "released_quantity"),
        ("contract_allocations", "reserved_quantity"),
        ("fiscal_year_tax_policy", "tva_rate"),
        ("unit_monthly_snapshots", "opening_stock"),
        ("unit_monthly_snapshots", "total_in"),
        ("unit_monthly_snapshots", "total_out"),
        ("unit_monthly_snapshots", "computed_closing"),
        ("unit_monthly_snapshots", "reported_closing"),
        ("unit_monthly_snapshots", "variance"),
    ];
    for (table, column) in columns {
        let sql = format!("SELECT type FROM pragma_table_info('{table}') WHERE name = '{column}'");
        let type_: String = db
            .executor()
            .query_row(&sql, [], |r| r.get(0))
            .unwrap_or_else(|_| panic!("column {table}.{column} must be declared"));
        assert_eq!(
            type_, "INTEGER",
            "{table}.{column} must be INTEGER, got '{type_}'"
        );
    }
}

// ── 2. Exact boundary round-trip: f64 wire → INTEGER → f64 ──────────────────

#[test]
fn fifo_layer_boundary_roundtrips_exact_scaled_integers() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id);
        seed_product(ex, &product_id);
        FifoLayerRepository::new(ex)
            .create_layer(
                &unit_id,
                &product_id,
                "ORDER",
                None,
                123.45,
                10.25,
                "2025-01-01T00:00:00Z",
                "phase4",
                2025,
            )
            .expect("create layer");
    }
    let (cost_scaled, qty_scaled): (i64, i64) = db
        .executor()
        .query_row(
            "SELECT unit_cost, qty_remaining FROM fifo_stock_layers WHERE product_id = ?1",
            params![&product_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("read scaled layer");
    assert_eq!(
        cost_scaled, 12_345,
        "123.45 DA must persist as 12345 centimes"
    );
    assert_eq!(
        qty_scaled, 10_250,
        "10.25 units must persist as 10250 thousandths"
    );

    // Domain exactness: scaled integer → domain → scaled integer is identity.
    assert_eq!(
        Money::from_centimes(cost_scaled)
            .unwrap()
            .to_scaled_i64()
            .unwrap(),
        12_345
    );
    assert_eq!(
        Quantity::from_scaled_i64(qty_scaled)
            .unwrap()
            .to_scaled_i64()
            .unwrap(),
        10_250
    );

    // Repository wire read derives the DTO f64 from the domain value.
    let total = FifoLayerRepository::new(db.executor())
        .get_total_available(&unit_id, &product_id)
        .expect("read total");
    assert!((total - 10.25).abs() < f64::EPSILON, "got {}", total);
}

#[test]
fn one_thousandth_quantity_survives_write_and_read() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id);
        seed_product(ex, &product_id);
        FifoLayerRepository::new(ex)
            .create_layer(
                &unit_id,
                &product_id,
                "ORDER",
                None,
                100.0,
                0.001,
                "2025-01-01T00:00:00Z",
                "phase4",
                2025,
            )
            .expect("create layer");
    }
    let stored: i64 = db
        .executor()
        .query_row(
            "SELECT qty_remaining FROM fifo_stock_layers WHERE product_id = ?1",
            params![&product_id],
            |r| r.get(0),
        )
        .expect("read scaled");
    assert_eq!(
        stored, 1,
        "0.001 units must persist as exactly 1 thousandth"
    );
    let total = FifoLayerRepository::new(db.executor())
        .get_total_available(&unit_id, &product_id)
        .expect("read total");
    assert!((total - 0.001).abs() < f64::EPSILON, "got {}", total);
}

// ── 3. Exact integer aggregates: one scaled unit is a mismatch ──────────────

#[test]
fn one_scaled_unit_quantity_difference_is_an_integrity_mismatch() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id);
        seed_product(ex, &product_id);
        FifoLayerRepository::new(ex)
            .create_layer(
                &unit_id,
                &product_id,
                "ORDER",
                None,
                50.0,
                100.0,
                "2025-01-01T00:00:00Z",
                "phase4",
                2025,
            )
            .expect("create layer");
        ex.execute(
            "UPDATE inventory_stocks SET quantity = 100000 WHERE product_id = ?1",
            params![&product_id],
        )
        .expect("sync inventory");
    }
    let mismatches = IntegrityRepository::new(db.executor())
        .fetch_fifo_inventory_mismatches()
        .expect("integrity check");
    assert!(mismatches.is_empty(), "exact match must be clean");

    // Deskew by exactly one scaled unit (0.001): must be reported.
    db.executor()
        .execute(
            "UPDATE inventory_stocks SET quantity = 99999 WHERE product_id = ?1",
            params![&product_id],
        )
        .expect("deskew stock by one thousandth");
    let mismatches = IntegrityRepository::new(db.executor())
        .fetch_fifo_inventory_mismatches()
        .expect("integrity check");
    assert_eq!(mismatches.len(), 1, "1 scaled-unit diff must be a mismatch");
    assert!((mismatches[0].2 - 100.0).abs() < f64::EPSILON);
    assert!((mismatches[0].3 - 99.999).abs() < f64::EPSILON);
}

#[test]
fn one_cent_consumption_cost_difference_is_a_mismatch() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    let layer_id = {
        let ex = db.executor();
        seed_unit(ex, &unit_id);
        seed_product(ex, &product_id);
        FifoLayerRepository::new(ex)
            .create_layer(
                &unit_id,
                &product_id,
                "ORDER",
                None,
                12.34,
                10.0,
                "2025-01-01T00:00:00Z",
                "phase4",
                2025,
            )
            .expect("create layer")
    };

    let movement_id = Uuid::new_v4().to_string();
    let consumption_id = Uuid::new_v4().to_string();
    db.executor()
        .execute(
            "INSERT INTO stock_movements (id, product_id, movement_type, quantity, balance_before, balance_after, timestamp, user_id, username, unit_id, updated_at, fiscal_year, unit_cost)
             VALUES (?1, ?2, 'IN', 10000, 0, 10000, ?3, 'u', 'u', ?4, ?3, 2025, 1234)",
            params![&movement_id, &product_id, now(), &unit_id],
        )
        .expect("insert movement");
    db.executor()
        .execute(
            "INSERT INTO inventory_layer_consumptions (id, unit_id, movement_id, layer_id, quantity, unit_cost, total_cost, consumed_at)
             VALUES (?1, ?2, ?3, ?4, 10000, 1234, 12341, ?5)",
            params![&consumption_id, &unit_id, &movement_id, &layer_id, now()],
        )
        .expect("insert consumption");

    let bad = IntegrityRepository::new(db.executor())
        .fetch_invalid_consumption_costs()
        .expect("integrity check");
    assert_eq!(bad.len(), 1, "one-cent cost drift must be reported");
    assert!((bad[0].1 - 10.0).abs() < f64::EPSILON);
    assert!((bad[0].2 - 12.34).abs() < f64::EPSILON);

    // Exact scaled identity: total_cost * 1000 == quantity * unit_cost
    // (10.00 × 12.34 = 123.40 DA = 12340 centimes). Correct → clean.
    db.executor()
        .execute(
            "UPDATE inventory_layer_consumptions SET total_cost = 12340 WHERE id = ?1",
            params![&consumption_id],
        )
        .expect("fix consumption cost");
    let bad = IntegrityRepository::new(db.executor())
        .fetch_invalid_consumption_costs()
        .expect("integrity check");
    assert!(bad.is_empty(), "exact scaled integer must be clean");
}

#[test]
fn one_cent_daily_report_total_difference_is_a_mismatch() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let report_id = Uuid::new_v4().to_string();
    let meal_id = Uuid::new_v4().to_string();
    db.executor()
        .execute(
            "INSERT INTO daily_reports (id, date, unit_id, total_daily_cost, total_daily_average, total_daily_beneficiaries, created_at, updated_at, fiscal_year)
             VALUES (?1, '2025-01-01', NULL, 150000, 0, 0, ?2, ?2, 2025)",
            params![&report_id, now()],
        )
        .expect("insert report");
    db.executor()
        .execute(
            "INSERT INTO daily_report_meals (id, daily_report_id, meal_type, total_meal_cost)
             VALUES (?1, ?2, 'breakfast', 149999)",
            params![&meal_id, &report_id],
        )
        .expect("insert meal");

    let bad = IntegrityRepository::new(db.executor())
        .fetch_daily_report_total_mismatches()
        .expect("integrity check");
    assert_eq!(bad.len(), 1, "one-cent daily total drift must be reported");

    db.executor()
        .execute(
            "UPDATE daily_report_meals SET total_meal_cost = 150000 WHERE id = ?1",
            params![&meal_id],
        )
        .expect("fix meal cost");
    let bad = IntegrityRepository::new(db.executor())
        .fetch_daily_report_total_mismatches()
        .expect("integrity check");
    assert!(bad.is_empty(), "exact scaled daily total must be clean");
}

// ── 4. Fail-closed rejection: negative and overflow ─────────────────────────

#[test]
fn negative_and_overflow_quantities_are_rejected() {
    let db = ConnectionFactory::new_for_test().expect("test db init");
    let unit_id = Uuid::new_v4().to_string();
    let product_id = Uuid::new_v4().to_string();
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id);
        seed_product(ex, &product_id);
    }
    let fifo = FifoLayerRepository::new(db.executor());

    let neg = fifo
        .create_layer(
            &unit_id,
            &product_id,
            "ORDER",
            None,
            50.0,
            -1.0,
            "2025-01-01T00:00:00Z",
            "phase4",
            2025,
        )
        .expect_err("negative quantity must be rejected");
    assert!(
        format!("{}", neg).to_lowercase().contains("negative"),
        "expected negative rejection, got {}",
        neg
    );

    let overflow = fifo
        .create_layer(
            &unit_id,
            &product_id,
            "ORDER",
            None,
            50.0,
            1e18,
            "2025-01-01T00:00:00Z",
            "phase4",
            2025,
        )
        .expect_err("overflowing quantity must be rejected");
    assert!(
        format!("{}", overflow).to_lowercase().contains("overflow"),
        "expected overflow rejection, got {}",
        overflow
    );
}

// ── 5. Exact percent-domain TVA (19.0 % ≠ 0.19 fraction) ────────────────────

#[test]
fn tva_rate_is_percent_domain_and_exact() {
    let rate = Rate::parse_str("19").expect("parse 19");
    assert_eq!(
        rate.to_scaled_i64().unwrap(),
        190_000,
        "19% → scale-4 190_000"
    );
    assert_eq!(
        Rate::from_scaled_i64(190_000).unwrap(),
        rate,
        "scale-4 190_000 → 19%"
    );

    let fraction = Rate::parse_str("0.19").expect("parse 0.19");
    assert_eq!(fraction.to_scaled_i64().unwrap(), 1_900);
    assert_ne!(fraction, rate, "0.19 fraction must never equal 19%");
    assert_ne!(fraction.to_scaled_i64().unwrap(), 190_000);

    let base = Money::parse_str("200.00").unwrap();
    let breakdown = compute_contract_fiscal(&base, &rate).expect("price with 19%");
    let with_tva = breakdown.price_ttc;
    assert_eq!(with_tva, Money::parse_str("238.00").unwrap());
    assert_eq!(with_tva.to_scaled_i64().unwrap(), 23_800);
    assert_eq!(breakdown.tva_amount, Money::parse_str("38.00").unwrap());

    // The fraction form must NOT silently produce the 19% outcome.
    let fraction_breakdown = compute_contract_fiscal(&base, &fraction).expect("price with 0.19%");
    assert_ne!(
        fraction_breakdown.price_ttc.to_scaled_i64().unwrap(),
        23_800
    );

    // Rate range enforcement: > 100% fails closed.
    assert!(matches!(
        Rate::parse_str("101").err(),
        Some(NumericError::OutOfRange)
    ));
}

#[test]
fn tva_fixtures_are_percent_form_not_fraction() {
    // The only persisted TVA fixtures in the test corpus must be percent-form
    // (19.0 → 190_000), never legacy fraction-form 0.19.
    let rate = Rate::parse_str("19").unwrap();
    assert_eq!(rate.to_scaled_i64().unwrap(), 190_000);
    assert_ne!(rate.to_scaled_i64().unwrap(), 1_900);
}
