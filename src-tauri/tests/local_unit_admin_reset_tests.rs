//! ADR-0063 Slice 4 — §7.1 local UNIT admin reset of the canonical `user`.
//!
//! Runtime behaviour is primary: these tests drive the real authz policy, the
//! real command seam, the real service, the real repository write and the real
//! transactional audit against a real SQLite database — the same fixture style
//! as `forced_credential_state_tests.rs`. The registry coverage test is
//! supplemental (it reads the registry source as the single source of truth).

#[allow(dead_code)]
mod common;

use grpc_lib::application::authz::Action;
use grpc_lib::application::services::{AuditTxService, UserContext};
use grpc_lib::commands::auth::{change_own_password_impl, reset_unit_user_password_impl};
use grpc_lib::commands::{
    authorize_command, enforce_forced_credential_state, AppState, FORCED_STATE_ALLOWED_COMMANDS,
};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::audit::AuditAction;
use grpc_lib::errors::{AppError, AuthorizationError};
use grpc_lib::repositories::UserRepository;
use rusqlite::params;

const UNIT_CODE: &str = "UNIT-01";
const UNIT_NAME: &str = "unit-scope-id";
const ADMIN_ID: &str = "admin-1";
const ADMIN_USERNAME: &str = "admin";
const OPERATOR_ID: &str = "op-1";
const OPERATOR_USERNAME: &str = "user";
const BOOTSTRAP_PASSWORD: &str = "0000";
const POLICY_PASSING: &str = "Abcdef12";

// ── fixtures ────────────────────────────────────────────────────────────────

fn operator_row(state: &AppState) -> (String, bool) {
    let guard = state.get_db().expect("db lock");
    let db = guard.as_ref().expect("database");
    db.get_connection()
        .query_row(
            "SELECT password_hash, must_change_password FROM users WHERE id = ?1",
            params![OPERATOR_ID],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
        )
        .expect("operator row")
}

fn session_id(state: &AppState) -> String {
    state
        .current_session
        .lock()
        .expect("session mutex")
        .as_ref()
        .expect("session")
        .session_id
        .clone()
}

fn insert_user(state: &AppState, id: &str, username: &str, role: &str, node_id: &str) {
    let guard = state.get_db().expect("db lock");
    let db = guard.as_ref().expect("database");
    let now = chrono::Utc::now().to_rfc3339();
    db.get_connection()
        .execute(
            "INSERT INTO users (id, username, password_hash, role, created_at, node_id, updated_at, must_change_password)
             VALUES (?1, ?2, 'x', ?3, ?4, ?5, ?4, 0)",
            params![id, username, role, now, node_id],
        )
        .expect("insert user");
}

/// UNIT node + canonical `admin` row + canonical `user` row (forced) + a live
/// admin session for that admin. `unit_code` is resolved from the `units` row
/// exactly like production `SettingsService::get_settings`.
fn setup(operator_forced: bool) -> AppState {
    let database = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(database);
    {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        let now = chrono::Utc::now().to_rfc3339();
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'UNIT', configured = 1, unit_name = ?1, wilaya_code = '16', wilaya_name = NULL WHERE id = 1",
                params![UNIT_NAME],
            )
            .expect("unit settings");
        db.get_connection()
            .execute(
                "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES ('u-1', ?1, 'Unit One', '16', ?2)",
                params![UNIT_CODE, now],
            )
            .expect("local unit row");
    }
    insert_user(&state, ADMIN_ID, ADMIN_USERNAME, "Admin", UNIT_CODE);
    let operator_hash = state
        .password_port
        .hash_node(BOOTSTRAP_PASSWORD, UNIT_CODE)
        .expect("hash_node");
    {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        let now = chrono::Utc::now().to_rfc3339();
        db.get_connection()
            .execute(
                "INSERT INTO users (id, username, password_hash, role, created_at, node_id, updated_at, must_change_password)
                 VALUES (?1, ?2, ?3, 'User', ?4, ?5, ?4, ?6)",
                params![
                    OPERATOR_ID,
                    OPERATOR_USERNAME,
                    operator_hash,
                    now,
                    UNIT_CODE,
                    operator_forced as i64
                ],
            )
            .expect("insert operator");
    }
    let session = common::create_test_session(ADMIN_ID, ADMIN_USERNAME, "Admin");
    *state.current_session.lock().expect("session mutex") = Some(session);
    state
}

