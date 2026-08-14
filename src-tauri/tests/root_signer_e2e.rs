//! `root-signer` end-to-end integration tests (B5 / ADR-0038).
//!
//! RFC 2026-08-04-node-identity-trust §3.6–3.7 / ADR-0038 / ADR-0039 §5 / ADR-0004.
//!
//! Proves the offline Authority Root path end-to-end by running the REAL
//! `root-signer` binary (`CARGO_BIN_EXE_root-signer`) over a genuine CSR
//! produced by `IdentityProvisioningService::generate_wilaya_request` — never a
//! reimplementation of the signing logic inside this test:
//!
//!   begin (CSR) → root-signer binary → signed WILAYA certificate →
//!   finalize_wilaya_provision → WILAYA_ACTIVE → issue_first_admin_key → READY
//!
//! The test authority Root uses the RFC 8032 §7.1 TEST 1 keypair, which is
//! exactly the `#[cfg(debug_assertions)]` development Root fallback in
//! `root_public_key.rs` — so `finalize` resolves the correct Root public key
//! without any process-environment mutation (parallel-safe). Passing these
//! tests proves protocol correctness ONLY; production readiness requires the
//! real Authority Root key and a matching `GRPC_ROOT_PUBLIC_KEY`.

#[allow(dead_code)]
mod common;

use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

use grpc_lib::application::services::{
    FinalizeWilayaProvisionResult, IdentityBootstrapStatusService, IdentityProvisioningService,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    CredentialStatus, Ed25519CertificateSignature, IdentityBootstrapState, IdentityCertificate,
    IdentitySignatureVerifier, IdentitySigner, SubjectType,
};
use grpc_lib::infrastructure::identity::{AdminKeyProvider, NodeKeyStore};
use grpc_lib::infrastructure::security::{Ed25519SignatureVerifier, Ed25519SigningProvider};
use grpc_lib::repositories::RepositoryProvider;

/// The real binary under test — set by Cargo for every integration test.
const ROOT_SIGNER_BIN: &str = env!("CARGO_BIN_EXE_root-signer");

/// RFC 8032 §7.1 TEST 1 secret — the matching public key IS the debug-mode
/// development Root fallback (`root_public_key.rs`). Never a production key.
const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
/// Base64 of the TEST 1 public key — the `DEV_ROOT_PUBLIC_KEY` fallback value.
const DEV_ROOT_PUBLIC_KEY_B64: &str = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";
/// A Root secret whose public key does NOT match the development Root — used
/// to prove GRPC rejects a certificate signed by the wrong Root.
const FOREIGN_ROOT_SECRET: [u8; 32] = [7u8; 32];

const BOOTSTRAP_USERNAME: &str = "admin";
const ADMIN_PASSPHRASE: &str = "correct horse battery staple";
const FIXED_NOW: &str = "2026-08-04T00:00:00Z";

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

/// Remove the seeded admin so the node behaves like a fresh B5 fleet node and
/// the first ADMIN key reaches the identity-only `READY` state.
fn remove_seeded_admin(node: &mut Node) {
    let admin = node
        .db
        .executor()
        .users()
        .get_user_by_username(BOOTSTRAP_USERNAME)
        .expect("seed query")
        .expect("seeded admin present");
    node.db
        .executor()
        .users()
        .delete_user(&admin.id)
        .expect("seeded admin removed");
}

/// Begin: generate a genuine WILAYA CSR through the same service the
/// `begin_wilaya_provision` command dispatches to.
fn generate_csr(node: &mut Node) -> IdentityCertificate {
    IdentityProvisioningService::new(&mut node.db)
        .generate_wilaya_request(uuid::Uuid::new_v4(), &node.node_key_store)
        .expect("csr generated")
}

fn write_json(path: &Path, certificate: &IdentityCertificate) {
    std::fs::write(path, serde_json::to_string_pretty(certificate).expect("serialize"))
        .expect("write file");
}

fn run_signer(args: &[String], grpc_root_public_key: Option<&str>) -> Output {
    let mut cmd = Command::new(ROOT_SIGNER_BIN);
    cmd.args(args);
    if let Some(public_key) = grpc_root_public_key {
        cmd.env("GRPC_ROOT_PUBLIC_KEY", public_key);
    }
    cmd.output().expect("root-signer binary executes")
}

