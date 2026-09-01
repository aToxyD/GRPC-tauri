//! root-signer — offline Authority Root signing tool for WILAYA identity
//! certificates.
//!
//! RFC 2026-08-04-node-identity-trust / ADR-0038 / ADR-0039 §5 / ADR-0004.
//!
//! The Authority Root is the offline trust anchor of the `grpc` identity chain
//! and is architecturally FORBIDDEN from living in `grpc-licensing` (ADR-0004).
//! This tool signs WILAYA bootstrap CSRs produced by `begin_wilaya_provision`.
//! It reuses `grpc_lib`'s `canonical_bytes()` and signature types so that what
//! the Root signs is byte-identical to what GRPC verifies at `finalize`.
//!
//! The Root private key NEVER lives in this repository, is never embedded in
//! this binary, and is never printed to stdout/stderr/logs. It is supplied at
//! runtime through `GRPC_ROOT_PRIVATE_KEY`, `--key-file`, or `--key-hex`.
//!
//! Guards (fail-closed):
//! - subject_type MUST be WILAYA — the Root signs only WILAYA (RFC §3.5).
//! - the CSR MUST NOT already carry a signature.
//! - algorithm_version MUST be the Ed25519 identity profile (2).
//! - credential status MUST be ACTIVE — the Root issues no revoked/expired
//!   certificates.
//! - public_key MUST be a 32-byte Ed25519 key.
//! - when `GRPC_ROOT_PUBLIC_KEY` is set, the derived Root public key MUST match.
//! - the produced signature is self-verified against the derived Root public key
//!   BEFORE the signed certificate is written.
//!
//! Known public test vectors (RFC 8032 §7.1 TEST 1 / TEST 2) are refused in
//! release builds and loudly warned in debug builds. Passing end-to-end tests
//! with TEST 1 proves protocol correctness only — NOT production readiness.

use std::path::Path;

use rand::rngs::OsRng;
use rand::RngCore;

use grpc_lib::domain::identity::{
    CredentialStatus, Ed25519CertificateSignature, IdentityCertificate, IdentitySignatureVerifier,
    IdentitySigner, SubjectType, IDENTITY_ALGORITHM_PROFILE_ED25519,
};
use grpc_lib::infrastructure::security::{Ed25519SignatureVerifier, Ed25519SigningProvider};

const ED25519_PUBLIC_KEY_LEN: usize = 32;

/// RFC 8032 §7.1 TEST 1 public key (Base64) — the debug-mode dev Root fallback.
const RFC8032_TEST1_PUBLIC_KEY_B64: &str = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";
/// RFC 8032 §7.1 TEST 2 public key (Base64) — former `PROD_ROOT_PUBLIC_KEY`
/// placeholder, superseded 2026-08-15 by the certified Production Authority
/// Root key (A44-06, docs/security/A44-06-production-root-key-certification.md).
/// Never a real production key — remains on the refusal list.
const RFC8032_TEST2_PUBLIC_KEY_B64: &str = "PUAXw+hDiVqStwqnTRt+vJyYLM8uxJaMwM1V8Sr0Zgw=";

struct SignArgs {
    csr_path: String,
    out_path: String,
    secret_key: [u8; 32],
    expected_root_public_key: Option<[u8; 32]>,
}

/// Arguments for the `init` ceremony subcommand.
struct InitArgs {
    secret_file: String,
    public_key_file: String,
}

/// Default secret artifact name for `root-signer init`.
const DEFAULT_SECRET_FILE: &str = "root-secret.hex";
/// Default public-key artifact name for `root-signer init`.
const DEFAULT_PUBLIC_KEY_FILE: &str = "root-public.key";

enum KeyPolicy {
    AllowWithWarning(String),
    Refuse(String),
}

fn main() {
    let code = match cli() {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("[root-signer] error: {message}");
            1
        }
    };
    std::process::exit(code);
}

fn cli() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return Ok(());
    }
    match args.first().map(String::as_str) {
        Some("sign") => run_sign_cli(&args),
        Some("init") => run_init_cli(&args),
        _ => Err(
            "usage: root-signer sign --csr <file.json> --out <signed.json> [--key-file <path> | --key-hex <64hex>] | root-signer init [--secret-file <path>] [--public-key-file <path>]"
                .to_string(),
        ),
    }
}