fn admin_session(state: &AppState) {
    let session = common::create_test_session(ADMIN_ID, ADMIN_USERNAME, "Admin");
    *state.current_session.lock().expect("session mutex") = Some(session);
}

fn operator_session(state: &AppState) {
    let session = common::create_test_session(OPERATOR_ID, OPERATOR_USERNAME, "User");
    *state.current_session.lock().expect("session mutex") = Some(session);
}

fn audit_rows(state: &AppState) -> Vec<Vec<String>> {
    let guard = state.get_db().expect("db lock");
    let db = guard.as_ref().expect("database");
    let mut stmt = db
        .get_connection()
        .prepare(
            "SELECT COALESCE(user_id, ''), username, COALESCE(session_id, ''), status,
                    COALESCE(old_value, ''), COALESCE(new_value, ''), COALESCE(details, ''),
                    COALESCE(metadata, ''), COALESCE(entity_name, '')
             FROM audit_log WHERE action = ?1 ORDER BY rowid",
        )
        .expect("prepare audit query");
    let rows: Vec<Vec<String>> = stmt
        .query_map([AuditAction::PasswordChange.as_str()], |row| {
            (0..9)
                .map(|idx| row.get::<_, String>(idx))
                .collect::<rusqlite::Result<Vec<String>>>()
        })
        .expect("audit query")
        .collect::<rusqlite::Result<_>>()
        .expect("audit rows");
    std::mem::drop(stmt);
    std::mem::drop(guard);
    rows
}

// ── §7.1 — happy path ───────────────────────────────────────────────────────

#[test]
fn admin_reset_sets_forced_state_node_bound_and_audits_with_admin_attribution() {
    let state = setup(true);
    let (hash_before, flag_before) = operator_row(&state);
    assert!(flag_before, "fixture operator starts forced");
    let sid_before = session_id(&state);
    let activity_before = state
        .current_session
        .lock()
        .expect("session mutex")
        .as_ref()
        .expect("session")
        .last_activity;

    let attempts_before = {
        let rl = state.rate_limiter.lock().expect("limiter");
        (rl.get_total_attempts(), rl.get_total_failed_attempts())
    };

    reset_unit_user_password_impl(&state, POLICY_PASSING).expect("admin reset must succeed");

    // Persisted state: new node-bound hash, forced flag STILL set (re-set).
    let (hash_after, flag_after) = operator_row(&state);
    assert_ne!(hash_after, hash_before, "operator password must change");
    assert!(flag_after, "the reset must set the forced credential state");
    assert!(state
        .password_port
        .verify_node(POLICY_PASSING, UNIT_CODE, &hash_after)
        .expect("verify new"));
    assert!(!state
        .password_port
        .verify_node(BOOTSTRAP_PASSWORD, UNIT_CODE, &hash_after)
        .expect("verify old"));

    // Admin session preserved: same id, activity advanced (authorized mutation).
    let (sid_after, activity_after) = {
        let guard = state.current_session.lock().expect("session mutex");
        let session = guard.as_ref().expect("session");
        (session.session_id.clone(), session.last_activity)
    };
    assert_eq!(sid_after, sid_before, "admin session must be preserved");
    assert!(activity_after >= activity_before);

    // Login rate-limiter buckets untouched (§7.1 / F29).
    let attempts_after = {
        let rl = state.rate_limiter.lock().expect("limiter");
        (rl.get_total_attempts(), rl.get_total_failed_attempts())
    };
    assert_eq!(attempts_after, attempts_before);

    // Transactional audit with authenticated ADMIN attribution — no credential
    // material anywhere in the row (§7.1 / §2.3).
    let rows = audit_rows(&state);
    assert_eq!(rows.len(), 1, "exactly one PasswordChange audit row");
    assert_eq!(rows[0][0], ADMIN_ID, "audit carries the admin user id");
    assert_eq!(
        rows[0][1], ADMIN_USERNAME,
        "audit carries the admin username"
    );
    assert_eq!(rows[0][2], sid_before, "audit carries the admin session id");
    assert_eq!(rows[0][3], "Success");
    let blob = rows[0][4..].join(" ");
    assert!(!blob.contains(POLICY_PASSING), "no plaintext password");
    assert!(!blob.contains(&hash_after), "no password hash");
}

// ── §7.1 — policy / rejection ───────────────────────────────────────────────

