//! ADR-0063 Slice 2 — §5 forced credential state enforcement and §6 canonical
//! UNIT operator self password change.
//!
//! Runtime behaviour is primary: these tests drive the real guards, services,
//! repositories and transactions against a real SQLite database. The registry
//! coverage test is supplemental (it reads the registry source as the single
//! source of truth for "every registered command" rather than duplicating the
//! list).
//!
//! Slice 5 appends the ADR-0063 §11 counterpart: a `.unit`-imported operator
//! lands in the very same forced dispatch state this file already enforces.

#[allow(dead_code)]
mod common;

use grpc_lib::application::authz::Action;
use grpc_lib::application::services::{
    AuditTxService, NodePackageService, UserContext, UserService,
};
use grpc_lib::commands::auth::{change_own_password_impl, get_current_user_impl};
use grpc_lib::commands::{
    authorize_command, enforce_forced_credential_state, AppState, FORCED_STATE_ALLOWED_COMMANDS,
};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::audit::AuditAction;
use grpc_lib::errors::{
    into_command_error, AppError, AuthenticationError, AuthorizationError, ValidationError,
};
use grpc_lib::models::{Unit, UnitNodePackage, UserExport};
use grpc_lib::repositories::UserRepository;
use rusqlite::params;

const UNIT_CODE: &str = "UNIT-01";
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

fn set_operator_row(state: &AppState, password: &str, forced: bool) {
    let hash = state
        .password_port
        .hash_node(password, UNIT_CODE)
        .expect("hash_node");
    let guard = state.get_db().expect("db lock");
    let db = guard.as_ref().expect("database");
    let now = chrono::Utc::now().to_rfc3339();
    db.get_connection()
        .execute(
            "INSERT INTO users (id, username, password_hash, role, created_at, node_id, updated_at, must_change_password)
             VALUES (?1, ?2, ?3, 'User', ?4, ?5, ?4, ?6)
             ON CONFLICT(id) DO UPDATE SET
                password_hash = excluded.password_hash,
                updated_at = excluded.updated_at,
                must_change_password = excluded.must_change_password",
            params![OPERATOR_ID, OPERATOR_USERNAME, hash, now, UNIT_CODE, forced as i64],
        )
        .expect("upsert operator");
}

/// UNIT node + canonical operator row + a live session for that operator.
fn setup(forced: bool) -> AppState {
    let database = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(database);
    {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'UNIT', configured = 1, unit_name = 'unit-scope-id', wilaya_code = '16', wilaya_name = NULL WHERE id = 1",
                [],
            )
            .expect("unit settings");
    }
    set_operator_row(&state, BOOTSTRAP_PASSWORD, forced);
    let session = common::create_test_session(OPERATOR_ID, OPERATOR_USERNAME, "User");
    *state.current_session.lock().expect("session mutex") = Some(session);
    state
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

fn assert_refused(state: &AppState, command: &str) {
    match enforce_forced_credential_state(state, command) {
        Err(AppError::Authorization(AuthorizationError::PasswordChangeRequired)) => {}
        other => {
            panic!("command `{command}` must be refused with PasswordChangeRequired, got {other:?}")
        }
    }
}

fn assert_allowed(state: &AppState, command: &str) {
    match enforce_forced_credential_state(state, command) {
        Ok(()) => {}
        other => panic!("command `{command}` must be allowed, got {other:?}"),
    }
}

// ── §5 — central enforcement at the dispatch choke point ────────────────────

#[test]
fn forced_state_refuses_every_non_allowlisted_command_with_a_backend_error() {
    let state = setup(true);

    // Owner-designated coverage: commands that never reach `authorize_command`.
    for command in [
        "get_build_info",
        "get_login_metrics",
        "generate_reports",
        "calculate_meal_cost",
        "calculate_meal_rate",
        "calculate_product_price_with_tva",
        // …and ordinary post-auth commands across other domains.
        "list_products",
        "create_unit",
        "export_products_package",
        "set_fleet_admin_password",
        "import_products_package",
        "generate_monthly_summary",
    ] {
        assert_refused(&state, command);
    }

    // The refusal is a real backend error, never an empty/success result, and
    // it carries a stable user-facing message and code.
    let err = enforce_forced_credential_state(&state, "list_products").expect_err("refused");
    assert_eq!(err.to_user_error().code, "AUTH_PASSWORD_CHANGE_REQUIRED");
    let message = into_command_error(err);
    assert!(!message.trim().is_empty(), "refusal must not be empty");
}

