//! UNIT bootstrap integration tests (RFC §3.12 / B6-A).
//!
//! RFC 2026-08-04-node-identity-trust §3.12 D2 / ADR-0038 / ADR-0039.
//!
//! Covers the strict two-step UNIT bootstrap flow:
//! - UNIT CSR generation resolves the LOCAL subject_id (its own `units` row);
//! - WILAYA-side signing validates the subject (never overrides it) and binds
//!   `issuer_identity_id` to the ACTIVE WILAYA (R5);
//! - the WILAYA certificate is installed as the UNIT's LOCAL TRUST ANCHOR in a
//!   standalone step (never bundled);
//! - `finalize_unit_provision` resolves the issuer via `issuer_identity_id`,
//!   never via `get_active_by_subject_type`, and fails closed on any mismatch;
//! - idempotent re-presentation (B5 style, zero writes);
//! - UNIT status chain: Uninitialized → UnitWaitingForCertificate → UnitActive.
//!
//! The test authority Root uses the RFC 8032 §7.1 TEST 1 keypair — exactly the
//! `#[cfg(debug_assertions)]` development Root fallback in `root_public_key.rs`.

#[allow(dead_code)]
mod common;

use tempfile::TempDir;

use grpc_lib::application::services::{
    FinalizeUnitProvisionResult, FinalizeWilayaProvisionResult, IdentityBootstrapStatusService,
    IdentityProvisioningService, IdentityTrustAnchorService, InstallWilayaCertificateResult,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    CredentialStatus, Ed25519CertificateSignature, IdentityBootstrapState, IdentityCertificate,
    IdentitySignatureVerifier, IdentitySigner, IdentityStorePort, SubjectType,
};
use grpc_lib::infrastructure::identity::{AdminKeyProvider, NodeKeyStore};
use grpc_lib::infrastructure::security::{Ed25519SignatureVerifier, Ed25519SigningProvider};
use grpc_lib::repositories::RepositoryProvider;

/// RFC 8032 §7.1 TEST 1 secret — the matching public key IS the debug-mode
/// development Root fallback (root_public_key.rs). Never a production key.
const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
/// Fixed timestamp for deterministic persistence.
const FIXED_NOW: &str = "2026-08-04T00:00:00Z";
/// A fixed local unit used by the UNIT node and mirrored on the WILAYA node.
const UNIT_ID: &str = "4e1c8f3a-9b2d-4c6e-8f0a-1b2c3d4e5f60";
const UNIT_CODE: &str = "1601";
const UNIT_NAME: &str = "وحدة حماية مدنية 01";
const UNIT_WILAYA: &str = "16";

/// A fresh node directory: real DB + node key store + `.adminkey` provider.
struct Node {
    _dir: TempDir,
    db: Database,
    node_key_store: NodeKeyStore,
    adminkey_provider: AdminKeyProvider,
}

fn fresh_node() -> Node {
    let dir = TempDir::new().expect("temp dir");
    let db = ConnectionFactory::new_for_test().expect("db");
    let node_dir = dir.path().join("node");
    Node {
        _dir: dir,
        db,
        node_key_store: NodeKeyStore::new(node_dir.clone()),
        adminkey_provider: AdminKeyProvider::new(node_dir.join("adminkey")),
    }
}

fn status(node: &Node) -> IdentityBootstrapState {
    IdentityBootstrapStatusService::compute(&node.db, &node.node_key_store, &node.adminkey_provider)
        .expect("status computed")
}

fn root_signer() -> Ed25519SigningProvider {
    Ed25519SigningProvider::new(TEST_ROOT_SECRET)
}

fn unit_uuid() -> uuid::Uuid {
    uuid::Uuid::parse_str(UNIT_ID).expect("unit id uuid")
}

