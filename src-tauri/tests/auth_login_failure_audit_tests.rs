//! Regression: the password-path failed-login audit must survive the
//! `audit_log.user_id -> users(id)` foreign key.
//!
//! The failed-login audit write used the literal actor `"SYSTEM"`, which is not
//! the id of any `users` row. With foreign keys enabled the insert aborted with
//! `SQLITE_CONSTRAINT_FOREIGNKEY` (extended 787) and the failure was never
//! recorded. These tests drive the real password authentication failure path
//! (`commands::auth::login_impl`) so the audit write is exercised end to end.

use grpc_lib::commands::auth::login_impl;
use grpc_lib::commands::AppState;
use grpc_lib::domain::audit::{audit_action_to_event_type, AuditAction};
use grpc_lib::domain::security::PasswordHashPort;
use grpc_lib::infrastructure::security::Argon2PasswordHashProvider;
use grpc_lib::models::LoginRequest;
use rusqlite::params;

/// The seeded system user id (canonical fresh-install migration).
const SEEDED_SYSTEM_USER_ID: &str = "system";

/// A fresh, migration-seeded WILAYA node (foreign keys ON).
fn wilaya_state() -> AppState {
    let db = grpc_lib::db::ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'WILAYA', configured = 1, wilaya_code = '16', wilaya_name = 'TestWilaya' WHERE id = 1",
                [],
            )
            .expect("settings");
    }
    state
}

/// Seed a local WILAYA account carrying a real Argon2 password credential, so
/// the password path reaches password verification instead of being routed to
/// Challenge–Response.
fn insert_wilaya_user(state: &AppState, username: &str, password: &str) {
    let hash = Argon2PasswordHashProvider
        .hash_node(password, "WILAYA")
        .expect("hash");
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    db.get_connection()
        .execute(
            "INSERT INTO users (id, username, password_hash, role, created_at, node_id) \
             VALUES (?1, ?2, ?3, 'User', ?4, 'WILAYA')",
            params![
                format!("u-{}", username),
                username,
                hash,
                chrono::Utc::now().to_rfc3339()
            ],
        )
        .expect("insert user");
}

fn failed_login(state: &AppState, username: &str) -> grpc_lib::models::LoginResponse {
    login_impl(
        state,
        LoginRequest {
            username: username.to_string(),
            password: "wrong-password".to_string(),
        },
    )
    .expect("a failed login must return a response, not a command error")
}

#[test]
fn fresh_database_seeds_the_system_user_without_rewriting_its_id() {
    let state = wilaya_state();
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let id: String = db
        .get_connection()
        .query_row(
            "SELECT id FROM users WHERE username = 'system'",
            [],
            |row| row.get(0),
        )
        .expect("fresh database must contain the seeded system user");
    assert_eq!(
        id, SEEDED_SYSTEM_USER_ID,
        "the seeded system user id must be the FK target the audit actor refers to"
    );
}

#[test]
fn failed_password_login_is_audited_under_the_seeded_system_identity() {
    let state = wilaya_state();
    insert_wilaya_user(&state, "operator", "correct-password");

    // A failed password authentication attempt reaching the failed-login audit
    // path. An audit write failure must not be reported as a command error.
    let response = failed_login(&state, "operator");

    // Authentication semantics are unchanged: still a rejected login.
    assert!(
        !response.success,
        "a wrong password must never authenticate successfully"
    );
    assert!(
        response.user.is_none(),
        "a failed login must not return a user"
    );

    let expected_action = AuditAction::Login.as_str();
    let expected_event_type = audit_action_to_event_type(&AuditAction::Login).as_str();

    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");

    // Exactly one login-failure entry was persisted: a foreign-key violation
    // would have aborted the insert and left the table empty.
    let count: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE username = ?1 AND action = ?2",
            params!["operator", expected_action],
            |row| row.get(0),
        )
        .expect("count audit entries");
    assert_eq!(
        count, 1,
        "expected exactly one persisted failed-login audit entry; a FOREIGN KEY \
         violation aborts the insert and records nothing"
    );

    let (user_id, username, status, event_type): (String, String, String, Option<String>) = db
        .get_connection()
        .query_row(
            "SELECT user_id, username, status, event_type FROM audit_log \
             WHERE username = ?1 AND action = ?2",
            params!["operator", expected_action],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("read audit entry");

    // The audit actor refers to the existing seeded system identity.
    assert_eq!(
        user_id, SEEDED_SYSTEM_USER_ID,
        "failed-login audit must be attributed to the seeded system user id"
    );
    // The attempted username is preserved in the free-text username field.
    assert_eq!(username, "operator");
    assert_eq!(status, "Failed", "the audit entry records a failure");
    assert_eq!(
        event_type.as_deref(),
        Some(expected_event_type),
        "the entry must use the event type derived from the existing action"
    );
}

#[test]
fn failed_login_audit_actor_references_an_existing_user_row() {
    // Guards the premise of the fix: the audit actor must resolve against
    // `users(id)`. A non-existent actor is rejected by the foreign key, which
    // is exactly the failure mode the regression above must never hit.
    let state = wilaya_state();
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let violation = db.get_connection().execute(
        "INSERT INTO audit_log \
             (id, user_id, username, action, entity_type, timestamp, status) \
             VALUES ('audit-fk-probe', 'SYSTEM', 'probe', 'Login', 'User', ?1, 'Failed')",
        params![chrono::Utc::now().to_rfc3339()],
    );
    assert!(
        violation.is_err(),
        "a non-existent audit actor must be rejected by the users(id) foreign key"
    );
}