#[test]
fn forced_state_permits_exactly_the_closed_allowlist() {
    let state = setup(true);
    for command in FORCED_STATE_ALLOWED_COMMANDS {
        assert_allowed(&state, command);
    }
}

#[test]
fn closed_allowlist_is_exactly_the_approved_set() {
    let mut approved = FORCED_STATE_ALLOWED_COMMANDS.to_vec();
    approved.sort_unstable();
    let mut expected = vec![
        "check_session",
        "change_own_password",
        "get_current_user",
        "get_settings",
        "import_admin_access_package",
        "is_configured",
        "login",
        "logout",
        "touch_session",
    ];
    expected.sort_unstable();
    assert_eq!(approved, expected);
}

#[test]
fn forced_state_gate_passes_without_session_and_with_an_expired_session() {
    // No session at all → authentication stays the command's own concern.
    let database = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(database);
    assert_allowed(&state, "list_products");

    // Expired session → `require_authenticated` owns the rejection.
    let state = setup(true);
    {
        let mut guard = state.current_session.lock().expect("session mutex");
        let session = guard.as_mut().expect("session");
        let long_ago = chrono::Utc::now() - chrono::Duration::hours(48);
        session.created_at = long_ago;
        session.last_activity = long_ago;
    }
    assert_allowed(&state, "list_products");
}

#[test]
fn missing_user_row_creates_no_authenticated_success_path() {
    // Session points at a row that no longer exists.
    let database = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(database);
    let session = common::create_test_session("ghost", "ghost", "User");
    *state.current_session.lock().expect("session mutex") = Some(session);

    // The forced-state gate defers (there is no flag to evaluate)…
    assert_allowed(&state, "list_products");

    // …and the command layer still fails closed: no authenticated path opens.
    let err = authorize_command(&state, Action::AuthenticatedOnly, None)
        .expect_err("must not authenticate a vanished user");
    assert!(matches!(
        err,
        AppError::Authentication(AuthenticationError::UserNotFound { .. })
    ));
    assert!(
        state
            .current_session
            .lock()
            .expect("session mutex")
            .is_none(),
        "a stale session must be invalidated"
    );
}

/// Supplemental: the registry source is the single source of truth for "every
/// registered command"; each entry is either explicitly allowed or explicitly
/// refused — never silently accepted.
#[test]
fn every_registered_command_is_either_allowed_or_refused_while_forced() {
    let state = setup(true);
    let names = registry_command_names();
    assert!(
        names.len() >= 150,
        "expected the full command registry, parsed {} entries",
        names.len()
    );

    let mut allowed = 0usize;
    let mut refused = 0usize;
    for name in &names {
        match enforce_forced_credential_state(&state, name) {
            Ok(()) => {
                assert!(
                    FORCED_STATE_ALLOWED_COMMANDS.contains(&name.as_str()),
                    "`{name}` was allowed but is not on the closed allowlist"
                );
                allowed += 1;
            }
            Err(AppError::Authorization(AuthorizationError::PasswordChangeRequired)) => {
                assert!(
                    !FORCED_STATE_ALLOWED_COMMANDS.contains(&name.as_str()),
                    "`{name}` is on the closed allowlist but was refused"
                );
                refused += 1;
            }
            other => panic!("`{name}` produced an unexpected outcome: {other:?}"),
        }
    }

    assert_eq!(allowed, FORCED_STATE_ALLOWED_COMMANDS.len());
    assert!(refused > 0);
    // The newly added §6 command must be registered and allowed.
    assert!(names.contains(&"change_own_password".to_string()));
}

#[test]
fn forced_state_gate_is_invisible_to_a_non_forced_account() {
    let state = setup(false);
    assert_allowed(&state, "list_products");
    let user = get_current_user_impl(&state)
        .expect("projection")
        .expect("user");
    assert!(!user.must_change_password);
}

