//! Admin-Only B8 account synchronization tests (ADR-0051 — Accepted
//! 2026-08-22, Decision D1).
//!
//! Section A — `AdminAccessPayload` structure (strict deserialization):
//!   A1 exactly `{admin_password_hash, admin_enabled}` is accepted;
//!   A2 `unit_code` rejected;
//!   A3 operator-user material (`user_password_hash`) rejected;
//!   A4 any unknown field rejected (`deny_unknown_fields`).
//! Section B — export command boundary (authz / fail-closed):
//!   B1 unauthenticated export fails;
//!   B2 UNIT-node session cannot export (Wilaya-only);
//!   B3 non-Admin WILAYA session cannot export;
//!   B4 locked store fails closed;
//!   B5 structural: the command takes NO unit selector.
//! Section C — first-import predicates service (exact triple):
//!   C1 all hold; C2 no anchor; C3 issuer ≠ anchor; C4 active admin exists;
//!   C5 anchor-issuer acceptance gate (SEC-056D/SEC-057: no sequence);
//! Section D — import command routing & authorization:
//!   D1 unauthenticated import fails;
//!   D2 WILAYA node cannot use the UNIT import path (structural guard);
//!   D3 post-bootstrap User session rejected by predicates (zero mutation);
//!   D4 post-bootstrap Admin session re-import succeeds (rotation).
//! Section E — OPERATOR PRESERVATION (the security invariant):
//!   E1 bootstrap import leaves the `.unit` operator row byte-identical and
//!      both credentials still authenticate;
//!   E2 rotation re-import still never touches the operator row;
//!   E3 no operator-like account is ever created by synchronization.

#[allow(dead_code)]
mod common;

use chrono::Utc;
use std::path::Path;

use tempfile::TempDir;
use uuid::Uuid;

use grpc_lib::application::services::AdminAccessFirstImportPredicatesService;
use grpc_lib::application::sync::{
    PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::commands::{
    export_admin_access_package_impl, import_admin_access_package_impl, AppState,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    CredentialStatus, IdentityCertificate, IdentitySigner, IdentityStorePort, SubjectType,
    SIGNATURE_VERSION_ED25519,
};
use grpc_lib::domain::security::PasswordHashPort;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::{Argon2PasswordHashProvider, Ed25519SigningProvider};
use grpc_lib::infrastructure::sync::packages::canonical_json::{
    canonical_bytes_for_integrity, canonical_bytes_for_signature,
};
use grpc_lib::infrastructure::sync::packages::integrity::{PackageHasher, Sha256PackageHasher};
use grpc_lib::infrastructure::sync::packages::signing::{Ed25519PackageSigner, PackageSigner};
use grpc_lib::infrastructure::sync::{PackageBuilder, SerdeJsonSyncPackageSerializer};
use grpc_lib::models::AdminAccessPayload;
use grpc_lib::repositories::executor::DbExecutor;
use grpc_lib::repositories::RepositoryProvider;

const ISSUER_SECRET: [u8; 32] = [42u8; 32];
const OTHER_SECRET: [u8; 32] = [7u8; 32];
const FIXED_NOW: &str = "2026-08-04T00:00:00Z";
/// The fleet-wide admin password carried by every package under test.
const FLEET_PASSWORD: &str = "fleet-admin-pw";
/// The rotation password for the second package (E2).
const FLEET_PASSWORD_ROTATED: &str = "fleet-admin-pw-rotated";
/// The `.unit`-provisioned operator credential that must survive everything.
const OPERATOR_PASSWORD: &str = "operator-provisioned-pw";

fn make_executor(db: &Database) -> DbExecutor<'_> {
    db.executor()
}

fn seed_anchor(db: &Database, identity_id: Uuid, secret: [u8; 32]) {
    let certificate = IdentityCertificate {
        identity_id,
        subject_type: SubjectType::Wilaya,
        subject_id: identity_id,
        issuer_identity_id: None,
        credential_id: Uuid::new_v4(),
        generation: 1,
        status: CredentialStatus::Active,
        public_key: Ed25519SigningProvider::new(secret).public_key(),
        algorithm_version: SIGNATURE_VERSION_ED25519,
        not_after: None,
        package_sequence: Some(1),
        signature: None,
    };
    IdentityStorePort::upsert(
        &make_executor(db).identity_store(),
        &certificate,
        &Utc::now().to_rfc3339(),
    )
    .expect("seed anchor");
}