/// Seed a `units` row + mark settings as a configured UNIT node.
fn make_unit_node(node: &mut Node) {
    node.db
        .executor()
        .units()
        .upsert_raw_unit(UNIT_ID, UNIT_CODE, UNIT_NAME, UNIT_WILAYA, FIXED_NOW)
        .expect("unit seeded");
    node.db
        .executor()
        .settings()
        .update_unit_node_settings(UNIT_NAME, UNIT_WILAYA)
        .expect("settings set to UNIT");
}

/// Full offline WILAYA bootstrap (Root-issued). Returns the ACTIVE WILAYA cert.
fn bootstrap_wilaya(node: &mut Node) -> IdentityCertificate {
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    let request = provisioning
        .generate_wilaya_request(uuid::Uuid::new_v4(), &node.node_key_store)
        .expect("csr generated");
    let signature = root_signer()
        .sign_certificate(&request)
        .expect("root signed request");
    let mut signed = request;
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig wrap"));
    match provisioning
        .finalize_wilaya_provision(&signed, &node.node_key_store, FIXED_NOW)
        .expect("wilaya finalized")
    {
        FinalizeWilayaProvisionResult::Provisioned(cert) => cert,
        FinalizeWilayaProvisionResult::AlreadyProvisioned(_) => {
            panic!("first finalize must provision")
        }
    }
}

/// ACTIVE WILAYA cert in the Identity Store (issuer identity for UNIT signing).
fn active_wilaya_cert(node: &Node) -> IdentityCertificate {
    node.db
        .executor()
        .identity_store()
        .get_active_by_subject_type(SubjectType::Wilaya)
        .expect("query")
        .expect("ACTIVE WILAYA present")
}

/// A fully-provisioned WILAYA node that also holds the shared unit row.
fn fresh_wilaya_with_unit() -> Node {
    let mut wilaya = fresh_node();
    bootstrap_wilaya(&mut wilaya);
    wilaya
        .db
        .executor()
        .units()
        .upsert_raw_unit(UNIT_ID, UNIT_CODE, UNIT_NAME, UNIT_WILAYA, FIXED_NOW)
        .expect("unit mirrored on wilaya");
    wilaya
}

/// UNIT side: resolve the local subject_id and generate the UNIT CSR.
fn begin_unit(node: &mut Node) -> IdentityCertificate {
    let provisioning = IdentityProvisioningService::new(&mut node.db);
    let subject_id = provisioning
        .resolve_local_unit_subject_id()
        .expect("local unit subject resolved");
    assert_eq!(subject_id, unit_uuid());
    provisioning
        .generate_identity_request(SubjectType::Unit, subject_id, &node.node_key_store)
        .expect("unit csr")
}

/// WILAYA side: sign a UNIT CSR with the ACTIVE local WILAYA.
fn sign_unit(wilaya_node: &mut Node, csr: &IdentityCertificate) -> IdentityCertificate {
    IdentityProvisioningService::new(&mut wilaya_node.db)
        .sign_unit_identity_request(csr, &wilaya_node.node_key_store)
        .expect("unit csr signed")
}

/// Re-sign `cert` with the WILAYA node's node secret (same issuer as the anchor).
fn resign_with_wilaya(wilaya: &Node, cert: &IdentityCertificate) -> IdentityCertificate {
    let secret = wilaya.node_key_store.read().expect("wilaya node key");
    let signer = Ed25519SigningProvider::new(secret);
    let mut signed = cert.clone();
    signed.signature = Some(
        Ed25519CertificateSignature::try_from(signer.sign_certificate(cert).expect("sig"))
            .expect("wrap"),
    );
    signed
}

/// Build a fully provisioned UNIT node + the signed UNIT cert + the WILAYA node.
/// The WILAYA trust anchor is already installed on the UNIT node.
struct ProvisionedUnit {
    unit: Node,
    signed_unit_cert: IdentityCertificate,
    wilaya: Node,
    wilaya_cert: IdentityCertificate,
}

