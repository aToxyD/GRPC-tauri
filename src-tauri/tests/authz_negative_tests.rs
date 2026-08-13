//! Authorization negative tests: deny-by-default, real `AppState` / policy paths.

#[allow(dead_code)]
mod common;

use grpc_lib::application::authz::{authorize, Action, Principal, ResourceContext};
use grpc_lib::commands::auth::touch_session_impl;
use grpc_lib::commands::{authorize_command, AppState};
use grpc_lib::errors::{AppError, AuthenticationError, AuthorizationError};
use grpc_lib::models::UserRole;

fn wilaya_configured_state() -> AppState {
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

fn unit_configured_state() -> AppState {
    let db = grpc_lib::db::ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'UNIT', configured = 1, unit_name = 'unit-scope-id', wilaya_code = '16', wilaya_name = NULL WHERE id = 1",
                [],
            )
            .expect("settings");
    }
    state
}

fn set_session(state: &AppState, session: grpc_lib::domain::session::CurrentSession) {
    // SEC-003-08: fabricate the matching user row so session revalidation
    // succeeds for sessions that represent a real (non-deleted) user.
    common::insert_test_user(
        state,
        &session.user_id,
        &session.username,
        &session.user_role.to_string(),
    );
    *state.current_session.lock().expect("session mutex") = Some(session);
}