#[test]
fn admin_reset_rejects_literal_zero_zero_zero_zero_and_weak_passwords_without_mutation() {
    let state = setup(true);
    let (hash_before, flag_before) = operator_row(&state);

    for bad in ["0000", "weakpass", "Ab1", "ABCDEF12", "abcdef12"] {
        let err = reset_unit_user_password_impl(&state, bad).expect_err("`{bad}` must be refused");
        assert!(
            err.contains("كلمة المرور") || err.contains("8 أحرف") || err.contains("حرف كبير"),
            "unexpected policy message for `{bad}`: {err}"
        );
        let (hash_after, flag_after) = operator_row(&state);
        assert_eq!(hash_after, hash_before, "`{bad}` must not mutate the hash");
        assert_eq!(flag_after, flag_before, "`{bad}` must not mutate the flag");
    }

    let rows = audit_rows(&state);
    assert_eq!(
        rows.len(),
        0,
        "rejected resets must not be audited as changes"
    );
}

#[test]
fn admin_reset_fails_closed_without_a_configured_local_unit_code() {
    let database = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(database);
    // UNIT node_type with NO `units` row → `Settings.unit_code` is None →
    // `get_unit_id()` fails closed (SEC-029; no name fallback).
    {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'UNIT', configured = 1, unit_name = ?1, wilaya_code = '16', wilaya_name = NULL WHERE id = 1",
                params![UNIT_NAME],
            )
            .expect("unit settings (no units row)");
    }
    insert_user(&state, ADMIN_ID, ADMIN_USERNAME, "Admin", "UNIT-NONE");
    insert_user(&state, OPERATOR_ID, OPERATOR_USERNAME, "User", "UNIT-NONE");
    admin_session(&state);

    let err = reset_unit_user_password_impl(&state, POLICY_PASSING)
        .expect_err("no unit code must fail closed");
    assert!(
        err.contains("operator reset requires a configured local UNIT node"),
        "missing unit code must surface the fail-closed refusal: {err}"
    );
}

// ── §7.1 — node binding ─────────────────────────────────────────────────────

#[test]
fn admin_reset_hash_is_node_bound_to_the_local_unit_code() {
    let state = setup(true);
    reset_unit_user_password_impl(&state, POLICY_PASSING).expect("admin reset");

    let (hash_after, _) = operator_row(&state);
    assert!(state
        .password_port
        .verify_node(POLICY_PASSING, UNIT_CODE, &hash_after)
        .expect("verify local node"));
    assert!(
        !state
            .password_port
            .verify_node(POLICY_PASSING, "OTHER-NODE", &hash_after)
            .expect("verify foreign node"),
        "another node id must not verify the hash"
    );
}

// ── §7.1 — target binding (no caller redirection) ───────────────────────────

#[test]
fn admin_reset_derives_the_target_server_side_and_ignores_foreign_operator_rows() {
    let state = setup(true);
    // A second canonical-username row bound to a DIFFERENT node scope: a
    // caller-driven redirect would target it; the server-side derivation must
    // not. Its presence must never be observable through the service.
    insert_user(
        &state,
        "op-foreign",
        OPERATOR_USERNAME,
        "User",
        "OTHER-NODE",
    );

    reset_unit_user_password_impl(&state, POLICY_PASSING).expect("admin reset");

    let (hash_after, _) = operator_row(&state);
    assert!(state
        .password_port
        .verify_node(POLICY_PASSING, UNIT_CODE, &hash_after)
        .expect("local operator got the new credential"));

    // The foreign row is untouched: its placeholder hash still verifies `x`-free.
    let guard = state.get_db().expect("db lock");
    let db = guard.as_ref().expect("database");
    let foreign_hash: String = db
        .get_connection()
        .query_row(
            "SELECT password_hash FROM users WHERE id = 'op-foreign'",
            [],
            |row| row.get(0),
        )
        .expect("foreign row");
    assert_eq!(foreign_hash, "x", "foreign row must not be touched");
}

#[test]
fn admin_reset_offers_no_unit_scope_redirect_surface() {
    // Even IF a caller could pass a target unit id, `authorize_command` maps it
    // to a `UnitScope` resource on a UNIT node, and the §7.1 policy requires a
    // `UnitNode` — so any unit-scoped call is denied before policy evaluation.
    let state = setup(true);
    let err = authorize_command(&state, Action::ResetLocalUnitUserPassword, Some(UNIT_NAME))
        .expect_err("unit-scoped reset must be refused");
    assert!(matches!(
        err,
        AppError::Authorization(AuthorizationError::RequiresUnitNode)
    ));
}