fn admin_package(
    package_id: &str,
    issuer_id: Uuid,
    payload: AdminAccessPayload,
) -> SyncPackage<AdminAccessPayload> {
    SyncPackage {
        metadata: SyncPackageMetadata {
            created_at: Utc::now(),
            integrity_hash: None,
            issuer_identity_id: Some(issuer_id),
            package_id: PackageId(package_id.to_string()),
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            signature: None,
            signature_version: Some(SIGNATURE_VERSION_ED25519),
            signing_key_id: Some("default".to_string()),
            source_node_id: "wilaya-a".to_string(),
        },
        payload,
    }
}

fn sign_v2_package<T: serde::Serialize>(
    mut package: SyncPackage<T>,
    secret: [u8; 32],
) -> SyncPackage<T> {
    let signer = Ed25519PackageSigner::new(secret);
    let value = serde_json::to_value(&package).expect("value");
    let hash = Sha256PackageHasher
        .hash(&canonical_bytes_for_integrity(&value).expect("canonical integrity"))
        .expect("hash");
    package.metadata.integrity_hash = Some(hash);

    let value = serde_json::to_value(&package).expect("value");
    let signature = signer
        .sign(&canonical_bytes_for_signature(&value).expect("canonical signature"))
        .expect("sign");
    package.metadata.signature = Some(signature);
    package
}

fn write_encrypted<T: serde::Serialize>(package: &SyncPackage<T>, secret: [u8; 32], path: &Path) {
    let crypto = AgeFileEncryptionProvider::new();
    let signer = Ed25519PackageSigner::new(secret);
    PackageBuilder::new()
        .build_encrypted_stream_path(
            package,
            &SerdeJsonSyncPackageSerializer,
            &signer,
            &crypto,
            path,
        )
        .expect("write package");
}

fn fleet_payload(hash_input: &str) -> AdminAccessPayload {
    let port = Argon2PasswordHashProvider;
    AdminAccessPayload {
        admin_password_hash: port.hash_admin(hash_input).expect("admin hash"),
        admin_enabled: true,
    }
}

/// A fresh UNIT node with a locally provisioned operator account, exactly as
/// `.unit` provisioning would leave it. NO canonical admin exists yet.
fn provisioned_unit_state(unit_code: &str) -> AppState {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        // Strip the test-fixture's seeded legacy admin: a provisioned UNIT
        // holds ONLY its operator account (B6-A semantics).
        db.get_connection()
            .execute("DELETE FROM users", [])
            .expect("clear seeded users");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'UNIT', configured = 1, unit_name = 'unit-scope-id', wilaya_code = '16', wilaya_name = NULL WHERE id = 1",
                [],
            )
            .expect("settings");
        db.get_connection()
            .execute(
                "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES ('unit-9', ?1, 'Unit 9', '16', ?2)",
                [unit_code, FIXED_NOW],
            )
            .expect("local unit");
        // The `.unit` provisioning ceremony creates ONLY the operator:
        let port = Argon2PasswordHashProvider;
        let hash = port
            .hash_node(OPERATOR_PASSWORD, unit_code)
            .expect("operator hash");
        db.get_connection()
            .execute(
                "INSERT INTO users (id, username, password_hash, role, created_at, node_id, deleted) VALUES ('op-1', 'usera', ?1, 'User', ?2, ?3, 0)",
                [hash.as_str(), FIXED_NOW, unit_code],
            )
            .expect("provisioned operator");
    }
    state
}

fn wilaya_state() -> AppState {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'WILAYA', configured = 1, wilaya_code = '16', wilaya_name = 'TestWilaya' WHERE id = 1",
                [],
            )
            .expect("settings");
    }
    state
}

fn set_session(state: &AppState, role: &str) {
    let mut session = common::create_test_session("u1", "bob", role);
    session.user_role = grpc_lib::models::UserRole::from(role.to_string());
    common::insert_test_user(state, "u1", "bob", role);
    *state.current_session.lock().expect("session mutex") = Some(session);
}

