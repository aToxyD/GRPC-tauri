//! Operator misuse containment — tokens, throttle, integrity preconditions.

mod common;

use grpc_lib::application::services::{
    GuardedOperation, OperationExecutionGuard, SystemIntegrityState,
};
use grpc_lib::errors::{AppError, BusinessLogicError};

#[test]
fn stale_execution_token_rejected_after_state_change() {
    let (state, _tmp) = common::create_test_state();
    let guard = state.db.lock().unwrap();
    let db = guard.as_ref().unwrap();
    let ex = db.executor();

    let open_year: i32 = ex
        .query_row(
            "SELECT year FROM fiscal_year_status WHERE status = 'open' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let op = GuardedOperation::FiscalClose {
        year: open_year,
        next_year: open_year + 1,
    };
    let token = OperationExecutionGuard::issue_execution_token(ex, op).unwrap();

    // Mutate archived max — must invalidate fingerprint.
    ex.execute(
        "INSERT INTO fiscal_year_status (year, status, opened_at, archived)
         VALUES (1900, 'closed', '1900-01-01', 1)",
        [],
    )
    .unwrap();

    let err = OperationExecutionGuard::validate_execution_token(ex, op, &token).unwrap_err();
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::StaleExecutionToken { .. })
    ));
}

#[test]
fn double_execution_throttled_within_window() {
    let guard = OperationExecutionGuard::new();
    let op = GuardedOperation::RestoreBackup;
    guard.record_execution(op);
    let err = guard.assert_not_throttled(op).unwrap_err();
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::OperationThrottled { .. })
    ));
}

#[test]
fn critical_integrity_blocks_restore_and_archive() {
    let (state, _tmp) = common::create_test_state();
    let guard = state.db.lock().unwrap();
    let db = guard.as_ref().unwrap();
    db.get_connection()
        .execute(
            "INSERT INTO integrity_verification_attempts (attempted_at, verification_type, outcome)
             VALUES (datetime('now'), 'AUDIT_CHAIN', 'FAIL')",
            [],
        )
        .unwrap();

    let resolved = SystemIntegrityState::resolve_from_executor(db.executor()).unwrap();
    assert_eq!(resolved, SystemIntegrityState::Critical);

    let err = OperationExecutionGuard::assert_integrity_allows_restore_or_archive(db.executor())
        .unwrap_err();
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::RestoreBlockedByIntegrity { .. })
    ));
}

#[test]
fn archive_rejected_when_critical_anomaly_present() {
    let (state, _tmp) = common::create_test_state();
    let guard = state.db.lock().unwrap();
    let db = guard.as_ref().unwrap();

    db.get_connection()
        .execute(
            "INSERT INTO fiscal_year_status (year, status, opened_at, closed_at, archived)
             VALUES (2020, 'closed', '2020-01-01', '2020-12-31', 0)",
            [],
        )
        .unwrap();

    for _ in 0..3 {
        db.get_connection()
            .execute(
                "INSERT INTO integrity_verification_attempts (attempted_at, verification_type, outcome)
                 VALUES (datetime('now', '-1 day'), 'FISCAL_DRIFT', 'FAIL')",
                [],
            )
            .unwrap();
    }

    let err = OperationExecutionGuard::assert_archive_no_critical_anomalies(db.executor(), 2020)
        .unwrap_err();
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
    ));
}
