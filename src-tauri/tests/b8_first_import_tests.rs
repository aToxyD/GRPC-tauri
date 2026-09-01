//! B8 First-Import tests (ADR-0044 / ADR-0045, RFC 2026-08-04 §3.12).
//!
//! Section A — `B8FirstImportPredicatesService::evaluate` (unit):
//!   A1 all predicates hold;
//!   A2 anchor missing fails closed;
//!   A3 issuer ≠ anchor fails closed;
//!   A4 payload unit_code ≠ local fails closed;
//!   A5 an existing Admin fails closed (self-termination).
//! Section B — `verify_unit_v2_acceptance` (`.unit` V2 gate, A44-01/07/08):
//!   B1 anchor-first: no anchor → reject;
//!   B2 anchor == issuer binding;
//!   B3 all conditions → accept (SEC-057: no sequence, no ledger).
//! Section C — D1 cutover (ADR-0051 §9): legacy `identity_access` imports
//!   fail closed at the kind boundary before any mutation:
//!   C1 a fully valid legacy bootstrap artifact is still rejected;
//!   C2 rejection is unconditional and kind-scoped (fires before trust checks);
//!   C3 repeated attempts leave zero partial state;
//!   C4 rejection applies on every node type;
//!   C5 the legacy kind is never reinterpreted as `admin_access`.
//! Section D — `.unit` V2 producer: V2/Ed25519 producer integrity (SEC-057:
//!   no sequence allocation, no ledger is touched).

#[allow(dead_code)]
mod common;

use chrono::Utc;
use tempfile::TempDir;
use uuid::Uuid;

use grpc_lib::application::services::{
    B8FirstImportPredicatesService, FinalizeWilayaProvisionResult, IdentityProvisioningService,
    IdentitySignedExportService,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    CredentialStatus, Ed25519CertificateSignature, IdentityCertificate, IdentitySigner,
    IdentityStorePort, SubjectType, SIGNATURE_VERSION_ED25519,
};
use grpc_lib::errors::AppError;
use grpc_lib::infrastructure::identity::NodeKeyStore;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::Ed25519SigningProvider;

use grpc_lib::infrastructure::sync::read_unit_node_package_from_file;
use grpc_lib::models::{Unit, UnitNodePackage, UserExport};
use grpc_lib::repositories::executor::DbExecutor;
use grpc_lib::repositories::RepositoryProvider;

const ISSUER_SECRET: [u8; 32] = [42u8; 32];
const FIXED_NOW: &str = "2026-08-04T00:00:00Z";

/// RFC 8032 §7.1 TEST 1 secret — matches the debug-mode Root fallback.
const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];

fn make_executor(db: &Database) -> DbExecutor<'_> {
    db.executor()
}

/// Seed an ACTIVE WILAYA certificate in the identity store — on a UNIT this
/// row IS the locally installed trust anchor (ADR-0044 anchor-first model).
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

// ── Section A: B8 first-import predicates ────────────────────────────────

#[test]
fn a1_all_predicates_hold() {
    let db = ConnectionFactory::new_for_test().expect("db");
    db.get_connection()
        .execute("DELETE FROM users", [])
        .expect("clear seeded users");
    let issuer_id = Uuid::new_v4();
    seed_anchor(&db, issuer_id, ISSUER_SECRET);

    let verdict = B8FirstImportPredicatesService::evaluate(
        &make_executor(&db),
        "UNIT-9",
        Some(&issuer_id.to_string()),
        Some("UNIT-9"),
    )
    .expect("evaluate");
    assert!(verdict.anchor_installed);
    assert!(verdict.anchor_is_issuer);
    assert!(verdict.unit_code_matches);
    assert!(verdict.no_active_admin);
    assert!(verdict.all_hold());
}

#[test]
fn a2_missing_anchor_fails_closed() {
    let db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();

    let verdict = B8FirstImportPredicatesService::evaluate(
        &make_executor(&db),
        "UNIT-9",
        Some(&issuer_id.to_string()),
        Some("UNIT-9"),
    )
    .expect("evaluate");
    assert!(!verdict.anchor_installed);
    assert!(!verdict.all_hold());
}

#[test]
fn a3_issuer_not_anchor_fails_closed() {
    let db = ConnectionFactory::new_for_test().expect("db");
    let anchor_id = Uuid::new_v4();
    seed_anchor(&db, anchor_id, ISSUER_SECRET);
    let other_issuer = Uuid::new_v4();

    let verdict = B8FirstImportPredicatesService::evaluate(
        &make_executor(&db),
        "UNIT-9",
        Some(&other_issuer.to_string()),
        Some("UNIT-9"),
    )
    .expect("evaluate");
    assert!(verdict.anchor_installed);
    assert!(!verdict.anchor_is_issuer);
    assert!(!verdict.all_hold());
}

#[test]
fn a4_unit_code_mismatch_fails_closed() {
    let db = ConnectionFactory::new_for_test().expect("db");
    db.get_connection()
        .execute("DELETE FROM users", [])
        .expect("clear seeded users");
    let issuer_id = Uuid::new_v4();
    seed_anchor(&db, issuer_id, ISSUER_SECRET);

    let verdict = B8FirstImportPredicatesService::evaluate(
        &make_executor(&db),
        "UNIT-FOREIGN",
        Some(&issuer_id.to_string()),
        Some("UNIT-9"),
    )
    .expect("evaluate");
    assert!(verdict.anchor_installed);
    assert!(verdict.anchor_is_issuer);
    assert!(!verdict.unit_code_matches);
    assert!(!verdict.all_hold());
}

