use grpc_lib::application::services::fiscal_integrity_service::FiscalIntegrityService;
use grpc_lib::application::services::inventory_integrity_service::InventoryIntegrityService;
use grpc_lib::db::ConnectionFactory;

#[test]
fn case_a_inventory_matches_stock_table() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let svc = InventoryIntegrityService::new(db.executor());
    let r = svc.verify_inventory_consistency(2024).unwrap();
    assert_eq!(r.mismatch_count, 0);
}

#[test]
fn inventory_reconciliation_is_exact_quantity() {
    // ADR-0048 (Target C): expected = opening + IN − OUT is exact scale-3
    // Quantity reconciliation. A perfectly matching ledger lands at zero
    // scaled units; a genuine ≥1-unit drift is flagged with exact deltas.
    use grpc_lib::models::Product;
    use grpc_lib::repositories::{ProductRepository, StockMovementRepository};
    use uuid::Uuid;

    let db = ConnectionFactory::new_for_test().unwrap();
    let executor = db.executor();
    let now = chrono::Utc::now().to_rfc3339();

    let product_repo = ProductRepository::new(executor);
    let seed = |id: &str, name: &str| {
        product_repo
            .insert_raw_product(
                &Product {
                    id: id.to_string(),
                    name: name.to_string(),
                    base_price: 100.0,
                    year: 2024,
                    created_at: chrono::Utc::now(),
                },
                &now,
            )
            .unwrap();
    };
    seed("prod-majo", "MATCHING");
    seed("prod-drift", "DRIFT");

    // product prod-majo: opening 1.000 + IN 2.000 − OUT 0.500 = stock 2.500
    executor
        .execute(
            "INSERT INTO opening_balance_snapshots (product_id, fiscal_year, opening_quantity, created_at, created_by) VALUES ('prod-majo', 2024, 1.0, ?1, 'test_user')",
            rusqlite::params![now.clone()],
        )
        .unwrap();
    executor
        .execute(
            "INSERT OR IGNORE INTO inventory_stocks (id, product_id, quantity, unit, last_updated) VALUES (?1, 'prod-majo', 2.5, 'unit', ?2)",
            rusqlite::params![Uuid::new_v4().to_string(), now],
        )
        .unwrap();

    // product prod-drift: opening 0 + IN 1.000 − OUT 0 = stock 2.000 → 1.000 drift
    executor
        .execute(
            "INSERT OR IGNORE INTO inventory_stocks (id, product_id, quantity, unit, last_updated) VALUES (?1, 'prod-drift', 2.0, 'unit', ?2)",
            rusqlite::params![Uuid::new_v4().to_string(), now],
        )
        .unwrap();

    let movements = StockMovementRepository::new(executor);
    use grpc_lib::models::StockMovementType;
    for (pid, mtype, qty) in [
        ("prod-majo", StockMovementType::In, 2.0),
        ("prod-majo", StockMovementType::Out, 0.5),
        ("prod-drift", StockMovementType::In, 1.0),
    ] {
        movements
            .insert_raw_stock_movement(&grpc_lib::models::StockMovement {
                id: Uuid::new_v4().to_string(),
                product_id: pid.to_string(),
                product_name: Some(pid.to_string()),
                movement_type: mtype,
                quantity: qty,
                balance_before: 0.0,
                balance_after: qty,
                reference_type: Some("Order".to_string()),
                reference_id: None,
                notes: None,
                timestamp: now.clone(),
                user_id: "test_user".to_string(),
                username: "test_user".to_string(),
                unit_id: Some("itest".to_string()),
                fiscal_year: Some(2024),
                unit_cost: None,
            })
            .unwrap();
    }

    let r = InventoryIntegrityService::new(executor)
        .verify_inventory_consistency(2024)
        .unwrap();
    assert_eq!(r.mismatch_count, 1);
    let issue = &r.issues[0];
    assert_eq!(issue.product_id, "prod-drift");
    assert_eq!(issue.expected_quantity, 1.0);
    assert_eq!(issue.actual_quantity, 2.0);
    assert_eq!(issue.delta, -1.0);
}

#[test]
fn case_g_multiple_open_years_detected() {
    let db = ConnectionFactory::new_for_test().unwrap();
    db.executor().execute("INSERT INTO fiscal_year_status(year,status,opened_at) VALUES (2025,'open',datetime('now'))",[]).unwrap();
    let r = FiscalIntegrityService::new(db.executor())
        .run_full_integrity_scan()
        .unwrap();
    assert!(r.warnings.iter().any(|w| w.code == "DRIFT_D"));
}