fn run_sign_cli(args: &[String]) -> Result<(), String> {
    let mut csr_path: Option<String> = None;
    let mut out_path: Option<String> = None;
    let mut key_file: Option<String> = None;
    let mut key_hex: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--csr" => csr_path = Some(arg_value(args, &mut i, "--csr")?),
            "--out" => out_path = Some(arg_value(args, &mut i, "--out")?),
            "--key-file" => key_file = Some(arg_value(args, &mut i, "--key-file")?),
            "--key-hex" => key_hex = Some(arg_value(args, &mut i, "--key-hex")?),
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }

    let csr_path = csr_path.ok_or("missing required --csr <file.json>")?;
    let out_path = out_path.ok_or("missing required --out <signed.json>")?;
    let secret_key = resolve_private_key(key_file, key_hex)?;
    let expected_root_public_key = std::env::var("GRPC_ROOT_PUBLIC_KEY")
        .ok()
        .map(|raw| decode_expected_root_public_key(&raw))
        .transpose()?;

    run_sign(&SignArgs {
        csr_path,
        out_path,
        secret_key,
        expected_root_public_key,
    })
}

fn print_usage() {
    eprintln!("root-signer — offline Authority Root WILAYA certificate signer");
    eprintln!("usage: root-signer sign --csr <file.json> --out <signed.json> [--key-file <path> | --key-hex <64hex>]");
    eprintln!("       root-signer init [--secret-file <path>] [--public-key-file <path>]");
    eprintln!("       Root private key sources (exactly one): GRPC_ROOT_PRIVATE_KEY env, --key-file, --key-hex.");
    eprintln!("       Optional: GRPC_ROOT_PUBLIC_KEY (Base64) — when set, the derived Root public key MUST match.");
    eprintln!("       init generates a fresh Authority Root keypair; defaults: ./root-secret.hex ./root-public.key. Run OUTSIDE the repository.");
}

fn arg_value(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn resolve_private_key(
    key_file: Option<String>,
    key_hex: Option<String>,
) -> Result<[u8; 32], String> {
    match (key_file, key_hex) {
        (Some(_), Some(_)) => {
            Err("provide exactly one key source: --key-file OR --key-hex".to_string())
        }
        (Some(path), None) => {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("cannot read key file {path}: {e}"))?;
            decode_private_key(&content)
        }
        (None, Some(hex_value)) => decode_private_key(&hex_value),
        (None, None) => match std::env::var("GRPC_ROOT_PRIVATE_KEY") {
            Ok(value) => decode_private_key(&value),
            Err(_) => Err(
                "no Root private key: set GRPC_ROOT_PRIVATE_KEY, or pass --key-file/--key-hex"
                    .to_string(),
            ),
        },
    }
}

fn decode_private_key(input: &str) -> Result<[u8; 32], String> {
    let trimmed = input.trim();
    let raw = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    let bytes: Vec<u8> = if raw.len() == 64 {
        hex::decode(raw).map_err(|e| format!("invalid private key hex: {e}"))?
    } else if raw.len() == 44 {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(raw)
            .map_err(|e| format!("invalid private key base64: {e}"))?
    } else {
        return Err(format!(
            "invalid private key length: expected 32 bytes (64 hex chars or 44 base64 chars), got {} chars",
            raw.len()
        ));
    };
    let len = bytes.len();
    bytes.try_into().map_err(|_| {
        format!(
            "invalid private key length: expected {} bytes, got {}",
            ED25519_PUBLIC_KEY_LEN, len
        )
    })
}

fn decode_expected_root_public_key(raw: &str) -> Result<[u8; 32], String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .map_err(|e| format!("GRPC_ROOT_PUBLIC_KEY: invalid base64: {e}"))?;
    bytes.try_into().map_err(|v: Vec<u8>| {
        format!(
            "GRPC_ROOT_PUBLIC_KEY: expected {} bytes, got {}",
            ED25519_PUBLIC_KEY_LEN,
            v.len()
        )
    })
}

