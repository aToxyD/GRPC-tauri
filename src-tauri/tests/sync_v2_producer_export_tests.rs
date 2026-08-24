//! Producer-side V2 (Ed25519) sync export integration tests — Commit ④b / B6-B.
//!
//! RFC 2026-08-04-node-identity-trust §3.4.1 / §3.10 / ADR-0038.
//!
//! Exercises `IdentitySignedExportService::export_v2_package` end-to-end:
//! - full V2 metadata (signature_version, issuer_identity_id, package_sequence,
//!   signing_key_id) with a signature that verifies against the issuer cert;
//! - consumer round-trip: Transport Guard accepts the first package, rejects a
//!   replay, defers an out-of-order package, and accepts the contiguous run;
//! - advance-on-success: a failed export burns no sequence and the retry reuses
//!   the same number;
//! - rotation continuity: the producer ledger is keyed on `identity_id`, so a
//!   re-issued credential (new credential_id/generation) continues the sequence;
//! - fail-closed: an unprovisioned node (no key or no ACTIVE certificate) cannot
//!   emit a V2 package — there is no HMAC fallback for production sync exports.
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
use grpc_lib::application::sync_integrity::transport_guard::{TransportGuard, TransportVerdict};
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

/// Export an empty products dataset as a V2 package; returns the issued sequence.
fn export_products(node: &Node, crypto: &AgeFileEncryptionProvider, target: &Path) -> u64 {
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

    let sequence = export_products(&node, &crypto, &path);
    assert_eq!(sequence, 1);

    let pkg = read_package(&crypto, &path);
    let meta = &pkg.metadata;
    assert_eq!(meta.signature_version, Some(SIGNATURE_VERSION_ED25519));
    assert_eq!(meta.issuer_identity_id, Some(wilaya_cert.identity_id));
    assert_eq!(meta.package_sequence, Some(1));
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
// Consumer round-trip: Transport Guard ordering
// ---------------------------------------------------------------------------

#[test]
fn consumer_accepts_then_rejects_replay_and_defers_out_of_order() {
    let mut producer = fresh_node();
    let wilaya_cert = bootstrap_wilaya(&mut producer);
    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");

    let p1 = dir.path().join("p1.sync");
    let p2 = dir.path().join("p2.sync");
    let p3 = dir.path().join("p3.sync");
    assert_eq!(export_products(&producer, &crypto, &p1), 1);
    assert_eq!(export_products(&producer, &crypto, &p2), 2);
    assert_eq!(export_products(&producer, &crypto, &p3), 3);

    // Consumer: a fresh node with the WILAYA cert installed as trusted issuer.
    let consumer = fresh_node();
    IdentityStorePort::upsert(
        &consumer.db.executor().identity_store(),
        &wilaya_cert,
        FIXED_NOW,
    )
    .expect("seed issuer cert");
    let issuer = wilaya_cert.identity_id.to_string();

    let pkg1 = read_package(&crypto, &p1);
    SyncPackageIdentityVerificationService::verify_v2_signature(consumer.db.executor(), &pkg1)
        .expect("pkg1 verifies");

    // First package from the issuer: accept and advance.
    let last = SyncPackageIdentityVerificationService::last_applied_sequence(
        consumer.db.executor(),
        &issuer,
    )
    .expect("last");
    assert_eq!(last, None);
    assert!(matches!(
        TransportGuard::check(&issuer, 1, last),
        TransportVerdict::Accept { .. }
    ));
    SyncPackageIdentityVerificationService::advance_issuer_sequence(
        consumer.db.executor(),
        &issuer,
        1,
    )
    .expect("advance");

    // Replay of pkg1 → Reject.
    let last = SyncPackageIdentityVerificationService::last_applied_sequence(
        consumer.db.executor(),
        &issuer,
    )
    .expect("last");
    assert_eq!(last, Some(1));
    assert!(matches!(
        TransportGuard::check(&issuer, 1, last),
        TransportVerdict::Replay { .. }
    ));

    // Out-of-order: pkg3 presented before pkg2 → deferred gap.
    let last = SyncPackageIdentityVerificationService::last_applied_sequence(
        consumer.db.executor(),
        &issuer,
    )
    .expect("last");
    assert!(matches!(
        TransportGuard::check(&issuer, 3, last),
        TransportVerdict::OutOfOrder {
            expected: 2,
            got: 3,
            ..
        }
    ));

    // Contiguous continuation (pkg2) restores order.
    let pkg2 = read_package(&crypto, &p2);
    SyncPackageIdentityVerificationService::verify_v2_signature(consumer.db.executor(), &pkg2)
        .expect("pkg2 verifies");
    let last = SyncPackageIdentityVerificationService::last_applied_sequence(
        consumer.db.executor(),
        &issuer,
    )
    .expect("last");
    assert!(matches!(
        TransportGuard::check(&issuer, 2, last),
        TransportVerdict::Accept { .. }
    ));
    SyncPackageIdentityVerificationService::advance_issuer_sequence(
        consumer.db.executor(),
        &issuer,
        2,
    )
    .expect("advance");
}

// ---------------------------------------------------------------------------
// Advance-on-success ledger semantics
// ---------------------------------------------------------------------------

#[test]
fn failed_export_burns_no_sequence_and_retry_reuses_number() {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");

    // Invalid target path (non-existent parent directory) → build fails.
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

    // Retry on a valid path → the same sequence is reused (no burned gaps).
    let good1 = dir.path().join("good1.sync");
    assert_eq!(export_products(&node, &crypto, &good1), 1);

    let good2 = dir.path().join("good2.sync");
    assert_eq!(export_products(&node, &crypto, &good2), 2);
}

// ---------------------------------------------------------------------------
// Rotation continuity (ledger keyed on identity_id, not credential_id)
// ---------------------------------------------------------------------------

#[test]
fn sequence_continues_across_credential_rotation() {
    let mut node = fresh_node();
    let cert_a = bootstrap_wilaya(&mut node);
    let crypto = AgeFileEncryptionProvider::new();
    let dir = TempDir::new().expect("temp dir");

    let p1 = dir.path().join("p1.sync");
    assert_eq!(export_products(&node, &crypto, &p1), 1);

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
    let seq2 = export_products(&node, &crypto, &p2);
    assert_eq!(
        seq2, 2,
        "ledger must continue by identity_id across credential rotation"
    );

    let pkg2 = read_package(&crypto, &p2);
    assert_eq!(pkg2.metadata.issuer_identity_id, Some(cert_a.identity_id));
    assert_eq!(pkg2.metadata.package_sequence, Some(2));
    SyncPackageIdentityVerificationService::verify_v2_signature(node.db.executor(), &pkg2)
        .expect("rotated package verifies against the (unchanged) public key");
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
