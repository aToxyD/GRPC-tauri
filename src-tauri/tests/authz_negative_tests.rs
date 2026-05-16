//! Authorization negative tests: deny-by-default, real `AppState` / policy paths.

#[allow(dead_code)]
mod common;

use grpc_lib::application::authz::{authorize, Action, Principal, ResourceContext};
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