fn run_sign(args: &SignArgs) -> Result<(), String> {
    let csr_json = std::fs::read_to_string(&args.csr_path)
        .map_err(|e| format!("cannot read CSR file {}: {e}", args.csr_path))?;
    let certificate: IdentityCertificate = serde_json::from_str(&csr_json)
        .map_err(|e| format!("malformed CSR file {}: {e}", args.csr_path))?;

    guard_wilaya_only(&certificate)?;
    guard_not_already_signed(&certificate)?;
    guard_algorithm_profile(&certificate)?;
    guard_active_status(&certificate)?;
    guard_public_key_len(&certificate)?;

    let signer = Ed25519SigningProvider::new(args.secret_key);
    let derived_public_key: [u8; 32] = signer
        .public_key()
        .try_into()
        .map_err(|_| "internal: signer produced a non-32-byte public key".to_string())?;

    match test_key_policy(&derived_public_key) {
        KeyPolicy::Refuse(reason) => return Err(reason),
        KeyPolicy::AllowWithWarning(warning) => {
            if !warning.is_empty() {
                eprintln!("{warning}");
            }
        }
    }

    if let Some(expected) = args.expected_root_public_key {
        if derived_public_key != expected {
            return Err(
                "derived Root public key does not match GRPC_ROOT_PUBLIC_KEY; refusing to sign"
                    .to_string(),
            );
        }
    }

    let signature_bytes = signer
        .sign_certificate(&certificate)
        .map_err(|e| format!("signing failed: {e}"))?;
    let signature = Ed25519CertificateSignature::try_from(signature_bytes)
        .map_err(|e| format!("signing failed: {e}"))?;

    let mut signed = certificate;
    signed.signature = Some(signature);

    let self_valid = Ed25519SignatureVerifier
        .verify_certificate(
            &signed,
            &derived_public_key,
            signed
                .signature
                .as_ref()
                .ok_or("internal: missing signature")?,
        )
        .map_err(|e| format!("self-verification error: {e}"))?;
    if !self_valid {
        return Err(
            "self-verification failed: signature did not validate against the derived Root \
             public key; refusing to write the signed certificate"
                .to_string(),
        );
    }

    let out_json =
        serde_json::to_string_pretty(&signed).map_err(|e| format!("serialization failed: {e}"))?;
    std::fs::write(&args.out_path, out_json)
        .map_err(|e| format!("cannot write signed certificate {}: {e}", args.out_path))?;
    eprintln!(
        "[root-signer] signed WILAYA certificate written to {}",
        args.out_path
    );
    Ok(())
}

fn run_init_cli(args: &[String]) -> Result<(), String> {
    let mut secret_file: Option<String> = None;
    let mut public_key_file: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--secret-file" => secret_file = Some(arg_value(args, &mut i, "--secret-file")?),
            "--public-key-file" => {
                public_key_file = Some(arg_value(args, &mut i, "--public-key-file")?)
            }
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }

    run_init(&InitArgs {
        secret_file: secret_file.unwrap_or_else(|| DEFAULT_SECRET_FILE.to_string()),
        public_key_file: public_key_file.unwrap_or_else(|| DEFAULT_PUBLIC_KEY_FILE.to_string()),
    })
}

/// Generates a fresh Authority Root Ed25519 keypair as two files. The secret is
/// written owner-only (0600 on Unix) and the pair is self-verified before any
/// file is created. The keypair exists only once: if either destination already
/// exists (file, directory, or dangling symlink), the command refuses and exits
/// without creating anything. This ceremony runs OUTSIDE the repository.
fn run_init(args: &InitArgs) -> Result<(), String> {
    if destination_occupied(&args.secret_file) {
        return Err(format!(
            "refusing to initialize: secret file {} already exists; the Authority Root keypair is created exactly once",
            args.secret_file
        ));
    }
    if destination_occupied(&args.public_key_file) {
        return Err(format!(
            "refusing to initialize: public key file {} already exists; the Authority Root keypair is created exactly once",
            args.public_key_file
        ));
    }

    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);

    let derived_public_key: [u8; 32] = Ed25519SigningProvider::new(seed)
        .public_key()
        .try_into()
        .map_err(|_| "internal: signer produced a non-32-byte public key".to_string())?;

    match test_key_policy(&derived_public_key) {
        KeyPolicy::Refuse(reason) => return Err(reason),
        KeyPolicy::AllowWithWarning(warning) => {
            if !warning.is_empty() {
                eprintln!("{warning}");
            }
        }
    }

    use base64::Engine;
    let secret_hex = format!("{}\n", hex::encode(seed));
    let public_key_b64 = format!(
        "{}\n",
        base64::engine::general_purpose::STANDARD.encode(derived_public_key)
    );

    verify_generated_pair(&secret_hex, &public_key_b64, &derived_public_key)?;
    self_verify_sign_roundtrip(seed, &derived_public_key)?;

    write_file_atomic(&args.secret_file, secret_hex.as_bytes(), true)?;
    write_file_atomic(&args.public_key_file, public_key_b64.as_bytes(), false)?;

    print!(
        "{}",
        operator_summary(&public_key_b64, &args.secret_file, &args.public_key_file)
    );
    Ok(())
}

