//! Producer-side V2 (Ed25519) sync export integration tests — Commit ④b / B6-B.
//!
//! RFC 2026-08-04-node-identity-trust §3.4.1 / §3.10 / ADR-0038.
//!
//! Exercises `IdentitySignedExportService::export_v2_package` end-to-end:
//! - full V2 metadata (signature_version, issuer_identity_id, signing_key_id)
//!   with a signature that verifies against the issuer cert; packages are
//!   unique by exact `package_id` only — no transport sequence (SEC-057);
//! - fail-closed: an unprovisioned node (no key or no ACTIVE certificate)
//!   cannot emit a V2 package — there is no HMAC fallback for production
//!   sync exports;
//! - failed exports write nothing and the retry onto a valid path succeeds;
//! - rotation continuity: the package stays bound to `identity_id` — a
//!   re-issued credential (new credential_id/generation) keeps emitting V2
//!   packages for the same identity whose signature verifies against the
//!   unchanged public key.
//!
//! The consumer-side Transport Guard ordering tests were removed with
//! SEC-057: there is no per-issuer sequence to guard anymore.
//!
//! The test authority Root uses the RFC 8032 §7.1 TEST 1 keypair — exactly the
//! `#[cfg(debug_assertions)]` development Root fallback in `root_public_key.rs`.

#[allow(dead_code)]
mod common;

use std::path::Path;

use tempfile::TempDir;

use grpc_lib::application::services::{
    FinalizeWilayaProvisionResult, IdentityProvisioningService, IdentitySignedExportService,
    SyncPackageIdentityVerificationService,
};
use grpc_lib::application::usecases::exports::types::ProductsExportDataset;
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    CredentialStatus, Ed25519CertificateSignature, IdentityCertificate, IdentitySigner,
    IdentityStorePort, SubjectType, SIGNATURE_VERSION_ED25519,
};
use grpc_lib::infrastructure::identity::NodeKeyStore;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::Ed25519SigningProvider;
use grpc_lib::infrastructure::sync::read_products_package_from_file;
use grpc_lib::repositories::RepositoryProvider;

/// RFC 8032 §7.1 TEST 1 secret — the matching public key IS the debug-mode
/// development Root fallback (root_public_key.rs). Never a production key.
const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
/// Fixed timestamp for deterministic persistence.
const FIXED_NOW: &str = "2026-08-04T00:00:00Z";

/// A fresh node directory: real DB + node key store (WILAYA provisioning).
struct Node {
    _dir: TempDir,
    db: Database,
    node_key_store: NodeKeyStore,
}

fn fresh_node() -> Node {
    let dir = TempDir::new().expect("temp dir");
    let db = ConnectionFactory::new_for_test().expect("db");
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
    Node {
        _dir: dir,
        db,
        node_key_store,
    }
}

fn root_signer() -> Ed25519SigningProvider {
    Ed25519SigningProvider::new(TEST_ROOT_SECRET)
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

/// Export an empty products dataset as a V2 package (SEC-057: returns nothing
/// — there is no sequence allocation to surface).
fn export_products(node: &Node, crypto: &AgeFileEncryptionProvider, target: &Path) {
    IdentitySignedExportService::new(&node.db, &node.node_key_store)
        .export_v2_package(
            ProductsExportDataset {
                product_rows: vec![],
            },
            "wilaya-test-node",
            "products",
            "UNIT-A",
            target,
            SubjectType::Wilaya,
            crypto,
        )
        .expect("export succeeds")
}

fn read_package(
    crypto: &AgeFileEncryptionProvider,
    path: &Path,
) -> grpc_lib::application::sync::SyncPackage<ProductsExportDataset> {
    read_products_package_from_file(path, crypto).expect("read back package")
}

// ---------------------------------------------------------------------------
// V2 metadata + signature validity
// ---------------------------------------------------------------------------

#[test]
fn producer_emits_v2_ed25519_package_with_full_metadata() {
    let mut node = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut node);
    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("products.sync");

    export_products(&node, &crypto, &path);

    let pkg = read_package(&crypto, &path);
    let meta = &pkg.metadata;
    assert_eq!(meta.signature_version, Some(SIGNATURE_VERSION_ED25519));
    assert_eq!(meta.issuer_identity_id, Some(wilaya_cert.identity_id));
    assert_eq!(
        meta.signing_key_id.as_deref(),
        Some(hex::encode(&wilaya_cert.public_key).as_str()),
        "signing_key_id must match the ACTIVE certificate public key (R5)"
    );
    assert!(meta.integrity_hash.is_some(), "integrity hash must be set");
    assert!(meta.signature.is_some(), "Ed25519 signature must be set");
    assert_eq!(meta.source_node_id, "wilaya-test-node");

    // The signature verifies against the issuing identity's certificate.
    SyncPackageIdentityVerificationService::verify_v2_signature(node.db.executor(), &pkg)
        .expect("signature verifies against issuer cert");
}

// ---------------------------------------------------------------------------
// Fail-closed + retry semantics (no sequence ledger)
// ---------------------------------------------------------------------------

