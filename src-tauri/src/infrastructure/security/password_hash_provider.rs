use crate::domain::security::PasswordHashPort;
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, Debug, Default)]
pub struct Argon2PasswordHashProvider;

impl PasswordHashPort for Argon2PasswordHashProvider {
    fn hash_password(&self, password: &str, node_id: &str) -> Result<String, String> {
        let mut mac = HmacSha256::new_from_slice(node_id.as_bytes())
            .map_err(|e| format!("HMAC init failed: {}", e))?;
        mac.update(password.as_bytes());
        let node_bound_password = hex::encode(mac.finalize().into_bytes());

        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(node_bound_password.as_bytes(), &salt)
            .map_err(|e| e.to_string())?;
        Ok(password_hash.to_string())
    }

    fn verify_password(&self, password: &str, node_id: &str, hash: &str) -> Result<bool, String> {
        let mut mac = HmacSha256::new_from_slice(node_id.as_bytes())
            .map_err(|e| format!("HMAC init failed: {}", e))?;
        mac.update(password.as_bytes());
        let node_bound_password = hex::encode(mac.finalize().into_bytes());

        let parsed_hash = PasswordHash::new(hash).map_err(|e| e.to_string())?;
        let argon2 = Argon2::default();
        Ok(argon2
            .verify_password(node_bound_password.as_bytes(), &parsed_hash)
            .is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let provider = Argon2PasswordHashProvider;
        let password = "MySecurePass123";
        let node_id = "TEST_NODE";

        let hash = provider.hash_password(password, node_id).unwrap();
        assert!(provider.verify_password(password, node_id, &hash).unwrap());
        assert!(!provider
            .verify_password("WrongPassword", node_id, &hash)
            .unwrap());
        assert!(!provider
            .verify_password(password, "WRONG_NODE", &hash)
            .unwrap());
    }
}
