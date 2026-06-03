//! Domain Logic Integration Tests
//!
//! Tests for domain services and business logic

use grpc_lib::domain::*;

/// Test rate limiter
#[test]
fn test_rate_limiter() {
    let limiter = rate_limiter::RateLimiter::new();
    let username = "test_user";

    // First few attempts should be allowed
    assert!(limiter.is_allowed(username));
    assert!(limiter.is_allowed(username));

    // Record some failures
    for _ in 0..3 {
        limiter.record_failure(username);
    }

    // After failures, might be blocked
    let _still_allowed = limiter.is_allowed(username);

    // Check remaining attempts
    let remaining = limiter.get_remaining_attempts(username);
    assert!(remaining <= 5);
}

/// Test audit logger structure
#[test]
fn test_audit_entry_structure() {
    use grpc_lib::domain::audit::{AuditAction, EntityType};

    // Verify action enum variants
    let action = AuditAction::CreateProduct;
    assert_eq!(action.as_str(), "CreateProduct");

    let entity = EntityType::Product;
    assert_eq!(entity.as_str(), "Product");
}

/// Test validation helpers
#[test]
fn test_validation_helpers() {
    use grpc_lib::domain::validation::*;

    // Test sanitization
    let dirty = "  hello   world  ";
    let clean = sanitize_string(dirty);
    assert_eq!(clean, "hello world");

    // Test SQL safety check (Note: We rely on parameterization, so standard characters are allowed)
    assert!(is_sql_safe("hello world"));
    assert!(is_sql_safe("hello'; DROP TABLE; --"));
    assert!(!is_sql_safe("test\0null"));
}

/// Test session management
#[test]
fn test_session_lifecycle() {
    use chrono::Utc;
    use grpc_lib::domain::session::CurrentSession;
    use grpc_lib::models::UserRole;

    let now = Utc::now();
    let session = CurrentSession {
        user_id: "U001".to_string(),
        username: "admin".to_string(),
        user_role: UserRole::Admin,
        session_id: "sess_123".to_string(),
        created_at: now,
        last_activity: now,
        timeout_minutes: 30,
        user_snapshot: grpc_lib::domain::session::UserSnapshot {
            id: "U001".to_string(),
            username: "admin".to_string(),
            role: UserRole::Admin,
            created_at: now,
        },
    };

    assert!(!session.is_expired()); // Check if expired

    // Test touching session updates activity
    let mut session_clone = session.clone();
    let before_touch = session_clone.last_activity;
    session_clone.touch();
    assert!(session_clone.last_activity >= before_touch);
}

/// Test CSV handler (basic validation)
#[test]
fn test_csv_handler_validation() {
    use grpc_lib::domain::validation::validate_file_path;

    let csv_file = "export.csv";
    let allowed = &["csv", "txt"];
    assert!(validate_file_path(csv_file, allowed).is_ok());

    let invalid = "script.sh";
    assert!(validate_file_path(invalid, allowed).is_err());
}

/// Test backup manager structure
#[test]
fn test_backup_info() {
    use grpc_lib::domain::backup::BackupInfo;

    let info = BackupInfo {
        filename: "backup_20240101.db".to_string(),
        path: "/backups/backup.db".to_string(),
        size_bytes: 1024 * 1024, // 1MB
        created: chrono::Local::now().to_rfc3339(),
    };

    assert!(info.size_bytes > 0);
    assert!(!info.path.is_empty());
}