#[test]
fn auth_a_user_denied_admin_only() {
    let state = wilaya_configured_state();
    let mut s = common::create_test_session("u1", "bob", "User");
    s.user_role = UserRole::User;
    set_session(&state, s);

    let err = authorize_command(&state, Action::AdminOnly, None).expect_err("deny");
    match err {
        AppError::Authorization(AuthorizationError::RequiresAdmin) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_b_missing_session() {
    let state = wilaya_configured_state();
    let err = authorize_command(&state, Action::ReadProducts, None).expect_err("deny");
    match err {
        AppError::Authentication(AuthenticationError::SessionNotFound) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_c_unit_denied_wilaya_only_reports_action() {
    let state = unit_configured_state();
    let s = common::create_test_session("u1", "bob", "Admin");
    set_session(&state, s);

    let err = authorize_command(&state, Action::ReadWilayaReports, None).expect_err("deny");
    match err {
        AppError::Authorization(AuthorizationError::RequiresWilayaNode) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_d_non_admin_cannot_read_other_user_activity() {
    let p = Principal {
        user_id: "u1".into(),
        username: "bob".into(),
        role: UserRole::User,
        session_id: Some("s".into()),
    };
    let err = authorize(
        &p,
        Action::ReadUserActivity,
        &ResourceContext::UserScope {
            user_id: "other".into(),
        },
    )
    .expect_err("deny");
    match err {
        AuthorizationError::RequiresAdmin => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_e_wrong_resource_context_denies() {
    let p = Principal {
        user_id: "u1".into(),
        username: "admin".into(),
        role: UserRole::Admin,
        session_id: Some("s".into()),
    };
    let err = authorize(
        &p,
        Action::ManageDailyReports,
        &ResourceContext::UserScope {
            user_id: "u1".into(),
        },
    )
    .expect_err("deny");
    match err {
        AuthorizationError::InsufficientPermissions => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_f_expired_session_denied() {
    let state = wilaya_configured_state();
    let mut s = common::create_test_session("u1", "admin", "Admin");
    s.last_activity = chrono::Utc::now()
        - chrono::Duration::minutes(grpc_lib::domain::session::SESSION_TIMEOUT_MINUTES + 1);
    set_session(&state, s);

    let err = authorize_command(&state, Action::ReadProducts, None).expect_err("deny");
    match err {
        AppError::Authentication(AuthenticationError::SessionExpired) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_g_unknown_role_string_maps_to_user_then_admin_action_denies() {
    let state = wilaya_configured_state();
    let s = common::create_test_session("u1", "eve", "NotARealRole");
    assert_eq!(s.user_role, UserRole::User);
    set_session(&state, s);

    let err = authorize_command(&state, Action::ViewSystemHealth, None).expect_err("deny");
    match err {
        AppError::Authorization(AuthorizationError::RequiresAdmin) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_wilaya_non_admin_denied_authenticated_gate() {
    let state = wilaya_configured_state();
    let s = common::create_test_session("u1", "user", "User");
    set_session(&state, s);

    let err = authorize_command(&state, Action::ReadProducts, None).expect_err("deny");
    match err {
        AppError::Authorization(AuthorizationError::RequiresAdmin) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_unit_admin_denied_manage_units_wilaya_node_guard() {
    // B8 ① closed the ManageUnits node-type gap: a UNIT admin may no longer
    // manage units through the AdminOnly-only arm (Rule 127 retired).
    let state = unit_configured_state();
    let s = common::create_test_session("u1", "unit-admin", "Admin");
    set_session(&state, s);

    let err = authorize_command(&state, Action::ManageUnits, None).expect_err("deny");
    match err {
        AppError::Authorization(AuthorizationError::InsufficientPermissions) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_wilaya_admin_allowed_manage_units_wilaya_node_guard() {
    let state = wilaya_configured_state();
    let s = common::create_test_session("u1", "wilaya-admin", "Admin");
    set_session(&state, s);

    authorize_command(&state, Action::ManageUnits, None).expect("allow");
}

// ─────────────────────────────────────────────────────────────────────────────
// SEC-004-01: WILAYA-side UNIT bootstrap CSR signing is WILAYA Admin-only.
// Only an authenticated WILAYA Admin may sign a UNIT identity request;
// unauthenticated callers, UNIT nodes, and WILAYA non-admin users are denied.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn auth_sec004_unauthenticated_caller_denied_sign_unit_identity_request() {
    let state = wilaya_configured_state();
    let err = authorize_command(&state, Action::SignUnitIdentityRequest, None).expect_err("deny");
    match err {
        AppError::Authentication(AuthenticationError::SessionNotFound) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_sec004_unit_admin_denied_sign_unit_identity_request() {
    let state = unit_configured_state();
    let s = common::create_test_session("u1", "unit-admin", "Admin");
    set_session(&state, s);

    let err = authorize_command(&state, Action::SignUnitIdentityRequest, None).expect_err("deny");
    match err {
        AppError::Authorization(AuthorizationError::InsufficientPermissions) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_sec004_unit_user_denied_sign_unit_identity_request() {
    let state = unit_configured_state();
    let s = common::create_test_session("u1", "unit-user", "User");
    set_session(&state, s);

    let err = authorize_command(&state, Action::SignUnitIdentityRequest, None).expect_err("deny");
    match err {
        AppError::Authorization(AuthorizationError::InsufficientPermissions) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_sec004_wilaya_user_denied_sign_unit_identity_request() {
    let state = wilaya_configured_state();
    let s = common::create_test_session("u1", "wilaya-user", "User");
    set_session(&state, s);

    let err = authorize_command(&state, Action::SignUnitIdentityRequest, None).expect_err("deny");
    match err {
        AppError::Authorization(AuthorizationError::RequiresAdmin) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_sec004_wilaya_admin_allowed_sign_unit_identity_request() {
    let state = wilaya_configured_state();
    let s = common::create_test_session("u1", "wilaya-admin", "Admin");
    set_session(&state, s);

    authorize_command(&state, Action::SignUnitIdentityRequest, None).expect("WILAYA Admin allowed");
}

// ---- SEC-003-06-a: UNIT node scoped unit_id must be the local unit ----

#[test]
fn auth_sec006a_unit_user_own_unit_id_allowed() {
    let state = unit_configured_state();
    let s = common::create_test_session("u1", "unit-user", "User");
    set_session(&state, s);

    authorize_command(&state, Action::ReadInventory, Some("unit-scope-id")).expect("allow");
}

#[test]
fn auth_sec006a_unit_admin_own_unit_id_allowed() {
    let state = unit_configured_state();
    let s = common::create_test_session("u1", "unit-admin", "Admin");
    set_session(&state, s);

    authorize_command(&state, Action::ReadInventory, Some("unit-scope-id")).expect("allow");
}

#[test]
fn auth_sec006a_unit_user_foreign_unit_id_denied() {
    let state = unit_configured_state();
    let s = common::create_test_session("u1", "unit-user", "User");
    set_session(&state, s);

    let err = authorize_command(&state, Action::ReadInventory, Some("foreign-unit-id")).expect_err("deny");
    match err {
        AppError::Authorization(AuthorizationError::InsufficientPermissions) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_sec006a_unit_admin_foreign_unit_id_denied() {
    let state = unit_configured_state();
    let s = common::create_test_session("u1", "unit-admin", "Admin");
    set_session(&state, s);

    let err = authorize_command(&state, Action::ReadInventory, Some("foreign-unit-id")).expect_err("deny");
    match err {
        AppError::Authorization(AuthorizationError::InsufficientPermissions) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_sec006a_wilaya_admin_arbitrary_unit_id_allowed() {
    let state = wilaya_configured_state();
    let s = common::create_test_session("u1", "wilaya-admin", "Admin");
    set_session(&state, s);

    authorize_command(&state, Action::ReadInventory, Some("any-unit-id")).expect("allow");
}

#[test]
fn auth_sec006a_unit_node_none_target_unchanged() {
    let state = unit_configured_state();
    let s = common::create_test_session("u1", "unit-admin", "Admin");
    set_session(&state, s);

    authorize_command(&state, Action::ReadInventory, None).expect("allow");
}

#[test]
fn auth_sec006a_unit_admin_read_units_guard_mirrors_get_unit_denies_foreign_unit_id() {
    // Mirrors the guard executed by `commands::units::get_unit`
    // (`authorize_command(&state, Action::ReadUnits, Some(&unit_id))`).
    let state = unit_configured_state();
    let s = common::create_test_session("u1", "unit-admin", "Admin");
    set_session(&state, s);

    let err =
        authorize_command(&state, Action::ReadUnits, Some("foreign-unit-id")).expect_err("deny");
    match err {
        AppError::Authorization(AuthorizationError::InsufficientPermissions) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SEC-003-08: session staleness — the session must not survive user deletion,
// disablement, or role change; absolute lifetime; touch requires authentication.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn auth_sec008_valid_session_with_persisted_user_allowed() {
    let state = wilaya_configured_state();
    let s = common::create_test_session("u1", "bob", "Admin");
    set_session(&state, s);

    authorize_command(&state, Action::ReadProducts, None).expect("valid session allowed");
}

#[test]
fn auth_sec008_missing_user_session_denied() {
    let state = wilaya_configured_state();
    let s = common::create_test_session("u1", "ghost", "Admin");
    // Session fabricated WITHOUT a corresponding users row.
    *state.current_session.lock().expect("session mutex") = Some(s);

    let err = authorize_command(&state, Action::ReadProducts, None).expect_err("deny");
    match err {
        AppError::Authentication(AuthenticationError::UserNotFound { .. }) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_sec008_deleted_user_session_denied() {
    let state = wilaya_configured_state();
    let s = common::create_test_session("u1", "bob", "Admin");
    set_session(&state, s);

    {
        let db = state.db.lock().unwrap();
        db.as_ref()
            .unwrap()
            .get_connection()
            .execute("UPDATE users SET deleted = 1 WHERE id = 'u1'", [])
            .expect("disable user");
    }

    let err = authorize_command(&state, Action::ReadProducts, None).expect_err("deny");
    match err {
        AppError::Authentication(AuthenticationError::UserNotFound { .. }) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_sec008_role_demoted_session_denied() {
    let state = wilaya_configured_state();
    let s = common::create_test_session("u1", "bob", "Admin");
    set_session(&state, s);

    {
        let db = state.db.lock().unwrap();
        db.as_ref()
            .unwrap()
            .get_connection()
            .execute("UPDATE users SET role = 'User' WHERE id = 'u1'", [])
            .expect("demote user");
    }

    // A demoted Admin loses Admin-gated access immediately, at the session layer.
    let err = authorize_command(&state, Action::ReadProducts, None).expect_err("deny");
    match err {
        AppError::Authentication(AuthenticationError::UserNotFound { .. }) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_sec008_revalidation_failure_clears_session() {
    let state = wilaya_configured_state();
    let s = common::create_test_session("u1", "ghost", "Admin");
    *state.current_session.lock().expect("session mutex") = Some(s);

    let _ = authorize_command(&state, Action::ReadProducts, None).expect_err("deny");

    let next = authorize_command(&state, Action::ReadProducts, None).expect_err("no session");
    match next {
        AppError::Authentication(AuthenticationError::SessionNotFound) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_sec008_absolute_lifetime_expired_denied() {
    let state = wilaya_configured_state();
    let mut s = common::create_test_session("u1", "bob", "Admin");
    s.created_at = chrono::Utc::now()
        - chrono::Duration::minutes(grpc_lib::domain::session::SESSION_ABSOLUTE_MAX_MINUTES);
    s.last_activity = chrono::Utc::now(); // still active, but absolutely stale
    set_session(&state, s);

    let err = authorize_command(&state, Action::ReadProducts, None).expect_err("deny");
    match err {
        AppError::Authentication(AuthenticationError::SessionExpired) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

// ── touch_session (SEC-003-08 Part B) ────────────────────────────────────────

#[test]
fn auth_sec008_touch_without_session_denied() {
    let state = wilaya_configured_state();
    let err = touch_session_impl(&state).expect_err("deny");
    match err {
        AppError::Authentication(AuthenticationError::SessionNotFound) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_sec008_touch_stale_user_denied() {
    let state = wilaya_configured_state();
    let s = common::create_test_session("u1", "ghost", "Admin");
    *state.current_session.lock().expect("session mutex") = Some(s);

    let err = touch_session_impl(&state).expect_err("deny");
    match err {
        AppError::Authentication(AuthenticationError::UserNotFound { .. }) => {}
        e => panic!("unexpected: {:?}", e),
    }
}

#[test]
fn auth_sec008_touch_valid_session_allowed() {
    let state = wilaya_configured_state();
    let s = common::create_test_session("u1", "bob", "Admin");
    set_session(&state, s);

    touch_session_impl(&state).expect("valid session may be extended");
}
