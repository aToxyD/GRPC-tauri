//! Protocol contract checks for the sync envelope (SEC-087 Phase 6A / ADR-0057:
//! schema V3; canonical JSON + Ed25519 + SHA-256).
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
use grpc_lib::infrastructure::sync::packages::signing::{
    Ed25519PackageSigner, DEFAULT_SIGNATURE_VERSION,
};
use grpc_lib::infrastructure::sync::{PackageBuilder, SerdeJsonSyncPackageSerializer};
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
            issuer_identity_id: None,
            package_id: PackageId("contract-pkg-monthly-001".into()),
            signature_version: Some(DEFAULT_SIGNATURE_VERSION),
            signing_key_id: None,
            integrity_hash: None,
            signature: None,
            export_mode: None,
            target_node_id: None,
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
    // SEC-007 (ADR-0047): V1/HMAC is removed; determinism is locked on Ed25519,
    // which is deterministic by construction (RFC 8032) for a fixed private key.
    let signer = Ed25519PackageSigner::new([0u8; 32]);

    let mut pkg = monthly_fixture_package();
    // The builder fills integrity_hash + signature; signing_key_id is the
    // signer's public key (fixed secret → fixed key → deterministic).
    pkg.metadata.signing_key_id = Some(signer.public_key_hex());
    assert_eq!(
        pkg.metadata.schema_version.as_u16(),
        SYNC_PACKAGE_SCHEMA_VERSION.as_u16()
    );
    assert_eq!(
        pkg.metadata.signature_version,
        Some(DEFAULT_SIGNATURE_VERSION)
    );

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let path_a = temp_dir.path().join("a.sync");
    let path_b = temp_dir.path().join("b.sync");

    let builder = PackageBuilder::new();
    let crypto_port = AgeFileEncryptionProvider::new();
    let serializer = SerdeJsonSyncPackageSerializer;

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
    assert_eq!(
        v["metadata"]["schema_version"],
        serde_json::json!(SYNC_PACKAGE_SCHEMA_VERSION.as_u16())
    );
    assert_eq!(v["metadata"]["signature_version"], 2);
    assert!(v["metadata"]["integrity_hash"].is_string());
    assert!(v["metadata"]["signature"].is_string());
    assert!(v["metadata"]["signing_key_id"].is_string());
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
