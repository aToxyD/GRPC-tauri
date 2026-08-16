//! APPKEY-003 Integration Tests — fleet App Key import → fresh-UNIT flow.
//!
//! Exercises the REAL import path (`import_app_key_into_store`) against an
//! isolated store dir, then proves the imported WILAYA key drives `.unit` V2
//! artifact decryption through `AgeFileEncryptionProvider` (the exact path
//! `import_unit_node_package` uses), and that a wrong imported key fails
//! closed. The global app-key cache is shared, so these tests serialize on a
//! static lock and always clear the cache around their own section.
//!
//! Reuses the B8 anchor-flow fixtures (RFC 8032 §7.1 TEST 1 secret matches the
//! debug-mode Root fallback, same vector as `b8_first_import_tests.rs`).

use std::sync::Mutex;
use tempfile::TempDir;

use grpc_lib::application::services::{
    FinalizeWilayaProvisionResult, IdentityProvisioningService, IdentitySignedExportService,
};
use grpc_lib::commands::import_app_key_into_store;
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    Ed25519CertificateSignature, IdentityCertificate, IdentitySigner, SubjectType,
    SIGNATURE_VERSION_ED25519,
};
use grpc_lib::infrastructure::identity::NodeKeyStore;
use grpc_lib::infrastructure::security::appkey_store::AppKeyStore;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::{
    cache_app_key, cached_app_key, clear_app_key_cache, Ed25519SigningProvider,
};
use grpc_lib::infrastructure::sync::read_unit_node_package_from_file;
use grpc_lib::models::{Unit, UnitNodePackage, UserExport};
use uuid::Uuid;

/// RFC 8032 §7.1 TEST 1 secret — matches the debug-mode Root fallback.
const IMPORT_TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
    0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
    0x7f, 0x60,
];

static APP_KEY_CACHE_LOCK: Mutex<()> = Mutex::new(());

fn fresh_valid_identity() -> String {
    use age::secrecy::ExposeSecret;
    age::x25519::Identity::generate()
        .to_string()
        .expose_secret()
        .to_string()
}

fn root_signer() -> Ed25519SigningProvider {
    Ed25519SigningProvider::new(IMPORT_TEST_ROOT_SECRET)
}

/// Seed an ACTIVE WILAYA certificate (the locally installed trust anchor on a
/// UNIT, ADR-0044 anchor-first model) via the real provisioning flow.
fn bootstrap_wilaya(db: &mut Database, node_key_store: &NodeKeyStore) -> IdentityCertificate {
    let mut provisioning = IdentityProvisioningService::new(db);
    let request = provisioning
        .generate_wilaya_request(Uuid::new_v4(), node_key_store)
        .expect("csr generated");
    let signature = root_signer()
        .sign_certificate(&request)
        .expect("root signed request");
    let mut signed = request;
    signed.signature =
        Some(Ed25519CertificateSignature::try_from(signature).expect("sig wrap"));
    match provisioning
        .finalize_wilaya_provision(&signed, node_key_store, "2026-08-04T00:00:00Z")
        .expect("wilaya finalized")
    {
        FinalizeWilayaProvisionResult::Provisioned(cert) => cert,
        FinalizeWilayaProvisionResult::AlreadyProvisioned(_) => {
            panic!("first finalize must provision")
        }
    }
}

fn export_unit_package(
    db: &Database,
    node_key_store: &NodeKeyStore,
    path: &std::path::Path,
    crypto: &AgeFileEncryptionProvider,
) -> UnitNodePackage {
    let dataset = UnitNodePackage {
        unit: Unit {
            id: "unit-import-9".into(),
            code: "UNIT-IMPORT-9".into(),
            name: "Unit Import 9".into(),
            wilaya_code: "16".into(),
            user_id: None,
            created_at: chrono::Utc::now(),
        },
        user: UserExport {
            username: "op".into(),
            password_hash: "hash".into(),
            role: "User".into(),
        },
        unit_certificate: None,
        unit_private_key: None,
    };
    IdentitySignedExportService::new(db, node_key_store)
        .export_v2_bootstrap_package(
            dataset.clone(),
            "wilaya-test-node",
            path,
            SubjectType::Wilaya,
            crypto,
        )
        .expect("bootstrap export");
    dataset
}

