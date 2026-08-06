//! Security Utilities
//!
//! Input sanitization, XSS prevention, and security helpers

pub trait PasswordHashPort: Send + Sync {
    /// Hash a node-bound password. The pre-hash HMAC key is derived from
    /// `node_id`, so the resulting hash is valid only on that node.
    fn hash_node(&self, password: &str, node_id: &str) -> Result<String, String>;
    fn verify_node(&self, password: &str, node_id: &str, hash: &str) -> Result<bool, String>;

    /// Hash / verify the fleet-wide synchronized `admin` account.
    ///
    /// The pre-hash HMAC key is a fixed global admin domain constant that is
    /// identical on every node, so a single hash authenticates the unified
    /// `admin` account anywhere in the fleet. The derivation domain lives
    /// exclusively inside the provider — callers never see it.
    fn hash_admin(&self, password: &str) -> Result<String, String>;
    fn verify_admin(&self, password: &str, hash: &str) -> Result<bool, String>;
}

use regex::Regex;
use std::sync::LazyLock;

static HTML_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[^>]+>").expect("Invalid HTML regex"));

static JS_PROTOCOL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)javascript:").expect("Invalid JS regex"));

static DATA_URI_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)data:[^;]*;base64,").expect("Invalid data URI regex"));

static SQL_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"(?i)(--|#|/\*|\*/)").expect("SQL comment regex"),
        Regex::new(
            r"(?i)(SELECT|INSERT|UPDATE|DELETE|DROP|CREATE|ALTER|EXEC|EXECUTE|UNION|WHERE|FROM|TABLE|DATABASE)\s+",
        )
        .expect("SQL keyword regex"),
        Regex::new(r"(?i)(;\s*--|;\s*#)").expect("SQL ending regex"),
    ]
});

static PATH_TRAVERSAL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\.\./|\.\.\\|%2e%2e/|%2e%2e%5c|%2e%2e%2f|%2e%2e%2F")
        .expect("Path traversal regex")
});

static SPACE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("Space regex"));

/// Comprehensive input sanitization for user inputs
pub fn sanitize_input(input: &str) -> String {
    let mut result = input.trim().to_string();

    // Remove HTML tags (XSS prevention)
    result = HTML_REGEX.replace_all(&result, "").to_string();

    // Remove JavaScript protocol
    result = JS_PROTOCOL_REGEX.replace_all(&result, "").to_string();

    // Remove data URIs
    result = DATA_URI_REGEX.replace_all(&result, "").to_string();

    // Normalize Arabic text (safe for storage)
    result = normalize_arabic_for_storage(&result);

    // Remove null bytes
    result = result.replace('\0', "");

    // Limit length (prevent DoS)
    if result.len() > 1000 {
        result.truncate(1000);
    }

    result
}

/// Sanitize for SQL contexts (table/column names)
pub fn sanitize_sql_identifier(input: &str) -> String {
    let sanitized: String = input
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .take(64) // Max identifier length
        .collect();

    // Ensure it starts with letter or underscore
    if sanitized.is_empty() {
        return "_".to_string();
    }

    if !sanitized.chars().next().unwrap().is_alphabetic() && !sanitized.starts_with('_') {
        format!("_{}", sanitized)
    } else {
        sanitized
    }
}

/// Check for SQL injection patterns
pub fn contains_sql_injection(input: &str) -> bool {
    SQL_PATTERNS.iter().any(|pattern| pattern.is_match(input))
}

/// Check for path traversal attempts
pub fn contains_path_traversal(input: &str) -> bool {
    PATH_TRAVERSAL_REGEX.is_match(input)
}

/// Validate and sanitize file path
pub fn sanitize_file_path(path: &str, allowed_extensions: &[&str]) -> Result<String, String> {
    // Check for path traversal
    if contains_path_traversal(path) {
        return Err("Path traversal detected".to_string());
    }

    // Check extension
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match extension {
        Some(ext) if allowed_extensions.contains(&ext.as_str()) => Ok(path.to_string()),
        _ => Err(format!(
            "Invalid file extension. Allowed: {:?}",
            allowed_extensions
        )),
    }
}