/// Validates that the encoded secret and public key round-trip to a matching
/// Ed25519 keypair. Runs BEFORE any file is written.
fn verify_generated_pair(
    secret_hex: &str,
    public_key_b64: &str,
    derived_public_key: &[u8; 32],
) -> Result<(), String> {
    use base64::Engine;
    let decoded_secret = decode_private_key(secret_hex)?;
    let rederived: [u8; 32] = Ed25519SigningProvider::new(decoded_secret)
        .public_key()
        .try_into()
        .map_err(|_| "internal: re-derived public key has an invalid length".to_string())?;
    if rederived != *derived_public_key {
        return Err(
            "internal error: generated keypair failed round-trip verification (public key mismatch); refusing to write"
                .to_string(),
        );
    }
    let decoded_public = base64::engine::general_purpose::STANDARD
        .decode(public_key_b64.trim())
        .map_err(|e| format!("internal error: generated public key encoding failed: {e}"))?;
    if decoded_public.len() != ED25519_PUBLIC_KEY_LEN || decoded_public != derived_public_key {
        return Err(
            "internal error: generated public key failed base64 round-trip verification; refusing to write"
                .to_string(),
        );
    }
    Ok(())
}

/// Signs and verifies a sample WILAYA certificate with the freshly generated
/// keypair, proving the pair can produce a valid chain of trust before any file
/// is created (ADR-0039 §5 — entity-level sign/verify, not raw key math).
fn self_verify_sign_roundtrip(seed: [u8; 32], public_key: &[u8; 32]) -> Result<(), String> {
    let mut certificate = IdentityCertificate {
        identity_id: uuid::Uuid::nil(),
        subject_type: SubjectType::Wilaya,
        subject_id: uuid::Uuid::nil(),
        issuer_identity_id: None,
        credential_id: uuid::Uuid::nil(),
        generation: 1,
        status: CredentialStatus::Active,
        public_key: vec![7u8; 32],
        algorithm_version: IDENTITY_ALGORITHM_PROFILE_ED25519,
        not_after: None,
        package_sequence: None,
        signature: None,
    };
    let signer = Ed25519SigningProvider::new(seed);
    let signature_bytes = signer
        .sign_certificate(&certificate)
        .map_err(|e| format!("internal error: self-verification signing failed: {e}"))?;
    let signature = Ed25519CertificateSignature::try_from(signature_bytes)
        .map_err(|e| format!("internal error: self-verification signature creation failed: {e}"))?;
    certificate.signature = Some(signature);
    let valid = Ed25519SignatureVerifier
        .verify_certificate(
            &certificate,
            public_key,
            certificate
                .signature
                .as_ref()
                .ok_or("internal: missing signature")?,
        )
        .map_err(|e| format!("internal error: self-verification failed: {e}"))?;
    if !valid {
        return Err(
            "internal error: generated keypair failed signature round-trip verification; refusing to write"
                .to_string(),
        );
    }
    Ok(())
}

/// Writes `contents` atomically (temp file in the same directory, then rename).
/// With `restrictive` true, the temp file is chmod 0600 before the rename, so
/// the final artifact is never briefly world-readable. Parent directories are
/// created only when the destination path has a non-empty parent.
fn write_file_atomic(path: &str, contents: &[u8], restrictive: bool) -> Result<(), String> {
    let destination = Path::new(path);
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create directory {}: {e}", parent.display()))?;
        }
    }
    let tmp_path = destination.with_extension("tmp");
    std::fs::write(&tmp_path, contents)
        .map_err(|e| format!("cannot write {}: {e}", tmp_path.display()))?;
    if restrictive {
        apply_restrictive_permissions(&tmp_path)?;
    }
    std::fs::rename(&tmp_path, destination)
        .map_err(|e| format!("cannot finalize {}: {e}", destination.display()))?;
    Ok(())
}

