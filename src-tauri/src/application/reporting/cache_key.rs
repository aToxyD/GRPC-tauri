use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Deterministic cache key for report results.
/// SHA256(slug | version | canonical_input_json | fiscal_scope).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheKey(String);

impl CacheKey {
    pub fn new(slug: &str, version: u32, input_json: &str, fiscal_scope: Option<i32>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(slug.as_bytes());
        hasher.update(b"|");
        hasher.update(version.to_string().as_bytes());
        hasher.update(b"|");
        hasher.update(input_json.as_bytes());
        hasher.update(b"|");
        match fiscal_scope {
            Some(y) => hasher.update(y.to_string().as_bytes()),
            None => hasher.update(b"null"),
        }
        Self(format!("{:x}", hasher.finalize()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for CacheKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_deterministic() {
        let k1 = CacheKey::new("test", 1, "{\"x\":1}", Some(2024));
        let k2 = CacheKey::new("test", 1, "{\"x\":1}", Some(2024));
        assert_eq!(k1, k2);
        assert_eq!(k1.as_str(), k2.as_str());
    }

    #[test]
    fn cache_key_changes_with_slug() {
        let k1 = CacheKey::new("a", 1, "{}", None);
        let k2 = CacheKey::new("b", 1, "{}", None);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_changes_with_version() {
        let k1 = CacheKey::new("r", 1, "{}", None);
        let k2 = CacheKey::new("r", 2, "{}", None);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_changes_with_input() {
        let k1 = CacheKey::new("r", 1, "{\"a\":1}", None);
        let k2 = CacheKey::new("r", 1, "{\"a\":2}", None);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_changes_with_fiscal_scope() {
        let k1 = CacheKey::new("r", 1, "{}", Some(2024));
        let k2 = CacheKey::new("r", 1, "{}", Some(2025));
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_is_hex_string() {
        let k = CacheKey::new("r", 1, "{}", None);
        assert!(k.as_str().len() == 64);
        assert!(k.as_str().chars().all(|c| c.is_ascii_hexdigit()));
    }
}
