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

use grpc_lib::domain::identity::{
    Ed25519CertificateSignature, IdentityCertificate, IdentitySignatureVerifier, IdentitySigner,
    CredentialStatus, IDENTITY_ALGORITHM_PROFILE_ED25519, SubjectType,
};
use grpc_lib::infrastructure::security::{
    Ed25519SignatureVerifier, Ed25519SigningProvider,
};

const ED25519_PUBLIC_KEY_LEN: usize = 32;

/// RFC 8032 §7.1 TEST 1 public key (Base64) — the debug-mode dev Root fallback.
const RFC8032_TEST1_PUBLIC_KEY_B64: &str = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";
/// RFC 8032 §7.1 TEST 2 public key (Base64) — the PROD_ROOT_PUBLIC_KEY pin
/// placeholder in `root_public_key.rs`. Never a real production key.
const RFC8032_TEST2_PUBLIC_KEY_B64: &str = "PUAXw+hDiVqStwqnTRt+vJyYLM8uxJaMwM1V8Sr0Zgw=";

struct SignArgs {
    csr_path: String,
    out_path: String,
    secret_key: [u8; 32],
    expected_root_public_key: Option<[u8; 32]>,
}

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
    if args.first().map(String::as_str) != Some("sign") {
        return Err("usage: root-signer sign --csr <file.json> --out <signed.json> [--key-file <path> | --key-hex <64hex>]".to_string());
    }

    let mut csr_path: Option<String> = None;
    let mut out_path: Option<String> = None;
    let mut key_file: Option<String> = None;
    let mut key_hex: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--csr" => csr_path = Some(arg_value(&args, &mut i, "--csr")?),
            "--out" => out_path = Some(arg_value(&args, &mut i, "--out")?),
            "--key-file" => key_file = Some(arg_value(&args, &mut i, "--key-file")?),
            "--key-hex" => key_hex = Some(arg_value(&args, &mut i, "--key-hex")?),
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
    eprintln!("       Root private key sources (exactly one): GRPC_ROOT_PRIVATE_KEY env, --key-file, --key-hex.");
    eprintln!("       Optional: GRPC_ROOT_PUBLIC_KEY (Base64) — when set, the derived Root public key MUST match.");
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
        (Some(_), Some(_)) => Err("provide exactly one key source: --key-file OR --key-hex".to_string()),
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
            signed.signature.as_ref().ok_or("internal: missing signature")?,
        )
        .map_err(|e| format!("self-verification error: {e}"))?;
    if !self_valid {
        return Err(
            "self-verification failed: signature did not validate against the derived Root \
             public key; refusing to write the signed certificate"
                .to_string(),
        );
    }

    let out_json = serde_json::to_string_pretty(&signed)
        .map_err(|e| format!("serialization failed: {e}"))?;
    std::fs::write(&args.out_path, out_json)
        .map_err(|e| format!("cannot write signed certificate {}: {e}", args.out_path))?;
    eprintln!(
        "[root-signer] signed WILAYA certificate written to {}",
        args.out_path
    );
    Ok(())
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
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
        0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
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
        std::fs::write(&csr_path, serde_json::to_string_pretty(certificate).unwrap()).unwrap();
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
        assert!(
            Ed25519SignatureVerifier
                .verify_certificate(&signed, &pk, &sig)
                .unwrap()
        );
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
}
