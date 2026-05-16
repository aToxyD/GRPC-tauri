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
fn case_g_multiple_open_years_detected() {
    let db = ConnectionFactory::new_for_test().unwrap();
    db.executor().execute("INSERT INTO fiscal_year_status(year,status,opened_at) VALUES (2025,'open',datetime('now'))",[]).unwrap();
    let r = FiscalIntegrityService::new(db.executor())
        .run_full_integrity_scan()
        .unwrap();
    assert!(r.warnings.iter().any(|w| w.code == "DRIFT_D"));
}