// ── §6 — canonical local UNIT operator self password change ─────────────────

#[test]
fn self_change_succeeds_and_clears_the_forced_state_atomically() {
    let state = setup(true);
    let sid_before = session_id(&state);
    let (hash_before, flag_before) = operator_row(&state);
    assert!(flag_before, "fixture must start in the forced state");

    let (attempts_before, failed_before, remaining_before) = {
        let rl = state.rate_limiter.lock().expect("limiter");
        (
            rl.get_total_attempts(),
            rl.get_total_failed_attempts(),
            rl.get_remaining_attempts(OPERATOR_USERNAME),
        )
    };
    let activity_before = state
        .current_session
        .lock()
        .expect("session mutex")
        .as_ref()
        .expect("session")
        .last_activity;

    change_own_password_impl(&state, BOOTSTRAP_PASSWORD, POLICY_PASSING)
        .expect("self change must succeed");

    // Persisted state: new node-bound hash, forced flag cleared.
    let (hash_after, flag_after) = operator_row(&state);
    assert_ne!(hash_after, hash_before);
    assert!(!flag_after, "forced state must be cleared");
    assert!(state
        .password_port
        .verify_node(POLICY_PASSING, UNIT_CODE, &hash_after)
        .expect("verify new"));
    assert!(!state
        .password_port
        .verify_node(BOOTSTRAP_PASSWORD, UNIT_CODE, &hash_after)
        .expect("verify old"));
    // Node binding preserved: another node's id must not verify the hash.
    assert!(!state
        .password_port
        .verify_node(POLICY_PASSING, "OTHER-NODE", &hash_after)
        .expect("verify foreign node"));

    // Session preserved, not re-minted; activity advanced (authorized mutation).
    let (sid_after, activity_after) = {
        let guard = state.current_session.lock().expect("session mutex");
        let session = guard.as_ref().expect("session");
        (session.session_id.clone(), session.last_activity)
    };
    assert_eq!(sid_after, sid_before);
    assert!(activity_after >= activity_before);

    // Login rate-limiter buckets untouched (ADR-0063 §6.7).
    {
        let rl = state.rate_limiter.lock().expect("limiter");
        assert_eq!(rl.get_total_attempts(), attempts_before);
        assert_eq!(rl.get_total_failed_attempts(), failed_before);
        assert_eq!(
            rl.get_remaining_attempts(OPERATOR_USERNAME),
            remaining_before
        );
    }

    // Transactional audit with authenticated session attribution — and no
    // credential material anywhere in the row (§6.8, verification item 13).
    let guard = state.get_db().expect("db lock");
    let db = guard.as_ref().expect("database");
    let mut stmt = db
        .get_connection()
        .prepare(
            "SELECT COALESCE(user_id, ''), username, COALESCE(session_id, ''), status,
                    COALESCE(old_value, ''), COALESCE(new_value, ''), COALESCE(details, ''),
                    COALESCE(metadata, ''), COALESCE(entity_name, '')
             FROM audit_log WHERE action = ?1",
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
    // Release the database mutex before any further gate/projection call.
    std::mem::drop(stmt);
    std::mem::drop(guard);

    assert_eq!(rows.len(), 1, "exactly one PasswordChange audit row");
    let row = &rows[0];
    assert_eq!(row[0], OPERATOR_ID);
    assert_eq!(row[1], OPERATOR_USERNAME);
    assert_eq!(
        row[2], sid_before,
        "audit must carry the session attribution"
    );
    assert_eq!(row[3], "Success");
    // Columns 4..9: old/new value, details, metadata, entity name.
    let blob = rows
        .iter()
        .map(|r| r[4..].join(" "))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(!blob.contains(POLICY_PASSING), "no plaintext password");
    assert!(!blob.contains(&hash_after), "no password hash");

    // The gate is now open: the same command that was refused is allowed.
    assert_allowed(&state, "list_products");
    let user = get_current_user_impl(&state)
        .expect("projection")
        .expect("user");
    assert!(!user.must_change_password);
}

#[test]
fn self_change_rejects_a_wrong_current_password_without_leaking_identity() {
    let state = setup(true);
    let (hash_before, flag_before) = operator_row(&state);
    let sid_before = session_id(&state);

    let err = change_own_password_impl(&state, "Totally-Wrong-1", POLICY_PASSING)
        .expect_err("wrong current password must be refused");
    assert!(
        err.contains("كلمة المرور الحالية غير صحيحة"),
        "unexpected message: {err}"
    );
    assert!(!err.contains(OPERATOR_ID), "must not leak the user id");
    assert!(
        !err.contains(POLICY_PASSING),
        "must not leak the proposed password"
    );

    // Nothing changed: hash, forced flag and session all intact.
    let (hash_after, flag_after) = operator_row(&state);
    assert_eq!(hash_after, hash_before);
    assert_eq!(flag_after, flag_before);
    assert!(flag_after);
    assert_eq!(session_id(&state), sid_before);
    assert_refused(&state, "list_products");

    // Exactly one PasswordChange audit row at most — i.e. none for the failure.
    let guard = state.get_db().expect("db lock");
    let db = guard.as_ref().expect("database");
    let count: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action = ?1",
            params![AuditAction::PasswordChange.as_str()],
            |r| r.get(0),
        )
        .expect("audit count");
    assert_eq!(
        count, 0,
        "a rejected change must not be audited as a change"
    );
}