fn provisioned_unit() -> ProvisionedUnit {
    let mut unit = fresh_node();
    make_unit_node(&mut unit);
    let csr = begin_unit(&mut unit);

    let mut wilaya = fresh_wilaya_with_unit();
    let wilaya_cert = active_wilaya_cert(&wilaya);
    let signed_unit_cert = sign_unit(&mut wilaya, &csr);

    let mut service = IdentityTrustAnchorService::new(unit.db.executor());
    service
        .install_wilaya_certificate(&wilaya_cert, FIXED_NOW)
        .expect("anchor installed");

    ProvisionedUnit {
        unit,
        signed_unit_cert,
        wilaya,
        wilaya_cert,
    }
}

// ---------------------------------------------------------------------------
// UNIT status chain + CSR generation
// ---------------------------------------------------------------------------

#[test]
fn unit_node_walks_unit_status_chain() {
    let mut node = fresh_node();
    // WILAYA chain is the DEFAULT: a bare UNCONFIGURED node is not a UNIT node.
    assert_eq!(status(&node), IdentityBootstrapState::Uninitialized);

    make_unit_node(&mut node);
    assert_eq!(status(&node), IdentityBootstrapState::Uninitialized);

    begin_unit(&mut node);
    assert_eq!(
        status(&node),
        IdentityBootstrapState::UnitWaitingForCertificate
    );
}

#[test]
fn begin_unit_resolves_local_subject_id_and_generates_unsigned_csr() {
    let mut node = fresh_node();
    make_unit_node(&mut node);

    let csr = begin_unit(&mut node);
    assert_eq!(csr.subject_type, SubjectType::Unit);
    assert_eq!(csr.subject_id, unit_uuid());
    assert_eq!(
        csr.issuer_identity_id, None,
        "CSR is unsigned, issuer unbound"
    );
    assert_eq!(csr.signature, None, "CSR is unsigned");
    assert_eq!(csr.generation, 1);
    assert_eq!(csr.status, CredentialStatus::Active);
}

#[test]
fn begin_unit_without_local_unit_row_fails_closed() {
    let mut node = fresh_node();
    let err = IdentityProvisioningService::new(&mut node.db)
        .resolve_local_unit_subject_id()
        .expect_err("must fail without a local unit row");
    assert!(err.to_string().contains("unit"), "got {err:?}");
}

#[test]
fn begin_unit_is_single_shot() {
    let mut node = fresh_node();
    make_unit_node(&mut node);
    begin_unit(&mut node);
    let err = IdentityProvisioningService::new(&mut node.db)
        .generate_identity_request(SubjectType::Unit, unit_uuid(), &node.node_key_store)
        .expect_err("second request must be rejected");
    assert!(err.to_string().contains("already"), "got {err:?}");
}

// ---------------------------------------------------------------------------
// WILAYA-side signing
// ---------------------------------------------------------------------------

#[test]
fn wilaya_signs_unit_csr_binding_issuer_identity() {
    let mut unit_node = fresh_node();
    make_unit_node(&mut unit_node);
    let csr = begin_unit(&mut unit_node);

    let mut wilaya = fresh_wilaya_with_unit();
    let wilaya_cert = active_wilaya_cert(&wilaya);

    let signed = sign_unit(&mut wilaya, &csr);
    assert_eq!(signed.subject_type, SubjectType::Unit);
    assert_eq!(
        signed.issuer_identity_id,
        Some(wilaya_cert.identity_id),
        "issuer must be the ACTIVE WILAYA identity"
    );
    assert!(signed.signature.is_some());
    signed
        .require_signed()
        .expect("signed certificate must be self-consistent");

    // R5: the signature verifies against the declared issuer's public key.
    let verifier = Ed25519SignatureVerifier;
    let valid = verifier
        .verify_certificate(
            &signed,
            &wilaya_cert.public_key,
            signed.signature.as_ref().expect("sig"),
        )
        .expect("verify");
    assert!(valid, "issuer signature must verify (R5)");
}