/// Normalize Arabic text for safe storage
pub fn normalize_arabic_for_storage(input: &str) -> String {
    let mut result = input.to_string();

    // Remove tatweel (kashida)
    result = result.replace('\u{0640}', "");

    // Replace multiple spaces
    result = SPACE_REGEX.replace_all(&result, " ").to_string();

    result.trim().to_string()
}

/// Normalize Arabic text for fuzzy search/comparison
pub fn normalize_arabic_for_search(input: &str) -> String {
    let mut result = input.to_string();

    // Remove tatweel (kashida)
    result = result.replace('\u{0640}', "");

    // Remove Arabic diacritics (tashkeel/harakat): U+064B to U+065F
    result = result
        .chars()
        .filter(|c| !(*c >= '\u{064B}' && *c <= '\u{065F}'))
        .collect();

    // Normalize alef variations
    result = result.replace(['أ', 'إ', 'آ', 'ٱ'], "ا");

    // Normalize hamza
    result = result.replace('ء', "").replace('ؤ', "و").replace('ئ', "ي");

    // Normalize ta marbuta
    result = result.replace('ة', "ه");

    // Replace multiple spaces
    result = SPACE_REGEX.replace_all(&result, " ").to_string();

    result.trim().to_string()
}

/// Validate password strength
pub fn validate_password_strength(password: &str) -> Result<(), String> {
    if password.len() < 8 {
        return Err("Password must be at least 8 characters".to_string());
    }

    if !password.chars().any(|c| c.is_uppercase()) {
        return Err("Password must contain at least one uppercase letter".to_string());
    }

    if !password.chars().any(|c| c.is_lowercase()) {
        return Err("Password must contain at least one lowercase letter".to_string());
    }

    if !password.chars().any(|c| c.is_numeric()) {
        return Err("Password must contain at least one number".to_string());
    }

    // Check for common passwords
    let common_passwords = ["password", "123456", "qwerty", "admin"];
    if common_passwords.contains(&password.to_lowercase().as_str()) {
        return Err("Password is too common".to_string());
    }

    Ok(())
}

/// Sanitize for HTML display (escape special chars)
pub fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

// End of domain/security.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_input() {
        assert_eq!(
            sanitize_input("<script>alert('xss')</script>"),
            "alert('xss')"
        );
        assert_eq!(sanitize_input("javascript:alert(1)"), "alert(1)");
        assert_eq!(sanitize_input("  hello   world  "), "hello world");
    }

    #[test]
    fn test_sql_injection_detection() {
        assert!(contains_sql_injection("'; DROP TABLE users; --"));
        assert!(contains_sql_injection("SELECT * FROM users"));
        assert!(!contains_sql_injection("hello world"));
    }

    #[test]
    fn test_path_traversal_detection() {
        assert!(contains_path_traversal("../../../etc/passwd"));
        assert!(!contains_path_traversal("/safe/path"));
    }

    #[test]
    fn test_password_strength() {
        assert!(validate_password_strength("Strong1Pass").is_ok());
        assert!(validate_password_strength("weak").is_err());
        assert!(validate_password_strength("password").is_err());
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(escape_html("&"), "&amp;");
    }

    #[test]
    fn test_arabic_normalization_for_storage() {
        assert_eq!(normalize_arabic_for_storage("مُتَمَرِّد"), "مُتَمَرِّد");
        assert_eq!(normalize_arabic_for_storage("كتابة"), "كتابة");
        assert_eq!(normalize_arabic_for_storage("مسؤول"), "مسؤول");
    }

    #[test]
    fn test_arabic_normalization_for_search() {
        assert_eq!(normalize_arabic_for_search("مُتَمَرِّد"), "متمرد");
        assert_eq!(normalize_arabic_for_search("كتابة"), "كتابه");
        assert_eq!(normalize_arabic_for_search("مسؤول"), "مسوول");
    }
}

// Tests using Argon2 are moved to infrastructure
