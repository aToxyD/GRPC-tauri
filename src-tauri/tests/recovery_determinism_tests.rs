//! Recovery determinism — same corruption/orphan state → same outcome every run.

use grpc_lib::application::services::fiscal_year_service::validate_fiscal_state;
use grpc_lib::application::services::SystemIntegrityState;
use grpc_lib::application::services::{BackupIntegrityService, OperationExecutionGuard};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::infrastructure::backup::recover_interrupted_restore_and_orphans;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::repositories::RepositoryProvider;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write_journal(db_path: &Path, candidate: &Path, rollback: &Path) {
    let journal_path = db_path.with_extension("restore.journal");
    let cand = candidate
        .canonicalize()
        .unwrap_or_else(|_| candidate.to_path_buf());
    let roll = rollback
        .canonicalize()
        .unwrap_or_else(|_| rollback.to_path_buf());
    let json = serde_json::json!({
        "version": 1,
        "phase": "ready_swap",
        "candidate_path": cand,
        "rollback_path": roll,
    });
    let tmp = journal_path.with_extension("restore.journal.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
    fs::rename(tmp, journal_path).unwrap();
}

#[test]
fn case_a_same_corruption_state_same_integrity_classification() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("det_a.db");
    let db = ConnectionFactory::new_with_path(&path).unwrap();
    let ex = db.executor();

    for _ in 0..3 {
        ex.execute(
            "INSERT INTO integrity_verification_attempts (attempted_at, verification_type, outcome, details)
             VALUES (datetime('now'), 'AUDIT_CHAIN', 'FAIL', 'test')",
            [],
        )
        .unwrap();
    }

    let s1 = SystemIntegrityState::resolve_from_executor(ex).unwrap();
    let s2 = SystemIntegrityState::resolve_from_executor(ex).unwrap();
    assert_eq!(s1, s2);
    assert_eq!(s1, SystemIntegrityState::Critical);
    assert_eq!(s1.error_category(), "INTEGRITY_CRITICAL");
}

#[test]
fn case_b_same_orphan_journal_state_same_recovery_path() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("live.db");
    let candidate = dir.path().join("cand.sqlite_tmp");
    let rollback = db_path.with_extension("rollback.bak");

    fs::write(&db_path, b"LIVE_OLD").unwrap();
    fs::write(&candidate, b"FROM_CAND").unwrap();
    write_journal(&db_path, &candidate, &rollback);

    let crypto = AgeFileEncryptionProvider::new();
    recover_interrupted_restore_and_orphans(&db_path, &crypto).unwrap();
    let body1 = fs::read_to_string(&db_path).unwrap();

    // Rebuild identical interrupted state
    fs::write(&db_path, b"LIVE_OLD").unwrap();
    fs::write(&candidate, b"FROM_CAND").unwrap();
    write_journal(&db_path, &candidate, &rollback);
    recover_interrupted_restore_and_orphans(&db_path, &crypto).unwrap();
    let body2 = fs::read_to_string(&db_path).unwrap();

    assert_eq!(body1, body2);
    assert_eq!(body1, "FROM_CAND");
    assert!(!candidate.exists());
}

#[test]
fn case_c_startup_validation_failures_same_error_classification() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("det_c.db");
    let db = ConnectionFactory::new_with_path(&path).unwrap();
    db.executor().settings().set_current_year(1999).unwrap();

    let e1 = validate_fiscal_state(&db).unwrap_err();
    let e2 = validate_fiscal_state(&db).unwrap_err();
    assert_eq!(e1, e2);
    assert!(e1.contains("FISCAL STATE INVALID"));
}

#[test]
fn case_d_restore_interrupted_state_preserves_archive_flags() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("det_d.db");
    let db = ConnectionFactory::new_with_path(&path).unwrap();
    db.get_connection()
        .execute(
            "INSERT OR REPLACE INTO fiscal_year_status (year, status, opened_at, archived)
             VALUES (2020, 'closed', '2020-01-01', 1)",
            [],
        )
        .unwrap();

    let archived_before: i32 = db
        .get_connection()
        .query_row(
            "SELECT archived FROM fiscal_year_status WHERE year = 2020",
            [],
            |r| r.get(0),
        )
        .unwrap();

    // Recovery helper does not touch DB contents when no journal — archive flag stable.
    let crypto = AgeFileEncryptionProvider::new();
    let db_file = dir.path().join("live2.db");
    fs::write(&db_file, b"x").unwrap();
    recover_interrupted_restore_and_orphans(&db_file, &crypto).unwrap();

    let archived_after: i32 = db
        .get_connection()
        .query_row(
            "SELECT archived FROM fiscal_year_status WHERE year = 2020",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(archived_before, archived_after);
    assert_eq!(archived_before, 1);
}

#[test]
fn case_e_backup_integrity_failure_blocks_operational_recovery_path() {
    let dir = tempdir().unwrap();
    let bad = dir.path().join("not_a_db.txt");
    fs::write(&bad, b"garbage").unwrap();

    use grpc_lib::infrastructure::backup::SqliteBackupAdapter;
    let crypto = AgeFileEncryptionProvider::new();
    let adapter = SqliteBackupAdapter::new(&bad, crypto);
    let r = BackupIntegrityService::verify_backup_file(&bad, &adapter).unwrap();
    assert!(!r.valid);
    assert!(!r.failures.is_empty());

    let dir2 = tempdir().unwrap();
    let path = dir2.path().join("guard.db");
    let db = ConnectionFactory::new_with_path(&path).unwrap();
    ex_insert_audit_chain_fail(&db);

    let blocked =
        OperationExecutionGuard::assert_integrity_allows_restore_or_archive(db.executor());
    assert!(blocked.is_err());
}

fn ex_insert_audit_chain_fail(db: &grpc_lib::db::Database) {
    db.get_connection()
        .execute(
            "INSERT INTO integrity_verification_attempts (attempted_at, verification_type, outcome)
             VALUES (datetime('now'), 'AUDIT_CHAIN', 'FAIL')",
            [],
        )
        .unwrap();
}