#[test]
fn sign_rejects_non_unit_request() {
    let mut wilaya = fresh_wilaya_with_unit();
    let mut csr = begin_unit_request();
    csr.subject_type = SubjectType::Admin;
    let err = IdentityProvisioningService::new(&mut wilaya.db)
        .sign_unit_identity_request(&csr, &wilaya.node_key_store)
        .expect_err("non-UNIT request must fail closed");
    assert!(err.to_string().contains("UNIT request"), "got {err:?}");
}

#[test]
fn sign_rejects_unknown_subject_id() {
    let mut wilaya = fresh_wilaya_with_unit();
    let mut csr = begin_unit_request();
    csr.subject_id = uuid::Uuid::new_v4();
    let err = IdentityProvisioningService::new(&mut wilaya.db)
        .sign_unit_identity_request(&csr, &wilaya.node_key_store)
        .expect_err("unknown subject_id must fail closed");
    assert!(err.to_string().contains("known local unit"), "got {err:?}");
}

#[test]
fn sign_requires_active_wilaya() {
    let mut wilaya = fresh_node();
    wilaya
        .db
        .executor()
        .units()
        .upsert_raw_unit(UNIT_ID, UNIT_CODE, UNIT_NAME, UNIT_WILAYA, FIXED_NOW)
        .expect("unit mirrored on wilaya");
    let csr = begin_unit_request();
    let err = IdentityProvisioningService::new(&mut wilaya.db)
        .sign_unit_identity_request(&csr, &wilaya.node_key_store)
        .expect_err("no ACTIVE WILAYA on the issuer node");
    assert!(err.to_string().contains("WILAYA"), "got {err:?}");
}

/// A UNIT CSR from a fresh, properly configured UNIT node.
fn begin_unit_request() -> IdentityCertificate {
    let mut node = fresh_node();
    make_unit_node(&mut node);
    begin_unit(&mut node)
}

// ---------------------------------------------------------------------------
// Strict two-step: trust anchor + finalize
// ---------------------------------------------------------------------------

#[test]
fn full_unit_bootstrap_reaches_unit_active() {
    let mut unit_node = fresh_node();
    make_unit_node(&mut unit_node);
    let csr = begin_unit(&mut unit_node);

    let mut wilaya = fresh_wilaya_with_unit();
    let wilaya_cert = active_wilaya_cert(&wilaya);
    let signed = sign_unit(&mut wilaya, &csr);

    // Step 1 — install the WILAYA trust anchor (standalone, never bundled).
    let anchor = IdentityTrustAnchorService::new(unit_node.db.executor())
        .install_wilaya_certificate(&wilaya_cert, FIXED_NOW)
        .expect("wilaya trust anchor installed");
    assert!(
        matches!(anchor, InstallWilayaCertificateResult::Installed(_)),
        "first install must install"
    );
    assert_eq!(
        status(&unit_node),
        IdentityBootstrapState::UnitWaitingForCertificate
    );

    // Step 2 — finalize the UNIT certificate.
    let outcome = IdentityProvisioningService::new(&mut unit_node.db)
        .finalize_unit_provision(&signed, &unit_node.node_key_store, FIXED_NOW)
        .expect("unit finalized");
    assert!(matches!(
        outcome,
        FinalizeUnitProvisionResult::Provisioned(_)
    ));
    assert_eq!(status(&unit_node), IdentityBootstrapState::UnitActive);
}

