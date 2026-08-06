mod ed25519_package_signer;
mod hmac_package_signer;
mod package_signer;
mod package_verifier;

pub const DEFAULT_SIGNATURE_VERSION: u16 = 1;

pub use ed25519_package_signer::{Ed25519PackageSigner, Ed25519PackageVerifier};
pub use hmac_package_signer::HmacPackageSigner;
pub use package_signer::PackageSigner;
pub use package_verifier::PackageVerifier;
