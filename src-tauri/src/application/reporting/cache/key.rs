use crate::application::reporting::CacheKey as ParentCacheKey;

pub type CacheKey = ParentCacheKey;

pub fn build_cache_key(
    slug: &str,
    version: u32,
    input_json: &str,
    fiscal_scope: Option<i32>,
) -> CacheKey {
    CacheKey::new(slug, version, input_json, fiscal_scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_cache_key_is_deterministic() {
        let k1 = build_cache_key("test", 1, "{}", Some(2024));
        let k2 = build_cache_key("test", 1, "{}", Some(2024));
        assert_eq!(k1, k2);
    }

    #[test]
    fn build_cache_key_differs_with_fiscal_scope() {
        let k1 = build_cache_key("r", 1, "{}", Some(2024));
        let k2 = build_cache_key("r", 1, "{}", Some(2025));
        assert_ne!(k1, k2);
    }
}
