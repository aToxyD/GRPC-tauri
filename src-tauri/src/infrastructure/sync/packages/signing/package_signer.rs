use crate::errors::AppResult;

pub trait PackageSigner: Send + Sync {
    fn sign(&self, plaintext: &[u8]) -> AppResult<String>;
}
