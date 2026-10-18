//! ADR-0063 Erratum E-4 — UNIT initialization-order gate (Slice B).
//!
//! Runtime behaviour is primary: these tests drive the real dispatch guard over
//! a real SQLite database using the same fixture style as
//! `forced_credential_state_tests.rs`. The registered-command coverage test is
//! supplemental (it reads the registry source as the single source of truth for
//! "every registered command" rather than duplicating the list).
//!
//! The gate is a dispatch-level restriction only; these tests exercise the guard
//! seam directly (a Tauri `Invoke` cannot be constructed in a unit test) plus a
//! small composition helper that mirrors `get_invoke_handler`'s ordering.

#[allow(dead_code)]
mod common;

use grpc_lib::commands::{
    enforce_forced_credential_state, enforce_unit_initialization_order, AppState,
    FORCED_STATE_ALLOWED_COMMANDS, INITIALIZATION_ALLOWED_COMMANDS,
};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::errors::{into_command_error, AppError, AuthorizationError};
use rusqlite::params;

const UNIT_CODE: &str = "UNIT-01";
const UNIT_NAME: &str = "unit-scope-id";
const OPERATOR_ID: &str = "op-1";
const OPERATOR_USERNAME: &str = "user";
const ADMIN_ID: &str = "admin-1";
const ADMIN_USERNAME: &str = "admin";

// ── fixtures ────────────────────────────────────────────────────────────────

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn base_state() -> AppState {
    let database = ConnectionFactory::new_for_test().expect("db");
    AppState::new_for_test(database)
}

/// Configure a UNIT node settings row; optionally insert the local `units` row
/// (the source of `Settings.unit_code`).
fn configure_unit(state: &AppState, insert_unit_row: bool) {
    let guard = state.get_db().expect("db lock");
    let db = guard.as_ref().expect("database");
    let ts = now();
    db.get_connection()
        .execute(
            "UPDATE settings SET node_type = 'UNIT', configured = 1, unit_name = ?1, wilaya_code = '16', wilaya_name = NULL WHERE id = 1",
            params![UNIT_NAME],
        )
        .expect("unit settings");
    if insert_unit_row {
        db.get_connection()
            .execute(
                "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES ('u-1', ?1, 'Unit One', '16', ?2)",
                params![UNIT_CODE, ts],
            )
            .expect("local unit row");
    }
}

fn configure_wilaya(state: &AppState) {
    let guard = state.get_db().expect("db lock");
    let db = guard.as_ref().expect("database");
    db.get_connection()
        .execute(
            "UPDATE settings SET node_type = 'WILAYA', configured = 1, unit_name = NULL, wilaya_code = '16', wilaya_name = 'Alger' WHERE id = 1",
            [],
        )
        .expect("wilaya settings");
}

fn insert_user(
    state: &AppState,
    id: &str,
    username: &str,
    role: &str,
    node_id: &str,
    forced: bool,
    deleted: bool,
) {
    let guard = state.get_db().expect("db lock");
    let db = guard.as_ref().expect("database");
    let ts = now();
    db.get_connection()
        .execute(
            "INSERT INTO users (id, username, password_hash, role, created_at, node_id, updated_at, deleted, must_change_password)
             VALUES (?1, ?2, 'hash', ?3, ?4, ?5, ?4, ?6, ?7)",
            params![
                id,
                username,
                role,
                ts,
                node_id,
                deleted as i64,
                forced as i64
            ],
        )
        .expect("insert user");
}

fn set_session(state: &AppState, id: &str, username: &str, role: &str) {
    let session = common::create_test_session(id, username, role);
    *state.current_session.lock().expect("session mutex") = Some(session);
}

/// Mirror of `get_invoke_handler` ordering: initialization gate first, then the
/// forced-credential gate.
fn dispatch_gate(state: &AppState, command: &str) -> Result<(), AppError> {
    enforce_unit_initialization_order(state, command)?;
    enforce_forced_credential_state(state, command)
}

