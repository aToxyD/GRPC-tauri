//! Integration tests: `AuditTxService` transactional semantics with real SQLite.

use grpc_lib::application::services::{AuditTxService, UserContext};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::audit::AuditAction;
use grpc_lib::errors::AppError;
use tempfile::tempdir;
use uuid::Uuid;

fn admin_user_id(db: &grpc_lib::db::Database) -> String {
    db.get_connection()
        .query_row(
            "SELECT id FROM users WHERE username = 'admin' LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .expect("default admin")
}

#[test]
fn audit_tx_case_a_closure_error_rolls_back_business_and_audit() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("audit_tx_a.db");
    let mut db = ConnectionFactory::new_with_path(&path).expect("db");
    let admin_id = admin_user_id(&db);
    let before_audit: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))
        .unwrap();

    let ctx = UserContext::new(&admin_id, "admin", None);
    let res: Result<(), AppError> = AuditTxService::execute_with_audit(
        &mut db,
        AuditAction::CreateUnit,
        &ctx,
        |tx| {
            let code = format!("C{}", Uuid::new_v4());
            tx.executor.execute(
            "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES ('u-fail-a', ?1, 'N', '16', datetime('now'))",
            [&code],
        )?;
            Err(AppError::Internal("deliberate".into()))
        },
    );
    assert!(res.is_err());

    let orphan: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM units WHERE id = 'u-fail-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(orphan, 0);

    let after_audit: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after_audit, before_audit);
}

#[test]
fn audit_tx_case_b_audit_insert_failure_rolls_back_entire_transaction() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("audit_tx_b.db");
    let mut db = ConnectionFactory::new_with_path(&path).expect("db");
    let admin_id = admin_user_id(&db);
    db.get_connection()
        .execute_batch(
            r#"
            CREATE TRIGGER IF NOT EXISTS trg_audit_tx_fail_test
            BEFORE INSERT ON audit_log
            FOR EACH ROW
            WHEN NEW.username = '__AUDIT_TX_FAIL__'
            BEGIN
                SELECT RAISE(ABORT, 'injected audit failure');
            END;
        "#,
        )
        .expect("trigger");

    let before_units: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM units", [], |r| r.get(0))
        .unwrap();
    let before_audit: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))
        .unwrap();

    let uid = Uuid::new_v4().to_string();
    let code = format!("C{}", Uuid::new_v4());
    let ctx = UserContext::new(&admin_id, "__AUDIT_TX_FAIL__", None);
    let res = AuditTxService::execute_with_audit(&mut db, AuditAction::CreateUnit, &ctx, |tx| {
        tx.executor.execute(
            "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1, ?2, 'N', '16', datetime('now'))",
            [&uid, &code],
        )?;
        Ok(())
    });
    assert!(res.is_err());

    let after_units: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM units", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after_units, before_units);

    let after_audit: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after_audit, before_audit);

    let _ = db
        .get_connection()
        .execute_batch("DROP TRIGGER IF EXISTS trg_audit_tx_fail_test;");
}

#[test]
fn audit_tx_case_c_success_commits_business_and_audit() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("audit_tx_c.db");
    let mut db = ConnectionFactory::new_with_path(&path).expect("db");
    let admin_id = admin_user_id(&db);
    let before_audit: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))
        .unwrap();

    let uid = Uuid::new_v4().to_string();
    let code = format!("C{}", Uuid::new_v4());
    let ctx = UserContext::new(&admin_id, "admin", None);
    AuditTxService::execute_with_audit(&mut db, AuditAction::CreateUnit, &ctx, |tx| {
        tx.executor.execute(
            "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1, ?2, 'N', '16', datetime('now'))",
            [&uid, &code],
        )?;
        Ok(())
    })
    .expect("success path");

    let u: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM units WHERE id = ?1", [&uid], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(u, 1);

    let row: (String, String) = db
        .get_connection()
        .query_row(
            "SELECT action, entity_type FROM audit_log ORDER BY rowid DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("audit row");
    assert_eq!(row.0, "CreateUnit");
    assert_eq!(row.1, "Unit");

    let after_audit: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after_audit, before_audit + 1);
}