// ── §7.1 — authorization matrix ─────────────────────────────────────────────

#[test]
fn admin_reset_is_denied_for_every_non_canonical_actor() {
    // UNIT operator (`user`, User role) — owns §6, never §7.1.
    let state = setup(true);
    operator_session(&state);
    let err = authorize_command(&state, Action::ResetLocalUnitUserPassword, None)
        .expect_err("a UNIT operator must be denied the admin reset");
    assert!(matches!(
        err,
        AppError::Authorization(AuthorizationError::InsufficientPermissions)
    ));

    // Canonical admin username with the User role → canonical admin semantics fail.
    let state = setup(true);
    {
        // Demote the DB row too, so session revalidation (SEC-003-08) passes
        // and the denial is decided by the §7.1 policy, not by staleness.
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        db.get_connection()
            .execute(
                "UPDATE users SET role = 'User' WHERE id = ?1",
                params![ADMIN_ID],
            )
            .expect("demote admin");
    }
    {
        let session = common::create_test_session(ADMIN_ID, ADMIN_USERNAME, "User");
        *state.current_session.lock().expect("session mutex") = Some(session);
    }
    let err = authorize_command(&state, Action::ResetLocalUnitUserPassword, None)
        .expect_err("a non-Admin session must be denied");
    assert!(matches!(
        err,
        AppError::Authorization(AuthorizationError::InsufficientPermissions)
    ));

    // UNIT Admin with a NON-canonical username (an `admin` row is the only
    // supported local admin, but the policy must still refuse anything else).
    let state = setup(true);
    insert_user(&state, "other-admin", "other-admin", "Admin", UNIT_CODE);
    {
        let session = common::create_test_session("other-admin", "other-admin", "Admin");
        *state.current_session.lock().expect("session mutex") = Some(session);
    }
    let err = authorize_command(&state, Action::ResetLocalUnitUserPassword, None)
        .expect_err("a non-canonical admin username must be denied");
    assert!(matches!(
        err,
        AppError::Authorization(AuthorizationError::InsufficientPermissions)
    ));

    // WILAYA node: no `UnitNode` resource at all. `seed_default_admin` already
    // provisions the canonical `admin@WILAYA` on every fresh test DB, so reuse
    // that row rather than inserting a duplicate (UNIQUE(username, node_id)).
    let database = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(database);
    let wilaya_admin_id: String = {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        db.get_connection()
            .query_row(
                "SELECT id FROM users WHERE username = ?1 AND node_id = 'WILAYA'",
                params![ADMIN_USERNAME],
                |row| row.get(0),
            )
            .expect("seeded wilaya admin")
    };
    {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'WILAYA', configured = 1, wilaya_code = '16', wilaya_name = 'Alger' WHERE id = 1",
                [],
            )
            .expect("wilaya settings");
    }
    {
        let session = common::create_test_session(&wilaya_admin_id, ADMIN_USERNAME, "Admin");
        *state.current_session.lock().expect("session mutex") = Some(session);
    }
    let err = authorize_command(&state, Action::ResetLocalUnitUserPassword, None)
        .expect_err("a WILAYA admin must be denied the UNIT-local reset");
    assert!(matches!(
        err,
        AppError::Authorization(AuthorizationError::RequiresUnitNode)
    ));
}

#[test]
fn admin_reset_never_touches_a_revoked_operator() {
    let state = setup(true);
    {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        db.get_connection()
            .execute(
                "UPDATE users SET deleted = 1 WHERE id = ?1",
                params![OPERATOR_ID],
            )
            .expect("revoke operator");
    }
    let before: String = {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        db.get_connection()
            .query_row(
                "SELECT password_hash FROM users WHERE id = ?1",
                params![OPERATOR_ID],
                |r| r.get(0),
            )
            .expect("operator hash")
    };

    let err = reset_unit_user_password_impl(&state, POLICY_PASSING)
        .expect_err("a revoked operator has no reset target");
    assert!(
        err.contains("operator reset target does not exist"),
        "unexpected message: {err}"
    );
    let after: String = {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        db.get_connection()
            .query_row(
                "SELECT password_hash FROM users WHERE id = ?1",
                params![OPERATOR_ID],
                |r| r.get(0),
            )
            .expect("operator hash")
    };
    assert_eq!(
        after, before,
        "the revoked row must not be re-enabled or rewritten"
    );
    let rows = audit_rows(&state);
    assert_eq!(rows.len(), 0);
}