fn registry_command_names() -> Vec<String> {
    let path = format!("{}/src/commands/registry.rs", env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(&path).expect("read registry source");
    let re = regex::Regex::new(r"commands::([a-z0-9_]+)\s*,").expect("regex");
    re.captures_iter(&source)
        .map(|c| c[1].to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn assert_ok_init(state: &AppState, command: &str) {
    match enforce_unit_initialization_order(state, command) {
        Ok(()) => {}
        other => panic!("command `{command}` must be admitted by the init gate, got {other:?}"),
    }
}

fn assert_init_incomplete(state: &AppState, command: &str) {
    match enforce_unit_initialization_order(state, command) {
        Err(AppError::Authorization(AuthorizationError::InitializationIncomplete)) => {}
        other => panic!(
            "command `{command}` must be refused with InitializationIncomplete, got {other:?}"
        ),
    }
}

// ── session semantics ───────────────────────────────────────────────────────

#[test]
fn no_session_defers_to_command_guards() {
    // Configured UNIT, no admin, but NO session: pre-authentication ceremonies
    // and normal commands alike are deferred to the command's own guards.
    let state = base_state();
    configure_unit(&state, true);

    for command in [
        "list_products",
        "begin_unit_provision",
        "finalize_unit_provision",
        "get_security_status",
        "get_build_info",
        "is_configured",
        "login",
    ] {
        assert_ok_init(&state, command);
    }
}

#[test]
fn expired_session_defers_authentication_failure() {
    let state = base_state();
    configure_unit(&state, true);
    set_session(&state, OPERATOR_ID, OPERATOR_USERNAME, "User");
    {
        let mut guard = state.current_session.lock().expect("session mutex");
        let session = guard.as_mut().expect("session");
        let long_ago = chrono::Utc::now() - chrono::Duration::hours(48);
        session.created_at = long_ago;
        session.last_activity = long_ago;
    }
    assert_ok_init(&state, "list_products");
}

// ── incomplete initialization ───────────────────────────────────────────────

#[test]
fn incomplete_initialization_refuses_normal_and_change_password_commands() {
    let state = base_state();
    configure_unit(&state, true);
    insert_user(
        &state,
        OPERATOR_ID,
        OPERATOR_USERNAME,
        "User",
        UNIT_CODE,
        true,
        false,
    );
    set_session(&state, OPERATOR_ID, OPERATOR_USERNAME, "User");

    for command in INITIALIZATION_ALLOWED_COMMANDS {
        assert_ok_init(&state, command);
    }

    for command in [
        "change_own_password",
        "list_products",
        "create_unit",
        "reset_unit_user_password",
        "generate_reports",
    ] {
        assert_init_incomplete(&state, command);
    }

    // The refusal is a real backend error with a stable code and a non-empty
    // command-error message (never an empty/success result).
    let err =
        enforce_unit_initialization_order(&state, "change_own_password").expect_err("refused");
    assert_eq!(err.to_user_error().code, "AUTH_INITIALIZATION_INCOMPLETE");
    let message = into_command_error(err);
    assert!(!message.trim().is_empty(), "refusal must not be empty");
}

#[test]
fn incomplete_initialization_blocks_even_without_the_forced_state() {
    // `must_change_password = false` but no local admin: normal commands must
    // still be blocked. The forced gate alone would permit them — this proves
    // the initialization gate is the blocking authority.
    let state = base_state();
    configure_unit(&state, true);
    insert_user(
        &state,
        OPERATOR_ID,
        OPERATOR_USERNAME,
        "User",
        UNIT_CODE,
        false,
        false,
    );
    set_session(&state, OPERATOR_ID, OPERATOR_USERNAME, "User");

    assert!(enforce_forced_credential_state(&state, "list_products").is_ok());
    assert_init_incomplete(&state, "list_products");
    assert_init_incomplete(&state, "change_own_password");
}

// ── completion transitions ──────────────────────────────────────────────────

#[test]
fn initialization_incomplete_takes_precedence_over_the_forced_state() {
    let state = base_state();
    configure_unit(&state, true);
    insert_user(
        &state,
        OPERATOR_ID,
        OPERATOR_USERNAME,
        "User",
        UNIT_CODE,
        true,
        false,
    );
    set_session(&state, OPERATOR_ID, OPERATOR_USERNAME, "User");

    // Ordinary command: incomplete initialization surfaces first.
    match dispatch_gate(&state, "list_products") {
        Err(AppError::Authorization(AuthorizationError::InitializationIncomplete)) => {}
        other => panic!("expected InitializationIncomplete, got {other:?}"),
    }
    // `change_own_password` is on the forced allowlist but must be denied while
    // incomplete — the initialization gate runs first.
    match dispatch_gate(&state, "change_own_password") {
        Err(AppError::Authorization(AuthorizationError::InitializationIncomplete)) => {}
        other => panic!("expected InitializationIncomplete, got {other:?}"),
    }

    // Establish the local admin: the initialization gate becomes a no-op and the
    // forced gate's own refusal takes over.
    insert_user(
        &state,
        ADMIN_ID,
        ADMIN_USERNAME,
        "Admin",
        UNIT_CODE,
        true,
        false,
    );
    match dispatch_gate(&state, "list_products") {
        Err(AppError::Authorization(AuthorizationError::PasswordChangeRequired)) => {}
        other => panic!("expected PasswordChangeRequired, got {other:?}"),
    }
    assert!(
        dispatch_gate(&state, "change_own_password").is_ok(),
        "the forced allowlist exception for change_own_password must survive"
    );
}

#[test]
fn after_admin_and_password_change_both_gates_admit_normal_commands() {
    let state = base_state();
    configure_unit(&state, true);
    insert_user(
        &state,
        OPERATOR_ID,
        OPERATOR_USERNAME,
        "User",
        UNIT_CODE,
        false,
        false,
    );
    insert_user(
        &state,
        ADMIN_ID,
        ADMIN_USERNAME,
        "Admin",
        UNIT_CODE,
        false,
        false,
    );
    set_session(&state, OPERATOR_ID, OPERATOR_USERNAME, "User");

    assert_ok_init(&state, "list_products");
    assert!(enforce_forced_credential_state(&state, "list_products").is_ok());
    assert!(dispatch_gate(&state, "list_products").is_ok());
}

// ── node-type boundaries ────────────────────────────────────────────────────

#[test]
fn wilaya_is_not_subjected_to_a_unit_local_admin_requirement() {
    let state = base_state();
    configure_wilaya(&state);
    set_session(&state, "admin-w", ADMIN_USERNAME, "Admin");

    // No-op on WILAYA: every command is deferred to its own guards.
    assert_ok_init(&state, "list_products");
    assert_ok_init(&state, "change_own_password");
}

#[test]
fn fresh_unconfigured_node_is_not_misclassified_as_a_unit() {
    // Default seed: `node_type = 'UNCONFIGURED'`, `configured = 0`. The settings
    // repository falls back to `NodeType::Unit` on parse failure, so `is_unit()`
    // is true — but the gate must still be a no-op because `configured` is false.
    let state = base_state();
    insert_user(
        &state,
        OPERATOR_ID,
        OPERATOR_USERNAME,
        "User",
        UNIT_CODE,
        true,
        false,
    );
    set_session(&state, OPERATOR_ID, OPERATOR_USERNAME, "User");

    assert_ok_init(&state, "list_products");
    assert_ok_init(&state, "change_own_password");
}

#[test]
fn configured_unit_without_a_resolvable_code_fails_closed() {
    // Configured UNIT but no `units` row → `Settings.unit_code` is `None`.
    let state = base_state();
    configure_unit(&state, false);
    insert_user(
        &state,
        OPERATOR_ID,
        OPERATOR_USERNAME,
        "User",
        UNIT_CODE,
        true,
        false,
    );
    set_session(&state, OPERATOR_ID, OPERATOR_USERNAME, "User");

    assert_ok_init(&state, "get_settings");
    assert_init_incomplete(&state, "list_products");
    assert_init_incomplete(&state, "change_own_password");
}

// ── latch semantics and fail-closed ─────────────────────────────────────────

#[test]
fn soft_disabled_admin_row_preserves_the_deletion_insensitive_latch() {
    let state = base_state();
    configure_unit(&state, true);
    insert_user(
        &state,
        OPERATOR_ID,
        OPERATOR_USERNAME,
        "User",
        UNIT_CODE,
        true,
        false,
    );
    insert_user(
        &state,
        ADMIN_ID,
        ADMIN_USERNAME,
        "Admin",
        UNIT_CODE,
        false,
        true,
    );
    set_session(&state, OPERATOR_ID, OPERATOR_USERNAME, "User");

    // The admin row exists (deleted = 1): initialization is complete, so the
    // init gate is a no-op and the forced gate then applies.
    assert_ok_init(&state, "list_products");
    match enforce_forced_credential_state(&state, "list_products") {
        Err(AppError::Authorization(AuthorizationError::PasswordChangeRequired)) => {}
        other => panic!("expected PasswordChangeRequired, got {other:?}"),
    }
}

#[test]
fn database_unavailable_fails_closed() {
    let state = base_state();
    configure_unit(&state, true);
    insert_user(
        &state,
        OPERATOR_ID,
        OPERATOR_USERNAME,
        "User",
        UNIT_CODE,
        true,
        false,
    );
    set_session(&state, OPERATOR_ID, OPERATOR_USERNAME, "User");

    // Remove the database handle to simulate an unavailable trusted context.
    let _db = state.take_db().expect("take db");

    match enforce_unit_initialization_order(&state, "list_products") {
        Err(_) => {}
        Ok(()) => panic!("must fail closed when the trusted settings read is unavailable"),
    }
    // Allowlisted commands short-circuit before any database access.
    assert_ok_init(&state, "get_settings");
}

// ── allowlist integrity and registered-command coverage ─────────────────────

#[test]
fn initialization_allowlist_is_forced_allowlist_minus_change_own_password() {
    let mut expected: Vec<&str> = FORCED_STATE_ALLOWED_COMMANDS
        .iter()
        .copied()
        .filter(|command| *command != "change_own_password")
        .collect();
    expected.sort_unstable();

    let mut actual: Vec<&str> = INITIALIZATION_ALLOWED_COMMANDS.to_vec();
    actual.sort_unstable();

    assert_eq!(
        actual, expected,
        "the initialization allowlist must be exactly the forced allowlist minus change_own_password"
    );
    assert!(FORCED_STATE_ALLOWED_COMMANDS.contains(&"change_own_password"));
    assert!(!INITIALIZATION_ALLOWED_COMMANDS.contains(&"change_own_password"));
}

#[test]
fn every_registered_command_is_either_admitted_or_refused_while_incomplete() {
    let state = base_state();
    configure_unit(&state, true);
    insert_user(
        &state,
        OPERATOR_ID,
        OPERATOR_USERNAME,
        "User",
        UNIT_CODE,
        true,
        false,
    );
    set_session(&state, OPERATOR_ID, OPERATOR_USERNAME, "User");

    let names = registry_command_names();
    assert!(
        names.len() >= 150,
        "expected the full command registry, parsed {} entries",
        names.len()
    );

    let mut admitted = 0usize;
    for name in &names {
        match enforce_unit_initialization_order(&state, name) {
            Ok(()) => {
                assert!(
                    INITIALIZATION_ALLOWED_COMMANDS.contains(&name.as_str()),
                    "`{name}` was admitted but is not on the closed initialization allowlist"
                );
                admitted += 1;
            }
            Err(AppError::Authorization(AuthorizationError::InitializationIncomplete)) => {
                assert!(
                    !INITIALIZATION_ALLOWED_COMMANDS.contains(&name.as_str()),
                    "`{name}` is on the initialization allowlist but was refused"
                );
            }
            other => panic!("`{name}` produced an unexpected outcome: {other:?}"),
        }
    }

    assert_eq!(admitted, INITIALIZATION_ALLOWED_COMMANDS.len());
    assert!(names.contains(&"import_admin_access_package".to_string()));
    assert!(names.contains(&"change_own_password".to_string()));
}
