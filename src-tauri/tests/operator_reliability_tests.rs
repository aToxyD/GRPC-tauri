//! Operator reliability phase — deployment readiness, maintenance mode, sessions, consistency.

use grpc_lib::application::services::{
    DeploymentReadinessService, DeploymentReadinessStatus, FiscalExportSnapshot,
    FiscalExportSnapshotService, ImportReproducibilityRecord, ImportReproducibilityService,
    MaintenanceBlockedOperation, OperationalConsistencyStatus, OperationalConsistencyVerifier,
    OperationalSessionService, SessionEndReason, SystemMaintenanceHandle, SystemMaintenanceState,
};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::errors::{AppError, BusinessLogicError};
use grpc_lib::infrastructure::backup::SqliteBackupAdapter;
use grpc_lib::infrastructure::identity::NodeKeyStore;

/// Test node key store in a throwaway temp dir (never touches the real data dir).
fn test_node_key_store() -> NodeKeyStore {
    NodeKeyStore::new(std::env::temp_dir().join(format!("grpc_reliability_test_{}", std::process::id())))
}

fn open_test_db() -> grpc_lib::db::Database {
    ConnectionFactory::new_for_test().unwrap()
}

#[test]
fn deployment_readiness_passes_on_fresh_test_db() {
    let db = open_test_db();
    let path = db.get_connection_path().unwrap();

    // Ensure log directory exists for test readiness
    if let Some(log_dir) = grpc_lib::infrastructure::logging::resolve_log_dir() {
        let _ = std::fs::create_dir_all(&log_dir);
    }

    // Ensure backup directory exists for test readiness
    let backup_dir = SqliteBackupAdapter::compute_backup_dir(&path);
    let _ = std::fs::create_dir_all(&backup_dir);

    let report = DeploymentReadinessService::new(
        db.executor(),
        path.clone(),
        &db,
        &test_node_key_store(),
    )
    .verify()
    .unwrap();
    assert_eq!(
        report.status,
        DeploymentReadinessStatus::Ready,
        "Deployment readiness failed: {:?}",
        report.blocking_failures
    );
    assert!(report.blocking_failures.is_empty());
}

#[test]
fn maintenance_mode_blocks_imports() {
    let handle = SystemMaintenanceHandle::new(SystemMaintenanceState::RestoreInProgress);
    let err = handle
        .assert_allows(MaintenanceBlockedOperation::Import)
        .unwrap_err();
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::MaintenanceModeBlocked { .. })
    ));
}

#[test]
fn operational_session_lifecycle_and_unexpected_termination() {
    let db = open_test_db();
    let svc = OperationalSessionService::new(db.executor());

    svc.begin_session("sess-1", "u1", "admin").unwrap();
    assert!(svc
        .get_session("sess-1")
        .unwrap()
        .unwrap()
        .ended_at
        .is_none());

    svc.end_session("sess-1", SessionEndReason::Logout).unwrap();
    let closed = svc.get_session("sess-1").unwrap().unwrap();
    assert_eq!(closed.end_reason.as_deref(), Some("LOGOUT"));

    svc.begin_session("sess-2", "u1", "admin").unwrap();
    let recovered = svc.recover_abandoned_sessions().unwrap();
    assert_eq!(recovered, 1);
    let abandoned = svc.get_session("sess-2").unwrap().unwrap();
    assert_eq!(
        abandoned.end_reason.as_deref(),
        Some("UNEXPECTED_TERMINATION")
    );
}

#[test]
fn import_reproducibility_metadata_append_only() {
    let db = open_test_db();
    let record = ImportReproducibilityRecord {
        package_id: "pkg-001".to_string(),
        package_kind: "products".to_string(),
        imported_at: chrono::Utc::now().to_rfc3339(),
        imported_by: "admin".to_string(),
        validation_state: "VALIDATED".to_string(),
        source_integrity_state: Some("Healthy".to_string()),
        rejected_records_count: 0,
    };
    ImportReproducibilityService::new(db.executor())
        .record_import_metadata(&record)
        .unwrap();

    let count: i64 = db
        .executor()
        .query_row(
            "SELECT COUNT(*) FROM import_reproducibility_metadata WHERE package_id = 'pkg-001'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn export_snapshot_stores_reproducibility_columns() {
    let db = open_test_db();
    let snap = FiscalExportSnapshot {
        export_hash: "hash-repro".to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        generated_by: "admin".to_string(),
        fiscal_year: db
            .executor()
            .query_row("SELECT current_year FROM settings WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap(),
        movement_count: 0,
        report_count: 0,
        inventory_total_value: 0.0,
        integrity_state: Some("Healthy".to_string()),
        archived_years_count: Some(0),
        active_anomalies_count: Some(0),
        signing_key_id: Some("test-key".to_string()),
        export_reason: Some("unit_test".to_string()),
    };
    FiscalExportSnapshotService::new(db.executor())
        .record_export_snapshot(&snap)
        .unwrap();

    let reason: String = db
        .executor()
        .query_row(
            "SELECT export_reason FROM fiscal_export_snapshots WHERE export_hash = 'hash-repro'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reason, "unit_test");
}

#[test]
fn operational_consistency_verifier_runs_read_only() {
    let db = open_test_db();
    let report = OperationalConsistencyVerifier::new(db.executor())
        .verify()
        .unwrap();
    assert!(matches!(
        report.status,
        OperationalConsistencyStatus::Consistent | OperationalConsistencyStatus::DriftDetected
    ));
}

#[test]
fn restore_journal_leftover_blocks_readiness() {
    let db = open_test_db();
    let path = db.get_connection_path().unwrap();
    let journal = path.with_extension("restore.journal");
    std::fs::write(&journal, b"{}").unwrap();

    let report = DeploymentReadinessService::new(
        db.executor(),
        path.clone(),
        &db,
        &test_node_key_store(),
    )
    .verify()
    .unwrap();
    assert_eq!(report.status, DeploymentReadinessStatus::NotReady);
    assert!(report
        .blocking_failures
        .iter()
        .any(|f| f.contains("I_restore_journal")));

    let _ = std::fs::remove_file(journal);
}

#[test]
fn backup_directory_probe_matches_adapter_layout() {
    let db = open_test_db();
    let db_path = db.get_connection_path().unwrap();
    let backup_dir = SqliteBackupAdapter::compute_backup_dir(&db_path);
    std::fs::create_dir_all(&backup_dir).unwrap();
    let report = DeploymentReadinessService::new(
        db.executor(),
        db_path,
        &db,
        &test_node_key_store(),
    )
    .verify()
    .unwrap();
    let backup_check = report
        .checks
        .iter()
        .find(|c| c.id == "B_backup_directory")
        .expect("backup check present");
    assert!(backup_check.passed);
}
