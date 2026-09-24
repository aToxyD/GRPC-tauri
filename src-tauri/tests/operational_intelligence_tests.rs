//! Operational Intelligence — Long-Term Integration Tests
//!
//! Covers the Controlled Operational Intelligence Phase end-to-end:
//!   Case A — anomaly detection identifies abnormal stock outbound spike
//!   Case B — timeline reconstruction contains fiscal close + archive events
//!   Case C — recommendation engine produces deterministic recommendations
//!   Case D — snapshot creation preserves integrity state
//!   Case E — typed confirmation rejects invalid confirmation
//!   Case F — historical imports appear in operational timeline
//!   Case G — backup validation findings appear in diagnostics
//!
//! All tests use the isolated AppState from common/mod.rs.

mod common;

use chrono::{Datelike, Duration, Utc};
use grpc_lib::application::services::{
    CriticalOperation, FiscalOperationalSnapshotService, FiscalTimelineQuery,
    FiscalTimelineService, IntegrityAttemptRecorder, OperationalAnomalyService,
    OperationalRecommendationService, OperatorSafetyService, TimelineEventKind,
    VerificationOutcome, VerificationType,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn seed_product(executor: grpc_lib::repositories::DbExecutor<'_>, id: &str, price: f64) {
    let year = Utc::now().year();
    executor
        .execute(
            "INSERT INTO products (id, name, base_price, year, created_at, updated_at, purchase_unit, consumption_unit, conversion_factor, tva_classification)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, 1, 1, 1, 0)",
            rusqlite::params![
                id,
                format!("Prod {}", id),
                price,
                year,
                Utc::now().to_rfc3339()
            ],
        )
        .unwrap();
    executor
        .execute(
            "INSERT INTO inventory_stocks (id, product_id, quantity, unit, consumption_unit, last_updated)
             VALUES (?1, ?2, 100.0, 'kg', 1, ?3)",
            rusqlite::params![format!("stock-{}", id), id, Utc::now().to_rfc3339()],
        )
        .unwrap();
}

fn insert_movement(
    executor: grpc_lib::repositories::DbExecutor<'_>,
    product_id: &str,
    movement_type: &str,
    quantity: f64,
    timestamp: chrono::DateTime<Utc>,
) {
    let mid = uuid::Uuid::new_v4().to_string();
    executor
        .execute(
            "INSERT INTO stock_movements
                (id, product_id, movement_type, quantity, balance_before, balance_after,
                 reference_type, reference_id, notes, user_id, username, timestamp, fiscal_year, unit_id)
             VALUES (?1, ?2, ?3, ?4, 0.0, 0.0, NULL, NULL, NULL, 'system', 'system', ?5, ?6, NULL)",
            rusqlite::params![
                mid,
                product_id,
                movement_type,
                quantity,
                timestamp.to_rfc3339(),
                timestamp.year()
            ],
        )
        .unwrap();
}

fn insert_audit(
    executor: grpc_lib::repositories::DbExecutor<'_>,
    action: &str,
    entity_id: Option<&str>,
    new_value: Option<&str>,
    timestamp: chrono::DateTime<Utc>,
) {
    let id = uuid::Uuid::new_v4().to_string();
    executor
        .execute(
            "INSERT INTO audit_log
                (id, user_id, username, action, entity_type, entity_id, entity_name,
                 old_value, new_value, session_id, timestamp, status, error_message, metadata,
                 previous_hash, entry_hash)
             VALUES (?1, 'system', 'system', ?2, 'System', ?3, NULL, NULL, ?4, NULL, ?5,
                     'Success', NULL, NULL, '', '')",
            rusqlite::params![id, action, entity_id, new_value, timestamp.to_rfc3339()],
        )
        .unwrap();
}

// ─── Case A — Outbound spike anomaly ─────────────────────────────────────────

#[test]
fn case_a_anomaly_detection_identifies_abnormal_stock_spike() {
    let (state, _temp) = common::create_test_state();
    let mut guard = state.db.lock().unwrap();
    let db = guard.as_mut().unwrap();

    db.with_transaction(|tx| {
        seed_product(tx, "p-spike", 10.0);

        // Baseline: 3 months × ~5 small outbound events (avg ~5/month).
        let now = Utc::now();
        for i in 1..=15 {
            let ts = now - Duration::days(40 + i);
            insert_movement(tx, "p-spike", "OUT", 1.0, ts);
        }

        // Current month: huge spike — 200 units (way above 3× avg).
        for i in 0..10 {
            let ts = now - Duration::days(i);
            insert_movement(tx, "p-spike", "OUT", 20.0, ts);
        }
        Ok::<_, grpc_lib::errors::AppError>(())
    })
    .unwrap();

    let report = db
        .with_transaction(|tx| OperationalAnomalyService::new(tx).run_operational_analysis())
        .expect("analysis should succeed");

    let has_spike = report
        .findings
        .iter()
        .any(|f| f.code == "ANOMALY_A_OUTBOUND_SPIKE" && f.message.contains("p-spike"));
    assert!(
        has_spike,
        "Expected ANOMALY_A_OUTBOUND_SPIKE for p-spike, got findings: {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
}

// ─── Case B — Timeline contains fiscal close + archive ───────────────────────

#[test]
fn case_b_timeline_reconstruction_contains_fiscal_close_and_archive_events() {
    let (state, _temp) = common::create_test_state();
    let mut guard = state.db.lock().unwrap();
    let db = guard.as_mut().unwrap();
    let current_year = Utc::now().year();

    db.with_transaction(|tx| {
        // Synthesize a fiscal close audit entry.
        insert_audit(
            tx,
            "FiscalYearClosed",
            Some(&current_year.to_string()),
            Some(&format!(
                "{{\"closed_year\":{},\"opened_year\":{}}}",
                current_year,
                current_year + 1
            )),
            Utc::now() - Duration::hours(2),
        );

        // Mark a year as archived in fiscal_year_status.
        tx.execute(
            "INSERT INTO fiscal_year_status (year, status, opened_at, closed_at, closed_by, archived)
             VALUES (?1, 'closed', ?2, ?2, 'admin', 1)",
            rusqlite::params![current_year - 1, Utc::now().to_rfc3339()],
        )?;
        Ok::<_, grpc_lib::errors::AppError>(())
    })
    .unwrap();

    let timeline = FiscalTimelineService::new(db.executor())
        .build_timeline(&FiscalTimelineQuery::default())
        .unwrap();

    assert!(
        timeline
            .iter()
            .any(|e| matches!(e.kind, TimelineEventKind::FiscalClose)),
        "Timeline should contain FISCAL_CLOSE event"
    );
    assert!(
        timeline
            .iter()
            .any(|e| matches!(e.kind, TimelineEventKind::Archival)),
        "Timeline should contain ARCHIVAL event"
    );
}

// ─── Case C — Deterministic recommendations ──────────────────────────────────

#[test]
fn case_c_recommendation_engine_produces_deterministic_recommendation() {
    let (state, _temp) = common::create_test_state();
    let mut guard = state.db.lock().unwrap();
    let db = guard.as_mut().unwrap();

    // Insert 2 failed integrity attempts → guarantees REC_INTEGRITY_FAILURES.
    db.with_transaction(|tx| {
        let recorder = IntegrityAttemptRecorder::new(tx);
        recorder.record(
            VerificationType::Inventory,
            VerificationOutcome::Fail,
            Some("synthetic-1"),
        );
        recorder.record(
            VerificationType::FiscalDrift,
            VerificationOutcome::Fail,
            Some("synthetic-2"),
        );
        Ok::<_, grpc_lib::errors::AppError>(())
    })
    .unwrap();

    let svc1 = OperationalRecommendationService::new(db.executor());
    let r1 = svc1.build_recommendations().unwrap();

    let svc2 = OperationalRecommendationService::new(db.executor());
    let r2 = svc2.build_recommendations().unwrap();

    // Determinism: same inputs → same output codes/order.
    let codes1: Vec<_> = r1.iter().map(|r| r.code.clone()).collect();
    let codes2: Vec<_> = r2.iter().map(|r| r.code.clone()).collect();
    assert_eq!(codes1, codes2, "Recommendations must be deterministic");

    assert!(
        r1.iter().any(|r| r.code == "REC_INTEGRITY_FAILURES"),
        "Expected REC_INTEGRITY_FAILURES, got: {:?}",
        codes1
    );
}

// ─── Case D — Snapshot preserves integrity state ─────────────────────────────

#[test]
fn case_d_snapshot_creation_preserves_integrity_state() {
    let (state, _temp) = common::create_test_state();
    let mut guard = state.db.lock().unwrap();
    let db = guard.as_mut().unwrap();
    let current_year = Utc::now().year();

    // Healthy snapshot first.
    let id1 = db
        .with_transaction(|tx| {
            FiscalOperationalSnapshotService::new(tx).create_snapshot(current_year, "admin")
        })
        .unwrap();
    assert!(id1 > 0);

    // Inject a fiscal drift condition: orphan opening-balance snapshot.
    db.executor()
        .execute(
            "INSERT INTO opening_balance_snapshots
                (id, product_id, fiscal_year, opening_quantity, unit_cost, total_value,
                 snapshot_reason, carried_from_year, created_by, created_at)
             VALUES (?1, 'ghost', 9999, 1.0, 0.0, 0.0, 'manual_test', NULL, 'admin', ?2)",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), Utc::now().to_rfc3339()],
        )
        .unwrap();

    let id2 = db
        .with_transaction(|tx| {
            FiscalOperationalSnapshotService::new(tx).create_snapshot(current_year, "admin")
        })
        .unwrap();
    assert!(id2 > id1);

    // List and verify integrity_state evolved from OK → WARNINGS.
    let svc = FiscalOperationalSnapshotService::new(db.executor());
    let snapshots = svc.list_snapshots(Some(current_year), 10).unwrap();
    assert!(snapshots.len() >= 2);

    // Newest first
    let newest = &snapshots[0];
    let previous = &snapshots[1];
    assert_eq!(newest.integrity_state, "WARNINGS");
    assert_eq!(previous.integrity_state, "OK");
}

// ─── Case E — Typed confirmation rejects invalid input ───────────────────────

#[test]
fn case_e_typed_confirmation_rejects_invalid_confirmation() {
    // Empty input
    let empty = OperatorSafetyService::require_confirmation(
        CriticalOperation::FiscalClose { year: 2026 },
        "",
    );
    assert!(empty.is_err(), "Empty confirmation must be rejected");

    // Wrong text
    let wrong = OperatorSafetyService::require_confirmation(
        CriticalOperation::FiscalClose { year: 2026 },
        "2025",
    );
    assert!(wrong.is_err(), "Mismatched year must be rejected");

    // Case mismatch
    let case_mismatch =
        OperatorSafetyService::require_confirmation(CriticalOperation::ArchiveYear, "archive");
    assert!(case_mismatch.is_err(), "Lowercase token must be rejected");

    // Correct
    let ok_close = OperatorSafetyService::require_confirmation(
        CriticalOperation::FiscalClose { year: 2026 },
        "  2026  ",
    );
    assert!(ok_close.is_ok(), "Trimmed exact match must be accepted");

    let ok_archive =
        OperatorSafetyService::require_confirmation(CriticalOperation::ArchiveYear, "ARCHIVE");
    assert!(ok_archive.is_ok());

    let ok_restore =
        OperatorSafetyService::require_confirmation(CriticalOperation::RestoreBackup, "RESTORE");
    assert!(ok_restore.is_ok());

    let ok_hist = OperatorSafetyService::require_confirmation(
        CriticalOperation::ImportHistoricalPackage,
        "IMPORT-HISTORICAL",
    );
    assert!(ok_hist.is_ok());
}

// ─── Case F — Historical imports appear in timeline ──────────────────────────

#[test]
fn case_f_historical_imports_appear_in_operational_timeline() {
    let (state, _temp) = common::create_test_state();
    let mut guard = state.db.lock().unwrap();
    let db = guard.as_mut().unwrap();

    // Synthesize a historical import (audit_log + import_audit_events).
    db.with_transaction(|tx| {
        insert_audit(
            tx,
            "ImportDailyReportPackage",
            Some("pkg-001"),
            Some("{\"fiscal_year\":2024}"),
            Utc::now() - Duration::days(15),
        );
        // Also insert into import_audit_events if the schema supports it.
        let _ = tx.execute(
            "INSERT INTO import_audit_events
                (event_type, package_id, package_kind, source_node_id, reason_code, occurred_at)
             VALUES ('APPLIED', 'pkg-001', 'DAILY_REPORT', 'unit-a', NULL, ?1)",
            rusqlite::params![(Utc::now() - Duration::days(15)).to_rfc3339()],
        );
        Ok::<_, grpc_lib::errors::AppError>(())
    })
    .unwrap();

    let timeline = FiscalTimelineService::new(db.executor())
        .build_timeline(&FiscalTimelineQuery::default())
        .unwrap();

    let import_events: Vec<_> = timeline
        .iter()
        .filter(|e| matches!(e.kind, TimelineEventKind::Import))
        .collect();

    assert!(
        !import_events.is_empty(),
        "Timeline should contain at least one IMPORT event after historical import was registered"
    );
}

// ─── Case G — Backup validation findings appear in diagnostics ───────────────

#[test]
fn case_g_backup_validation_findings_appear_in_diagnostics() {
    let (state, _temp) = common::create_test_state();
    let mut guard = state.db.lock().unwrap();
    let db = guard.as_mut().unwrap();

    // No backup recorded at all → recommendation REC_NO_BACKUP must appear.
    let recs_before = OperationalRecommendationService::new(db.executor())
        .build_recommendations()
        .unwrap();
    assert!(
        recs_before.iter().any(|r| r.code == "REC_NO_BACKUP"),
        "Expected REC_NO_BACKUP when no backup audit entries exist, got: {:?}",
        recs_before.iter().map(|r| &r.code).collect::<Vec<_>>()
    );

    // Now insert a STALE backup (35 days old) → REC_BACKUP_VERY_STALE.
    db.with_transaction(|tx| {
        insert_audit(
            tx,
            "CreateBackup",
            Some("bk-1"),
            None,
            Utc::now() - Duration::days(35),
        );
        // Also record a backup integrity failure event.
        IntegrityAttemptRecorder::new(tx).record(
            VerificationType::BackupFile,
            VerificationOutcome::Fail,
            Some("simulated bad backup"),
        );
        Ok::<_, grpc_lib::errors::AppError>(())
    })
    .unwrap();

    let recs_after = OperationalRecommendationService::new(db.executor())
        .build_recommendations()
        .unwrap();
    assert!(
        recs_after.iter().any(|r| r.code == "REC_BACKUP_VERY_STALE"),
        "Expected REC_BACKUP_VERY_STALE for 35-day-old backup, got: {:?}",
        recs_after.iter().map(|r| &r.code).collect::<Vec<_>>()
    );

    // Backup integrity failure shows up in the timeline as IntegrityFailure.
    let timeline = FiscalTimelineService::new(db.executor())
        .build_timeline(&FiscalTimelineQuery::default())
        .unwrap();
    assert!(
        timeline
            .iter()
            .any(|e| matches!(e.kind, TimelineEventKind::IntegrityFailure)
                && e.summary.contains("BACKUP_FILE")),
        "Timeline should surface backup integrity failure"
    );
}
