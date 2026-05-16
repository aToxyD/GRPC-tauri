use grpc_lib::application::services::BackupIntegrityService;
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::backup::BackupPort;
use grpc_lib::infrastructure::backup::recover_interrupted_restore_and_orphans;
use grpc_lib::infrastructure::backup::SqliteBackupAdapter;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_sqlite_header_corruption_startup_failure() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("corrupted.db");

    // Create valid DB
    {
        let _db = ConnectionFactory::new_with_path(&db_path).unwrap();
    }

    // Corrupt header (first 16 bytes)
    let mut data = fs::read(&db_path).unwrap();
    for byte in data.iter_mut().take(16) {
        *byte = 0xFF;
    }
    fs::write(&db_path, &data).unwrap();

    // Attempting to open should fail or fail integrity check
    let db_res = ConnectionFactory::new_with_path(&db_path);
    // ConnectionFactory::new_with_path often succeeds in opening but PRAGMAs might fail later.
    // However, the deployment readiness check should catch it.

    if let Ok(db) = db_res {
        let check = db
            .get_connection()
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0));
        assert!(check.is_err() || check.unwrap() != "ok");
    }
}

#[test]
fn test_truncated_encrypted_backup_restore_failure() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("live.db");
    let _backup_path = dir.path().join("test.bak");

    let db = ConnectionFactory::new_with_path(&db_path).unwrap();
    db.get_connection()
        .execute("CREATE TABLE t (id INT)", [])
        .unwrap();

    let crypto = AgeFileEncryptionProvider::new();
    let adapter = SqliteBackupAdapter::new(&db_path, crypto);

    // Perform backup
    let actual_backup_path = adapter.create_backup().unwrap();

    // Truncate backup
    let mut data = fs::read(&actual_backup_path).unwrap();
    data.truncate(data.len() - 100);
    fs::write(&actual_backup_path, &data).unwrap();

    // Attempt restore
    let res = adapter.restore_backup_atomic(&actual_backup_path);

    assert!(res.is_err());
    let err_msg = res.unwrap_err().to_string().to_lowercase();
    assert!(err_msg.contains("decryption") || err_msg.contains("decrypt"));
}

#[test]
fn test_orphan_temp_restore_cleanup_on_startup() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("live.db");
    let orphan_tmp = db_path.with_extension("restore.tmp");
    let orphan_journal = db_path.with_extension("restore.journal");

    fs::write(&db_path, b"LIVE").unwrap();
    fs::write(&orphan_tmp, b"ORPHAN").unwrap();
    fs::write(&orphan_journal, b"{}").unwrap(); // Invalid journal but should be cleaned up

    let crypto = AgeFileEncryptionProvider::new();
    recover_interrupted_restore_and_orphans(&db_path, &crypto).unwrap();

    assert!(!orphan_tmp.exists());
    assert!(!orphan_journal.exists());
    assert_eq!(fs::read_to_string(&db_path).unwrap(), "LIVE");
}

#[test]
fn test_partial_encrypted_payload_verification() {
    let dir = tempdir().unwrap();
    let bad_payload = dir.path().join("partial.age");
    fs::write(&bad_payload, b"age-encryption.org/v1\n...").unwrap();

    let crypto = AgeFileEncryptionProvider::new();
    let adapter = SqliteBackupAdapter::new(Path::new("none"), crypto);

    // Verification should fail because it's not a valid SQLite file (after attempted decryption or just header check)
    let res = BackupIntegrityService::verify_backup_file(&bad_payload, &adapter).unwrap();
    assert!(!res.valid);
}