// ── §7.1 — registration & forced-state gate ────────────────────────────────

#[test]
fn reset_command_is_registered_and_never_allowlisted_while_forced() {
    // The admin's own session is NOT forced, so the command proceeds auth-wise.
    // The important invariant: the new command is registered...
    let names: Vec<String> = {
        let path = format!("{}/src/commands/registry.rs", env!("CARGO_MANIFEST_DIR"));
        let source = std::fs::read_to_string(&path).expect("read registry source");
        let re = regex::Regex::new(r"commands::([a-z0-9_]+)\s*,").expect("regex");
        re.captures_iter(&source)
            .map(|c| c[1].to_string())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    };
    assert!(names.contains(&"reset_unit_user_password".to_string()));
    // ...and is NOT in the closed §5 allowlist.
    assert!(
        !FORCED_STATE_ALLOWED_COMMANDS.contains(&"reset_unit_user_password"),
        "the admin reset must never bypass the forced-state gate for an operator"
    );

    // A FORCED operator session must keep refusing the reset command: the gate
    // is hit at dispatch before any command logic runs.
    let state = setup(true);
    operator_session(&state);
    let err = enforce_forced_credential_state(&state, "reset_unit_user_password")
        .expect_err("a forced operator must be refused the admin reset");
    assert!(matches!(
        err,
        AppError::Authorization(AuthorizationError::PasswordChangeRequired)
    ));
}

// ── §7.1 — transactional audit rollback ─────────────────────────────────────

#[test]
fn admin_reset_write_and_forced_state_set_roll_back_together() {
    let state = setup(true);
    let (hash_before, flag_before) = operator_row(&state);

    let new_hash = state
        .password_port
        .hash_node(POLICY_PASSING, UNIT_CODE)
        .expect("hash_node");
    let sid = session_id(&state);
    let ctx = UserContext::new(ADMIN_ID, ADMIN_USERNAME, Some(&sid));
    let now = chrono::Utc::now().to_rfc3339();

    let mut guard = state.get_db().expect("db lock");
    let db = guard.as_mut().expect("database");
    let result: Result<(), AppError> =
        AuditTxService::execute_with_audit(db, AuditAction::PasswordChange, &ctx, |tx| {
            UserRepository::new(tx.executor).change_password_and_set_forced_state(
                OPERATOR_ID,
                &new_hash,
                &now,
            )?;
            // Force a failure AFTER the write so rollback is observable.
            Err(AppError::Internal("deliberate rollback".to_string()))
        });
    std::mem::drop(guard);
    assert!(result.is_err(), "the injected failure must surface");

    let (hash_after, flag_after) = operator_row(&state);
    assert_eq!(hash_after, hash_before, "password write must roll back");
    assert_eq!(flag_after, flag_before, "forced-state set must roll back");
    let rows = audit_rows(&state);
    assert_eq!(
        rows.len(),
        0,
        "the audit row must roll back with the transaction"
    );
}

// ── §6 — unchanged interplay ────────────────────────────────────────────────

#[test]
fn reset_output_feeds_the_existing_self_change_path() {
    // After an admin reset, the operator logs in with the temporary credential
    // and completes the §6 self change exactly as before — proving §7.1 hands
    // the lifecycle back to the existing mechanism instead of duplicating it.
    let state = setup(false);
    reset_unit_user_password_impl(&state, POLICY_PASSING).expect("admin reset");
    let (hash_after, flag_after) = operator_row(&state);
    assert!(flag_after, "after reset the operator must be forced");

    operator_session(&state);
    let new_password = "Xyzw1234";
    change_own_password_impl(&state, POLICY_PASSING, new_password)
        .expect("operator self-change with the temporary credential");
    let (hash_final, flag_final) = operator_row(&state);
    assert_ne!(hash_final, hash_after);
    assert!(!flag_final, "§6 self-change clears the forced state");

    // Both mutations audited transactionally, each with its own actor.
    let rows = audit_rows(&state);
    assert_eq!(rows.len(), 2, "one admin reset + one operator self-change");
}