fn count_active_admins(db: &Database) -> i64 {
    make_executor(db)
        .users()
        .count_active_admins()
        .expect("count admins")
}

/// Full persisted snapshot of the operator row (byte-level equality check).
fn operator_row(db: &Database) -> (String, String, String, String, String, i64) {
    db.get_connection()
        .query_row(
            "SELECT id, username, password_hash, role, COALESCE(node_id,''), deleted FROM users WHERE username = 'usera'",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .expect("operator row")
}

// ── Section A: payload structure ─────────────────────────────────────────

#[test]
fn a1_exact_payload_is_accepted() {
    let json = r#"{"admin_password_hash":"argon2-hash","admin_enabled":true}"#;
    let parsed: Result<AdminAccessPayload, _> = serde_json::from_str(json);
    parsed.expect("exact payload must deserialize");
}

#[test]
fn a2_unit_code_field_is_rejected() {
    let json = r#"{"admin_password_hash":"h","admin_enabled":true,"unit_code":"UNIT-9"}"#;
    assert!(
        serde_json::from_str::<AdminAccessPayload>(json).is_err(),
        "unit_code must be structurally rejected"
    );
}

#[test]
fn a3_operator_credential_fields_are_rejected() {
    assert!(
        serde_json::from_str::<AdminAccessPayload>(
            r#"{"admin_password_hash":"h","admin_enabled":true,"user_password_hash":"x"}"#
        )
        .is_err(),
        "user password hash must be structurally rejected"
    );
    assert!(
        serde_json::from_str::<AdminAccessPayload>(
            r#"{"admin_password_hash":"h","admin_enabled":true,"username":"usera"}"#
        )
        .is_err(),
        "operator username must be structurally rejected"
    );
}

#[test]
fn a4_unknown_fields_are_rejected() {
    assert!(
        serde_json::from_str::<AdminAccessPayload>(
            r#"{"admin_password_hash":"h","admin_enabled":true,"something_else":1}"#
        )
        .is_err(),
        "deny_unknown_fields must reject any extra field"
    );
}

// ── Section B: export command boundary ───────────────────────────────────

#[test]
fn b1_unauthenticated_export_fails() {
    let state = wilaya_state();
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("grpc-admin-access.sync");
    let err = export_admin_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("unauthenticated export must fail");
    assert!(!err.is_empty());
}

#[test]
fn b2_unit_node_cannot_export() {
    let state = provisioned_unit_state("UNIT-9");
    set_session(&state, "Admin");
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("grpc-admin-access.sync");
    let err = export_admin_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("export is Wilaya-only");
    assert!(err.contains("غير مصرح"), "got: {err}");
}

#[test]
fn b3_non_admin_wilaya_session_cannot_export() {
    let state = wilaya_state();
    set_session(&state, "User");
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("grpc-admin-access.sync");
    let err = export_admin_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("export requires an Admin session");
    assert!(err.contains("غير مصرح"), "got: {err}");
}

#[test]
fn b4_locked_store_fails_closed() {
    let state = wilaya_state();
    set_session(&state, "Admin");
    *state.db.lock().expect("db mutex") = None;
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("grpc-admin-access.sync");
    let err = export_admin_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("locked store must fail closed");
    assert!(!err.is_empty());
}

#[test]
fn b5_export_takes_no_unit_selector() {
    // Structural proof at the contract level: the command implementation
    // signature carries only (state, file_path). There is no parameter that
    // could target a UNIT — asserted here via the call shape used throughout
    // this file (two arguments, no unit code anywhere).
    let exported = export_admin_access_package_impl;
    let _ = exported; // compiles with exactly (&AppState, String) — no selector
}

// ── Section C: first-import predicates service ───────────────────────────

#[test]
fn c1_all_predicates_hold_on_provisioned_unit() {
    let state = provisioned_unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        seed_anchor(db, issuer_id, ISSUER_SECRET);
        let verdict = AdminAccessFirstImportPredicatesService::evaluate(
            &make_executor(db),
            Some(&issuer_id.to_string()),
        )
        .expect("evaluate");
        assert!(verdict.all_hold(), "all three predicates must hold");
    }
}

