use crate::domain::security::PasswordHashPort;
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Fixed pre-hash derivation domain for the fleet-wide synchronized `admin`
/// account (B8 — Identity & Access Synchronization). Identical on every node
/// so a single hash authenticates the unified `admin` anywhere in the fleet.
/// Lives exclusively inside this provider; no caller ever references it.
const GLOBAL_ADMIN_DOMAIN: &str = "offline-pos-admin-v1";

#[derive(Clone, Copy, Debug, Default)]
pub struct Argon2PasswordHashProvider;

impl PasswordHashPort for Argon2PasswordHashProvider {
    fn hash_node(&self, password: &str, node_id: &str) -> Result<String, String> {
        Self::hash_bound(password, node_id)
    }

    fn verify_node(&self, password: &str, node_id: &str, hash: &str) -> Result<bool, String> {
        Self::verify_bound(password, node_id, hash)
    }

    fn hash_admin(&self, password: &str) -> Result<String, String> {
        Self::hash_bound(password, GLOBAL_ADMIN_DOMAIN)
    }

    fn verify_admin(&self, password: &str, hash: &str) -> Result<bool, String> {
        Self::verify_bound(password, GLOBAL_ADMIN_DOMAIN, hash)
    }
}

impl Argon2PasswordHashProvider {
    fn hash_bound(password: &str, key: &str) -> Result<String, String> {
        let bound_password = pre_hash(password, key)?;

        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(bound_password.as_bytes(), &salt)
            .map_err(|e| e.to_string())?;
        Ok(password_hash.to_string())
    }

    fn verify_bound(password: &str, key: &str, hash: &str) -> Result<bool, String> {
        let bound_password = pre_hash(password, key)?;

        let parsed_hash = PasswordHash::new(hash).map_err(|e| e.to_string())?;
        let argon2 = Argon2::default();
        Ok(argon2
            .verify_password(bound_password.as_bytes(), &parsed_hash)
            .is_ok())
    }
}

fn pre_hash(password: &str, key: &str) -> Result<String, String> {
    let mut mac = HmacSha256::new_from_slice(key.as_bytes())
        .map_err(|e| format!("HMAC init failed: {}", e))?;
    mac.update(password.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_node_password() {
        let provider = Argon2PasswordHashProvider;
        let password = "MySecurePass123";
        let node_id = "TEST_NODE";

        let hash = provider.hash_node(password, node_id).unwrap();
        assert!(provider.verify_node(password, node_id, &hash).unwrap());
        assert!(!provider
            .verify_node("WrongPassword", node_id, &hash)
            .unwrap());
        assert!(!provider.verify_node(password, "WRONG_NODE", &hash).unwrap());
    }

    #[test]
    fn test_admin_hash_is_fleet_wide_and_distinct_from_node_bound() {
        let provider = Argon2PasswordHashProvider;
        let password = "MySecurePass123";

        let hash = provider.hash_admin(password).unwrap();
        assert!(provider.verify_admin(password, &hash).unwrap());
        assert!(!provider.verify_admin("WrongPassword", &hash).unwrap());

        let node_hash = provider.hash_node(password, "TEST_NODE").unwrap();
        assert!(!provider.verify_admin(password, &node_hash).unwrap());
        assert!(!provider.verify_node(password, "TEST_NODE", &hash).unwrap());

        let another = provider.hash_admin(password).unwrap();
        assert_ne!(hash, another);
    }
}