#[test]
fn self_change_rejects_policy_violations_and_reuse() {
    let state = setup(true);

    for bad in ["0000", "weakpass", "Ab1", "ABCDEF12", "abcdef12"] {
        let result = change_own_password_impl(&state, BOOTSTRAP_PASSWORD, bad);
        assert!(result.is_err(), "`{bad}` must be rejected");
        let err = result.expect_err("rejected");
        assert!(
            err.contains("كلمة المرور") || err.contains("8 أحرف") || err.contains("حرف كبير"),
            "unexpected policy message for `{bad}`: {err}"
        );
        let (_, flag) = operator_row(&state);
        assert!(flag, "forced state must survive a rejected change");
    }

    // Reuse: the current credential is policy-passing, so rejection is decided
    // by node-bound verification of the candidate — never a plaintext compare.
    let state = setup(false);
    set_operator_row(&state, POLICY_PASSING, true);
    let (hash_before, _) = operator_row(&state);
    let err = change_own_password_impl(&state, POLICY_PASSING, POLICY_PASSING)
        .expect_err("reuse must be refused");
    assert_eq!(
        into_command_error(AppError::Validation(ValidationError::PasswordReuse {
            reason: String::new()
        })),
        err,
        "reuse must surface the stable VAL_PASSWORD_REUSE refusal"
    );
    let (hash_after, flag_after) = operator_row(&state);
    assert_eq!(hash_after, hash_before);
    assert!(flag_after, "reuse must not clear the forced state");
}

