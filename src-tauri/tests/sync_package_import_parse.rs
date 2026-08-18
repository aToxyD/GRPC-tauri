//! Parse + validate plaintext sync packages (SEC-007: V2-only, Ed25519).

use chrono::{NaiveDate, TimeZone, Utc};

use grpc_lib::application::sync::{
    validate_monthly_summary_package_for_import, PackageId, SchemaVersion, SyncPackage,
    SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::application::usecases::exports::types::MonthlySummaryExportDataset;
use grpc_lib::errors::AppError;
use grpc_lib::infrastructure::security::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::sync::packages::signing::{
    DEFAULT_SIGNATURE_VERSION, Ed25519PackageSigner,
};
use grpc_lib::infrastructure::sync::{
    read_monthly_summary_package_from_file, SerdeJsonSyncPackageDeserializer,
};
use grpc_lib::models::{DailyDetailSyncSnapshot, MonthlySummary};

fn monthly_dataset() -> MonthlySummaryExportDataset {
    MonthlySummaryExportDataset {
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
    }
}

#[test]
fn v2_package_roundtrips_encrypted_and_validates() {
    // SEC-007 (ADR-0047): the V2 (Ed25519) package path is the only supported
    // one; the legacy golden fixtures are removed.
    let signer = Ed25519PackageSigner::new([42u8; 32]);
    let pkg: SyncPackage<MonthlySummaryExportDataset> = SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at: Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap(),
            source_node_id: "u-1".into(),
            package_sequence: None,
            issuer_identity_id: None,
            package_id: PackageId("p-v2-rt".into()),
            signature_version: Some(DEFAULT_SIGNATURE_VERSION),
            signing_key_id: Some(signer.public_key_hex()),
            integrity_hash: None,
            signature: None,
        },
        payload: monthly_dataset(),
    };

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let path = temp_dir.path().join("monthly.sync");
    let crypto_port = AgeFileEncryptionProvider;
    let builder = grpc_lib::infrastructure::sync::PackageBuilder::new();
    builder
        .build_encrypted_stream_path(
            &pkg,
            &grpc_lib::infrastructure::sync::SerdeJsonSyncPackageSerializer,
            &signer,
            &crypto_port,
            &path,
        )
        .expect("build");

    let decoded =
        read_monthly_summary_package_from_file(&path, &crypto_port).expect("read+decode");
    validate_monthly_summary_package_for_import(&decoded).expect("validate");
    assert_eq!(decoded.payload.summary.month, 1);
}

#[test]
fn rejects_unsupported_schema_version() {
    let created_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let pkg = SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SchemaVersion::new(SYNC_PACKAGE_SCHEMA_VERSION.as_u16() + 99),
            created_at,
            source_node_id: "u".into(),
            package_sequence: None,
            issuer_identity_id: None,
            package_id: PackageId("p1".into()),
            signature_version: Some(DEFAULT_SIGNATURE_VERSION),
            signing_key_id: None,
            integrity_hash: None,
            signature: None,
        },
        payload: monthly_dataset(),
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
    let created_at = Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap();
    let pkg = SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at,
            source_node_id: "u-1".into(),
            package_sequence: None,
            issuer_identity_id: None,
            package_id: PackageId("p2".into()),
            signature_version: Some(DEFAULT_SIGNATURE_VERSION),
            signing_key_id: None,
            integrity_hash: Some("deadbeef".into()),
            signature: None,
        },
        payload: monthly_dataset(),
    };

    let plaintext = serde_json::to_vec(&pkg).expect("serialize");
    let err = SerdeJsonSyncPackageDeserializer::monthly_summary_from_reader(std::io::Cursor::new(
        &plaintext,
    ))
    .expect_err("must reject integrity mismatch");
    assert!(matches!(err, AppError::Validation(_)));
}

#[test]
fn rejects_package_without_integrity_hash() {
    // SEC-007 (ADR-0047): a V2-shaped package without integrity_hash is
    // refused fail-closed (legacy V1 shape is removed).
    let created_at = Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap();
    let pkg = SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at,
            source_node_id: "u-1".into(),
            package_sequence: None,
            issuer_identity_id: None,
            package_id: PackageId("p-no-hash".into()),
            signature_version: Some(DEFAULT_SIGNATURE_VERSION),
            signing_key_id: None,
            integrity_hash: None,
            signature: None,
        },
        payload: monthly_dataset(),
    };

    let plaintext = serde_json::to_vec(&pkg).expect("serialize");
    let err = SerdeJsonSyncPackageDeserializer::monthly_summary_from_reader(std::io::Cursor::new(
        &plaintext,
    ))
    .expect_err("must reject package without integrity hash");
    assert!(matches!(err, AppError::Validation(_)));
}

#[test]
fn rejects_package_too_old_schema() {
    let created_at = Utc.with_ymd_and_hms(2026, 1, 3, 0, 0, 0).unwrap();
    let pkg = SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SchemaVersion::new(0),
            created_at,
            source_node_id: "u".into(),
            package_sequence: None,
            issuer_identity_id: None,
            package_id: PackageId("p-old".into()),
            signature_version: Some(DEFAULT_SIGNATURE_VERSION),
            signing_key_id: None,
            integrity_hash: None,
            signature: None,
        },
        payload: monthly_dataset(),
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
