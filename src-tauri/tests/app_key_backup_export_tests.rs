//! SEC-017 — secure, path-parameterized App-Key backup re-export.
//!
//! Covers the `export_app_key_backup_to_path` command seam:
//! - success: Admin session + cached key + valid `.age` destination → file
//!   written with bytes identical to the cached raw identity
//! - locked store → error, no backup produced
//! - authenticated User → RequiresAdmin
//! - unauthenticated → SessionNotFound
//! - invalid destination extension → validation error, no file written
//!
//! The raw App-Key never crosses IPC on this path: the seam returns
//! `Result<(), AppError>` only and writes the file backend-side.
//!
//! The App-Key cache is process-global (`APP_KEY_CACHE`); tests within this
//! binary serialize on `CACHE_LOCK` and reset the cache on both entry and
//! exit so parallel execution stays deterministic (same pattern as the env
//! serialization lock in `app_key_keyring_tests`).

mod common;

use common::{create_test_session, insert_test_user};
use grpc_lib::commands::export_app_key_backup_to_path_impl;
use grpc_lib::commands::AppState;
use grpc_lib::db::ConnectionFactory;
use grpc_lib::errors::AppError;
use grpc_lib::infrastructure::security::{cache_app_key, clear_app_key_cache};
use std::sync::Mutex as StdMutex;
use tempfile::TempDir;

const CACHED_KEY: &str = common::IDENTITY_A;

static CACHE_LOCK: StdMutex<()> = StdMutex::new(());

fn state_with_session(role: &str) -> AppState {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    let session = create_test_session("u1", "bob", role);
    *state.current_session.lock().expect("session mutex") = Some(session);
    insert_test_user(&state, "u1", "bob", role);
    state
}

#[test]
fn success_writes_cached_identity_bytes_to_destination() {
    let _serialization = CACHE_LOCK.lock().expect("cache lock");
    clear_app_key_cache();
    cache_app_key(CACHED_KEY).expect("cache app key");

    let state = state_with_session("Admin");
    let dir = TempDir::new().expect("tmpdir");
    let dest = dir.path().join("grpc-app-key.age");
    let dest_path = dest.to_str().expect("utf8 path");

    export_app_key_backup_to_path_impl(&state, dest_path).expect("export succeeds");

    let written = std::fs::read(&dest).expect("backup file exists");
    assert_eq!(written, CACHED_KEY.as_bytes());

    clear_app_key_cache();
}

#[test]
fn locked_store_fails_and_produces_no_backup() {
    let _serialization = CACHE_LOCK.lock().expect("cache lock");
    clear_app_key_cache();

    let state = state_with_session("Admin");
    let dir = TempDir::new().expect("tmpdir");
    let dest = dir.path().join("grpc-app-key.age");

    let err = export_app_key_backup_to_path_impl(&state, dest.to_str().unwrap())
        .expect_err("locked store must fail");
    assert!(
        matches!(err, AppError::Configuration(_)),
        "expected Configuration, got {err:?}"
    );
    assert!(!dest.exists(), "no backup may be produced when locked");

    clear_app_key_cache();
}

#[test]
fn non_admin_session_is_rejected() {
    let _serialization = CACHE_LOCK.lock().expect("cache lock");
    clear_app_key_cache();
    cache_app_key(CACHED_KEY).expect("cache app key");

    let state = state_with_session("User");
    let dir = TempDir::new().expect("tmpdir");
    let dest = dir.path().join("grpc-app-key.age");

    let err = export_app_key_backup_to_path_impl(&state, dest.to_str().unwrap())
        .expect_err("User must be denied");
    match err {
        AppError::Authorization(grpc_lib::errors::AuthorizationError::RequiresAdmin) => {}
        e => panic!("expected RequiresAdmin, got {e:?}"),
    }
    assert!(!dest.exists(), "denied export must not write a file");

    clear_app_key_cache();
}

#[test]
fn unauthenticated_request_is_rejected() {
    let _serialization = CACHE_LOCK.lock().expect("cache lock");
    clear_app_key_cache();
    cache_app_key(CACHED_KEY).expect("cache app key");

    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    let dir = TempDir::new().expect("tmpdir");
    let dest = dir.path().join("grpc-app-key.age");

    let err = export_app_key_backup_to_path_impl(&state, dest.to_str().unwrap())
        .expect_err("no session must be denied");
    match err {
        AppError::Authentication(grpc_lib::errors::AuthenticationError::SessionNotFound) => {}
        e => panic!("expected SessionNotFound, got {e:?}"),
    }
    assert!(!dest.exists(), "denied export must not write a file");

    clear_app_key_cache();
}

#[test]
fn invalid_extension_is_rejected_before_writing() {
    let _serialization = CACHE_LOCK.lock().expect("cache lock");
    clear_app_key_cache();
    cache_app_key(CACHED_KEY).expect("cache app key");

    let state = state_with_session("Admin");
    let dir = TempDir::new().expect("tmpdir");
    let dest = dir.path().join("backup.pem");

    let err = export_app_key_backup_to_path_impl(&state, dest.to_str().unwrap())
        .expect_err("unsupported extension must fail validation");
    assert!(
        matches!(err, AppError::Validation(_)),
        "expected Validation, got {err:?}"
    );
    assert!(!dest.exists(), "rejected destination must not be written");

    clear_app_key_cache();
}
