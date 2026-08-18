//! Common Test Utilities
//!
//! Shared helpers and fixtures for all tests

use grpc_lib::commands::AppState;
use grpc_lib::db::ConnectionFactory;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

/// Create a test AppState with isolated database
#[allow(dead_code)]
pub fn create_test_state() -> (AppState, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Initialize test database
    let db = ConnectionFactory::new_with_path(&db_path).expect("Failed to create test database");

    let state = AppState {
        db: Arc::new(Mutex::new(Some(db))),
        rate_limiter: Arc::new(Mutex::new(
            grpc_lib::domain::rate_limiter::RateLimiter::new(),
        )),
        operation_guard: Arc::new(grpc_lib::application::services::OperationExecutionGuard::new()),
        maintenance: grpc_lib::application::services::SystemMaintenanceHandle::new(
            grpc_lib::application::services::SystemMaintenanceState::Normal,
        ),
        current_session: Arc::new(Mutex::new(None)),
        identity_challenge: Arc::new(Mutex::new(
            grpc_lib::domain::identity::IdentityChallengeState::default(),
        )),
        crypto_port:
            grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider::new(),
        password_port: Arc::new(grpc_lib::infrastructure::security::Argon2PasswordHashProvider),
        process_start_time: std::time::Instant::now(),
    };

    (state, temp_dir)
}

/// Create a test user session
#[allow(dead_code)]
pub fn create_test_session(
    user_id: &str,
    username: &str,
    role: &str,
) -> grpc_lib::domain::session::CurrentSession {
    let role = grpc_lib::models::UserRole::from(role.to_string());
    let now = chrono::Utc::now();
    grpc_lib::domain::session::CurrentSession {
        user_id: user_id.to_string(),
        username: username.to_string(),
        user_role: role.clone(),
        session_id: uuid::Uuid::new_v4().to_string(),
        created_at: now,
        last_activity: now,
        timeout_minutes: 30,
        user_snapshot: grpc_lib::domain::session::UserSnapshot {
            id: user_id.to_string(),
            username: username.to_string(),
            role,
            created_at: now,
        },
    }
}

/// Insert a minimal user row so session revalidation (SEC-003-08) finds it.
/// Idempotent: replaces any row with the same id/username.
#[allow(dead_code)]
pub fn insert_test_user(
    state: &grpc_lib::commands::AppState,
    id: &str,
    username: &str,
    role: &str,
) {
    use rusqlite::params;
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    db.get_connection()
        .execute(
            "INSERT OR REPLACE INTO users (id, username, password_hash, role, created_at, node_id) VALUES (?1, ?2, 'x', ?3, ?4, 'WILAYA')",
            params![id, username, role, now],
        )
        .expect("insert test user");
}

/// Test assertion helpers
#[allow(dead_code)]
pub fn assert_success<T>(result: Result<T, String>) -> T {
    result.expect("Expected success but got error")
}

#[allow(dead_code)]
pub fn assert_error<T: std::fmt::Debug>(result: Result<T, String>, expected_msg: &str) {
    let err = result.expect_err("Expected error but got success");
    assert!(
        err.contains(expected_msg),
        "Error '{}' should contain '{}'",
        err,
        expected_msg
    );
}

