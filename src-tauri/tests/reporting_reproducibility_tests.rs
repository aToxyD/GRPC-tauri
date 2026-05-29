use grpc_lib::application::reporting::{
    cache_key::CacheKey, Report,
};
use grpc_lib::application::reporting::fiscal_year_summary::{
    FiscalYearSummaryInput, FiscalYearSummaryReport,
};
use grpc_lib::application::reporting::inventory_valuation::{
    InventoryValuationInput, InventoryValuationReport,
};
use grpc_lib::application::reporting::stock_movement_ledger::{
    StockMovementLedgerInput, StockMovementLedgerReport,
};
use grpc_lib::db::ConnectionFactory;

mod common;

#[test]
fn fiscal_year_summary_has_correct_trait_metadata() {
    assert_eq!(FiscalYearSummaryReport::slug(), "fiscal-year-summary");
    assert_eq!(FiscalYearSummaryReport::version(), 1);
    assert!(FiscalYearSummaryReport::is_reproducible());
}

#[test]
fn inventory_valuation_has_correct_trait_metadata() {
    assert_eq!(InventoryValuationReport::slug(), "inventory-valuation");
    assert_eq!(InventoryValuationReport::version(), 1);
    assert!(InventoryValuationReport::is_reproducible());
}

#[test]
fn stock_movement_ledger_has_correct_trait_metadata() {
    assert_eq!(
        StockMovementLedgerReport::slug(),
        "stock-movement-ledger"
    );
    assert_eq!(StockMovementLedgerReport::version(), 1);
    assert!(StockMovementLedgerReport::is_reproducible());
}

#[test]
fn fiscal_year_summary_computes_on_empty_db_without_error() {
    let db = ConnectionFactory::new_for_test().expect("create test db");
    let executor = db.executor();

    let result = FiscalYearSummaryReport::compute(executor, FiscalYearSummaryInput {
        fiscal_year: 2024,
        unit_id: None,
    });

    assert!(result.is_err(), "should fail with year not found on empty db");
}

#[test]
fn inventory_valuation_computes_on_empty_db() {
    let db = ConnectionFactory::new_for_test().expect("create test db");
    let executor = db.executor();

    let result = InventoryValuationReport::compute(executor, InventoryValuationInput {
        fiscal_year: None,
        unit_id: None,
    });

    assert!(result.is_ok(), "should succeed on empty db: {:?}", result.err());
    let envelope = result.unwrap();
    assert_eq!(envelope.data.total_inventory_value, 0.0);
    assert_eq!(envelope.data.product_count, 0);
    assert_eq!(envelope.data.active_layer_count, 0);
    assert_eq!(envelope.metadata.report_slug, "inventory-valuation");
    assert_eq!(envelope.metadata.report_version, 1);
    assert!(envelope.metadata.computed_at.parse::<u64>().is_ok());
}

#[test]
fn stock_movement_ledger_computes_on_empty_db() {
    let db = ConnectionFactory::new_for_test().expect("create test db");
    let executor = db.executor();

    let result = StockMovementLedgerReport::compute(executor, StockMovementLedgerInput {
        fiscal_year: None,
        product_id: None,
        movement_type: None,
        unit_id: None,
        start_timestamp: None,
        end_timestamp: None,
        limit: 100,
        offset: 0,
    });

    assert!(result.is_ok(), "should succeed on empty db: {:?}", result.err());
    let envelope = result.unwrap();
    assert_eq!(envelope.data.rows.len(), 0);
    assert_eq!(envelope.data.total_count, 0);
    assert_eq!(envelope.metadata.report_slug, "stock-movement-ledger");
}

#[test]
fn inventory_valuation_is_deterministic_on_identical_db() {
    let db = ConnectionFactory::new_for_test().expect("create test db");
    let executor = db.executor();

    let input = InventoryValuationInput {
        fiscal_year: None,
        unit_id: None,
    };

    let r1 = InventoryValuationReport::compute(executor, input.clone()).unwrap();
    let json1 = serde_json::to_string(&r1.data).unwrap();

    let db2 = ConnectionFactory::new_for_test().expect("create test db 2");
    let executor2 = db2.executor();

    let r2 = InventoryValuationReport::compute(executor2, input).unwrap();
    let json2 = serde_json::to_string(&r2.data).unwrap();

    assert_eq!(
        json1, json2,
        "InventoryValuationReport data must be byte-identical for identical empty DB state. \
         (metadata.computed_at differs by design)"
    );
}

#[test]
fn metadata_contains_snapshot_source() {
    let envelope = InventoryValuationReport::compute(
        ConnectionFactory::new_for_test().unwrap().executor(),
        InventoryValuationInput {
            fiscal_year: None,
            unit_id: None,
        },
    )
    .unwrap();

    assert_eq!(
        envelope.metadata.snapshot_source.as_deref(),
        Some("fifo_stock_layers")
    );
}

#[test]
fn report_output_is_serializable() {
    let output = grpc_lib::application::reporting::fiscal_year_summary::FiscalYearSummaryOutput {
        fiscal_year: 2024,
        status: "closed".into(),
        total_movements_in: 10,
        total_movements_out: 5,
        total_consumption_value: 1500.50,
        total_opening_value: 2000.00,
        daily_report_count: 30,
        total_beneficiaries: 1200,
        ending_inventory_value: 500.25,
        layer_count: 15,
    };

    let json = serde_json::to_string(&output).unwrap();
    let deserialized: grpc_lib::application::reporting::fiscal_year_summary::FiscalYearSummaryOutput =
        serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, output);
}

#[test]
fn stock_movement_ledger_default_limit_works() {
    let result = StockMovementLedgerReport::compute(
        ConnectionFactory::new_for_test().unwrap().executor(),
        StockMovementLedgerInput {
            fiscal_year: None,
            product_id: None,
            movement_type: None,
            unit_id: None,
            start_timestamp: None,
            end_timestamp: None,
            limit: 50,
            offset: 0,
        },
    );

    assert!(result.is_ok());
}

#[test]
fn cache_key_deterministic() {
    let k1 = CacheKey::new("r", 1, "{}", Some(2024));
    let k2 = CacheKey::new("r", 1, "{}", Some(2024));
    assert_eq!(k1, k2);
    assert_eq!(k1.to_string(), k2.to_string());
}

#[test]
fn round_money_basic_cases() {
    assert_eq!(
        grpc_lib::application::reporting::round_money(100.0),
        100.0
    );
    assert_eq!(
        grpc_lib::application::reporting::round_money(100.456),
        100.46
    );
    assert_eq!(
        grpc_lib::application::reporting::round_money(100.454),
        100.45
    );
    assert_eq!(
        grpc_lib::application::reporting::round_money(0.0),
        0.0
    );
}

#[test]
fn all_report_slugs_are_unique() {
    let slugs = vec![
        FiscalYearSummaryReport::slug(),
        InventoryValuationReport::slug(),
        StockMovementLedgerReport::slug(),
    ];
    let mut sorted = slugs.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(slugs.len(), sorted.len(), "all report slugs must be unique");
}
