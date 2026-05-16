//! Hot-path EXPLAIN QUERY PLAN regression guards — indexes only when scans are real.

use grpc_lib::db::ConnectionFactory;
use tempfile::tempdir;

fn explain_uses_index(conn: &rusqlite::Connection, sql: &str) -> bool {
    let mut stmt = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {}", sql))
        .unwrap();
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(3))
        .unwrap()
        .map(|s| s.unwrap_or_default())
        .collect::<Vec<_>>();
    rows.iter().any(|detail| {
        detail.contains("USING INDEX")
            || detail.contains("USING COVERING INDEX")
            || detail.contains("SEARCH")
    })
}

fn seed_operational_volume(conn: &rusqlite::Connection) {
    for i in 0..200 {
        conn.execute(
            "INSERT INTO operational_findings_log (emitted_at, severity, category, code, message, recommendation)
             VALUES (?1, 'WARNING', 'STOCK_MOVEMENT', ?2, 'm', 'r')",
            rusqlite::params![format!("2026-01-{:02}T00:00:00Z", (i % 28) + 1), format!("C{}", i)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO integrity_verification_attempts (attempted_at, verification_type, outcome)
             VALUES (?1, 'FISCAL_DRIFT', 'FAIL')",
            [format!("2026-02-{:02}T00:00:00Z", (i % 28) + 1)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fiscal_operational_snapshots
             (snapshot_date, fiscal_year, total_inventory_value, product_count, movement_count,
              report_count, integrity_state, created_by, created_at)
             VALUES (?1, 2026, 1.0, 1, 1, 1, 'OK', 'admin', ?1)",
            [format!("2026-03-{:02}", (i % 28) + 1)],
        )
        .unwrap();
    }
}

#[test]
fn operational_findings_timeline_query_uses_index() {
    let dir = tempdir().unwrap();
    let db = ConnectionFactory::new_with_path(&dir.path().join("qp1.db")).unwrap();
    let conn = db.get_connection();
    seed_operational_volume(conn);

    let sql = r#"
        SELECT emitted_at, message, category, code
        FROM operational_findings_log
        WHERE severity = 'CRITICAL'
        ORDER BY emitted_at DESC
        LIMIT 500
    "#;
    assert!(
        explain_uses_index(conn, sql),
        "operational_findings_log CRITICAL timeline should use an index"
    );
}

#[test]
fn integrity_attempts_anomaly_query_uses_index() {
    let dir = tempdir().unwrap();
    let db = ConnectionFactory::new_with_path(&dir.path().join("qp2.db")).unwrap();
    let conn = db.get_connection();
    seed_operational_volume(conn);

    let sql = r#"
        SELECT COUNT(*) FROM integrity_verification_attempts
        WHERE outcome = 'FAIL' AND attempted_at >= datetime('now', '-30 days')
    "#;
    assert!(
        explain_uses_index(conn, sql),
        "integrity_verification_attempts FAIL window should use an index"
    );
}

#[test]
fn fiscal_operational_snapshots_by_year_uses_index() {
    let dir = tempdir().unwrap();
    let db = ConnectionFactory::new_with_path(&dir.path().join("qp3.db")).unwrap();
    let conn = db.get_connection();
    seed_operational_volume(conn);

    let sql = r#"
        SELECT id, snapshot_date, fiscal_year
        FROM fiscal_operational_snapshots
        WHERE fiscal_year = 2026
        ORDER BY id DESC
        LIMIT 30
    "#;
    assert!(
        explain_uses_index(conn, sql),
        "fiscal_operational_snapshots fiscal_year filter should use idx_fiscal_op_snapshots_year"
    );
}

#[test]
fn audit_log_timestamp_scan_uses_index() {
    let dir = tempdir().unwrap();
    let db = ConnectionFactory::new_with_path(&dir.path().join("qp4.db")).unwrap();
    let conn = db.get_connection();
    let admin_id: String = conn
        .query_row(
            "SELECT id FROM users WHERE username = 'admin' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    for i in 0..50 {
        conn.execute(
            "INSERT INTO audit_log (id, user_id, username, action, entity_type, timestamp, status, previous_hash, entry_hash)
             VALUES (?1, ?2, 'admin', 'Login', 'User', ?3, 'Success', '', '')",
            rusqlite::params![
                format!("a{}", i),
                admin_id,
                format!("2026-04-{:02}T12:00:00Z", (i % 28) + 1)
            ],
        )
        .unwrap();
    }

    let sql = r#"
        SELECT timestamp, action FROM audit_log
        WHERE action IN ('FiscalYearClosed', 'CreateBackup')
        ORDER BY timestamp DESC
        LIMIT 500
    "#;
    assert!(
        explain_uses_index(conn, sql),
        "audit_log timeline should use idx_audit_timestamp or idx_audit_action"
    );
}