#[test]
fn self_change_is_denied_for_every_non_canonical_actor() {
    // A local admin on the same UNIT node.
    let state = setup(true);
    let admin_id = "admin-local";
    {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        let now = chrono::Utc::now().to_rfc3339();
        db.get_connection()
            .execute(
                "INSERT INTO users (id, username, password_hash, role, created_at, node_id, updated_at, must_change_password)
                 VALUES (?1, 'admin', 'x', 'User', ?2, ?3, ?2, 0)",
                params![admin_id, now, UNIT_CODE],
            )
            .expect("insert local admin");
    }
    let admin_session = common::create_test_session(admin_id, "admin", "User");
    *state.current_session.lock().expect("session mutex") = Some(admin_session);
    let err = authorize_command(&state, Action::ChangeOwnPassword, None)
        .expect_err("only the canonical operator may self-change");
    assert!(matches!(
        err,
        AppError::Authorization(AuthorizationError::InsufficientPermissions)
    ));

    // The canonical username with an Admin role.
    let state = setup(true);
    {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        db.get_connection()
            .execute(
                "UPDATE users SET role = 'Admin' WHERE id = ?1",
                params![OPERATOR_ID],
            )
            .expect("promote operator");
    }
    let promoted = common::create_test_session(OPERATOR_ID, OPERATOR_USERNAME, "Admin");
    *state.current_session.lock().expect("session mutex") = Some(promoted);
    let err = authorize_command(&state, Action::ChangeOwnPassword, None)
        .expect_err("an Admin may not use the operator self-change");
    assert!(matches!(
        err,
        AppError::Authorization(AuthorizationError::InsufficientPermissions)
    ));

    // A WILAYA node has no `UnitNode` resource at all.
    let database = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(database);
    {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'WILAYA', configured = 1, wilaya_code = '16', wilaya_name = 'Alger' WHERE id = 1",
                [],
            )
            .expect("wilaya settings");
        let now = chrono::Utc::now().to_rfc3339();
        db.get_connection()
            .execute(
                "INSERT INTO users (id, username, password_hash, role, created_at, node_id, updated_at, must_change_password)
                 VALUES ('u-w', 'user', 'x', 'User', ?1, 'WILAYA', ?1, 0)",
                params![now],
            )
            .expect("insert user");
    }
    let session = common::create_test_session("u-w", "user", "User");
    *state.current_session.lock().expect("session mutex") = Some(session);
    let err = authorize_command(&state, Action::ChangeOwnPassword, None)
        .expect_err("self-change requires a UNIT node");
    assert!(matches!(
        err,
        AppError::Authorization(AuthorizationError::RequiresUnitNode)
    ));
}

#[test]
fn projection_reports_the_persisted_forced_state_truthfully() {
    let state = setup(true);

    // Login response payload: the row returned by the login lookup carries the
    // persisted flag (the same DTO `login` embeds in `LoginResponse.user`).
    let guard = state.get_db().expect("db lock");
    let db = guard.as_ref().expect("database");
    let row_user = UserService::new(db.executor(), state.password_port.as_ref())
        .get_user_by_username(OPERATOR_USERNAME, UNIT_CODE)
        .expect("login lookup")
        .expect("operator");
    std::mem::drop(guard);
    let value = serde_json::to_value(&row_user).expect("serialize login user");
    assert_eq!(value.get("must_change_password"), Some(&true.into()));
    assert!(
        value.get("password_hash").is_none(),
        "hash never serialized"
    );
    assert!(value.get("node_id").is_none(), "node id never serialized");
    assert!(value.get("deleted").is_none(), "deleted never serialized");

    // Session projection: `get_current_user` re-reads the persisted flag.
    let user = get_current_user_impl(&state)
        .expect("projection")
        .expect("user");
    assert!(user.must_change_password);
    let value = serde_json::to_value(&user).expect("serialize projection");
    assert_eq!(value.get("must_change_password"), Some(&true.into()));
    assert!(value.get("password_hash").is_none());

    // …and it flips to false once the §6 change succeeded.
    change_own_password_impl(&state, BOOTSTRAP_PASSWORD, POLICY_PASSING).expect("self change");
    let user = get_current_user_impl(&state)
        .expect("projection")
        .expect("user");
    assert!(!user.must_change_password);
}

#[test]
fn password_write_and_forced_state_clear_roll_back_together() {
    let state = setup(true);
    let (hash_before, flag_before) = operator_row(&state);
    assert!(flag_before);

    let new_hash = state
        .password_port
        .hash_node(POLICY_PASSING, UNIT_CODE)
        .expect("hash_node");
    let sid = session_id(&state);
    let ctx = UserContext::new(OPERATOR_ID, OPERATOR_USERNAME, Some(&sid));
    let now = chrono::Utc::now().to_rfc3339();

    let mut guard = state.get_db().expect("db lock");
    let db = guard.as_mut().expect("database");
    let result: Result<(), AppError> =
        AuditTxService::execute_with_audit(db, AuditAction::PasswordChange, &ctx, |tx| {
            UserRepository::new(tx.executor).change_password_and_clear_forced_state(
                OPERATOR_ID,
                &new_hash,
                &now,
            )?;
            // Force a failure AFTER both statements so rollback is observable.
            Err(AppError::Internal("deliberate rollback".to_string()))
        });
    std::mem::drop(guard);
    assert!(result.is_err(), "the injected failure must surface");

    let (hash_after, flag_after) = operator_row(&state);
    assert_eq!(hash_after, hash_before, "password write must roll back");
    assert!(flag_after, "forced-state clear must roll back");

    let guard = state.get_db().expect("db lock");
    let db = guard.as_ref().expect("database");
    let count: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action = ?1",
            params![AuditAction::PasswordChange.as_str()],
            |r| r.get(0),
        )
        .expect("audit count");
    assert_eq!(
        count, 0,
        "the audit row must roll back with the transaction"
    );
}

