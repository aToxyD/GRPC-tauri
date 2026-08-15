// Infrastructure Layer (EXCEPTION)
// ALLOWED: PRAGMA & MIGRATIONS are infrastructure-level SQL

use crate::db::get_connection_path;
use crate::domain::ports::backup::BackupPort;
use crate::infrastructure::backup::SqliteBackupAdapter;
use crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use rusqlite::{params, Connection};

fn migrations_vec() -> Vec<(i32, &'static str, &'static str)> {
    vec![
        (
            1,
            "Initial schema creation",
            include_str!("migrations/001_initial.sql"),
        ),
        (
            2,
            "Identity Store (RFC 2026-08-04-node-identity-trust, ADR-0038)",
            include_str!("migrations/002_identity_store.sql"),
        ),
        (
            3,
            "Identity certificate signature (ADR-0039 §6)",
            include_str!("migrations/003_certificate_signature.sql"),
        ),
        (
            4,
            "Sync transport ordering ledger (RFC 2026-08-04 §3.4.1, ADR-0038, B4)",
            include_str!("migrations/004_sync_issuer_sequence.sql"),
        ),
        (
            5,
            "Registry fleet-state snapshots (RFC 2026-08-04 §3.9, B4)",
            include_str!("migrations/005_registry_snapshots.sql"),
        ),
        (
            6,
            "Producer sequence state ledger (RFC 2026-08-04 §3.4.1, ADR-0038, B6-B Commit ④)",
            include_str!("migrations/006_issuer_sequence_state.sql"),
        ),
        (
            7,
            "Licensing consumer derived view (ADR-0042, Phase B)",
            include_str!("migrations/007_licensing.sql"),
        ),
        (
            8,
            "Single ACTIVE ADMIN invariant (SEC-002)",
            include_str!("migrations/008_single_active_admin.sql"),
        ),
        (
            9,
            "identity_access per-target producer sequence stream (RFC 2026-08-04 §3.4.1 amendment, ADR-0045 §26.9 F-1 Option A)",
            include_str!("migrations/009_identity_access_export_sequence.sql"),
        ),
    ]
}

/// Latest schema version expected after all embedded migrations have run.
pub fn expected_schema_version() -> i32 {
    migrations_vec()
        .iter()
        .map(|(v, _, _)| *v)
        .max()
        .unwrap_or(0)
}

fn maybe_pre_migration_backup(conn: &Connection) -> Result<(), String> {
    let current_version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to read current schema version: {}", e))?;

    let migrations = migrations_vec();
    let latest: i32 = migrations.iter().map(|(v, _, _)| *v).max().unwrap_or(0);
    let pending = migrations
        .iter()
        .any(|(version, _, _)| current_version < *version);

    if !pending || current_version >= latest {
        log::info!(
            target: "grpc::migrations",
            "database migration: skip pre-migration backup (current_version={}, latest={}, pending={})",
            current_version,
            latest,
            pending
        );
        return Ok(());
    }

    log::info!(
        target: "grpc::migrations",
        "database migration: start (current_version={}, latest={})",
        current_version,
        latest
    );
    log::info!(
        target: "grpc::migrations",
        "database migration: pre-migration backup start"
    );

    let db_path = get_connection_path(conn)
        .map_err(|e| format!("Failed to resolve database path for backup: {}", e))?;
    let crypto = AgeFileEncryptionProvider::new();
    let adapter = SqliteBackupAdapter::new(&db_path, crypto);
    match adapter.create_backup_from_conn(conn) {
        Ok(path) => {
            log::info!(
                target: "grpc::migrations",
                "database migration: pre-migration backup success path={}",
                path.display()
            );
            Ok(())
        }
        Err(e) => {
            let msg = format!("Pre-migration backup failed: {}", e);
            log::error!(target: "grpc::migrations", "{}", msg);
            Err(msg)
        }
    }
}

/// Run database migrations to ensure schema is up to date
pub fn run_migrations(conn: &Connection) -> Result<(), String> {
    // Create schema_version table first (for migration tracking)
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            description TEXT
        );
    "#,
    )
    .map_err(|e| format!("Failed to create schema_version table: {}", e))?;

    // Get current schema version (0 if no migrations applied yet)
    let mut current_version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to read current schema version: {}", e))?;

    let migrations = migrations_vec();

    maybe_pre_migration_backup(conn)?;

    // Apply missing migrations inside a transaction
    for (version, description, sql) in &migrations {
        if current_version < *version {
            log::info!(
                target: "grpc::migrations",
                "database migration: applying version {} ({})",
                version,
                description
            );
            let tx = conn.unchecked_transaction().map_err(|e| {
                let msg = format!(
                    "Failed to start transaction for migration {}: {}",
                    version, e
                );
                log::error!(target: "grpc::migrations", "{}", msg);
                msg
            })?;

            if let Err(e) = tx.execute_batch(sql) {
                let msg = format!("Migration {} failed: {}", version, e);
                log::error!(target: "grpc::migrations", "{}", msg);
                return Err(msg);
            }

            if let Err(e) = tx.execute(
                "INSERT INTO schema_version (version, description) VALUES (?1, ?2)",
                params![version, description],
            ) {
                let msg = format!("Failed to record migration {}: {}", version, e);
                log::error!(target: "grpc::migrations", "{}", msg);
                return Err(msg);
            }

            if let Err(e) = tx.commit() {
                let msg = format!("Failed to commit migration {}: {}", version, e);
                log::error!(target: "grpc::migrations", "{}", msg);
                return Err(msg);
            }

            log::info!(
                target: "grpc::migrations",
                "database migration: version {} applied successfully",
                version
            );
            current_version = *version;
        }
    }

    log::info!(
        target: "grpc::migrations",
        "database migration: complete (schema_version={})",
        current_version
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::tempdir;

    fn empty_db_file() -> (tempfile::TempDir, std::path::PathBuf) {
        let d = tempdir().unwrap();
        let p = d.path().join("mig.db");
        (d, p)
    }

    #[test]
    fn migration_no_backup_when_schema_already_at_latest() {
        let (_d, p) = empty_db_file();
        let conn = Connection::open(&p).unwrap();
        run_migrations(&conn).unwrap();
        let v: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(v >= 1);
        // Second run: no pending migrations — must not error (backup skipped)
        run_migrations(&conn).unwrap();
    }

    /// Cross-platform: verify that a backup failure prevents migration progress.
    #[test]
    fn migration_backup_failure_blocks_pending_migrations() {
        use std::fs;

        let dir = tempdir().unwrap();
        let db_path = dir.path().join("app.db");

        // 1. Manually initialize to version 0 (empty schema_version)
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE schema_version (
                    version INTEGER PRIMARY KEY,
                    applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    description TEXT
                );
                INSERT INTO schema_version (version, description) VALUES (0, 'Initial state');
            "#,
            )
            .unwrap();
        }

        // 2. Force backup failure by making the directory read-only (Unix)
        // or by creating a file where the 'backups' directory should be (Cross-platform)
        let backups_path = dir.path().join("backups");
        fs::write(&backups_path, "blocking-file").unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let err = run_migrations(&conn)
            .expect_err("Migration should fail because backup cannot be created");

        assert!(
            err.contains("backup failed") || err.contains("backup"),
            "Expected backup failure error, got: {}",
            err
        );

        // 3. Verify version didn't advance
        let v: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            v, 0,
            "Migration must not advance when pre-migration backup fails"
        );
    }
}