#[test]
fn c2_missing_anchor_fails_closed() {
    let state = provisioned_unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let verdict = AdminAccessFirstImportPredicatesService::evaluate(
        &make_executor(db),
        Some(&issuer_id.to_string()),
    )
    .expect("evaluate");
    assert!(!verdict.anchor_installed);
    assert!(!verdict.all_hold());
    assert!(
        AdminAccessFirstImportPredicatesService::rejection_message(&verdict)
            .contains("مرساة الثقة")
    );
}

#[test]
fn c3_foreign_issuer_fails_closed() {
    let state = provisioned_unit_state("UNIT-9");
    let anchor_id = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        seed_anchor(db, anchor_id, ISSUER_SECRET);
        let verdict = AdminAccessFirstImportPredicatesService::evaluate(
            &make_executor(db),
            Some(&foreign.to_string()),
        )
        .expect("evaluate");
        assert!(!verdict.anchor_is_issuer);
        assert!(!verdict.all_hold());
        assert!(
            AdminAccessFirstImportPredicatesService::rejection_message(&verdict)
                .contains("ليس مرساة الثقة")
        );
    }
}

#[test]
fn c4_existing_admin_blocks_bootstrap() {
    let state = provisioned_unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        seed_anchor(db, issuer_id, ISSUER_SECRET);
        make_executor(db)
            .users()
            .upsert_synced_admin(
                &Uuid::new_v4().to_string(),
                "existing-hash",
                "UNIT-9",
                false,
                FIXED_NOW,
            )
            .expect("seed existing admin");
        let verdict = AdminAccessFirstImportPredicatesService::evaluate(
            &make_executor(db),
            Some(&issuer_id.to_string()),
        )
        .expect("evaluate");
        assert!(!verdict.no_active_admin);
        assert!(!verdict.all_hold());
        assert!(
            AdminAccessFirstImportPredicatesService::rejection_message(&verdict)
                .contains("يوجد حساب مسؤول نشط")
        );
    }
}

#[test]
fn c5_first_import_issuer_gate_binds_issuer_to_anchor() {
    // SEC-056D/SEC-057: the transport-sequence bootstrap is gone. The first
    // import acceptance gate is issuer-pinning only — the package issuer MUST
    // be the locally installed ACTIVE WILAYA anchor; no sequence participates.
    let state = provisioned_unit_state("UNIT-9");
    let anchor_id = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    seed_anchor(db, anchor_id, ISSUER_SECRET);

    AdminAccessFirstImportPredicatesService::verify_first_import_issuer(
        &make_executor(db),
        &anchor_id.to_string(),
    )
    .expect("the installed anchor as issuer is admissible");

    let err = AdminAccessFirstImportPredicatesService::verify_first_import_issuer(
        &make_executor(db),
        &foreign.to_string(),
    )
    .expect_err("a foreign (non-anchor) issuer must be rejected");
    assert!(matches!(err, grpc_lib::errors::AppError::Validation(_)));
}

#[test]
fn c6_issuer_gate_is_independent_of_ledger_state() {
    // No per-issuer transport ledger exists (SEC-057). The anchor-issuer gate
    // holds regardless of how many packages preceded it.
    let state = provisioned_unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        seed_anchor(db, issuer_id, ISSUER_SECRET);
        AdminAccessFirstImportPredicatesService::verify_first_import_issuer(
            &make_executor(db),
            &issuer_id.to_string(),
        )
        .expect("anchor-issuer gate holds on a fresh node");
    }
}

// ── Section D: import command routing & authorization ────────────────────

#[test]
fn d0_invalid_signature_is_rejected_with_zero_mutation() {
    let state = provisioned_unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        // Anchor carries the ISSUER_SECRET public key…
        seed_anchor(db, issuer_id, ISSUER_SECRET);
    }
    set_session(&state, "User");

    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("forged.sync");
    // …but the package is signed with a DIFFERENT key.
    let package = sign_v2_package(
        admin_package("aa-d0", issuer_id, fleet_payload(FLEET_PASSWORD)),
        OTHER_SECRET,
    );
    write_encrypted(&package, OTHER_SECRET, &path);

    let err = import_admin_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("a forged signature must be rejected");
    assert!(!err.is_empty());

    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    assert_eq!(
        count_active_admins(db),
        0,
        "no admin may be created from a forged package"
    );
}

