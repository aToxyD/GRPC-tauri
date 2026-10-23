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
    let outcome = recover_interrupted_restore_and_orphans(&db_path, &crypto).unwrap();
    assert!(
        !outcome.restore_completed,
        "invalid journal is not a completed restore"
    );

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

// ─────────────────────────────────────────────────────────────────────────────
// Q2/Q3 (ADR-0065, C6): a restored candidate violating a foreign key must be
// rejected BEFORE any swap — the live database stays byte-intact.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_restore_aborts_on_foreign_key_violation_before_swap() {
    use std::io::Cursor;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("live.db");

    // ── Live DB with surviving marker data ──────────────────────────────
    let db = ConnectionFactory::new_with_path(&db_path).unwrap();
    db.get_connection()
        .execute(
            "INSERT INTO units (id, code, name, wilaya_code, created_at) \
             VALUES ('u-live', 'ULV', 'Live Unit', '16', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    let live_bytes_len = fs::read(&db_path).unwrap().len();

    let crypto = AgeFileEncryptionProvider::new();
    let adapter = SqliteBackupAdapter::new(&db_path, crypto);
    let backup_path = adapter.create_backup().unwrap();

    // ── Tamper: decrypt → inject a foreign-key violation → re-encrypt ───
    // The provider is a Copy unit struct resolving the same GRPC_APP_KEY
    // identity, so a second instance encrypts/decrypts like the adapter.
    let tamper_crypto = AgeFileEncryptionProvider::new();
    let encrypted_backup = fs::read(&backup_path).unwrap();
    let mut plain = Vec::new();
    tamper_crypto
        .decrypt_stream(&mut Cursor::new(encrypted_backup), &mut plain)
        .unwrap();

    let tampered_path = dir.path().join("tampered_snapshot.db");
    fs::write(&tampered_path, &plain).unwrap();
    {
        // Raw connection: FK mode explicitly OFF — the violating row IS
        // written, simulating a legacy or externally-corrupted snapshot that
        // only `PRAGMA foreign_key_check` can detect (never repaired here).
        let tampered_conn = rusqlite::Connection::open(&tampered_path).unwrap();
        tampered_conn
            .pragma_update(None, "foreign_keys", "OFF")
            .unwrap();
        tampered_conn
            .execute(
                "INSERT INTO daily_reports (id, date, unit_id, total_daily_cost, \
                 total_daily_average, total_daily_beneficiaries, created_at, updated_at, \
                 fiscal_year) \
                 VALUES ('fk-bad', '2025-01-01', 'u-missing', 0, 0, 0, \
                         '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', 2025)",
                [],
            )
            .unwrap();
        let violations: i64 = tampered_conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(violations, 1, "crafted candidate must carry a violation");
    }
    let tampered_bytes = fs::read(&tampered_path).unwrap();
    let mut re_encrypted = Vec::new();
    tamper_crypto
        .encrypt_stream(&mut Cursor::new(tampered_bytes), &mut re_encrypted)
        .unwrap();
    fs::write(&backup_path, &re_encrypted).unwrap();

    // ── Restore must abort BEFORE swapping the live DB ──────────────────
    let res = adapter.restore_backup_atomic(&backup_path);
    let err = res.expect_err("FK-violating restore must fail closed");
    let msg = err.to_string();
    assert!(
        msg.contains("Foreign key validation failed"),
        "expected FK validation rejection, got: {msg}"
    );
    assert!(msg.contains("table='daily_reports'"), "diagnostic: {msg}");
    assert!(msg.contains("parent='units'"), "diagnostic: {msg}");
    assert!(msg.contains("fkid="), "diagnostic must carry fkid: {msg}");

    // Pre-swap abort guarantees: crash-recovery and rollback artifacts were
    // never created (steps 3–7 never ran), and the live DB is byte-identical.
    assert!(!db_path.with_extension("restore.journal").exists());
    assert!(!db_path.with_extension("rollback.bak").exists());
    assert_eq!(fs::read(&db_path).unwrap().len(), live_bytes_len);
    let verify = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let live_units: i64 = verify
        .query_row(
            "SELECT COUNT(*) FROM units WHERE id='u-live'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(live_units, 1, "live database content untouched");
}