/// Insert a test product and its inventory_stocks row.
/// Returns the product_id.
#[allow(dead_code)]
pub fn create_test_product(
    state: &grpc_lib::commands::AppState,
    name: &str,
    base_price: f64,
    fiscal_year: i32,
) -> String {
    use rusqlite::params;
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let product_id = format!("test-prod-{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    db.get_connection()
        .execute(
            "INSERT INTO products (id, name, base_price, tva, supplier_name, year, created_at, updated_at)              VALUES (?1, ?2, ?3, 0.0, NULL, ?4, ?5, ?5)",
            params![product_id, name, base_price, fiscal_year, now],
        )
        .expect("insert product");
    db.get_connection()
        .execute(
            "INSERT INTO inventory_stocks (id, product_id, quantity, unit, last_updated, updated_at)              VALUES (?1, ?2, 0.0, 'unit', ?3, ?3)",
            params![format!("stock-{}", product_id), product_id, now],
        )
        .expect("insert inventory_stocks");
    product_id
}

/// Set the quantity in inventory_stocks for a test product.
#[allow(dead_code)]
pub fn set_test_stock(state: &grpc_lib::commands::AppState, product_id: &str, quantity: f64) {
    use rusqlite::params;
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    db.get_connection()
        .execute(
            "UPDATE inventory_stocks SET quantity = ?1, last_updated = ?2 WHERE product_id = ?3",
            params![quantity, now, product_id],
        )
        .expect("update inventory_stocks quantity");

    let unit_cost: f64 = db
        .get_connection()
        .query_row(
            "SELECT base_price FROM products WHERE id = ?1",
            params![product_id],
            |row| row.get(0),
        )
        .unwrap_or(0.0);

    // Create a dummy unit if not exists
    db.get_connection()
        .execute(
            "INSERT OR IGNORE INTO units (id, code, name, wilaya_code, created_at) VALUES ('test-unit', 'TU', 'Test Unit', '00', ?1)",
            params![now],
        )
        .expect("insert dummy unit");

    if quantity > 0.0 {
        db.get_connection()
            .execute(
                "INSERT INTO fifo_stock_layers (id, unit_id, product_id, source_type, source_id, unit_cost, qty_original, qty_remaining, received_at, created_by, origin_fiscal_year)
                 VALUES (?1, 'test-unit', ?2, 'ORDER', 'test-source', ?3, ?4, ?4, ?5, 'system', ?6)",
                params![uuid::Uuid::new_v4().to_string(), product_id, unit_cost, quantity, now, 2025],
            )
            .expect("insert fifo stock layer");
    }
}

/// Seed a fiscal year row as 'open' and sync settings.current_year.
#[allow(dead_code)]
pub fn seed_fiscal_year_open(state: &grpc_lib::commands::AppState, year: i32) {
    use rusqlite::params;
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    db.get_connection()
        .execute(
            "INSERT OR REPLACE INTO fiscal_year_status (year, status, opened_at) VALUES (?1, 'open', ?2)",
            params![year, now],
        )
        .expect("seed fiscal_year_status open");
    db.get_connection()
        .execute(
            "UPDATE settings SET current_year = ?1 WHERE id = 1",
            params![year],
        )
        .expect("sync settings.current_year");
}

/// Clear all fiscal year status rows.
#[allow(dead_code)]
pub fn clear_fiscal_status(state: &grpc_lib::commands::AppState) {
    let db = state.db.lock().unwrap();
    let db = db.as_ref().unwrap();
    db.get_connection()
        .execute("DELETE FROM fiscal_year_status", [])
        .expect("clear fiscal_year_status");
}

/// Seed an ACTIVE WILAYA identity whose Ed25519 signing key is derived from
/// `secret` (SEC-008 test pattern — the fiscal closure signer).
///
/// Returns the identity_id so callers can reference the issuer in packages.
#[allow(dead_code)]
pub fn seed_wilaya_identity(db: &grpc_lib::db::Database, secret: [u8; 32]) -> uuid::Uuid {
    use grpc_lib::domain::identity::{
        CredentialStatus, IdentityCertificate, IdentitySigner, IdentityStorePort, SubjectType,
        SIGNATURE_VERSION_ED25519,
    };
    use grpc_lib::infrastructure::security::Ed25519SigningProvider;
    use grpc_lib::repositories::RepositoryProvider;
    let identity_id = uuid::Uuid::new_v4();
    let certificate = IdentityCertificate {
        identity_id,
        subject_type: SubjectType::Wilaya,
        subject_id: identity_id,
        issuer_identity_id: None,
        credential_id: uuid::Uuid::new_v4(),
        generation: 1,
        status: CredentialStatus::Active,
        public_key: Ed25519SigningProvider::new(secret).public_key(),
        algorithm_version: SIGNATURE_VERSION_ED25519,
        not_after: None,
        package_sequence: Some(1),
        signature: None,
    };
    db.executor()
        .identity_store()
        .upsert(&certificate, &chrono::Utc::now().to_rfc3339())
        .expect("seed wilaya identity");
    identity_id
}
