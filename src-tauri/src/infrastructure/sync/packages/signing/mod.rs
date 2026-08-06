mod ed25519_package_signer;
mod hmac_package_signer;
mod package_signer;
mod package_verifier;

/// Default `metadata.signature_version` for NEW package exports.
///
/// B6-B (Commit ④, RFC 2026-08-04 §3.10): Ed25519 (2) is the production
/// default. HMAC (1) remains READ-ONLY during the deprecation window and on the
/// `.unit` bootstrap channel (see RFC §7); it is never produced by default.
pub const DEFAULT_SIGNATURE_VERSION: u16 = 2;

pub use ed25519_package_signer::{Ed25519PackageSigner, Ed25519PackageVerifier};
pub use hmac_package_signer::HmacPackageSigner;
pub use package_signer::PackageSigner;
pub use package_verifier::PackageVerifier;