fn sign_args(csr: &Path, out: &Path, secret_hex: &str) -> Vec<String> {
    vec![
        "sign".to_string(),
        "--csr".to_string(),
        csr.to_string_lossy().into_owned(),
        "--out".to_string(),
        out.to_string_lossy().into_owned(),
        "--key-hex".to_string(),
        secret_hex.to_string(),
    ]
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

// ---------------------------------------------------------------------------
// Positive end-to-end path through the REAL binary
// ---------------------------------------------------------------------------

#[test]
fn real_binary_signs_csr_and_grpc_reaches_wilaya_active_then_ready() {
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);

    let csr = generate_csr(&mut node);
    assert_eq!(csr.subject_type, SubjectType::Wilaya);
    assert!(csr.signature.is_none());

    let csr_path = node._dir.path().join("wilaya-csr.json");
    let signed_path = node._dir.path().join("wilaya-signed.json");
    write_json(&csr_path, &csr);

    // The binary signs with the dev Root secret while GRPC_ROOT_PUBLIC_KEY
    // pins the dev Root public key — proving the deployment-key match guard.
    let output = run_signer(
        &sign_args(&csr_path, &signed_path, &hex::encode(TEST_ROOT_SECRET)),
        Some(DEV_ROOT_PUBLIC_KEY_B64),
    );
    assert!(
        output.status.success(),
        "root-signer must succeed: {}",
        stderr(&output)
    );
    // The real binary must surface the test-key warning on stderr.
    assert!(
        stderr(&output).contains("KNOWN public test vector"),
        "test-key warning missing from stderr: {}",
        stderr(&output)
    );

    let signed_json = std::fs::read_to_string(&signed_path).expect("signed file written");
    let signed_cert: IdentityCertificate = serde_json::from_str(&signed_json).expect("valid JSON");
    assert!(signed_cert.signature.is_some());

    // Finalize through GRPC's own path: the produced certificate is exactly
    // what GRPC verifies, and the node reaches WILAYA_ACTIVE.
    {
        let mut provisioning = IdentityProvisioningService::new(&mut node.db);
        match provisioning
            .finalize_wilaya_provision(&signed_cert, &node.node_key_store, FIXED_NOW)
            .expect("finalize must succeed")
        {
            FinalizeWilayaProvisionResult::Provisioned(cert) => cert,
            FinalizeWilayaProvisionResult::AlreadyProvisioned(_) => {
                panic!("first finalize must provision")
            }
        };
    }
    assert_eq!(status(&node), IdentityBootstrapState::WilayaActive);

    // The remaining chain: first ADMIN key + READY.
    let admin_cert = {
        let mut provisioning = IdentityProvisioningService::new(&mut node.db);
        provisioning
            .issue_first_admin_key(
                BOOTSTRAP_USERNAME,
                ADMIN_PASSPHRASE,
                &node.node_key_store,
                &node.adminkey_provider,
                FIXED_NOW,
            )
            .expect("admin key issued")
    };
    assert_eq!(admin_cert.subject_type, SubjectType::Admin);
    assert_eq!(admin_cert.status, CredentialStatus::Active);
    assert_eq!(status(&node), IdentityBootstrapState::Ready);

    // The produced signature verifies under GRPC's own verifier against the
    // dev Root public key — what the Root signed is what GRPC verifies.
    let signature = signed_cert.signature.expect("signature present");
    let derived: [u8; 32] = Ed25519SigningProvider::new(TEST_ROOT_SECRET)
        .public_key()
        .try_into()
        .expect("32-byte key");
    assert!(
        Ed25519SignatureVerifier
            .verify_certificate(&signed_cert, &derived, &signature)
            .expect("verify")
    );
}

// ---------------------------------------------------------------------------
// Negative: the wrong Root key
// ---------------------------------------------------------------------------

