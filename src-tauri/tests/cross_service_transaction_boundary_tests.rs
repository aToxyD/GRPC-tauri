//! Cross-service transaction boundary tests — real SQLite, explicit failure injection.

use grpc_lib::application::services::{
    FiscalClosingService, FiscalExportSnapshot, FiscalExportSnapshotService,
    OperationalAnomalyService,
};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::repositories::RepositoryProvider;
use tempfile::tempdir;

fn seed_open_year(db: &grpc_lib::db::Database, year: i32) {
    let ex = db.executor();
    ex.execute("DELETE FROM fiscal_year_status", []).unwrap();
    ex.fiscal_year_status()
        .seed_year(year, "open", "2026-01-01T00:00:00Z")
        .unwrap();
    ex.settings().set_current_year(year).unwrap();
    ex.execute(
        "INSERT INTO products (id, name, base_price, year, created_at, updated_at)
         VALUES ('p-tx', 'Tx Product', 10.0, ?1, datetime('now'), datetime('now'))",
        [year],
    )
    .unwrap();
    ex.execute(
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, last_updated, updated_at)
         VALUES ('s-tx', 'p-tx', 5.0, 'kg', datetime('now'), datetime('now'))",
        [],
    )
    .unwrap();
}

fn admin_id(db: &grpc_lib::db::Database) -> String {
    db.get_connection()
        .query_row(
            "SELECT id FROM users WHERE username = 'admin' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

fn fiscal_status(db: &grpc_lib::db::Database, year: i32) -> String {
    db.get_connection()
        .query_row(
            "SELECT status FROM fiscal_year_status WHERE year = ?1",
            [year],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn case_a_fiscal_close_audit_failure_rolls_back_entire_transaction() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("tx_a.db");
    let mut db = ConnectionFactory::new_with_path(&path).unwrap();
    seed_open_year(&db, 2024);
    let uid = admin_id(&db);

    db.get_connection()
        .execute_batch(
            r#"
            CREATE TRIGGER IF NOT EXISTS trg_fiscal_close_audit_fail
            BEFORE INSERT ON audit_log
            FOR EACH ROW
            WHEN NEW.username = '__FISCAL_CLOSE_AUDIT_FAIL__'
            BEGIN
                SELECT RAISE(ABORT, 'injected audit failure');
            END;
        "#,
        )
        .unwrap();

    let before_snapshots: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM opening_balance_snapshots WHERE fiscal_year = 2025",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let before_audit: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))
        .unwrap();

    let res = db.with_transaction(|tx| {
        FiscalClosingService::new(tx).close_year(
            2024,
            2025,
            &uid,
            "__FISCAL_CLOSE_AUDIT_FAIL__",
            None,
        )
    });
    assert!(res.is_err());

    assert_eq!(fiscal_status(&db, 2024), "open");
    let current: i32 = db
        .get_connection()
        .query_row("SELECT current_year FROM settings WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(current, 2024);

    let after_snapshots: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM opening_balance_snapshots WHERE fiscal_year = 2025",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after_snapshots, before_snapshots);

    let after_audit: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after_audit, before_audit);
}

#[test]
fn case_b_snapshot_creation_failure_does_not_lock_year() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("tx_b.db");
    let mut db = ConnectionFactory::new_with_path(&path).unwrap();
    seed_open_year(&db, 2024);
    let uid = admin_id(&db);

    db.get_connection()
        .execute_batch(
            r#"
            CREATE TRIGGER IF NOT EXISTS trg_opening_snapshot_fail
            BEFORE INSERT ON opening_balance_snapshots
            FOR EACH ROW
            WHEN NEW.snapshot_reason = 'year_close'
            BEGIN
                SELECT RAISE(ABORT, 'injected snapshot failure');
            END;
        "#,
        )
        .unwrap();

    let res = db.with_transaction(|tx| {
        FiscalClosingService::new(tx).close_year(2024, 2025, &uid, "admin", None)
    });
    assert!(res.is_err());
    assert_eq!(fiscal_status(&db, 2024), "open");
    let snaps: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM opening_balance_snapshots WHERE fiscal_year = 2025",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(snaps, 0);
}

#[test]
fn case_c_settings_current_year_failure_rolls_back_snapshots() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("tx_c.db");
    let mut db = ConnectionFactory::new_with_path(&path).unwrap();
    seed_open_year(&db, 2024);
    let uid = admin_id(&db);

    db.get_connection()
        .execute_batch(
            r#"
            CREATE TRIGGER IF NOT EXISTS trg_settings_block_year
            BEFORE UPDATE OF current_year ON settings
            FOR EACH ROW
            WHEN NEW.current_year = 2025
            BEGIN
                SELECT RAISE(ABORT, 'injected settings failure');
            END;
        "#,
        )
        .unwrap();

    let res = db.with_transaction(|tx| {
        FiscalClosingService::new(tx).close_year(2024, 2025, &uid, "admin", None)
    });
    assert!(res.is_err());
    assert_eq!(fiscal_status(&db, 2024), "open");
    let snaps: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM opening_balance_snapshots WHERE fiscal_year = 2025",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(snaps, 0);
    let current: i32 = db
        .get_connection()
        .query_row("SELECT current_year FROM settings WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(current, 2024);
}

#[test]
fn case_d_operational_finding_failure_no_partial_diagnostics_persisted() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("tx_d.db");
    let mut db = ConnectionFactory::new_with_path(&path).unwrap();

    db.get_connection()
        .execute_batch(
            r#"
            CREATE TRIGGER IF NOT EXISTS trg_op_findings_fail
            BEFORE INSERT ON operational_findings_log
            FOR EACH ROW
            WHEN NEW.code = 'ANOMALY_C_LOW_REPORTING'
            BEGIN
                SELECT RAISE(ABORT, 'injected findings failure');
            END;
        "#,
        )
        .unwrap();

    let before: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM operational_findings_log", [], |r| {
            r.get(0)
        })
        .unwrap();

    let res =
        db.with_transaction(|tx| OperationalAnomalyService::new(tx).run_operational_analysis());
    assert!(res.is_err());

    let after: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM operational_findings_log", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(after, before);
}

#[test]
fn case_e_export_snapshot_failure_no_partial_export_metadata() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("tx_e.db");
    let mut db = ConnectionFactory::new_with_path(&path).unwrap();
    seed_open_year(&db, 2024);

    db.get_connection()
        .execute_batch(
            r#"
            CREATE TRIGGER IF NOT EXISTS trg_export_snapshot_fail
            BEFORE INSERT ON fiscal_export_snapshots
            FOR EACH ROW
            WHEN NEW.export_hash = 'INJECT_FAIL'
            BEGIN
                SELECT RAISE(ABORT, 'injected export failure');
            END;
        "#,
        )
        .unwrap();

    let snap = FiscalExportSnapshot {
        export_hash: "INJECT_FAIL".to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        generated_by: "admin".to_string(),
        fiscal_year: 2024,
        movement_count: 1,
        report_count: 0,
        inventory_total_value: 50.0,
        integrity_state: None,
        archived_years_count: None,
        active_anomalies_count: None,
        signing_key_id: None,
        export_reason: None,
    };

    let before: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM fiscal_export_snapshots", [], |r| {
            r.get(0)
        })
        .unwrap();

    let res = db
        .with_transaction(|tx| FiscalExportSnapshotService::new(tx).record_export_snapshot(&snap));
    assert!(res.is_err());

    let after: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM fiscal_export_snapshots", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(after, before);
}
