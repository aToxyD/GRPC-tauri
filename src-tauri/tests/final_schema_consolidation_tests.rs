//! SEC-055 — consolidated fresh-install baseline.
//!
//! The application is pre-release and has never been deployed. SEC-049/SEC-051
//! folded the migration history into a single consolidated baseline; SEC-055
//! folds the B4 sync transport ledger (formerly migration 004) and the unified
//! producer transport sequence (formerly migration 011) directly into
//! `001_initial.sql`.
//!
//! This test proves that a genuinely fresh database built through the Rust
//! migration runner (migration 1 only) contains the complete final schema:
//! every retained object present, every intentionally-retired object absent,
//! and the schema_version === 1.

#[allow(dead_code)]
mod common;

use grpc_lib::db::ConnectionFactory;

fn table_exists(conn: &rusqlite::Connection, name: &str) -> bool {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [name],
            |r| r.get(0),
        )
        .expect("table probe");
    n == 1
}

fn table_absent(conn: &rusqlite::Connection, name: &str) {
    assert!(
        !table_exists(conn, name),
        "{name} must NOT exist on a fresh consolidated baseline"
    );
}

fn index_exists(conn: &rusqlite::Connection, name: &str) -> bool {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
            [name],
            |r| r.get(0),
        )
        .expect("index probe");
    n == 1
}

fn column_exists(conn: &rusqlite::Connection, table: &str, column: &str) -> bool {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .expect("pragma");
    let names: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .expect("map")
        .collect::<Result<_, _>>()
        .expect("cols");
    names.iter().any(|c| c == column)
}

#[test]
fn fresh_baseline_is_single_migration_with_complete_schema() {
    let db = ConnectionFactory::new_for_test().expect("db");
    let conn = db.get_connection();

    // The consolidation reduces the chain to a single canonical migration.
    let version: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .expect("schema version");
    assert_eq!(version, 1, "expected_schema_version must be 1");

    // --- Retained transport/security objects (must EXIST) ---
    assert!(table_exists(conn, "sync_issuer_sequence"), "consumer ledger present");
    assert!(table_exists(conn, "transport_export_sequence"), "producer ledger present");
    assert!(table_exists(conn, "applied_sync_packages"), "applied ledger present");
    assert!(table_exists(conn, "identity_store"), "identity store present");
    assert!(column_exists(conn, "identity_store", "signature"), "identity_store.signature present");
    assert!(table_exists(conn, "audit_summary"), "audit_summary present");
    assert!(table_exists(conn, "import_reproducibility_metadata"), "import_reproducibility_metadata present");
    assert!(table_exists(conn, "domain_events"), "domain_events present");

    // B4 transport metadata folded onto the applied ledger.
    assert!(column_exists(conn, "applied_sync_packages", "package_sequence"), "applied.package_sequence present");
    assert!(column_exists(conn, "applied_sync_packages", "issuer_identity_id"), "applied.issuer_identity_id present");

    // --- Intentionally-retired objects (must NOT EXIST) ---
    assert!(
        !column_exists(conn, "identity_store", "package_sequence"),
        "identity_store.package_sequence must NOT exist"
    );
    table_absent(conn, "reference_price_snapshots");
    table_absent(conn, "report_generation_metadata");
    table_absent(conn, "sync_issuer_sequence_state");
    table_absent(conn, "identity_access_export_sequence");
    table_absent(conn, "admin_access_export_sequence");

    // --- Required identity/security indexes ---
    for idx in [
        "idx_identity_active_subject",
        "idx_identity_single_active_admin",
        "idx_identity_credential_generation",
        "idx_identity_issuer",
        "idx_identity_subject_type_status",
    ] {
        assert!(index_exists(conn, idx), "identity index {idx} present");
    }

    // --- Required transport/sync indexes ---
    for idx in [
        "idx_applied_sync_packages_imported_at",
        "idx_import_audit_events_package",
        "idx_import_audit_events_type_time",
        "idx_sync_conflicts_unresolved",
    ] {
        assert!(index_exists(conn, idx), "sync index {idx} present");
    }
}
