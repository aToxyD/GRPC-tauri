//! Idempotency / replay-guard tests for sync package imports.

use chrono::Utc;

use grpc_lib::application::sync::{
    PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::application::usecases::exports::types::MonthlySummaryExportDataset;
use grpc_lib::application::usecases::sync::import_monthly_summary_package::{
    execute as apply_monthly_summary_package, ImportMonthlySummaryPackageInput,
    MONTHLY_SUMMARY_PACKAGE_KIND,
};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::errors::{AppError, BusinessLogicError};
use grpc_lib::infrastructure::db::sync_import::SqliteImportedPackageRegistry;
use grpc_lib::models::{
    DailyDetailSyncSnapshot, MonthlySummary, NodeType, WilayaNodeConfiguration,
};
use grpc_lib::repositories::{SettingsRepository, UnitRepository};

fn fixture_package_with_id(
    package_id: &str,
    source_node_id: &str,
) -> SyncPackage<MonthlySummaryExportDataset> {
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at: Utc::now(),
            source_node_id: source_node_id.to_string(),
            package_sequence: None,
            issuer_identity_id: None,
            package_id: PackageId(package_id.to_string()),
            signature_version: None,
            signing_key_id: None,
            integrity_hash: None,
            signature: None,
        },
        payload: MonthlySummaryExportDataset {
            summary: MonthlySummary {
                month: 1,
                year: 2026,
                total_beneficiaries: 0,
                total_consumption_value: 0.0,
                breakfast_average: 0.0,
                lunch_average: 0.0,
                dinner_average: 0.0,
                daily_average: 0.0,
                report_count: 0,
            },
            daily_detail_rows: vec![DailyDetailSyncSnapshot {
                date: chrono::NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
                total_daily_beneficiaries: 0,
                total_daily_cost: 0.0,
                breakfast_average: 0.0,
                lunch_average: 0.0,
                dinner_average: 0.0,
                daily_average: 0.0,
            }],
        },
    }
}

#[test]
fn importing_same_package_id_twice_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");

    // Configure as WILAYA node (required by import command; mirrors real trust boundary).
    SettingsRepository::new(db.executor())
        .configure_wilaya(&WilayaNodeConfiguration {
            node_type: NodeType::Wilaya,
            wilaya_code: Some("16".into()),
            wilaya_name: Some("Alger".into()),
        })
        .expect("configure wilaya");

    // Create a unit row under wilaya 16.
    let now = Utc::now().to_rfc3339();
    let unit_id = uuid::Uuid::new_v4().to_string();
    UnitRepository::new(db.executor())
        .upsert_raw_unit(&unit_id, "C01", "Alpha Base", "16", &now)
        .expect("unit");

    // Apply same package twice inside DB transactions.
    let pkg_id = "pkg-dup-001";
    let pkg = fixture_package_with_id(pkg_id, &unit_id); // source_node_id = unit_id (strongest match)

    let input = ImportMonthlySummaryPackageInput {
        package: pkg.clone(),
        unit_id: unit_id.clone(),
        importer_wilaya_code: "16".into(),
        imported_by: "admin".into(),
    };

    let source_for_reg = pkg.metadata.source_node_id.trim().to_string();

    // First time should succeed.
    db.with_transaction(|tx| {
        let src_opt = (!source_for_reg.is_empty()).then_some(source_for_reg.as_str());
        let reg =
            SqliteImportedPackageRegistry::new(tx, MONTHLY_SUMMARY_PACKAGE_KIND, src_opt, "admin", None, None);
        apply_monthly_summary_package(tx, &reg, input.clone())
    })
    .expect("first import ok");

    // Second time should be rejected.
    let err = db
        .with_transaction(|tx| {
            let src_opt = (!source_for_reg.is_empty()).then_some(source_for_reg.as_str());
            let reg = SqliteImportedPackageRegistry::new(
                tx,
                MONTHLY_SUMMARY_PACKAGE_KIND,
                src_opt,
                "admin",
                None,
                None,
            );
            apply_monthly_summary_package(tx, &reg, input)
        })
        .expect_err("second import must fail");

    match err {
        AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage { package_id }) => {
            assert_eq!(package_id, pkg_id);
        }
        other => panic!("unexpected error: {:?}", other),
    }
}