#[test]
fn fresh_unit_import_flow_decrypts_unit_with_imported_key() {
    let _guard = APP_KEY_CACHE_LOCK.lock().unwrap();
    clear_app_key_cache();

    let artifact_dir = TempDir::new().unwrap();
    let store_dir = TempDir::new().unwrap();
    let identity = fresh_valid_identity();
    let artifact = artifact_dir.path().join("grpc-app-key.age");
    std::fs::write(&artifact, format!("{identity}\n")).unwrap();
    let store = AppKeyStore::new(store_dir.path().to_path_buf());

    let (result, imported) = import_app_key_into_store(
        &store,
        "correct-horse-import",
        artifact.to_str().unwrap(),
    )
    .expect("import succeeds on fresh store");
    assert!(result.provisioned);
    assert!(result.unlocked);
    assert_eq!(imported, identity);

    cache_app_key(&imported).expect("cache prime");
    assert_eq!(cached_app_key().unwrap(), identity);

    let dir = TempDir::new().unwrap();
    let mut db = ConnectionFactory::new_for_test().unwrap();
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
    let wilaya_cert = bootstrap_wilaya(&mut db, &node_key_store);
    let crypto = AgeFileEncryptionProvider::new();
    let unit_dir = TempDir::new().unwrap();
    let unit_path = unit_dir.path().join("unit.unit");

    let dataset = export_unit_package(&db, &node_key_store, &unit_path, &crypto);

    let pkg =
        read_unit_node_package_from_file(&unit_path, &crypto).expect("imported key must decrypt");
    assert_eq!(pkg.metadata.signature_version, Some(SIGNATURE_VERSION_ED25519));
    assert_eq!(pkg.metadata.issuer_identity_id, Some(wilaya_cert.identity_id));
    assert_eq!(pkg.payload.unit.code, dataset.unit.code);
    assert_eq!(pkg.payload.unit.name, dataset.unit.name);
    assert_eq!(pkg.payload.user.username, dataset.user.username);

    clear_app_key_cache();
    assert!(cached_app_key().is_none());
}

#[test]
fn fresh_unit_import_wrong_key_fails_closed() {
    let _guard = APP_KEY_CACHE_LOCK.lock().unwrap();
    clear_app_key_cache();

    let artifact_dir = TempDir::new().unwrap();
    let store_dir = TempDir::new().unwrap();
    let key_a = fresh_valid_identity();
    let artifact_a = artifact_dir.path().join("grpc-app-key.age");
    std::fs::write(&artifact_a, format!("{key_a}\n")).unwrap();
    let store_a = AppKeyStore::new(store_dir.path().to_path_buf());
    let (_, imported_a) = import_app_key_into_store(
        &store_a,
        "correct-horse-import",
        artifact_a.to_str().unwrap(),
    )
    .expect("import A");
    cache_app_key(&imported_a).expect("cache A");

    let dir = TempDir::new().unwrap();
    let mut db = ConnectionFactory::new_for_test().unwrap();
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
    bootstrap_wilaya(&mut db, &node_key_store);
    let crypto = AgeFileEncryptionProvider::new();
    let unit_dir = TempDir::new().unwrap();
    let unit_path = unit_dir.path().join("unit.unit");
    export_unit_package(&db, &node_key_store, &unit_path, &crypto);
    assert!(
        read_unit_node_package_from_file(&unit_path, &crypto).is_ok(),
        "precondition: key A decrypts its own .unit"
    );

    let store_dir_b = TempDir::new().unwrap();
    let key_b = fresh_valid_identity();
    assert_ne!(key_a, key_b);
    let artifact_b = artifact_dir.path().join("grpc-app-key-b.age");
    std::fs::write(&artifact_b, format!("{key_b}\n")).unwrap();
    let store_b = AppKeyStore::new(store_dir_b.path().to_path_buf());
    let (_, imported_b) = import_app_key_into_store(
        &store_b,
        "correct-horse-import-b",
        artifact_b.to_str().unwrap(),
    )
    .expect("import B");
    assert_ne!(imported_a, imported_b);

    cache_app_key(&imported_b).expect("cache B — simulates a node holding the wrong key");
    assert!(
        read_unit_node_package_from_file(&unit_path, &crypto).is_err(),
        "wrong imported key must fail closed: .unit decryption denied"
    );

    clear_app_key_cache();
    assert!(cached_app_key().is_none());
}