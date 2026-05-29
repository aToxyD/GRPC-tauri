//! Hostile tampering tests: `audit_log` hash chain + linkage via real SQLite (temp DB).

use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::audit::NewAuditEntry;
use grpc_lib::domain::audit_chain::compute_entry_hash;
use grpc_lib::repositories::executor::DbExecutor;
use grpc_lib::repositories::RepositoryProvider;
use tempfile::tempdir;

fn admin_user_id(conn: &rusqlite::Connection) -> String {
    conn.query_row(
        "SELECT id FROM users WHERE username = 'admin' LIMIT 1",
        [],
        |row| row.get::<_, String>(0),
    )
    .expect("default admin")
}

fn base_entry(id: &str, ts: &str, user_id: &str) -> NewAuditEntry {
    NewAuditEntry {
        id: id.to_string(),
        user_id: user_id.into(),
        username: "alice".into(),
        action: "Login".into(),
        entity_type: "User".into(),
        entity_id: None,
        entity_name: None,
        old_value: None,
        new_value: None,
        session_id: None,
        timestamp: ts.into(),
        status: "Success".into(),
        error_message: None,
        metadata: None,
        previous_hash: None,
        entry_hash: None,
        event_type: None,
        actor_id: None,
        target_type: None,
        target_id: None,
        fiscal_year: None,
        before_snapshot: None,
        after_snapshot: None,
        node_id: None,
        details: None,
    }
}

fn insert_two_linking_rows(conn: &rusqlite::Connection) -> (String, String) {
    let uid = admin_user_id(conn);
    let ex = DbExecutor::Conn(conn);
    let repo = ex.audit();
    let e1 = base_entry("e1", "2026-05-01T10:00:00Z", &uid);
    let h1 = compute_entry_hash(None, &e1);
    let row1 = NewAuditEntry {
        previous_hash: None,
        entry_hash: Some(h1.clone()),
        ..e1
    };
    repo.insert_audit_log(&row1).expect("insert e1");
    let e2 = base_entry("e2", "2026-05-01T10:01:00Z", &uid);
    let h2 = compute_entry_hash(Some(&h1), &e2);
    let row2 = NewAuditEntry {
        previous_hash: Some(h1.clone()),
        entry_hash: Some(h2.clone()),
        ..e2
    };
    repo.insert_audit_log(&row2).expect("insert e2");
    (h1, h2)
}

fn verify_stream(conn: &rusqlite::Connection) -> bool {
    let ex = DbExecutor::Conn(conn);
    ex.audit()
        .verify_audit_chain_streaming()
        .expect("verify query")
        .is_valid
}

#[test]
fn tamper_a_modify_chained_row_fails() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("tamper_a.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    insert_two_linking_rows(conn);
    assert!(verify_stream(conn));

    conn.execute("UPDATE audit_log SET username = 'root' WHERE id = 'e1'", [])
        .unwrap();
    assert!(!verify_stream(conn));
}

#[test]
fn tamper_b_delete_row_fails() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("tamper_b.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    insert_two_linking_rows(conn);
    conn.execute("DELETE FROM audit_log WHERE id = 'e1'", [])
        .unwrap();
    assert!(!verify_stream(conn));
}

#[test]
fn tamper_c_insert_forged_mid_chain_fails() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("tamper_c.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    insert_two_linking_rows(conn);
    assert!(verify_stream(conn));

    let uid = admin_user_id(conn);
    let forged = base_entry("evil", "2026-05-01T10:00:30Z", &uid);
    let forged_digest = compute_entry_hash(None, &forged);
    conn.execute(
        "INSERT INTO audit_log (id, user_id, username, action, entity_type, entity_id, entity_name,
            old_value, new_value, session_id, timestamp, status, error_message, metadata, previous_hash, entry_hash)
         VALUES ('evil', ?1, 'alice', 'Login', 'User', NULL, NULL, NULL, NULL, NULL, '2026-05-01T10:00:30Z', 'Success', NULL, NULL, NULL, ?2)",
        rusqlite::params![&uid, &forged_digest],
    )
    .unwrap();
    assert!(!verify_stream(conn));
}

#[test]
fn tamper_d_physical_row_reorder_fails() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("tamper_d.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_user_id(conn);
    let (h1, h2) = insert_two_linking_rows(conn);

    conn.execute_batch("DELETE FROM audit_log;").unwrap();

    let ex = DbExecutor::Conn(conn);
    let repo = ex.audit();
    let e2 = base_entry("e2", "2026-05-01T10:01:00Z", &uid);
    let row2_first = NewAuditEntry {
        previous_hash: Some(h1.clone()),
        entry_hash: Some(h2.clone()),
        ..e2
    };
    repo.insert_audit_log(&row2_first).unwrap();
    let e1 = base_entry("e1", "2026-05-01T10:00:00Z", &uid);
    let row1_second = NewAuditEntry {
        previous_hash: None,
        entry_hash: Some(h1.clone()),
        ..e1
    };
    repo.insert_audit_log(&row1_second).unwrap();

    assert!(!verify_stream(conn));
}

#[test]
fn tamper_e_payload_only_timestamp_change_fails() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("tamper_e.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    insert_two_linking_rows(conn);
    conn.execute(
        "UPDATE audit_log SET timestamp = '2099-01-01T00:00:00Z' WHERE id = 'e1'",
        [],
    )
    .unwrap();
    assert!(!verify_stream(conn));
}

#[test]
fn tamper_f_wrong_previous_hash_fails() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("tamper_f.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    insert_two_linking_rows(conn);
    conn.execute(
        "UPDATE audit_log SET previous_hash = '00' WHERE id = 'e2'",
        [],
    )
    .unwrap();
    assert!(!verify_stream(conn));
}
