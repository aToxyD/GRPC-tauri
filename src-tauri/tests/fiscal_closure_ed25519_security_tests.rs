//! SEC-008 (ADR-0048): fiscal closure package Ed25519 security matrix.
//!
//! Covers the migration mission requirements §13 (fail-closed verification)
//! and §14 (key separation: no HMAC shared-secret path remains, no env-based
//! signing secret is consulted, only the trusted ACTIVE WILAYA identity can
//! authorize a fiscal closure).

use chrono::{Duration, Utc};
use grpc_lib::application::services::FiscalClosurePackageService;
use grpc_lib::domain::identity::{
    CredentialStatus, IdentityCertificate, IdentitySigner, IdentityStorePort, SubjectType,
    SIGNATURE_VERSION_ED25519,
};
use grpc_lib::errors::AppError;
use grpc_lib::infrastructure::security::Ed25519SigningProvider;
use grpc_lib::infrastructure::sync::packages::signing::Ed25519PackageSigner;
use grpc_lib::repositories::RepositoryProvider;
use serde_json::json;
use std::sync::Mutex;

mod common;

const WILAYA_SECRET: [u8; 32] = [42u8; 32];
const OTHER_SECRET: [u8; 32] = [7u8; 32];

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn signer(secret: [u8; 32]) -> Ed25519PackageSigner {
    Ed25519PackageSigner::new(secret)
}

fn seed_cert(
    db: &grpc_lib::db::Database,
    subject_type: SubjectType,
    secret: [u8; 32],
    status: CredentialStatus,
    not_after: Option<chrono::DateTime<chrono::Utc>>,
) -> uuid::Uuid {
    let identity_id = uuid::Uuid::new_v4();
    let certificate = IdentityCertificate {
        identity_id,
        subject_type,
        subject_id: identity_id,
        issuer_identity_id: None,
        credential_id: uuid::Uuid::new_v4(),
        generation: 1,
        status,
        public_key: Ed25519SigningProvider::new(secret).public_key(),
        algorithm_version: SIGNATURE_VERSION_ED25519,
        not_after,
        package_sequence: Some(1),
        signature: None,
    };
    db.executor()
        .identity_store()
        .upsert(&certificate, &Utc::now().to_rfc3339())
        .expect("seed certificate");
    identity_id
}

fn build_package(
    issuer_id: uuid::Uuid,
    signing_key_id: &str,
    closed_year: i32,
) -> grpc_lib::application::services::FiscalClosurePackage {
    FiscalClosurePackageService::build_closure_package(
        "WILAYA-01",
        "wilaya-admin",
        closed_year,
        closed_year + 1,
        &Utc::now().to_rfc3339(),
        &grpc_lib::application::services::FiscalClosurePackageSignerInfo {
            issuer_identity_id: issuer_id.to_string(),
            signing_key_id: signing_key_id.to_string(),
        },
        None,
    )
    .expect("build package")
}

fn export(db: &grpc_lib::db::Database, path: &str, secret: [u8; 32]) -> uuid::Uuid {
    let issuer_id = common::seed_wilaya_identity(db, secret);
    let s = signer(secret);
    let pkg = build_package(issuer_id, &s.public_key_hex(), 2025);
    FiscalClosurePackageService::new(db.executor())
        .export_to_file(&pkg, &s, path)
        .expect("export package");
    issuer_id
}

fn preview_err(db: &grpc_lib::db::Database, path: &str) -> AppError {
    FiscalClosurePackageService::new(db.executor())
        .preview_closure_package(path)
        .expect_err("preview must fail closed")
}

fn write_package(path: &str, json: serde_json::Value) {
    std::fs::write(path, serde_json::to_string_pretty(&json).unwrap()).unwrap();
}

/// Open the exported envelope JSON of a package file.
fn read_envelope(path: &str) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

// ─── §13: v3 package + Ed25519 metadata + valid verification ────────────────

