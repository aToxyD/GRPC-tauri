//! Hostile historical mutation tests — archived years must remain immutable (fail-closed).

use grpc_lib::application::services::{
    FiscalExportSnapshot, FiscalExportSnapshotService, FiscalHistoricalGuard, FiscalYearService,
    ImportSyncService,
};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::ports::backup::BackupPort;
use grpc_lib::errors::{AppError, BusinessLogicError};
use grpc_lib::models::{DailyReport, DailyReportMeal, DailyReportResult, MealSectionResult};
use grpc_lib::repositories::RepositoryProvider;
use tempfile::tempdir;

fn seed_archived_closed_year(db: &grpc_lib::db::Database, year: i32) {
    let ex = db.executor();
    ex.execute("DELETE FROM fiscal_year_status", []).unwrap();
    ex.execute(
        "INSERT INTO fiscal_year_status (year, status, opened_at, closed_at, archived)
         VALUES (?1, 'closed', '2020-01-01', '2020-12-31', 1)",
        [year],
    )
    .unwrap();
    ex.execute(
        "INSERT INTO opening_balance_snapshots
         (id, product_id, fiscal_year, opening_quantity, created_at, created_by)
         VALUES ('obs-1', 'p1', ?1, 1.0, '2020-01-01', 'admin')",
        [year],
    )
    .unwrap();
}

#[test]
fn case_a_rejects_mutation_of_archived_fiscal_year_status() {
    let dir = tempdir().unwrap();
    let db = ConnectionFactory::new_with_path(&dir.path().join("imm_a.db")).unwrap();
    seed_archived_closed_year(&db, 2019);

    let err = db
        .executor()
        .fiscal_year_status()
        .update_status(2019, "open", None, None)
        .unwrap_err();
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::FiscalYearArchived { year: 2019 })
    ));
}

#[test]
fn case_b_rejects_import_records_for_archived_year() {
    let dir = tempdir().unwrap();
    let db = ConnectionFactory::new_with_path(&dir.path().join("imm_b.db")).unwrap();
    seed_archived_closed_year(&db, 2018);
    db.executor()
        .fiscal_year_status()
        .seed_year(2026, "open", "2026-01-01")
        .unwrap();
    db.executor().settings().set_current_year(2026).unwrap();

    let report = DailyReportResult {
        report: DailyReport {
            id: "r1".into(),
            date: chrono::NaiveDate::from_ymd_opt(2018, 6, 1).unwrap(),
            unit_id: None,
            total_daily_cost: 0.0,
            total_daily_average: 0.0,
            total_daily_beneficiaries: 2,
            created_at: chrono::Utc::now(),
            fiscal_year: 2018,
        },
        meals: vec![MealSectionResult {
            meal: DailyReportMeal {
                id: "m1".into(),
                daily_report_id: "r1".into(),
                meal_type: grpc_lib::models::MealType::Breakfast,
                staff_24h_count: 1,
                staff_8h_count: 0,
                reservation_count: 0,
                mission_count: 0,
                guest_count: 1,
                total_beneficiaries: 2,
                total_meal_cost: 0.0,
                meal_average: 0.0,
            },
            items: vec![],
        }],
    };

    let err = ImportSyncService::new(db.executor())
        .import_daily_reports(vec![report])
        .unwrap_err();
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::FiscalYearArchived { year: 2018 })
    ));
}

#[test]
fn case_c_rejects_restore_older_than_live_archived_state() {
    use grpc_lib::infrastructure::backup::SqliteBackupAdapter;
    use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;

    let dir = tempdir().unwrap();
    let live = dir.path().join("live.db");
    let backup_src = dir.path().join("backup_src.db");
    let db = ConnectionFactory::new_with_path(&live).unwrap();
    seed_archived_closed_year(&db, 2021);

    let crypto = AgeFileEncryptionProvider::new();
    let encrypted_backup_path = {
        let _backup_db = ConnectionFactory::new_with_path(&backup_src).unwrap();
        _backup_db
            .get_connection()
            .execute(
                "INSERT INTO fiscal_year_status (year, status, opened_at, archived)
                 VALUES (2019, 'closed', '2019-01-01', 1)",
                [],
            )
            .unwrap();

        let adapter = SqliteBackupAdapter::new(&backup_src, crypto);
        adapter.create_backup().unwrap()
    };

    let backup_manager = SqliteBackupAdapter::new(&live, crypto);
    let err = FiscalHistoricalGuard::new(db.executor())
        .assert_restore_would_not_regress_archived_state(&encrypted_backup_path, &backup_manager)
        .unwrap_err();
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::RestoreWouldRegressArchivedState { .. })
    ));
}

#[test]
fn case_d_rejects_deleting_opening_snapshots_for_archived_year() {
    let dir = tempdir().unwrap();
    let db = ConnectionFactory::new_with_path(&dir.path().join("imm_d.db")).unwrap();
    seed_archived_closed_year(&db, 2017);

    let err = db
        .executor()
        .opening_balances()
        .create_snapshot(
            grpc_lib::repositories::opening_balances::CreateSnapshotParams {
                id: &uuid::Uuid::new_v4().to_string(),
                product_id: "p-new",
                fiscal_year: 2017,
                quantity: 1.0,
                unit_cost: 1.0,
                total_value: 1.0,
                snapshot_reason: "manual",
                carried_from: None,
                created_by: "admin",
            },
        )
        .unwrap_err();
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::FiscalYearArchived { year: 2017 })
    ));
}

#[test]
fn case_e_rejects_overwrite_of_fiscal_export_snapshot_for_archived_year() {
    let dir = tempdir().unwrap();
    let db = ConnectionFactory::new_with_path(&dir.path().join("imm_e.db")).unwrap();
    seed_archived_closed_year(&db, 2016);

    let snap = FiscalExportSnapshot {
        export_hash: "abc123".to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        generated_by: "admin".to_string(),
        fiscal_year: 2016,
        movement_count: 0,
        report_count: 0,
        inventory_total_value: 0.0,
        integrity_state: None,
        archived_years_count: None,
        active_anomalies_count: None,
        signing_key_id: None,
        export_reason: None,
    };

    let err = FiscalExportSnapshotService::new(db.executor())
        .record_export_snapshot(&snap)
        .unwrap_err();
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::FiscalYearArchived { year: 2016 })
    ));
}

#[test]
fn archive_closed_year_succeeds_when_not_yet_archived() {
    let dir = tempdir().unwrap();
    let mut db = ConnectionFactory::new_with_path(&dir.path().join("imm_ok.db")).unwrap();
    db.get_connection()
        .execute(
            "INSERT INTO fiscal_year_status (year, status, opened_at, closed_at, archived)
             VALUES (2015, 'closed', '2015-01-01', '2015-12-31', 0)",
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
    db.with_transaction(|tx| FiscalYearService::new(tx).archive_year(2015, &admin_id, "admin"))
        .unwrap();

    let archived: i32 = db
        .get_connection()
        .query_row(
            "SELECT archived FROM fiscal_year_status WHERE year = 2015",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(archived, 1);
}
