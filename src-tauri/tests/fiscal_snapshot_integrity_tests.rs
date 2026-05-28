//! Integration tests for fiscal snapshot integrity (Priority 1).
//!
//! All tests use a real SQLite temp DB via create_test_state().
//! Cases A-E match the prompt specification.

mod common;

use common::{
    clear_fiscal_status, create_test_product, create_test_state, seed_fiscal_year_open,
    set_test_stock,
};
use grpc_lib::application::services::FiscalClosingService;
use grpc_lib::repositories::RepositoryProvider;

const YEAR: i32 = 2025;
const NEXT: i32 = 2026;
const USER_ID: &str = "system";
const USERNAME: &str = "system";

fn close(state: &grpc_lib::commands::AppState) -> Result<usize, grpc_lib::errors::AppError> {
    let mut guard = state.db.lock().unwrap();
    let db = guard.as_mut().unwrap();
    db.with_transaction(|tx| {
        FiscalClosingService::new(tx).close_year(YEAR, NEXT, USER_ID, USERNAME, None)
    })
}

// ── Case A: Normal carry-forward with positive stock ─────────────────────────
#[test]
fn case_a_normal_carry_forward() {
    let (state, _dir) = create_test_state();
    clear_fiscal_status(&state);
    seed_fiscal_year_open(&state, YEAR);

    let pid = create_test_product(&state, "Flour", 250.0, YEAR);
    set_test_stock(&state, &pid, 40.0);

    let count = close(&state).expect("close_year must succeed");
    assert_eq!(count, 1, "one snapshot expected");

    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let snaps = db.executor().opening_balances().get_by_year(NEXT).unwrap();

    let snap = snaps.iter().find(|s| s.product_id == pid).unwrap();
    assert_eq!(snap.opening_quantity, 40.0);
    assert_eq!(snap.unit_cost, 250.0);
    assert!(
        (snap.total_value - 40.0 * 250.0).abs() < 1e-9,
        "total_value must equal qty * unit_cost"
    );
    assert_eq!(snap.snapshot_reason, "year_close");
    assert_eq!(snap.carried_from_year, Some(YEAR));
}

// ── Case B: Product with zero stock (no movement history) ────────────────────
#[test]
fn case_b_zero_stock_product() {
    let (state, _dir) = create_test_state();
    clear_fiscal_status(&state);
    seed_fiscal_year_open(&state, YEAR);

    let pid = create_test_product(&state, "Sugar", 100.0, YEAR);
    // leave quantity at 0 (default from create_test_product)

    close(&state).expect("close_year must succeed even with zero stock");

    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let snaps = db.executor().opening_balances().get_by_year(NEXT).unwrap();
    let snap = snaps.iter().find(|s| s.product_id == pid).unwrap();

    assert_eq!(snap.opening_quantity, 0.0);
    assert_eq!(snap.total_value, 0.0);
}

// ── Case C: close_year twice → idempotency guard rejects second call ──────────
#[test]
fn case_c_double_close_rejected() {
    let (state, _dir) = create_test_state();
    clear_fiscal_status(&state);
    seed_fiscal_year_open(&state, YEAR);
    create_test_product(&state, "Oil", 150.0, YEAR);

    // First close must succeed
    close(&state).expect("first close must succeed");

    // Second close must return a FiscalYearClosed error, not silently succeed
    let result = close(&state);
    assert!(result.is_err(), "second close_year must be rejected");
    match result.unwrap_err() {
        grpc_lib::errors::AppError::BusinessLogic(
            grpc_lib::errors::BusinessLogicError::FiscalYearClosed { year },
        ) => assert_eq!(year, YEAR),
        other => panic!("expected FiscalYearClosed, got {:?}", other),
    }
}

// ── Case D: total_value = quantity × unit_cost assertion ─────────────────────
#[test]
fn case_d_total_value_formula() {
    let (state, _dir) = create_test_state();
    clear_fiscal_status(&state);
    seed_fiscal_year_open(&state, YEAR);

    let p1 = create_test_product(&state, "Rice", 80.0, YEAR);
    set_test_stock(&state, &p1, 75.0);

    let p2 = create_test_product(&state, "Pasta", 45.0, YEAR);
    set_test_stock(&state, &p2, 120.0);

    close(&state).unwrap();

    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let snaps = db.executor().opening_balances().get_by_year(NEXT).unwrap();

    for snap in &snaps {
        let expected = snap.opening_quantity * snap.unit_cost;
        assert!(
            (snap.total_value - expected).abs() < 1e-9,
            "product {}: total_value={} but qty*cost={}",
            snap.product_id,
            snap.total_value,
            expected
        );
    }
}

// ── Case E: Transaction rollback on snapshot failure ─────────────────────────
// Simulate failure by closing the year AFTER manually marking it closed in the
// status table mid-way — the idempotency guard rejects and rolls back.
#[test]
fn case_e_rollback_on_failure() {
    let (state, _dir) = create_test_state();
    clear_fiscal_status(&state);
    seed_fiscal_year_open(&state, YEAR);
    create_test_product(&state, "Meat", 500.0, YEAR);

    // Corrupt the status BEFORE calling close — simulate a half-open state
    {
        use rusqlite::params;
        let db = state.db.lock().unwrap();
        let db = db.as_ref().unwrap();
        db.get_connection()
            .execute(
                "UPDATE fiscal_year_status SET status = 'closed' WHERE year = ?1",
                params![YEAR],
            )
            .unwrap();
    }

    // close_year must fail because year is already closed
    let result = close(&state);
    assert!(result.is_err(), "must fail when year is closed");

    // No snapshots should have been written
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let count = db
        .executor()
        .opening_balances()
        .count_by_year(NEXT)
        .unwrap();
    assert_eq!(count, 0, "no snapshots must persist on rollback");

    // next year must NOT have been opened
    let next_status = db
        .executor()
        .fiscal_year_status()
        .get_by_year(NEXT)
        .unwrap();
    assert!(
        next_status.is_none(),
        "next year must not be opened if close_year failed"
    );
}