#[cfg(unix)]
fn apply_restrictive_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("cannot restrict permissions on {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn apply_restrictive_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// True when `path` already exists as any filesystem entry — a regular file, a
/// directory, or a dangling symlink (detected via `symlink_metadata`, which
/// does not follow the final component).
fn destination_occupied(path: &str) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Operator-facing ceremony summary. Contains ONLY the public key, never the
/// secret. The caller prints this to stdout.
fn operator_summary(public_key_b64: &str, secret_file: &str, public_key_file: &str) -> String {
    format!(
        "[root-signer] new Authority Root keypair initialized.\n\
         [root-signer] Root PUBLIC KEY (Base64, 32 bytes Ed25519) — the only value that may leave this ceremony:\n\
         [root-signer]   {}\n\
         [root-signer] secret: {} (owner-only, offline — destroy any copies)\n\
         [root-signer] public: {}\n\
         [root-signer] next step: write this PUBLIC KEY (trimmed, no newline) into GRPC's PROD_ROOT_PUBLIC_KEY pin and rebuild the release.\n",
        public_key_b64.trim(),
        secret_file,
        public_key_file
    )
}

fn guard_wilaya_only(certificate: &IdentityCertificate) -> Result<(), String> {
    if certificate.subject_type != SubjectType::Wilaya {
        return Err(format!(
            "refusing to sign: subject_type is {}, but the Authority Root signs ONLY WILAYA certificates (RFC §3.5)",
            certificate.subject_type
        ));
    }
    Ok(())
}

fn guard_not_already_signed(certificate: &IdentityCertificate) -> Result<(), String> {
    if certificate.signature.is_some() {
        return Err("refusing to sign: CSR already carries a signature".to_string());
    }
    Ok(())
}

fn guard_algorithm_profile(certificate: &IdentityCertificate) -> Result<(), String> {
    if certificate.algorithm_version != IDENTITY_ALGORITHM_PROFILE_ED25519 {
        return Err(format!(
            "refusing to sign: algorithm_version is {}, expected the Ed25519 identity profile ({})",
            certificate.algorithm_version, IDENTITY_ALGORITHM_PROFILE_ED25519
        ));
    }
    Ok(())
}

fn guard_active_status(certificate: &IdentityCertificate) -> Result<(), String> {
    if certificate.status != CredentialStatus::Active {
        return Err(format!(
            "refusing to sign: credential status is {}, but the Authority Root issues only ACTIVE certificates",
            certificate.status
        ));
    }
    Ok(())
}

fn guard_public_key_len(certificate: &IdentityCertificate) -> Result<(), String> {
    if certificate.public_key.len() != ED25519_PUBLIC_KEY_LEN {
        return Err(format!(
            "refusing to sign: certificate public key is {} bytes, expected {} (Ed25519)",
            certificate.public_key.len(),
            ED25519_PUBLIC_KEY_LEN
        ));
    }
    Ok(())
}

fn test_key_policy(derived_public_key: &[u8; 32]) -> KeyPolicy {
    test_key_policy_impl(derived_public_key, cfg!(debug_assertions))
}

fn test_key_policy_impl(derived_public_key: &[u8; 32], allow_test_keys: bool) -> KeyPolicy {
    use base64::Engine;
    let is_known_test = [RFC8032_TEST1_PUBLIC_KEY_B64, RFC8032_TEST2_PUBLIC_KEY_B64]
        .iter()
        .any(|encoded| {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .ok()
                .as_deref()
                == Some(derived_public_key.as_slice())
        });
    if !is_known_test {
        return KeyPolicy::AllowWithWarning(String::new());
    }
    if allow_test_keys {
        KeyPolicy::AllowWithWarning(
            "[SECURITY] Root key matches a KNOWN public test vector (RFC 8032 §7.1 TEST 1/TEST 2). \
             The signed certificate is NOT valid for production; use the real Authority Root key."
                .to_string(),
        )
    } else {
        KeyPolicy::Refuse(
            "refusing to sign: Root key matches a KNOWN public test vector (RFC 8032 §7.1 TEST 1/TEST 2). \
             Production signing requires the real Authority Root key and matching GRPC_ROOT_PUBLIC_KEY"
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grpc_lib::domain::identity::{
        CredentialStatus, Ed25519CertificateSignature, IdentityCertificate,
        IdentitySignatureVerifier, SubjectType, IDENTITY_ALGORITHM_PROFILE_ED25519,
    };

    /// RFC 8032 §7.1 TEST 1 secret — the matching public key is the debug-mode
    /// dev Root fallback (`root_public_key.rs`). Never a production key.
    const TEST_ROOT_SECRET: [u8; 32] = [
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
        0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
        0x7f, 0x60,
    ];

    fn sample_wilaya_csr() -> IdentityCertificate {
        IdentityCertificate {
            identity_id: uuid::Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap(),
            subject_type: SubjectType::Wilaya,
            subject_id: uuid::Uuid::parse_str("22222222-3333-4444-5555-666666666666").unwrap(),
            issuer_identity_id: None,
            credential_id: uuid::Uuid::parse_str("33333333-4444-5555-6666-777777777777").unwrap(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: vec![7u8; 32],
            algorithm_version: IDENTITY_ALGORITHM_PROFILE_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        }
    }

    fn run_for(
        certificate: &IdentityCertificate,
        expected_root_public_key: Option<[u8; 32]>,
    ) -> Result<String, String> {
        let dir = tempfile::TempDir::new().unwrap();
        let csr_path = dir.path().join("csr.json");
        let out_path = dir.path().join("signed.json");
        std::fs::write(
            &csr_path,
            serde_json::to_string_pretty(certificate).unwrap(),
        )
        .unwrap();
        let args = SignArgs {
            csr_path: csr_path.to_string_lossy().into_owned(),
            out_path: out_path.to_string_lossy().into_owned(),
            secret_key: TEST_ROOT_SECRET,
            expected_root_public_key,
        };
        match run_sign(&args) {
            Ok(()) => {
                let content = std::fs::read_to_string(&out_path)
                    .map_err(|e| format!("cannot read signed output: {e}"))?;
                Ok(content)
            }
            Err(e) => Err(e),
        }
    }

    #[test]
    fn signs_wilaya_csr_end_to_end_and_self_verifies() {
        let written = run_for(&sample_wilaya_csr(), None).unwrap();
        assert!(written.contains("\"signature\": \""));
        let signed: IdentityCertificate = serde_json::from_str(&written).unwrap();
        assert!(signed.signature.is_some());
        let sig = signed.signature.unwrap();
        let pk: [u8; 32] = Ed25519SigningProvider::new(TEST_ROOT_SECRET)
            .public_key()
            .try_into()
            .unwrap();
        assert!(Ed25519SignatureVerifier
            .verify_certificate(&signed, &pk, &sig)
            .unwrap());
    }

    #[test]
    fn refuses_non_wilaya_subject() {
        let mut certificate = sample_wilaya_csr();
        certificate.subject_type = SubjectType::Unit;
        let err = run_for(&certificate, None).unwrap_err();
        assert!(err.contains("WILAYA"));
    }

    #[test]
    fn refuses_already_signed_csr() {
        let mut certificate = sample_wilaya_csr();
        certificate.signature = Some(Ed25519CertificateSignature::from_bytes([1u8; 64]));
        let err = run_for(&certificate, None).unwrap_err();
        assert!(err.contains("already carries a signature"));
    }

    #[test]
    fn refuses_wrong_algorithm_version() {
        let mut certificate = sample_wilaya_csr();
        certificate.algorithm_version = 3;
        let err = run_for(&certificate, None).unwrap_err();
        assert!(err.contains("algorithm_version"));
    }

    #[test]
    fn refuses_non_active_status() {
        let mut certificate = sample_wilaya_csr();
        certificate.status = CredentialStatus::Revoked;
        let err = run_for(&certificate, None).unwrap_err();
        assert!(err.contains("ACTIVE"));
    }

    #[test]
    fn refuses_short_public_key() {
        let mut certificate = sample_wilaya_csr();
        certificate.public_key = vec![1u8; 16];
        let err = run_for(&certificate, None).unwrap_err();
        assert!(err.contains("Ed25519"));
    }

    #[test]
    fn refuses_root_public_key_mismatch() {
        let err = run_for(&sample_wilaya_csr(), Some([9u8; 32])).unwrap_err();
        assert!(err.contains("GRPC_ROOT_PUBLIC_KEY"));
    }

    #[test]
    fn rejects_malformed_private_key() {
        assert!(decode_private_key("not-a-key").is_err());
        assert!(decode_private_key("ab").is_err());
        assert!(decode_private_key(&"0".repeat(64)).is_ok());
    }

    #[test]
    fn parses_hex_and_base64_secret_formats() {
        use base64::Engine;
        let hex_secret = hex::encode(TEST_ROOT_SECRET);
        assert_eq!(decode_private_key(&hex_secret).unwrap(), TEST_ROOT_SECRET);
        let b64_secret = base64::engine::general_purpose::STANDARD.encode(TEST_ROOT_SECRET);
        assert_eq!(decode_private_key(&b64_secret).unwrap(), TEST_ROOT_SECRET);
    }

    #[test]
    fn test_key_policy_refuses_known_vectors_in_prod_semantics() {
        use base64::Engine;
        let test1: [u8; 32] = base64::engine::general_purpose::STANDARD
            .decode(RFC8032_TEST1_PUBLIC_KEY_B64)
            .unwrap()
            .try_into()
            .unwrap();
        let test2: [u8; 32] = base64::engine::general_purpose::STANDARD
            .decode(RFC8032_TEST2_PUBLIC_KEY_B64)
            .unwrap()
            .try_into()
            .unwrap();
        assert!(matches!(
            test_key_policy_impl(&test1, false),
            KeyPolicy::Refuse(_)
        ));
        assert!(matches!(
            test_key_policy_impl(&test2, false),
            KeyPolicy::Refuse(_)
        ));
        match test_key_policy_impl(&test1, true) {
            KeyPolicy::AllowWithWarning(w) => assert!(!w.is_empty()),
            KeyPolicy::Refuse(_) => panic!("debug semantics must allow test keys with a warning"),
        }
    }

    #[test]
    fn test_key_policy_allows_unknown_key_without_warning() {
        match test_key_policy_impl(&[42u8; 32], false) {
            KeyPolicy::AllowWithWarning(w) => assert!(w.is_empty()),
            KeyPolicy::Refuse(_) => panic!("an unknown key must be allowed"),
        }
    }

    fn init_in(dir: &tempfile::TempDir) -> InitArgs {
        InitArgs {
            secret_file: dir
                .path()
                .join("root-secret.hex")
                .to_string_lossy()
                .into_owned(),
            public_key_file: dir
                .path()
                .join("root-public.key")
                .to_string_lossy()
                .into_owned(),
        }
    }

    #[test]
    fn init_generates_valid_keypair_files() {
        use base64::Engine;
        let dir = tempfile::TempDir::new().unwrap();
        let args = init_in(&dir);
        run_init(&args).unwrap();

        let secret = std::fs::read_to_string(&args.secret_file).unwrap();
        assert_eq!(secret.len(), 65, "64 hex chars + trailing newline");
        assert!(secret.ends_with('\n'));
        assert!(secret.trim().chars().all(|c| c.is_ascii_hexdigit()));
        let seed = decode_private_key(&secret).unwrap();

        let public = std::fs::read_to_string(&args.public_key_file).unwrap();
        assert!(public.ends_with('\n'));
        let pk = base64::engine::general_purpose::STANDARD
            .decode(public.trim())
            .unwrap();
        assert_eq!(pk.len(), ED25519_PUBLIC_KEY_LEN);

        let expected: [u8; 32] = Ed25519SigningProvider::new(seed)
            .public_key()
            .try_into()
            .unwrap();
        assert_eq!(pk, expected);
    }

    #[cfg(unix)]
    #[test]
    fn init_secret_is_owner_only_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let args = init_in(&dir);
        run_init(&args).unwrap();
        let secret_mode = std::fs::metadata(&args.secret_file)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(secret_mode & 0o777, 0o600);
        let public_mode = std::fs::metadata(&args.public_key_file)
            .unwrap()
            .permissions()
            .mode();
        assert_ne!(public_mode & 0o777, 0o600);
    }

    #[test]
    fn init_refuses_existing_secret_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let args = init_in(&dir);
        std::fs::write(&args.secret_file, "x").unwrap();
        let err = run_init(&args).unwrap_err();
        assert!(err.contains("root-secret.hex"));
        assert!(err.contains("already exists"));
        assert!(!Path::new(&args.public_key_file).exists());
    }

    #[test]
    fn init_refuses_existing_public_key_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let args = init_in(&dir);
        std::fs::write(&args.public_key_file, "x").unwrap();
        let err = run_init(&args).unwrap_err();
        assert!(err.contains("root-public.key"));
        assert!(err.contains("already exists"));
        assert!(!Path::new(&args.secret_file).exists());
    }

    #[test]
    fn init_refuses_partial_initialization() {
        let dir = tempfile::TempDir::new().unwrap();
        let args = init_in(&dir);
        run_init(&args).unwrap();
        let secret_before = std::fs::read_to_string(&args.secret_file).unwrap();
        let err = run_init(&args).unwrap_err();
        assert!(err.contains("already exists"));
        assert_eq!(
            std::fs::read_to_string(&args.secret_file).unwrap(),
            secret_before,
            "a failed re-init must never overwrite the existing secret"
        );
    }

    #[test]
    fn init_refuses_directory_destination() {
        let dir = tempfile::TempDir::new().unwrap();
        let args = init_in(&dir);
        std::fs::create_dir(&args.secret_file).unwrap();
        let err = run_init(&args).unwrap_err();
        assert!(err.contains("already exists"));
        assert!(!Path::new(&args.public_key_file).exists());
    }

    #[test]
    fn init_rejects_force_as_unknown_argument() {
        let err = run_init_cli(&["init".to_string(), "--force".to_string()]).unwrap_err();
        assert!(err.contains("unknown argument"));
        assert!(err.contains("--force"));
    }

    #[test]
    fn init_secret_never_enters_operator_output() {
        let dir = tempfile::TempDir::new().unwrap();
        let args = init_in(&dir);
        run_init(&args).unwrap();
        let secret = std::fs::read_to_string(&args.secret_file).unwrap();
        let public = std::fs::read_to_string(&args.public_key_file).unwrap();
        let summary = operator_summary(&public, &args.secret_file, &args.public_key_file);
        assert!(!summary.contains(secret.trim()));
    }

    #[test]
    fn init_generated_pair_signs_and_verifies() {
        let dir = tempfile::TempDir::new().unwrap();
        let args = init_in(&dir);
        run_init(&args).unwrap();
        let seed =
            decode_private_key(&std::fs::read_to_string(&args.secret_file).unwrap()).unwrap();
        let public: [u8; 32] = Ed25519SigningProvider::new(seed)
            .public_key()
            .try_into()
            .unwrap();
        self_verify_sign_roundtrip(seed, &public).unwrap();
    }

    #[test]
    fn init_secret_is_consumable_by_key_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let args = init_in(&dir);
        run_init(&args).unwrap();

        let secret =
            decode_private_key(&std::fs::read_to_string(&args.secret_file).unwrap()).unwrap();
        let csr_path = dir.path().join("csr.json");
        let out_path = dir.path().join("signed.json");
        std::fs::write(
            &csr_path,
            serde_json::to_string_pretty(&sample_wilaya_csr()).unwrap(),
        )
        .unwrap();

        run_sign(&SignArgs {
            csr_path: csr_path.to_string_lossy().into_owned(),
            out_path: out_path.to_string_lossy().into_owned(),
            secret_key: secret,
            expected_root_public_key: None,
        })
        .unwrap();

        let signed: IdentityCertificate =
            serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
        let pk: [u8; 32] = Ed25519SigningProvider::new(secret)
            .public_key()
            .try_into()
            .unwrap();
        assert!(Ed25519SignatureVerifier
            .verify_certificate(&signed, &pk, signed.signature.as_ref().unwrap())
            .unwrap());
    }

    #[test]
    fn init_secret_encoding_roundtrips_a_fixed_seed() {
        use base64::Engine;
        let secret_hex = format!("{}\n", hex::encode(TEST_ROOT_SECRET));
        let public: [u8; 32] = Ed25519SigningProvider::new(TEST_ROOT_SECRET)
            .public_key()
            .try_into()
            .unwrap();
        let public_key_b64 = format!(
            "{}\n",
            base64::engine::general_purpose::STANDARD.encode(public)
        );
        verify_generated_pair(&secret_hex, &public_key_b64, &public).unwrap();
        assert_eq!(decode_private_key(&secret_hex).unwrap(), TEST_ROOT_SECRET);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(public_key_b64.trim())
                .unwrap(),
            public
        );
    }
}