#[test]
fn certificate_signed_by_wrong_root_key_is_rejected_by_grpc_finalize() {
    let mut node = fresh_node();
    let csr = generate_csr(&mut node);

    let csr_path = node._dir.path().join("csr.json");
    let signed_path = node._dir.path().join("signed.json");
    write_json(&csr_path, &csr);

    // A foreign Root secret (NOT the dev Root) signs the same CSR through the
    // real binary. No GRPC_ROOT_PUBLIC_KEY pin here, so the binary signs.
    let output = run_signer(
        &sign_args(&csr_path, &signed_path, &hex::encode(FOREIGN_ROOT_SECRET)),
        None,
    );
    assert!(output.status.success(), "signer must sign: {}", stderr(&output));

    let signed_json = std::fs::read_to_string(&signed_path).expect("signed file written");
    let signed_cert: IdentityCertificate = serde_json::from_str(&signed_json).expect("valid JSON");

    let err = IdentityProvisioningService::new(&mut node.db)
        .finalize_wilaya_provision(&signed_cert, &node.node_key_store, FIXED_NOW)
        .expect_err("wrong Root signature must fail closed");
    assert!(
        err.to_string().contains("not valid for the Authority Root"),
        "got {err:?}"
    );
    assert_eq!(
        status(&node),
        IdentityBootstrapState::WaitingForRootCertificate
    );
}

#[test]
fn binary_refuses_when_derived_key_mismatches_grpc_root_public_key() {
    let mut node = fresh_node();
    let csr = generate_csr(&mut node);

    let csr_path = node._dir.path().join("csr.json");
    let signed_path = node._dir.path().join("signed.json");
    write_json(&csr_path, &csr);

    let output = run_signer(
        &sign_args(&csr_path, &signed_path, &hex::encode(FOREIGN_ROOT_SECRET)),
        Some(DEV_ROOT_PUBLIC_KEY_B64),
    );
    assert!(!output.status.success(), "signer must refuse a key mismatch");
    assert!(
        stderr(&output).contains("GRPC_ROOT_PUBLIC_KEY"),
        "got: {}",
        stderr(&output)
    );
    assert!(
        !signed_path.exists(),
        "no signed output may be written on refusal"
    );
}

// ---------------------------------------------------------------------------
// Negative: CSR shape guards (all must refuse via the REAL binary)
// ---------------------------------------------------------------------------

#[test]
fn binary_refuses_csr_that_is_already_signed() {
    let mut node = fresh_node();
    let mut csr = generate_csr(&mut node);
    csr.signature = Some(Ed25519CertificateSignature::from_bytes([1u8; 64]));

    let csr_path = node._dir.path().join("csr.json");
    let signed_path = node._dir.path().join("signed.json");
    write_json(&csr_path, &csr);

    let output = run_signer(
        &sign_args(&csr_path, &signed_path, &hex::encode(TEST_ROOT_SECRET)),
        None,
    );
    assert!(!output.status.success(), "already-signed CSR must be refused");
    assert!(
        stderr(&output).contains("already carries a signature"),
        "got: {}",
        stderr(&output)
    );
}

#[test]
fn binary_refuses_non_wilaya_csr() {
    let mut node = fresh_node();
    let mut csr = generate_csr(&mut node);
    csr.subject_type = SubjectType::Unit;

    let csr_path = node._dir.path().join("csr.json");
    let signed_path = node._dir.path().join("signed.json");
    write_json(&csr_path, &csr);

    let output = run_signer(
        &sign_args(&csr_path, &signed_path, &hex::encode(TEST_ROOT_SECRET)),
        None,
    );
    assert!(!output.status.success(), "non-WILAYA CSR must be refused");
    assert!(
        stderr(&output).contains("WILAYA"),
        "got: {}",
        stderr(&output)
    );
}

#[test]
fn binary_refuses_unsupported_algorithm_version() {
    let mut node = fresh_node();
    let mut csr = generate_csr(&mut node);
    csr.algorithm_version = 3;

    let csr_path = node._dir.path().join("csr.json");
    let signed_path = node._dir.path().join("signed.json");
    write_json(&csr_path, &csr);

    let output = run_signer(
        &sign_args(&csr_path, &signed_path, &hex::encode(TEST_ROOT_SECRET)),
        None,
    );
    assert!(!output.status.success(), "unsupported version must be refused");
    assert!(
        stderr(&output).contains("algorithm_version"),
        "got: {}",
        stderr(&output)
    );
}

// ---------------------------------------------------------------------------
// init ceremony (offline Authority Root keypair generation)
// ---------------------------------------------------------------------------

