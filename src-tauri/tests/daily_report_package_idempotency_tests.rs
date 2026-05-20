//! Idempotency / replay-guard tests for daily report package imports.

use chrono::{NaiveDate, Utc};

use grpc_lib::application::sync::{
    PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::application::usecases::exports::types::DailyReportExportDataset;
use grpc_lib::application::usecases::sync::import_daily_report_package::{
    execute as apply_daily_report_package, ImportDailyReportPackageInput, DAILY_REPORT_PACKAGE_KIND,
};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::errors::{AppError, BusinessLogicError};
use grpc_lib::infrastructure::db::sync_import::SqliteImportedPackageRegistry;
use grpc_lib::models::{
    DailyConsumptionSyncLine, DailyReportSyncSnapshot, MealSectionSyncSnapshot, MealType,
    NodeType, WilayaNodeConfiguration,
};
use grpc_lib::repositories::{SettingsRepository, UnitRepository};

fn fixture_daily_package(
    pkg_id: &str,
    source_node_id: &str,
) -> SyncPackage<DailyReportExportDataset> {
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at: Utc::now(),
            source_node_id: source_node_id.to_string(),
            package_id: PackageId(pkg_id.to_string()),
            signature_version: None,
            signing_key_id: None,
            integrity_hash: None,
            signature: None,
        },
        payload: DailyReportExportDataset {
            snapshot: DailyReportSyncSnapshot {
                report_id: "rep-1".into(),
                date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                total_daily_cost: 120.0,
                total_daily_average: 12.0,
                total_daily_beneficiaries: 10,
                meals: vec![MealSectionSyncSnapshot {
                    meal_type: MealType::Breakfast,
                    staff_24h_count: 5,
                    staff_8h_count: 3,
                    reservation_count: 0,
                    mission_count: 0,
                    guest_count: 2,
                    total_beneficiaries: 10,
                    total_meal_cost: 120.0,
                    meal_average: 12.0,
                    items: vec![DailyConsumptionSyncLine {
                        product_id: "prod-1".into(),
                        product_name: "Bread".into(),
                        quantity: 1.0,
                        unit_price: 20.0,
                        total_cost: 20.0,
                    }],
                }],
            },
        },
    }
}

#[test]
fn daily_report_package_same_id_is_rejected_second_time() {
    let mut db = ConnectionFactory::new_for_test().expect("db");

    SettingsRepository::new(db.executor())
        .configure_wilaya(&WilayaNodeConfiguration {
            node_type: NodeType::Wilaya,
            wilaya_code: Some("16".into()),
            wilaya_name: Some("Alger".into()),
        })
        .expect("configure wilaya");

    let now = Utc::now().to_rfc3339();
    let unit_id = uuid::Uuid::new_v4().to_string();
    UnitRepository::new(db.executor())
        .upsert_raw_unit(&unit_id, "U16A", "Alpha Unit", "16", &now)
        .expect("unit");

    let pkg_id = "pkg-daily-dup-001";
    let package = fixture_daily_package(pkg_id, &unit_id);

    let input = ImportDailyReportPackageInput {
        package: package.clone(),
        unit_id: unit_id.clone(),
        importer_wilaya_code: "16".into(),
        imported_by: "admin".into(),
    };

    let source_for_reg = package.metadata.source_node_id.trim().to_string();

    db.with_transaction(|tx| {
        let src_opt = (!source_for_reg.is_empty()).then_some(source_for_reg.as_str());
        let reg =
            SqliteImportedPackageRegistry::new(tx, DAILY_REPORT_PACKAGE_KIND, src_opt, "admin");
        apply_daily_report_package(tx, &reg, input.clone())
    })
    .expect("first import ok");

    let err = db
        .with_transaction(|tx| {
            let src_opt = (!source_for_reg.is_empty()).then_some(source_for_reg.as_str());
            let reg =
                SqliteImportedPackageRegistry::new(tx, DAILY_REPORT_PACKAGE_KIND, src_opt, "admin");
            apply_daily_report_package(tx, &reg, input)
        })
        .expect_err("second import must fail");

    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage { .. })
    ));
}
