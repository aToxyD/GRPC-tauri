mod ed25519_package_signer;
mod package_signer;
mod package_verifier;

/// Default `metadata.signature_version` for NEW package exports.
///
/// B6-B (Commit ④, RFC 2026-08-04 §3.10): Ed25519 (2) is the production
/// default. SEC-007 (ADR-0047): V1/HMAC sync packages are permanently removed —
/// Ed25519 (2) is the ONLY supported signature version.
pub const DEFAULT_SIGNATURE_VERSION: u16 = 2;

pub use ed25519_package_signer::{Ed25519PackageSigner, Ed25519PackageVerifier};
pub use package_signer::PackageSigner;
pub use package_verifier::PackageVerifier;
