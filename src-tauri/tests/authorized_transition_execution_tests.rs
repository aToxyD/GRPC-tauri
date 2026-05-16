use chrono::{Duration, Utc};
use grpc_lib::application::services::FiscalClosurePackageService;
use grpc_lib::repositories::RepositoryProvider;

mod common;

#[test]
fn test_authorized_transition_execution_flow() {
    let (_state, _temp) = common::create_test_state();

    // Case A: Wilaya authorization (Decision)
    let pkg = FiscalClosurePackageService::build_closure_package(
        "WILAYA-01",
        "wilaya-admin",
        2024,
        2025,
        &Utc::now().to_rfc3339(),
        None,
    )
    .unwrap();

    assert_eq!(pkg.closed_year, 2024);
    assert_eq!(pkg.opened_year, 2025);
    assert!(
        pkg.authorized_execution_window.expires_at > pkg.authorized_execution_window.not_before
    );
}

#[test]
fn test_transition_window_expiration() {
    let (state, _temp) = common::create_test_state();
    let db_lock = state.db.lock().unwrap();
    let db = db_lock.as_ref().unwrap();
    let service = FiscalClosurePackageService::new(db.executor());

    // Create a package that expired 1 hour ago
    let now = Utc::now();
    let mut pkg = FiscalClosurePackageService::build_closure_package(
        "WILAYA-01",
        "admin",
        2024,
        2025,
        &now.to_rfc3339(),
        None,
    )
    .unwrap();

    pkg.authorized_execution_window.not_before = (now - Duration::hours(2)).to_rfc3339();
    pkg.authorized_execution_window.expires_at = (now - Duration::hours(1)).to_rfc3339();

    // Run pre-flight
    let issues = service.pre_flight_checks(&pkg).unwrap();
    assert!(issues.iter().any(|i: &String| i.contains("expired")));
}

#[test]
fn test_transition_window_not_yet_started() {
    let (state, _temp) = common::create_test_state();
    let db_lock = state.db.lock().unwrap();
    let db = db_lock.as_ref().unwrap();
    let service = FiscalClosurePackageService::new(db.executor());

    // Create a package that starts in 1 hour
    let now = Utc::now();
    let mut pkg = FiscalClosurePackageService::build_closure_package(
        "WILAYA-01",
        "admin",
        2024,
        2025,
        &now.to_rfc3339(),
        None,
    )
    .unwrap();

    pkg.authorized_execution_window.not_before = (now + Duration::hours(1)).to_rfc3339();
    pkg.authorized_execution_window.expires_at = (now + Duration::hours(2)).to_rfc3339();

    // Run pre-flight
    let issues = service.pre_flight_checks(&pkg).unwrap();
    assert!(issues
        .iter()
        .any(|i: &String| i.contains("not yet started")));
}

#[test]
fn test_transition_replay_protection() {
    let (state, _temp) = common::create_test_state();
    let db_lock = state.db.lock().unwrap();
    let db = db_lock.as_ref().unwrap();
    let executor = db.executor();
    let service = FiscalClosurePackageService::new(executor);

    let pkg = FiscalClosurePackageService::build_closure_package(
        "WILAYA-01",
        "admin",
        2024,
        2025,
        &Utc::now().to_rfc3339(),
        None,
    )
    .unwrap();

    // Record it as already applied
    executor
        .fiscal_transitions()
        .record_application(&pkg.fiscal_transition_id, 2024, 2025, "operator")
        .unwrap();

    // Run pre-flight
    let issues = service.pre_flight_checks(&pkg).unwrap();
    assert!(issues
        .iter()
        .any(|i: &String| i.contains("already been applied")));
}