#[test]
fn finalize_is_idempotent_for_identical_certificate() {
    let p = provisioned_unit();
    let mut unit = p.unit;
    let signed = p.signed_unit_cert;
    let _ = (p.wilaya, p.wilaya_cert);

    {
        let mut service = IdentityProvisioningService::new(&mut unit.db);
        service
            .finalize_unit_provision(&signed, &unit.node_key_store, FIXED_NOW)
            .expect("first finalize");
    }

    let before = unit
        .db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .len();
    let outcome = {
        let mut service = IdentityProvisioningService::new(&mut unit.db);
        service
            .finalize_unit_provision(&signed, &unit.node_key_store, FIXED_NOW)
            .expect("re-finalize identical")
    };
    let FinalizeUnitProvisionResult::AlreadyProvisioned(stored) = outcome else {
        panic!("identical re-presentation must be a no-op");
    };
    assert!(stored.is_identical_to(&signed));
    let after = unit
        .db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .len();
    assert_eq!(
        before, after,
        "identical re-presentation must do ZERO writes"
    );
}

// ---------------------------------------------------------------------------
// finalize fail-closed matrix (issuer resolved via issuer_identity_id)
// ---------------------------------------------------------------------------

#[test]
fn finalize_rejects_unknown_issuer_identity() {
    let mut p = provisioned_unit();
    let mut signed = p.signed_unit_cert;
    signed.issuer_identity_id = Some(uuid::Uuid::new_v4());
    let signed = resign_with_wilaya(&p.wilaya, &signed);
    let err = IdentityProvisioningService::new(&mut p.unit.db)
        .finalize_unit_provision(&signed, &p.unit.node_key_store, FIXED_NOW)
        .expect_err("unknown issuer must fail closed");
    assert!(err.to_string().contains("does not exist"), "got {err:?}");
}

#[test]
fn finalize_rejects_non_active_issuer() {
    let mut p = provisioned_unit();
    // Revoke the WILAYA anchor on the UNIT node → issuer exists but not ACTIVE.
    let mut revoked = p.wilaya_cert.clone();
    revoked.status = CredentialStatus::Revoked;
    revoked.signature = Some(
        Ed25519CertificateSignature::try_from(
            root_signer()
                .sign_certificate(&revoked)
                .expect("root signed"),
        )
        .expect("wrap"),
    );
    p.unit
        .db
        .executor()
        .identity_store()
        .upsert(&revoked, FIXED_NOW)
        .expect("revoked upsert");
    let err = IdentityProvisioningService::new(&mut p.unit.db)
        .finalize_unit_provision(&p.signed_unit_cert, &p.unit.node_key_store, FIXED_NOW)
        .expect_err("non-ACTIVE issuer must fail closed");
    assert!(err.to_string().contains("ACTIVE"), "got {err:?}");
}

#[test]
fn finalize_rejects_non_wilaya_issuer() {
    let mut p = provisioned_unit();
    // Replace the issuer row with an ADMIN subject sharing the identity_id.
    let mut admin_issuer = p.wilaya_cert.clone();
    admin_issuer.subject_type = SubjectType::Admin;
    admin_issuer.signature = Some(
        Ed25519CertificateSignature::try_from(
            root_signer()
                .sign_certificate(&admin_issuer)
                .expect("root signed"),
        )
        .expect("wrap"),
    );
    p.unit
        .db
        .executor()
        .identity_store()
        .upsert(&admin_issuer, FIXED_NOW)
        .expect("admin issuer upsert");
    let err = IdentityProvisioningService::new(&mut p.unit.db)
        .finalize_unit_provision(&p.signed_unit_cert, &p.unit.node_key_store, FIXED_NOW)
        .expect_err("non-WILAYA issuer must fail closed");
    assert!(err.to_string().contains("WILAYA"), "got {err:?}");
}

#[test]
fn finalize_rejects_wrong_issuer_signature() {
    let mut p = provisioned_unit();
    let mut signed = p.signed_unit_cert;
    let imposter = Ed25519SigningProvider::new([7u8; 32]);
    signed.signature = Some(
        Ed25519CertificateSignature::try_from(imposter.sign_certificate(&signed).expect("sig"))
            .expect("wrap"),
    );
    let err = IdentityProvisioningService::new(&mut p.unit.db)
        .finalize_unit_provision(&signed, &p.unit.node_key_store, FIXED_NOW)
        .expect_err("imposter signature must fail closed");
    assert!(
        err.to_string()
            .contains("not valid for the declared WILAYA issuer"),
        "got {err:?}"
    );
}