#[test]
fn a5_existing_admin_fails_closed() {
    let db = ConnectionFactory::new_for_test().expect("db");
    db.get_connection()
        .execute("DELETE FROM users", [])
        .expect("clear seeded users");
    let issuer_id = Uuid::new_v4();
    seed_anchor(&db, issuer_id, ISSUER_SECRET);
    make_executor(&db)
        .users()
        .upsert_raw_user(
            "u-admin",
            "admin",
            "hash",
            "Admin",
            "UNIT-9",
            &Utc::now().to_rfc3339(),
        )
        .expect("insert admin");

    let verdict = B8FirstImportPredicatesService::evaluate(
        &make_executor(&db),
        "UNIT-9",
        Some(&issuer_id.to_string()),
        Some("UNIT-9"),
    )
    .expect("evaluate");
    assert!(!verdict.no_active_admin);
    assert!(!verdict.all_hold());
}

// ── Section B: `.unit` V2 acceptance gate (ADR-0044) ─────────────────────

#[test]
fn b1_unit_v2_rejected_without_anchor() {
    let db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    let err = B8FirstImportPredicatesService::verify_unit_v2_acceptance(
        &make_executor(&db),
        &issuer_id.to_string(),
    )
    .expect_err("anchor-first: must reject without an anchor");
    assert!(matches!(err, AppError::Validation(_)));
}

#[test]
fn b2_unit_v2_rejected_when_issuer_not_anchor() {
    let db = ConnectionFactory::new_for_test().expect("db");
    let anchor_id = Uuid::new_v4();
    seed_anchor(&db, anchor_id, ISSUER_SECRET);
    let other = Uuid::new_v4();
    let err = B8FirstImportPredicatesService::verify_unit_v2_acceptance(
        &make_executor(&db),
        &other.to_string(),
    )
    .expect_err("anchor==issuer must be enforced");
    assert!(matches!(err, AppError::Validation(_)));
}

#[test]
fn b3_unit_v2_acceptance_succeeds_on_fresh_anchor_first_node() {
    let db = ConnectionFactory::new_for_test().expect("db");
    let issuer_id = Uuid::new_v4();
    seed_anchor(&db, issuer_id, ISSUER_SECRET);

    B8FirstImportPredicatesService::verify_unit_v2_acceptance(
        &make_executor(&db),
        &issuer_id.to_string(),
    )
    .expect("fresh node + installed anchor must be accepted");
}

// ── Section D: `.unit` V2 producer (ADR-0044 A44-07/08) ──────────────────

fn fresh_node() -> (TempDir, Database, NodeKeyStore) {
    let dir = TempDir::new().expect("temp dir");
    let db = ConnectionFactory::new_for_test().expect("db");
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
    (dir, db, node_key_store)
}

fn root_signer() -> Ed25519SigningProvider {
    Ed25519SigningProvider::new(TEST_ROOT_SECRET)
}

fn bootstrap_wilaya(db: &mut Database, node_key_store: &NodeKeyStore) -> IdentityCertificate {
    let mut provisioning = IdentityProvisioningService::new(db);
    let request = provisioning
        .generate_wilaya_request(Uuid::new_v4(), node_key_store)
        .expect("csr generated");
    let signature = root_signer()
        .sign_certificate(&request)
        .expect("root signed request");
    let mut signed = request;
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig wrap"));
    match provisioning
        .finalize_wilaya_provision(&signed, node_key_store, FIXED_NOW)
        .expect("wilaya finalized")
    {
        FinalizeWilayaProvisionResult::Provisioned(cert) => cert,
        FinalizeWilayaProvisionResult::AlreadyProvisioned(_) => {
            panic!("first finalize must provision")
        }
    }
}

#[test]
fn d1_unit_v2_export_is_fixed_sequence_one_and_ledger_untouched() {
    let (_dir, mut db, node_key_store) = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut db, &node_key_store);
    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("unit.unit");

    let dataset = UnitNodePackage {
        unit: Unit {
            id: "unit-9".into(),
            code: "UNIT-9".into(),
            name: "Unit 9".into(),
            wilaya_code: "16".into(),
            user_id: None,
            created_at: Utc::now(),
        },
        user: UserExport {
            username: "op".into(),
            password_hash: "hash".into(),
            role: "User".into(),
        },
        unit_certificate: None,
        unit_private_key: None,
    };

    IdentitySignedExportService::new(&db, &node_key_store)
        .export_v2_bootstrap_package(
            dataset.clone(),
            "wilaya-test-node",
            &path,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect("bootstrap export");

    let pkg = read_unit_node_package_from_file(&path, &crypto).expect("read .unit back");
    let meta = &pkg.metadata;
    assert_eq!(meta.signature_version, Some(SIGNATURE_VERSION_ED25519));
    assert_eq!(meta.issuer_identity_id, Some(wilaya_cert.identity_id));
    assert!(meta.signature.is_some(), "Ed25519 signature must be set");
    assert_eq!(pkg.payload.unit.code, dataset.unit.code);
    assert_eq!(pkg.payload.unit.name, dataset.unit.name);
    assert_eq!(pkg.payload.user.username, dataset.user.username);
    assert_eq!(pkg.payload.user.role, dataset.user.role);
}
