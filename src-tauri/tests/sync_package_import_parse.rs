//! Parse + validate plaintext sync packages against golden fixtures.

use std::path::Path;

use grpc_lib::application::sync::validate_monthly_summary_package_for_import;
use grpc_lib::errors::AppError;
use grpc_lib::infrastructure::sync::SerdeJsonSyncPackageDeserializer;

#[test]
fn golden_monthly_plaintext_deserializes_and_validates() {
    let golden_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots/sync/monthly_summary_package_plain.json");
    let bytes = std::fs::read(&golden_path).expect("fixture");

    let package =
        SerdeJsonSyncPackageDeserializer::monthly_summary_from_reader(std::io::Cursor::new(&bytes))
            .expect("deserialize SyncPackage JSON");

    validate_monthly_summary_package_for_import(&package).expect("validate");
}

#[test]
fn rejects_unsupported_schema_version() {
    use chrono::{NaiveDate, TimeZone, Utc};

    use grpc_lib::application::sync::{
        PackageId, SchemaVersion, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
    };
    use grpc_lib::application::usecases::exports::types::MonthlySummaryExportDataset;
    use grpc_lib::models::{DailyDetailSyncSnapshot, MonthlySummary};

    let created_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let dataset = MonthlySummaryExportDataset {
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
            date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            total_daily_beneficiaries: 0,
            total_daily_cost: 0.0,
            breakfast_average: 0.0,
            lunch_average: 0.0,
            dinner_average: 0.0,
            daily_average: 0.0,
        }],
    };
    let pkg = SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SchemaVersion::new(SYNC_PACKAGE_SCHEMA_VERSION.as_u16() + 99),
            created_at,
            source_node_id: "u".into(),
            package_id: PackageId("p1".into()),
            signature_version: None,
            signing_key_id: None,
            integrity_hash: None,
            signature: None,
        },
        payload: dataset,
    };

    let err =
        validate_monthly_summary_package_for_import(&pkg).expect_err("must reject future schema");
    match err {
        AppError::Validation(grpc_lib::errors::ValidationError::InvalidFormat {
            message, ..
        }) => {
            assert!(message.contains("PACKAGE_TOO_NEW"));
        }
        other => panic!("unexpected error: {:?}", other),
    }
}

#[test]
fn rejects_integrity_hash_mismatch() {
    use chrono::{NaiveDate, TimeZone, Utc};

    use grpc_lib::application::sync::{
        PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
    };
    use grpc_lib::application::usecases::exports::types::MonthlySummaryExportDataset;
    use grpc_lib::models::{DailyDetailSyncSnapshot, MonthlySummary};

    let created_at = Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap();
    let pkg = SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at,
            source_node_id: "u-1".into(),
            package_id: PackageId("p2".into()),
            signature_version: None,
            signing_key_id: None,
            integrity_hash: Some("deadbeef".into()),
            signature: None,
        },
        payload: MonthlySummaryExportDataset {
            summary: MonthlySummary {
                month: 1,
                year: 2026,
                total_beneficiaries: 1,
                total_consumption_value: 10.0,
                breakfast_average: 10.0,
                lunch_average: 0.0,
                dinner_average: 0.0,
                daily_average: 10.0,
                report_count: 1,
            },
            daily_detail_rows: vec![DailyDetailSyncSnapshot {
                date: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
                total_daily_beneficiaries: 1,
                total_daily_cost: 10.0,
                breakfast_average: 10.0,
                lunch_average: 0.0,
                dinner_average: 0.0,
                daily_average: 10.0,
            }],
        },
    };

    let plaintext = serde_json::to_vec(&pkg).expect("serialize");
    let err = SerdeJsonSyncPackageDeserializer::monthly_summary_from_reader(std::io::Cursor::new(
        &plaintext,
    ))
    .expect_err("must reject integrity mismatch");
    assert!(matches!(err, AppError::Validation(_)));
}

#[test]
fn accepts_package_without_integrity_hash_v1() {
    let golden_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots/sync/monthly_summary_package_plain.json");
    let bytes = std::fs::read(&golden_path).expect("fixture");

    // Old fixtures don't carry integrity_hash. They must remain importable.
    let package =
        SerdeJsonSyncPackageDeserializer::monthly_summary_from_reader(std::io::Cursor::new(&bytes))
            .expect("deserialize without integrity hash");
    validate_monthly_summary_package_for_import(&package).expect("validate");
}

#[test]
fn rejects_package_too_old_schema() {
    use chrono::{NaiveDate, TimeZone, Utc};

    use grpc_lib::application::sync::{PackageId, SchemaVersion, SyncPackage, SyncPackageMetadata};
    use grpc_lib::application::usecases::exports::types::MonthlySummaryExportDataset;
    use grpc_lib::models::{DailyDetailSyncSnapshot, MonthlySummary};

    let created_at = Utc.with_ymd_and_hms(2026, 1, 3, 0, 0, 0).unwrap();
    let pkg = SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SchemaVersion::new(0),
            created_at,
            source_node_id: "u".into(),
            package_id: PackageId("p-old".into()),
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
                date: NaiveDate::from_ymd_opt(2026, 1, 4).unwrap(),
                total_daily_beneficiaries: 0,
                total_daily_cost: 0.0,
                breakfast_average: 0.0,
                lunch_average: 0.0,
                dinner_average: 0.0,
                daily_average: 0.0,
            }],
        },
    };

    let err =
        validate_monthly_summary_package_for_import(&pkg).expect_err("must reject old schema");
    match err {
        AppError::Validation(grpc_lib::errors::ValidationError::InvalidFormat {
            message, ..
        }) => {
            assert!(message.contains("PACKAGE_TOO_OLD"));
        }
        other => panic!("unexpected error: {:?}", other),
    }
}