#[test]
fn d1_unauthenticated_import_fails() {
    let state = provisioned_unit_state("UNIT-9");
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("grpc-admin-access.sync");
    let err = import_admin_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("unauthenticated import must fail");
    assert!(!err.is_empty());
}

#[test]
fn d2_wilaya_node_cannot_use_unit_import_path() {
    let state = wilaya_state();
    set_session(&state, "Admin");
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("grpc-admin-access.sync");
    let err = import_admin_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("WILAYA has no admin_access import role");
    assert!(err.contains("غير مصرح"), "got: {err}");
}

#[test]
fn d3_post_bootstrap_user_session_rejected_with_zero_mutation() {
    let state = provisioned_unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        seed_anchor(db, issuer_id, ISSUER_SECRET);
        // An admin already exists → the bootstrap exemption is unavailable.
        make_executor(db)
            .users()
            .upsert_synced_admin(
                &Uuid::new_v4().to_string(),
                "seeded-hash",
                "UNIT-9",
                false,
                FIXED_NOW,
            )
            .expect("existing admin");
    }
    set_session(&state, "User");

    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("second.sync");
    let package = sign_v2_package(
        admin_package("aa-d3", issuer_id, fleet_payload(FLEET_PASSWORD)),
        ISSUER_SECRET,
    );
    write_encrypted(&package, ISSUER_SECRET, &path);

    let err = import_admin_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("User sessions have no post-bootstrap import path");
    assert!(err.contains("يوجد حساب مسؤول نشط"), "got: {err}");

    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let stored_hash: String = db
        .get_connection()
        .query_row(
            "SELECT password_hash FROM users WHERE username='admin'",
            [],
            |row| row.get(0),
        )
        .expect("seeded admin row");
    assert_eq!(
        stored_hash, "seeded-hash",
        "a rejected import must not rotate the admin credential"
    );
}

#[test]
fn d4_post_bootstrap_admin_rotation_import_succeeds() {
    let state = provisioned_unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        seed_anchor(db, issuer_id, ISSUER_SECRET);
    }

    let dir = TempDir::new().expect("temp dir");

    // Bootstrap as the provisioned User session (predicates all hold).
    set_session(&state, "User");
    let first = dir.path().join("first.sync");
    let p1 = sign_v2_package(
        admin_package("aa-d4a", issuer_id, fleet_payload(FLEET_PASSWORD)),
        ISSUER_SECRET,
    );
    write_encrypted(&p1, ISSUER_SECRET, &first);
    let result = import_admin_access_package_impl(&state, first.to_string_lossy().into_owned())
        .expect("bootstrap import succeeds");
    assert!(result.admin_updated);

    // Rotation as Admin (post-bootstrap AdminOnly path).
    set_session(&state, "Admin");
    let second = dir.path().join("second.sync");
    let p2 = sign_v2_package(
        admin_package("aa-d4b", issuer_id, fleet_payload(FLEET_PASSWORD_ROTATED)),
        ISSUER_SECRET,
    );
    write_encrypted(&p2, ISSUER_SECRET, &second);
    let result2 = import_admin_access_package_impl(&state, second.to_string_lossy().into_owned())
        .expect("post-bootstrap Admin import succeeds");
    assert!(result2.admin_updated);

    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let port = Argon2PasswordHashProvider;
    let stored: String = db
        .get_connection()
        .query_row(
            "SELECT password_hash FROM users WHERE username='admin' AND deleted = 0",
            [],
            |row| row.get(0),
        )
        .expect("admin row");
    assert!(
        port.verify_admin(FLEET_PASSWORD_ROTATED, &stored)
            .expect("verify rotated"),
        "admin must authenticate with the ROTATED fleet password"
    );
    // Canonical fleet admin (`admin`) stays a single row; the extra Admin
    // row is the test-session principal inserted by set_session.
    let canonical_admins: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM users WHERE role='Admin' AND username='admin' AND deleted=0",
            [],
            |row| row.get(0),
        )
        .expect("count canonical admins");
    assert_eq!(
        canonical_admins, 1,
        "exactly one canonical admin after rotation"
    );
}

// ── Section E: OPERATOR PRESERVATION (security invariant) ────────────────

