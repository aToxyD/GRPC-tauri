use crate::errors::AppResult;

pub trait PackageHasher: Send + Sync {
    fn hash(&self, plaintext: &[u8]) -> AppResult<String>;
}
