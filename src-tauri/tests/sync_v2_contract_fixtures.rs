//! Protocol contract checks for sync schema V2 (canonical JSON + HMAC + SHA-256).
//!
//! Golden JSON files under `tests/fixtures/sync/` document stable **shape**; builders still
//! emit fresh `integrity_hash` / `signature` per key material. This test locks **determinism**
//! and **schema_version** for trial builds.

use chrono::{TimeZone, Utc};

use grpc_lib::application::sync::{
    PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::application::usecases::exports::types::MonthlySummaryExportDataset;
use grpc_lib::infrastructure::security::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::sync::{
    HmacPackageSigner, PackageBuilder, SerdeJsonSyncPackageSerializer,
};
use grpc_lib::models::{DailyDetailSyncSnapshot, MonthlySummary};
// PlaintextSeal is no longer needed as we test through the real age-encryption provider
// but verify the decrypted content for determinism.

fn monthly_fixture_package() -> SyncPackage<MonthlySummaryExportDataset> {
    let created_at = Utc
        .with_ymd_and_hms(2026, 5, 9, 12, 0, 7)
        .single()
        .expect("dt");
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at,
            source_node_id: "contract-node".into(),
            package_sequence: None,
            issuer_identity_id: None,
            package_id: PackageId("contract-pkg-monthly-001".into()),
            signature_version: None,
            signing_key_id: None,
            integrity_hash: None,
            signature: None,
        },
        payload: MonthlySummaryExportDataset {
            summary: MonthlySummary {
                month: 5,
                year: 2026,
                total_beneficiaries: 1,
                total_consumption_value: 1.0,
                breakfast_average: 1.0,
                lunch_average: 0.0,
                dinner_average: 0.0,
                daily_average: 1.0,
                report_count: 1,
            },
            daily_detail_rows: vec![DailyDetailSyncSnapshot {
                date: chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
                total_daily_beneficiaries: 1,
                total_daily_cost: 1.0,
                breakfast_average: 1.0,
                lunch_average: 0.0,
                dinner_average: 0.0,
                daily_average: 1.0,
            }],
        },
    }
}

#[test]
fn v2_builder_is_deterministic_for_fixed_key_and_payload() {
    use base64::{engine::general_purpose, Engine as _};
    let key_bytes = [0u8; 32];
    let key = general_purpose::STANDARD.encode(key_bytes);
    std::env::set_var("GRPC_PACKAGE_SIGNING_KEY", &key);
    std::env::remove_var("GRPC_ENV");

    let pkg = monthly_fixture_package();
    assert_eq!(pkg.metadata.schema_version.as_u16(), 2);

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let path_a = temp_dir.path().join("a.sync");
    let path_b = temp_dir.path().join("b.sync");

    let builder = PackageBuilder::new();
    let crypto_port = AgeFileEncryptionProvider::new();
    let serializer = SerdeJsonSyncPackageSerializer;
    let signer = HmacPackageSigner;

    // Build twice to different files
    builder
        .build_encrypted_stream_path(&pkg, &serializer, &signer, &crypto_port, &path_a)
        .expect("build a");
    builder
        .build_encrypted_stream_path(&pkg, &serializer, &signer, &crypto_port, &path_b)
        .expect("build b");

    // Decrypt both (even if encrypted bytes differ due to age nonces, plaintext MUST be identical)
    let content_a =
        std::fs::read_to_string(decrypt_to_temp(&path_a, &crypto_port)).expect("read a");
    let content_b =
        std::fs::read_to_string(decrypt_to_temp(&path_b, &crypto_port)).expect("read b");

    assert_eq!(
        content_a, content_b,
        "decrypted plaintext MUST be identical for deterministic protocol contract"
    );

    let v: serde_json::Value = serde_json::from_str(&content_a).expect("json");
    assert_eq!(v["metadata"]["schema_version"], 2);
    assert!(v["metadata"]["integrity_hash"].is_string());
    assert!(v["metadata"]["signature"].is_string());

    std::env::remove_var("GRPC_PACKAGE_SIGNING_KEY");
}

fn decrypt_to_temp(
    path: &std::path::Path,
    crypto_port: &AgeFileEncryptionProvider,
) -> std::path::PathBuf {
    let file = std::fs::File::open(path).expect("open encrypted");
    let mut reader = std::io::BufReader::new(file);
    let temp = tempfile::NamedTempFile::new().expect("temp");
    let (mut temp_file, temp_path) = temp.keep().expect("persist temp");

    crypto_port
        .decrypt_stream(&mut reader, &mut temp_file)
        .expect("decrypt");
    temp_path
}