#[test]
fn v3_package_with_ed25519_envelope_metadata() {
    let (state, temp) = common::create_test_state();
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();

    export(db, path, WILAYA_SECRET);
    let envelope = read_envelope(path);

    assert_eq!(envelope["package"]["schema_version"], 3);
    assert_eq!(envelope["signature_version"], 2);
    let pubkey = envelope["signer_public_key_hex"].as_str().unwrap();
    let signature = envelope["signature_hex"].as_str().unwrap();
    assert_eq!(pubkey.len(), 64);
    assert_eq!(signature.len(), 128);
    assert!(pubkey.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(envelope["package"]["issuer_identity_id"].is_string());
    assert!(envelope["package"]["package_fingerprint"].is_string());
    assert_eq!(
        envelope["package"]["signing_key_id"].as_str().unwrap(),
        pubkey
    );
}

#[test]
fn valid_package_previews_and_applies() {
    let (state, temp) = common::create_test_state();
    common::clear_fiscal_status(&state);
    common::seed_fiscal_year_open(&state, 2025);
    {
        let guard = state.db.lock().unwrap();
        let db = guard.as_ref().unwrap();
        db.executor().settings().set_current_year(2025).unwrap();
    }

    let db_guard = state.db.lock().unwrap();
    let db = db_guard.as_ref().unwrap();
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();
    let issuer_id = export(db, path, WILAYA_SECRET);
    assert_eq!(issuer_id, read_envelope(path)["package"]["issuer_identity_id"].as_str().unwrap().parse::<uuid::Uuid>().unwrap());

    // export → preview → apply full success flow
    let service = FiscalClosurePackageService::new(db.executor());
    let preview = service.preview_closure_package(path).unwrap();
    assert!(preview.validation_ok, "issues: {:?}", preview.validation_issues);
    assert_eq!(preview.closed_year, 2025);
    assert_eq!(preview.opened_year, 2026);
    assert_eq!(preview.schema_version, 3);

    let applied = service
        .apply_closure_package(path, "system", "user1")
        .expect("apply must succeed");
    assert_eq!(applied.closed_year, 2025);
    assert_eq!(applied.opened_year, 2026);
}

// ─── §13: fail-closed rejection matrix ──────────────────────────────────────

#[test]
fn tampered_package_is_rejected() {
    let (state, temp) = common::create_test_state();
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();
    export(db, path, WILAYA_SECRET);

    let mut envelope = read_envelope(path);
    envelope["package"]["closed_year"] = json!(1999);
    write_package(path, envelope);

    let err = preview_err(db, path);
    let msg = format!("{:?}", err);
    assert!(
        matches!(err, AppError::BusinessLogic(_)),
        "tamper must be OperationNotPermitted, got: {msg}"
    );
}

#[test]
fn invalid_signature_is_rejected() {
    let (state, temp) = common::create_test_state();
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();
    export(db, path, WILAYA_SECRET);

    let mut envelope = read_envelope(path);
    envelope["signature_hex"] = json!("00".repeat(64));
    write_package(path, envelope);

    assert!(matches!(
        preview_err(db, path),
        AppError::BusinessLogic(_)
    ));
}

#[test]
fn unknown_signer_is_rejected() {
    let (state, temp) = common::create_test_state();
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let s = signer(WILAYA_SECRET);
    let unknown_issuer = uuid::Uuid::new_v4();
    let pkg = build_package(unknown_issuer, &s.public_key_hex(), 2025);
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();
    FiscalClosurePackageService::new(db.executor())
        .export_to_file(&pkg, &s, path)
        .unwrap();

    assert!(matches!(
        preview_err(db, path),
        AppError::BusinessLogic(_)
    ));
}

#[test]
fn signer_whose_key_mismatches_certificate_is_rejected() {
    let (state, temp) = common::create_test_state();
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    // ACTIVE WILAYA cert is keyed with OTHER_SECRET, but the package is signed
    // with WILAYA_SECRET → envelope key ≠ certificate key.
    let issuer_id = common::seed_wilaya_identity(db, OTHER_SECRET);
    let s = signer(WILAYA_SECRET);
    let pkg = build_package(issuer_id, &s.public_key_hex(), 2025);
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();
    FiscalClosurePackageService::new(db.executor())
        .export_to_file(&pkg, &s, path)
        .unwrap();

    assert!(matches!(
        preview_err(db, path),
        AppError::BusinessLogic(_)
    ));
}

#[test]
fn expired_signer_certificate_is_rejected() {
    let (state, temp) = common::create_test_state();
    let db_guard = state.db.lock().unwrap();
    let db = db_guard.as_ref().unwrap();
    let issuer_id = seed_cert(
        db,
        SubjectType::Wilaya,
        WILAYA_SECRET,
        CredentialStatus::Active,
        Some(Utc::now() - Duration::hours(1)),
    );
    let s = signer(WILAYA_SECRET);
    let pkg = build_package(issuer_id, &s.public_key_hex(), 2025);
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();
    FiscalClosurePackageService::new(db.executor())
        .export_to_file(&pkg, &s, path)
        .unwrap();

    assert!(matches!(
        preview_err(db, path),
        AppError::BusinessLogic(_)
    ));
}

#[test]
fn revoked_signer_certificate_is_rejected() {
    let (state, temp) = common::create_test_state();
    let db_guard = state.db.lock().unwrap();
    let db = db_guard.as_ref().unwrap();
    let issuer_id = seed_cert(
        db,
        SubjectType::Wilaya,
        WILAYA_SECRET,
        CredentialStatus::Revoked,
        None,
    );
    let s = signer(WILAYA_SECRET);
    let pkg = build_package(issuer_id, &s.public_key_hex(), 2025);
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();
    FiscalClosurePackageService::new(db.executor())
        .export_to_file(&pkg, &s, path)
        .unwrap();

    assert!(matches!(
        preview_err(db, path),
        AppError::BusinessLogic(_)
    ));
}

#[test]
fn missing_signature_is_rejected() {
    let (state, temp) = common::create_test_state();
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();
    export(db, path, WILAYA_SECRET);

    let mut envelope = read_envelope(path);
    envelope["signature_hex"] = json!("");
    write_package(path, envelope);

    assert!(matches!(
        preview_err(db, path),
        AppError::BusinessLogic(_)
    ));
}

#[test]
fn unsupported_signature_version_is_rejected() {
    let (state, temp) = common::create_test_state();
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();
    export(db, path, WILAYA_SECRET);

    let mut envelope = read_envelope(path);
    envelope["signature_version"] = json!(1);
    write_package(path, envelope);

    assert!(matches!(
        preview_err(db, path),
        AppError::BusinessLogic(_)
    ));
}

#[test]
fn unsupported_schema_version_is_rejected() {
    let (state, temp) = common::create_test_state();
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();
    export(db, path, WILAYA_SECRET);

    let mut envelope = read_envelope(path);
    envelope["package"]["schema_version"] = json!(2);
    write_package(path, envelope);

    assert!(matches!(
        preview_err(db, path),
        AppError::BusinessLogic(_)
    ));
}

// ─── §13: legacy HMAC envelopes can never authorize ─────────────────────────

#[test]
fn legacy_v2_hmac_envelope_is_rejected() {
    let (state, temp) = common::create_test_state();
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let path = temp.path().join("legacy.sync");
    let path = path.to_str().unwrap();

    // Old V2 on-disk shape: package + hmac_hex, no signature fields.
    let legacy = json!({
        "package": {
            "schema_version": 2,
            "closed_year": 2025,
            "opened_year": 2026,
            "closure_timestamp_utc": "2026-01-01T00:00:00Z",
            "closure_authority_node_id": "WILAYA-01",
            "closure_authority_username": "admin",
            "fiscal_transition_id": uuid::Uuid::new_v4().to_string(),
            "package_created_at": "2026-01-01T00:00:00Z",
            "signing_key_id": "legacy-hmac-key",
            "authorized_execution_window": {
                "not_before": "2026-01-01T00:00:00Z",
                "expires_at": "2026-02-01T00:00:00Z"
            },
            "package_fingerprint": "AA"
        },
        "hmac_hex": "deadbeef"
    });
    write_package(path, legacy);

    // Either JSON-shape rejection or fail-closed verification — never success.
    let result = FiscalClosurePackageService::new(db.executor()).preview_closure_package(path);
    assert!(result.is_err(), "legacy HMAC envelope must be rejected");
}

// ─── §14: key separation — only the trusted ACTIVE WILAYA can authorize ─────

#[test]
fn unit_identity_cannot_forge_wilaya_package() {
    let (state, temp) = common::create_test_state();
    let db_guard = state.db.lock().unwrap();
    let db = db_guard.as_ref().unwrap();
    // Only a UNIT identity exists on this node — no WILAYA cert at all.
    let unit_id = seed_cert(db, SubjectType::Unit, WILAYA_SECRET, CredentialStatus::Active, None);
    let s = signer(WILAYA_SECRET);
    let pkg = build_package(unit_id, &s.public_key_hex(), 2025);
    let path = temp.path().join("unit-forged.sync");
    let path = path.to_str().unwrap();
    FiscalClosurePackageService::new(db.executor())
        .export_to_file(&pkg, &s, path)
        .unwrap();

    assert!(matches!(
        preview_err(db, path),
        AppError::BusinessLogic(_)
    ));
}

#[test]
fn unrelated_wilaya_cannot_authorize() {
    let (state, temp) = common::create_test_state();
    let db_guard = state.db.lock().unwrap();
    let db = db_guard.as_ref().unwrap();
    // ACTIVE anchor = WILAYA keyed with OTHER_SECRET; the package is signed by
    // a second WILAYA (WILAYA_SECRET) that is known but not the anchor.
    let _anchor = common::seed_wilaya_identity(db, OTHER_SECRET);
    let attacker_id = seed_cert(
        db,
        SubjectType::Wilaya,
        WILAYA_SECRET,
        CredentialStatus::Superseded,
        None,
    );
    let s = signer(WILAYA_SECRET);
    let pkg = build_package(attacker_id, &s.public_key_hex(), 2025);
    let path = temp.path().join("other-wilaya.sync");
    let path = path.to_str().unwrap();
    FiscalClosurePackageService::new(db.executor())
        .export_to_file(&pkg, &s, path)
        .unwrap();

    assert!(matches!(
        preview_err(db, path),
        AppError::BusinessLogic(_)
    ));
}

#[test]
fn hmac_env_config_cannot_authorize_packages() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("GRPC_PACKAGE_SIGNING_KEY", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=");
    std::env::set_var("GRPC_ACTIVE_SIGNING_KEY_ID", "legacy-hmac-key");

    let (state, temp) = common::create_test_state();
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();

    // With HMAC env vars set, an Ed25519-signed package still works (env never
    // consulted), and a forged HMAC-style envelope still fails.
    export(db, path, WILAYA_SECRET);
    let service = FiscalClosurePackageService::new(db.executor());
    let preview = service.preview_closure_package(path);
    assert!(preview.is_ok(), "env vars must not influence Ed25519 verify");

    let legacy_path = temp.path().join("legacy.sync");
    let legacy_path = legacy_path.to_str().unwrap();
    let legacy = json!({
        "package": {
            "schema_version": 2,
            "closed_year": 2025,
            "opened_year": 2026,
            "closure_timestamp_utc": "2026-01-01T00:00:00Z",
            "closure_authority_node_id": "WILAYA-01",
            "closure_authority_username": "admin",
            "fiscal_transition_id": uuid::Uuid::new_v4().to_string(),
            "package_created_at": "2026-01-01T00:00:00Z",
            "signing_key_id": "legacy-hmac-key",
            "authorized_execution_window": {
                "not_before": "2026-01-01T00:00:00Z",
                "expires_at": "2026-02-01T00:00:00Z"
            },
            "package_fingerprint": "AA"
        },
        "hmac_hex": "deadbeef"
    });
    write_package(legacy_path, legacy);
    assert!(service.preview_closure_package(legacy_path).is_err());

    std::env::remove_var("GRPC_PACKAGE_SIGNING_KEY");
    std::env::remove_var("GRPC_ACTIVE_SIGNING_KEY_ID");
}

#[test]
fn export_does_not_require_any_env_signing_secret() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("GRPC_PACKAGE_SIGNING_KEY");
    std::env::remove_var("GRPC_ACTIVE_SIGNING_KEY_ID");

    let (state, temp) = common::create_test_state();
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();

    // No env signing key configured → export and verify still succeed via the
    // WILAYA node identity (no zero-key fallback, no env dependency).
    export(db, path, WILAYA_SECRET);
    assert!(FiscalClosurePackageService::new(db.executor())
        .preview_closure_package(path)
        .is_ok());
}

