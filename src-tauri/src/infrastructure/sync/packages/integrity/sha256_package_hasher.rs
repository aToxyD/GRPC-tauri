use crate::errors::AppResult;

use super::package_hasher::PackageHasher;

#[derive(Clone, Copy, Debug, Default)]
pub struct Sha256PackageHasher;

impl PackageHasher for Sha256PackageHasher {
    fn hash(&self, plaintext: &[u8]) -> AppResult<String> {
        use sha2::{Digest, Sha256};

        let digest = Sha256::digest(plaintext);
        Ok(digest
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>())
    }
}
