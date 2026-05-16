//! Operational survivability integration tests (replaces placeholders).

use grpc_lib::application::services::{
    FiscalHistoricalGuard, FiscalYearService, SystemIntegrityState,
};
use grpc_lib::db::ConnectionFactory;
use tempfile::tempdir;

#[test]
fn archived_year_rejects_mutation() {
    let dir = tempdir().unwrap();
    let db = ConnectionFactory::new_with_path(&dir.path().join("surv_a.db")).unwrap();
    db.get_connection()
        .execute(
            "INSERT INTO fiscal_year_status (year, status, opened_at, archived)
             VALUES (2010, 'closed', '2010-01-01', 1)",
            [],
        )
        .unwrap();

    assert!(FiscalHistoricalGuard::new(db.executor())
        .assert_year_not_archived(2010)
        .is_err());
}

#[test]
fn backup_integrity_detects_corruption() {
    let dir = tempdir().unwrap();
    let bad = dir.path().join("bad.txt");
    std::fs::write(&bad, b"x").unwrap();
    use grpc_lib::infrastructure::backup::SqliteBackupAdapter;
    use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
    let crypto = AgeFileEncryptionProvider::new();
    let adapter = SqliteBackupAdapter::new(&bad, crypto);
    let r =
        grpc_lib::application::services::BackupIntegrityService::verify_backup_file(&bad, &adapter)
            .unwrap();
    assert!(!r.valid);
}

#[test]
fn critical_integrity_state_blocks_financial_operations() {
    assert!(SystemIntegrityState::Critical.blocks_financial_operations());
    assert!(!SystemIntegrityState::Healthy.blocks_financial_operations());
}

#[test]
fn archive_service_marks_closed_year_immutable() {
    let dir = tempdir().unwrap();
    let mut db = ConnectionFactory::new_with_path(&dir.path().join("surv_d.db")).unwrap();
    db.get_connection()
        .execute(
            "INSERT INTO fiscal_year_status (year, status, opened_at, closed_at, archived)
             VALUES (2012, 'closed', '2012-01-01', '2012-12-31', 0)",
            [],
        )
        .unwrap();

    let admin_id: String = db
        .get_connection()
        .query_row(
            "SELECT id FROM users WHERE username = 'admin' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    db.with_transaction(|tx| FiscalYearService::new(tx).archive_year(2012, &admin_id, "admin"))
        .unwrap();

    let archived: i32 = db
        .get_connection()
        .query_row(
            "SELECT archived FROM fiscal_year_status WHERE year = 2012",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(archived, 1);
}