// ─── §13: replay protection & retention semantics unchanged ────────────────

#[test]
fn replay_protection_is_unchanged() {
    let (state, temp) = common::create_test_state();
    common::clear_fiscal_status(&state);
    common::seed_fiscal_year_open(&state, 2025);
    {
        let guard = state.db.lock().unwrap();
        let db = guard.as_ref().unwrap();
        db.executor().settings().set_current_year(2025).unwrap();
    }

    let db_guard = state.db.lock().unwrap();
    let db = db_guard.as_ref().unwrap();
    let path = temp.path().join("pkg.sync");
    let path = path.to_str().unwrap();
    let issuer_id = export(db, path, WILAYA_SECRET);

    let service = FiscalClosurePackageService::new(db.executor());
    service
        .apply_closure_package(path, "system", "user1")
        .expect("first apply succeeds");

    // Same file re-applied → replay rejected (same fiscal_transition_id).
    let replay = service.apply_closure_package(path, "system", "user1");
    let err_str = format!("{:?}", replay);
    assert!(
        replay.is_err(),
        "replay must be rejected; got: {err_str}"
    );
    assert!(
        err_str.contains("OperationNotPermitted") || err_str.contains("DuplicateSyncPackage"),
        "unexpected replay error: {err_str}"
    );

    // Registry retains the applied package with the Ed25519 public key id.
    let registry = db
        .executor()
        .fiscal_package_registry()
        .list_all()
        .unwrap();
    assert_eq!(registry.len(), 1);
    assert_eq!(registry[0].transition_id, read_envelope(path)["package"]["fiscal_transition_id"].as_str().unwrap());
    assert_eq!(
        registry[0].signing_key_id,
        read_envelope(path)["package"]["signing_key_id"].as_str().unwrap()
    );
    assert_eq!(registry[0].signing_key_id.len(), 64, "must be Ed25519 public key hex");
    assert!(registry[0].applied_at.is_some());

    // Retention lifecycle unchanged: archive keeps the record queryable.
    db.executor()
        .fiscal_package_registry()
        .mark_archived(&registry[0].transition_id, true)
        .unwrap();
    let registry = db
        .executor()
        .fiscal_package_registry()
        .list_all()
        .unwrap();
    assert!(registry[0].archived);
    assert_eq!(issuer_id.to_string(), read_envelope(path)["package"]["issuer_identity_id"].as_str().unwrap());
}