#[test]
fn failed_export_writes_nothing_and_retry_succeeds() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");

    // Invalid target path (non-existent parent directory) → build fails closed.
    let bad = dir.path().join("no_such_dir").join("bad.sync");
    let err = IdentitySignedExportService::new(&node.db, &node.node_key_store)
        .export_v2_package(
            ProductsExportDataset {
                product_rows: vec![],
            },
            "wilaya-test-node",
            "products",
            "UNIT-A",
            &bad,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect_err("invalid path must fail");
    assert!(
        err.to_string().contains("Failed to create target file"),
        "got {err:?}"
    );
    assert!(
        !bad.exists(),
        "no artifact may be written by a failed export"
    );

    // Retry on a valid path succeeds — every successful export is a distinct
    // exact package (no sequence to burn or reuse, SEC-057).
    let good1 = dir.path().join("good1.sync");
    export_products(&node, &crypto, &good1);
    assert!(good1.exists());

    let good2 = dir.path().join("good2.sync");
    export_products(&node, &crypto, &good2);
    assert!(good2.exists());

    let p1 = read_package(&crypto, &good1);
    let p2 = read_package(&crypto, &good2);
    assert_ne!(
        p1.metadata.package_id, p2.metadata.package_id,
        "every export is a distinct exact package"
    );
    for pkg in [&p1, &p2] {
        SyncPackageIdentityVerificationService::verify_v2_signature(node.db.executor(), pkg)
            .expect("V2 signature verifies against issuer cert");
    }
}

// ---------------------------------------------------------------------------
// Rotation continuity (package bound to identity_id, not credential_id)
// ---------------------------------------------------------------------------

#[test]
fn identity_bound_signing_continues_across_credential_rotation() {
    let mut node = fresh_node();
    let cert_a = bootstrap_wilaya(&mut node);
    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");

    let p1 = dir.path().join("p1.sync");
    export_products(&node, &crypto, &p1);

    // Re-issue: same identity_id + same node public key, NEW credential_id +
    // higher generation. R5 still holds (public key unchanged).
    let mut cert_b = cert_a.clone();
    cert_b.credential_id = uuid::Uuid::new_v4();
    cert_b.generation = cert_a.generation + 1;
    cert_b.status = CredentialStatus::Active;
    let sig = root_signer()
        .sign_certificate(&cert_b)
        .expect("root re-signs rotated cert");
    cert_b.signature = Some(Ed25519CertificateSignature::try_from(sig).expect("wrap"));
    IdentityStorePort::upsert(&node.db.executor().identity_store(), &cert_b, FIXED_NOW)
        .expect("rotated cert upsert");

    let p2 = dir.path().join("p2.sync");
    export_products(&node, &crypto, &p2);

    let pkg1 = read_package(&crypto, &p1);
    let pkg2 = read_package(&crypto, &p2);
    assert_eq!(
        pkg2.metadata.issuer_identity_id,
        Some(cert_a.identity_id),
        "the package stays bound to the identity across rotation (SEC-057)"
    );
    assert_eq!(
        pkg1.metadata.issuer_identity_id, pkg2.metadata.issuer_identity_id,
        "rotation does not change the issuer identity"
    );
    assert_ne!(
        pkg1.metadata.package_id, pkg2.metadata.package_id,
        "pre- and post-rotation exports are distinct exact packages"
    );
    for pkg in [&pkg1, &pkg2] {
        SyncPackageIdentityVerificationService::verify_v2_signature(node.db.executor(), pkg)
            .expect("package verifies against the (unchanged) public key");
    }
}

// ---------------------------------------------------------------------------
// Fail-closed: no HMAC fallback for production sync exports
// ---------------------------------------------------------------------------

#[test]
fn unprovisioned_node_fails_closed_without_hmac_fallback() {
    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");

    // No node key, no certificate.
    let bare = fresh_node();
    let path = dir.path().join("bare.sync");
    let err = IdentitySignedExportService::new(&bare.db, &bare.node_key_store)
        .export_v2_package(
            ProductsExportDataset {
                product_rows: vec![],
            },
            "n",
            "products",
            "UNIT-A",
            &path,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect_err("unprovisioned node must fail closed");
    assert!(err.to_string().contains("غير مزوّدة"), "got {err:?}");
    assert!(!path.exists(), "no file must be produced");

    // Node key present but no ACTIVE WILAYA certificate.
    let key_only = fresh_node();
    key_only
        .node_key_store
        .write(&[42u8; 32])
        .expect("write key");
    let path = dir.path().join("key_only.sync");
    let err = IdentitySignedExportService::new(&key_only.db, &key_only.node_key_store)
        .export_v2_package(
            ProductsExportDataset {
                product_rows: vec![],
            },
            "n",
            "products",
            "UNIT-A",
            &path,
            SubjectType::Wilaya,
            &crypto,
        )
        .expect_err("key without certificate must fail closed");
    assert!(err.to_string().contains("غير مزوّدة"), "got {err:?}");
    assert!(!path.exists(), "no file must be produced");
}