fn init_args(dir: &TempDir) -> (std::path::PathBuf, std::path::PathBuf, Vec<String>) {
    let secret_file = dir.path().join("root-secret.hex");
    let public_key_file = dir.path().join("root-public.key");
    let args = vec![
        "init".to_string(),
        "--secret-file".to_string(),
        secret_file.to_string_lossy().into_owned(),
        "--public-key-file".to_string(),
        public_key_file.to_string_lossy().into_owned(),
    ];
    (secret_file, public_key_file, args)
}

#[test]
fn real_binary_init_creates_keypair_and_sign_consumes_it() {
    use base64::Engine;

    let dir = TempDir::new().expect("temp dir");
    let (secret_file, public_key_file, args) = init_args(&dir);

    let output = run_signer(&args, None);
    assert!(
        output.status.success(),
        "root-signer init must succeed: {}",
        stderr(&output)
    );

    let output_text = stdout(&output);
    assert!(
        output_text.contains("[root-signer] new Authority Root keypair initialized."),
        "ceremony summary missing: {output_text}"
    );

    let secret_hex = std::fs::read_to_string(&secret_file).expect("secret file written");
    let public_key_b64 = std::fs::read_to_string(&public_key_file).expect("public file written");
    assert!(
        output_text.contains(public_key_b64.trim()),
        "stdout must surface the public key"
    );
    assert!(
        !output_text.contains(secret_hex.trim()),
        "stdout must NEVER contain the secret"
    );

    // Exact artifact formats.
    assert_eq!(secret_hex.len(), 65, "64 hex chars + trailing newline");
    assert!(secret_hex.ends_with('\n'));
    assert!(secret_hex.trim().chars().all(|c| c.is_ascii_hexdigit()));
    let seed: [u8; 32] = hex::decode(secret_hex.trim())
        .expect("valid hex")
        .try_into()
        .expect("32 bytes");
    assert!(public_key_b64.ends_with('\n'));
    let public_bytes = base64::engine::general_purpose::STANDARD
        .decode(public_key_b64.trim())
        .expect("valid base64");
    assert_eq!(public_bytes.len(), 32);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&secret_file)
            .expect("secret metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "secret must be owner-only");
    }

    // The generated secret is consumable by `sign --key-file`, and the signed
    // certificate verifies against the generated public key.
    let mut node = fresh_node();
    remove_seeded_admin(&mut node);
    let csr = generate_csr(&mut node);
    let csr_path = dir.path().join("wilaya-csr.json");
    let signed_path = dir.path().join("wilaya-signed.json");
    write_json(&csr_path, &csr);

    let sign_output = run_signer(
        &[
            "sign".to_string(),
            "--csr".to_string(),
            csr_path.to_string_lossy().into_owned(),
            "--out".to_string(),
            signed_path.to_string_lossy().into_owned(),
            "--key-file".to_string(),
            secret_file.to_string_lossy().into_owned(),
        ],
        Some(public_key_b64.trim()),
    );
    assert!(
        sign_output.status.success(),
        "sign with the generated secret must succeed: {}",
        stderr(&sign_output)
    );

    let signed_json = std::fs::read_to_string(&signed_path).expect("signed file written");
    let signed_cert: IdentityCertificate = serde_json::from_str(&signed_json).expect("valid JSON");
    let derived: [u8; 32] = Ed25519SigningProvider::new(seed)
        .public_key()
        .try_into()
        .expect("32-byte key");
    assert!(
        Ed25519SignatureVerifier
            .verify_certificate(
                &signed_cert,
                &derived,
                signed_cert.signature.as_ref().expect("signature present"),
            )
            .expect("verify"),
        "the signed certificate must verify against the generated public key"
    );

    // The keypair exists exactly once: re-running init must refuse without
    // overwriting the existing secret.
    let secret_before = std::fs::read_to_string(&secret_file).expect("secret file");
    let rerun = run_signer(&args, None);
    assert!(!rerun.status.success(), "re-running init must refuse");
    assert!(
        stderr(&rerun).contains("already exists"),
        "got: {}",
        stderr(&rerun)
    );
    assert_eq!(
        std::fs::read_to_string(&secret_file).expect("secret file"),
        secret_before,
        "a refused re-init must never overwrite the secret"
    );
}
