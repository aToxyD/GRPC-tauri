use crate::errors::AppResult;

pub trait PackageVerifier: Send + Sync {
    fn verify(&self, plaintext: &[u8], signature: &str) -> AppResult<bool>;
}