#[test]
fn finalize_rejects_cross_device_public_key() {
    let mut p = provisioned_unit();
    let mut signed = p.signed_unit_cert;
    let foreign = Ed25519SigningProvider::new([9u8; 32]);
    signed.public_key = foreign.public_key();
    let signed = resign_with_wilaya(&p.wilaya, &signed);
    let err = IdentityProvisioningService::new(&mut p.unit.db)
        .finalize_unit_provision(&signed, &p.unit.node_key_store, FIXED_NOW)
        .expect_err("cross-device public key must fail closed");
    assert!(
        err.to_string()
            .contains("does not match the node signing key"),
        "got {err:?}"
    );
}

#[test]
fn finalize_rejects_subject_id_not_matching_local_unit() {
    let mut p = provisioned_unit();
    let mut signed = p.signed_unit_cert;
    signed.subject_id = uuid::Uuid::new_v4();
    let signed = resign_with_wilaya(&p.wilaya, &signed);
    let err = IdentityProvisioningService::new(&mut p.unit.db)
        .finalize_unit_provision(&signed, &p.unit.node_key_store, FIXED_NOW)
        .expect_err("foreign subject_id must fail closed");
    assert!(err.to_string().contains("local unit"), "got {err:?}");
}

// ---------------------------------------------------------------------------
// Trust anchor idempotency (B5 style)
// ---------------------------------------------------------------------------

#[test]
fn identical_wilaya_anchor_reinstall_is_idempotent_zero_writes() {
    let p = provisioned_unit();
    let before = p
        .unit
        .db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .len();
    let outcome = {
        let mut service = IdentityTrustAnchorService::new(p.unit.db.executor());
        service
            .install_wilaya_certificate(&p.wilaya_cert, FIXED_NOW)
            .expect("re-install identical")
    };
    assert!(matches!(
        outcome,
        InstallWilayaCertificateResult::AlreadyInstalled(_)
    ));
    let after = p
        .unit
        .db
        .executor()
        .identity_store()
        .list_all()
        .expect("list")
        .len();
    assert_eq!(before, after, "identical re-install must do ZERO writes");
}

#[test]
fn trust_anchor_rejects_same_identity_different_credential() {
    let p = provisioned_unit();
    let mut rotated = p.wilaya_cert.clone();
    rotated.credential_id = uuid::Uuid::new_v4();
    rotated.signature = Some(
        Ed25519CertificateSignature::try_from(
            root_signer()
                .sign_certificate(&rotated)
                .expect("root signed"),
        )
        .expect("wrap"),
    );
    let err = IdentityTrustAnchorService::new(p.unit.db.executor())
        .install_wilaya_certificate(&rotated, FIXED_NOW)
        .expect_err("different credential for same identity must fail closed");
    assert!(err.to_string().contains("already installed"), "got {err:?}");
}

// ---------------------------------------------------------------------------
// Challenge–Response is NOT applicable on a UNIT node (no ADMIN issued)
// ---------------------------------------------------------------------------

#[test]
fn unit_node_never_issues_admin_and_status_stays_unit_chain() {
    let mut p = provisioned_unit();
    {
        let mut service = IdentityProvisioningService::new(&mut p.unit.db);
        service
            .finalize_unit_provision(&p.signed_unit_cert, &p.unit.node_key_store, FIXED_NOW)
            .expect("unit finalized");
    }
    assert_eq!(status(&p.unit), IdentityBootstrapState::UnitActive);
    assert_eq!(
        p.unit
            .db
            .executor()
            .identity_store()
            .get_active_by_subject_type(SubjectType::Admin)
            .expect("query"),
        None,
        "UNIT nodes hold no ADMIN identity"
    );
}
