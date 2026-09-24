//! SEC-087 Phase 2 — C2: export reproducibility persistence.
//!
//! Focused persistence-layer tests proving that `export_mode` and
//! `target_node_id` are stored and retrieved without loss or transformation.
//! These are persistence-only tests: no exporter wiring, no selection logic.

use grpc_lib::db::ConnectionFactory;
use grpc_lib::repositories::fiscal_snapshots::FiscalExportSnapshot;
use grpc_lib::repositories::RepositoryProvider;

fn open_test_db() -> grpc_lib::db::Database {
    ConnectionFactory::new_for_test().unwrap()
}

fn base_snapshot(export_hash: &str) -> FiscalExportSnapshot {
    FiscalExportSnapshot {
        export_hash: export_hash.to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        generated_by: "admin".to_string(),
        fiscal_year: 2024,
        movement_count: 12,
        report_count: 3,
        inventory_total_value: 4567.89,
        integrity_state: Some("Healthy".to_string()),
        archived_years_count: Some(2),
        active_anomalies_count: Some(0),
        signing_key_id: Some("test-key".to_string()),
        export_reason: Some("unit_test".to_string()),
        export_mode: None,
        target_node_id: None,
    }
}

#[test]
fn fleet_restore_mode_round_trips_with_null_target() {
    let db = open_test_db();
    let mut snap = base_snapshot("rt-fleet-restore");
    snap.export_mode = Some("fleet_restore".to_string());

    let repo = db.executor().fiscal_snapshots();
    repo.record_export_snapshot(&snap).unwrap();

    let read_back = repo
        .get_export_snapshot_by_hash("rt-fleet-restore")
        .unwrap();
    let read_back = read_back.expect("snapshot must be readable");
    assert_eq!(read_back.export_mode.as_deref(), Some("fleet_restore"));
    assert_eq!(read_back.target_node_id.as_deref(), None);
}

#[test]
fn unit_distribution_mode_round_trips_with_exact_target_code() {
    let db = open_test_db();
    let mut snap = base_snapshot("rt-unit-dist");
    snap.export_mode = Some("unit_distribution".to_string());
    snap.target_node_id = Some("UNIT-A".to_string());

    let repo = db.executor().fiscal_snapshots();
    repo.record_export_snapshot(&snap).unwrap();

    let read_back = repo.get_export_snapshot_by_hash("rt-unit-dist").unwrap();
    let read_back = read_back.expect("snapshot must be readable");
    assert_eq!(read_back.export_mode.as_deref(), Some("unit_distribution"));
    assert_eq!(read_back.target_node_id.as_deref(), Some("UNIT-A"));
}

#[test]
fn target_node_id_is_preserved_byte_for_byte() {
    let db = open_test_db();
    let mut snap = base_snapshot("rt-exact-code");
    snap.export_mode = Some("unit_distribution".to_string());
    snap.target_node_id = Some(" UNIT-A ".to_string());

    let repo = db.executor().fiscal_snapshots();
    repo.record_export_snapshot(&snap).unwrap();

    let read_back = repo.get_export_snapshot_by_hash("rt-exact-code").unwrap();
    let read_back = read_back.expect("snapshot must be readable");
    assert_eq!(read_back.target_node_id.as_deref(), Some(" UNIT-A "));
}

#[test]
fn null_mode_and_target_are_preserved_as_null() {
    let db = open_test_db();
    let snap = base_snapshot("rt-null");

    let repo = db.executor().fiscal_snapshots();
    repo.record_export_snapshot(&snap).unwrap();

    let read_back = repo.get_export_snapshot_by_hash("rt-null").unwrap();
    let read_back = read_back.expect("snapshot must be readable");
    assert_eq!(read_back.export_mode.as_deref(), None);
    assert_eq!(read_back.target_node_id.as_deref(), None);

    let mode: Option<String> = db
        .executor()
        .query_row(
            "SELECT export_mode FROM fiscal_export_snapshots WHERE export_hash = 'rt-null'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let target: Option<String> = db
        .executor()
        .query_row(
            "SELECT target_node_id FROM fiscal_export_snapshots WHERE export_hash = 'rt-null'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(mode, None);
    assert_eq!(target, None);
}

#[test]
fn unknown_future_export_mode_string_is_not_rewritten() {
    let db = open_test_db();
    let mut snap = base_snapshot("rt-future-mode");
    snap.export_mode = Some("unit_distribution_v2".to_string());

    let repo = db.executor().fiscal_snapshots();
    repo.record_export_snapshot(&snap).unwrap();

    let read_back = repo.get_export_snapshot_by_hash("rt-future-mode").unwrap();
    let read_back = read_back.expect("snapshot must be readable");
    assert_eq!(
        read_back.export_mode.as_deref(),
        Some("unit_distribution_v2")
    );
}

#[test]
fn legacy_snapshot_without_new_columns_remains_readable() {
    let db = open_test_db();
    db.executor()
        .execute(
            "INSERT INTO fiscal_export_snapshots
                (export_hash, generated_at, generated_by, fiscal_year,
                 movement_count, report_count, inventory_total_value,
                 integrity_state, signing_key_id, export_reason)
             VALUES ('legacy-1', '2024-01-01T00:00:00Z', 'admin', 2024,
                     0, 0, 0, 'Healthy', 'old-key', 'legacy')",
            [],
        )
        .unwrap();

    let read_back = db
        .executor()
        .fiscal_snapshots()
        .get_export_snapshot_by_hash("legacy-1")
        .unwrap();
    let read_back = read_back.expect("legacy snapshot must remain readable");
    assert_eq!(read_back.export_hash, "legacy-1");
    assert_eq!(read_back.export_mode.as_deref(), None);
    assert_eq!(read_back.target_node_id.as_deref(), None);
    assert_eq!(read_back.export_reason.as_deref(), Some("legacy"));
}