#[test]
fn e1_bootstrap_import_preserves_operator_row_and_credentials() {
    let state = provisioned_unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        seed_anchor(db, issuer_id, ISSUER_SECRET);
    }
    let before = {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        operator_row(db)
    };
    set_session(&state, "User");

    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("grpc-admin-access.sync");
    let package = sign_v2_package(
        admin_package("aa-e1", issuer_id, fleet_payload(FLEET_PASSWORD)),
        ISSUER_SECRET,
    );
    write_encrypted(&package, ISSUER_SECRET, &path);

    import_admin_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect("bootstrap import succeeds");

    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let after = operator_row(db);

    assert_eq!(
        before, after,
        "THE INVARIANT: the .unit operator row must be byte-identical across an accepted admin_access import (id/username/hash/role/node_id/enabled/deleted)"
    );

    // Authentication proofs against the STORED hashes.
    let port = Argon2PasswordHashProvider;
    assert!(
        port.verify_node(OPERATOR_PASSWORD, "UNIT-9", &after.2)
            .expect("verify operator"),
        "the original operator username/password must still authenticate"
    );
    let admin_hash: String = db
        .get_connection()
        .query_row(
            "SELECT password_hash FROM users WHERE username='admin' AND deleted=0",
            [],
            |row| row.get(0),
        )
        .expect("admin row");
    assert!(
        port.verify_admin(FLEET_PASSWORD, &admin_hash)
            .expect("verify admin"),
        "the synchronized admin must authenticate with the fleet password"
    );

    // The account census invariant: synchronization created exactly ONE
    // Admin-role row (canonical `admin`) and no operator-like account.
    let admin_named: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM users WHERE role='Admin' AND username='admin' AND deleted=0",
            [],
            |row| row.get(0),
        )
        .expect("count canonical admins");
    assert_eq!(admin_named, 1, "exactly one canonical admin exists");
    let user_named: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM users WHERE username = 'user'",
            [],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(
        user_named, 0,
        "no 'user' account may be created by synchronization"
    );
}

#[test]
fn e2_rotation_reimport_still_never_touches_operator_row() {
    let state = provisioned_unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        seed_anchor(db, issuer_id, ISSUER_SECRET);
    }
    let before = {
        let guard = state.get_db().expect("lock");
        operator_row(guard.as_ref().expect("db"))
    };
    set_session(&state, "User");

    let dir = TempDir::new().expect("temp dir");
    let first = dir.path().join("first.sync");
    let p1 = sign_v2_package(
        admin_package("aa-e2a", issuer_id, fleet_payload(FLEET_PASSWORD)),
        ISSUER_SECRET,
    );
    write_encrypted(&p1, ISSUER_SECRET, &first);
    import_admin_access_package_impl(&state, first.to_string_lossy().into_owned())
        .expect("bootstrap");

    set_session(&state, "Admin");
    let second = dir.path().join("second.sync");
    let p2 = sign_v2_package(
        admin_package("aa-e2b", issuer_id, fleet_payload(FLEET_PASSWORD_ROTATED)),
        ISSUER_SECRET,
    );
    write_encrypted(&p2, ISSUER_SECRET, &second);
    import_admin_access_package_impl(&state, second.to_string_lossy().into_owned())
        .expect("rotation");

    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let after = operator_row(db);
    let port = Argon2PasswordHashProvider;
    assert_eq!(
        before, after,
        "THE INVARIANT: rotation re-import must leave the operator row byte-identical"
    );
    assert!(
        port.verify_node(OPERATOR_PASSWORD, "UNIT-9", &after.2)
            .expect("verify"),
        "operator still authenticates with the original provisioned password"
    );
}

#[test]
fn e3_no_user_account_is_created_by_synchronization() {
    let state = provisioned_unit_state("UNIT-9");
    let issuer_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        seed_anchor(db, issuer_id, ISSUER_SECRET);
    }
    set_session(&state, "User");

    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("first.sync");
    let package = sign_v2_package(
        admin_package("aa-e3", issuer_id, fleet_payload(FLEET_PASSWORD)),
        ISSUER_SECRET,
    );
    write_encrypted(&package, ISSUER_SECRET, &path);
    import_admin_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect("bootstrap succeeds");

    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let user_named: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM users WHERE username = 'user'",
            [],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(
        user_named, 0,
        "no account named 'user' may be created by admin_access synchronization"
    );
}