// ── Slice 5 — ADR-0063 §11: a `.unit`-imported operator ────────────────────

/// The `.unit` payload shape that provisions the canonical local operator.
fn unit_node_package(password_hash: &str) -> UnitNodePackage {
    UnitNodePackage {
        unit: Unit {
            id: uuid::Uuid::new_v4().to_string(),
            code: UNIT_CODE.into(),
            name: "unit-scope-id".into(),
            wilaya_code: "16".into(),
            user_id: None,
            created_at: chrono::Utc::now(),
        },
        user: UserExport {
            username: OPERATOR_USERNAME.into(),
            password_hash: password_hash.into(),
            role: "User".into(),
        },
        unit_certificate: None,
        unit_private_key: None,
    }
}

/// §11: the `.unit` import itself puts the operator into the forced dispatch
/// state this file enforces — and the existing §6 path clears it, unchanged.
#[test]
fn unit_import_lands_the_operator_in_the_forced_state_gate() {
    let database = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(database);

    // The exported credential exactly as WILAYA mints it: the node-bound hash
    // of the published bootstrap value. The import stores it verbatim under
    // `node_id = UNIT_CODE` (no re-hash, no second hashing mechanism).
    let exported_hash = state
        .password_port
        .hash_node(BOOTSTRAP_PASSWORD, UNIT_CODE)
        .expect("exported node-bound hash");
    {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        NodePackageService::new(db.executor())
            .import_unit_node_package(&unit_node_package(&exported_hash))
            .expect("unit import");
    }

    // Importing `.unit` configures the node as a UNIT node (existing
    // `update_unit_node_settings` behavior — unchanged by this slice).
    let (operator_id, forced) = {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        let node_type: String = db
            .get_connection()
            .query_row("SELECT node_type FROM settings WHERE id = 1", [], |r| {
                r.get(0)
            })
            .expect("node type");
        assert_eq!(node_type, "UNIT", "the .unit import configures the node");
        db.get_connection()
            .query_row(
                "SELECT id, must_change_password FROM users WHERE username = ?1 AND node_id = ?2",
                params![OPERATOR_USERNAME, UNIT_CODE],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
            )
            .expect("imported operator row")
    };
    assert!(
        forced,
        "ADR-0063 §11: a .unit-imported operator must be in the forced state"
    );

    let session = common::create_test_session(&operator_id, OPERATOR_USERNAME, "User");
    *state.current_session.lock().expect("session mutex") = Some(session);

    // The existing dispatch gate consumes the persisted state: a
    // non-allowlisted command refuses with a backend error, and the only way
    // out of the forced state stays reachable.
    assert_refused(&state, "list_products");
    assert_allowed(&state, "change_own_password");

    // The existing §6 path completes from this imported state — the slice did
    // not touch it: rotation succeeds, the forced flag clears, the gate opens.
    change_own_password_impl(&state, BOOTSTRAP_PASSWORD, POLICY_PASSING)
        .expect("the imported operator completes the self change");
    let flag_after = {
        let guard = state.get_db().expect("db lock");
        let db = guard.as_ref().expect("database");
        db.get_connection()
            .query_row(
                "SELECT must_change_password FROM users WHERE id = ?1",
                params![operator_id],
                |row| row.get::<_, bool>(0),
            )
            .expect("forced flag")
    };
    assert!(
        !flag_after,
        "the existing self-change clears the forced state exactly as before"
    );
    assert_allowed(&state, "list_products");
}
