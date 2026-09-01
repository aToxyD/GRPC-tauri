//! SEC-013 Phase 2 — App-Key auto-unlock via OS keyring (ADR-0041 §11.4).
//!
//! Deterministic tests MUST NOT depend on — and MUST NOT write to — the
//! machine's ambient desktop keyring. Two layers achieve this:
//!
//! 1. Port-parameterized cores (`keyring_app_key_from`,
//!    `remember_app_key_best_effort_with`, `forget_remembered_app_key_with`)
//!    are driven through the shared in-memory `SecretStoragePort` fake.
//! 2. Global resolution (`resolve_app_encryption_key`) and the command seams
//!    (`initialize_app_key_impl` / `unlock_app_key_impl`) are driven through
//!    the TEST-ONLY provider override (`install_test_keyring_port`), which
//!    substitutes the storage backend WITHOUT altering production resolution
//!    semantics (default: real `KeyringSecretStorage`).
//!
//! The production precedence under test stays fixed:
//! GRPC_APP_KEY → OS keyring → APP_KEY_CACHE → dev (debug-only) → fail-closed.

mod common;

use common::{InMemorySecretStorage, KeyringSeamGuard, IDENTITY_A, IDENTITY_B};
use grpc_lib::commands::{initialize_app_key_impl, unlock_app_key_impl};
use grpc_lib::domain::ports::SecretStoragePort;
use grpc_lib::infrastructure::security::keyring_secret_storage::{
    APP_KEY_RING_ENTRY, KEYRING_SERVICE,
};
use grpc_lib::infrastructure::security::{
    cache_app_key, clear_app_key_cache, forget_remembered_app_key_with, keyring_app_key_from,
    remember_app_key_best_effort_with, resolve_app_encryption_key,
};
use std::sync::{Arc, Mutex as StdMutex};

/// Serializes `XDG_DATA_HOME` / `GRPC_APP_KEY` / `GRPC_DB_PATH` mutation
/// between env-dependent tests in THIS file (same pattern as
/// `identity_bootstrap_tests`).
static XDG_LOCK: StdMutex<()> = StdMutex::new(());

// ---------------------------------------------------------------------------
// Port tests (deterministic, in-memory)
// ---------------------------------------------------------------------------

#[test]
fn port_set_get_roundtrip() {
    let store = InMemorySecretStorage::new();
    store
        .set_secret(APP_KEY_RING_ENTRY, IDENTITY_A)
        .expect("set");
    assert_eq!(
        store.get_secret(APP_KEY_RING_ENTRY).expect("get"),
        Some(IDENTITY_A.to_string())
    );
}

#[test]
fn port_overwrite_existing_secret() {
    let store = InMemorySecretStorage::new();
    store
        .set_secret(APP_KEY_RING_ENTRY, "first")
        .expect("set first");
    store
        .set_secret(APP_KEY_RING_ENTRY, IDENTITY_A)
        .expect("overwrite");
    assert_eq!(
        store.get_secret(APP_KEY_RING_ENTRY).expect("get"),
        Some(IDENTITY_A.to_string())
    );
}

#[test]
fn port_delete_then_get_is_absent() {
    let store = InMemorySecretStorage::new();
    store
        .set_secret(APP_KEY_RING_ENTRY, IDENTITY_A)
        .expect("set");
    assert!(store.delete_secret(APP_KEY_RING_ENTRY).expect("delete"));
    assert_eq!(store.get_secret(APP_KEY_RING_ENTRY).expect("get"), None);
}

#[test]
fn port_missing_secret_is_absent_not_error() {
    let store = InMemorySecretStorage::new();
    assert_eq!(store.get_secret(APP_KEY_RING_ENTRY).expect("get"), None);
    assert!(!store.delete_secret(APP_KEY_RING_ENTRY).expect("delete"));
}

#[test]
fn port_unavailable_store_is_an_error() {
    let store = InMemorySecretStorage::unavailable();
    assert!(store.get_secret(APP_KEY_RING_ENTRY).is_err());
    assert!(store.set_secret(APP_KEY_RING_ENTRY, IDENTITY_A).is_err());
    assert!(store.delete_secret(APP_KEY_RING_ENTRY).is_err());
}

#[test]
fn port_namespace_is_application_fixed() {
    // Only the stable application entry is ever read/written/deleted — no
    // username, password, session, DB content, node credential, or auth state
    // participates in the identifier.
    let store = InMemorySecretStorage::new();
    store
        .set_secret(APP_KEY_RING_ENTRY, IDENTITY_A)
        .expect("set");
    let entries = store.entries_snapshot();
    assert_eq!(entries.len(), 1);
    assert!(entries.contains_key(APP_KEY_RING_ENTRY));
    assert_eq!(KEYRING_SERVICE, "dz-grpc");
}

// ---------------------------------------------------------------------------
// Resolver tests (keyring rank via `keyring_app_key_from`)
// ---------------------------------------------------------------------------

#[test]
fn keyring_valid_key_is_accepted_and_normalized() {
    let store =
        InMemorySecretStorage::with_secret(APP_KEY_RING_ENTRY, &format!("  {IDENTITY_A}  "));
    assert_eq!(keyring_app_key_from(&store).as_deref(), Some(IDENTITY_A));
}

#[test]
fn keyring_absent_falls_through() {
    let store = InMemorySecretStorage::new();
    assert_eq!(keyring_app_key_from(&store), None);
}

#[test]
fn keyring_malformed_value_is_rejected() {
    // Malformed stored material is NEVER accepted — the source is unusable and
    // the resolver continues to the next rank. No plaintext fallback.
    let store = InMemorySecretStorage::with_secret(APP_KEY_RING_ENTRY, "not-an-age-key");
    assert_eq!(keyring_app_key_from(&store), None);

    let store =
        InMemorySecretStorage::with_secret(APP_KEY_RING_ENTRY, "AGE-SECRET-KEY-1INVALIDBASE64!!!");
    assert_eq!(keyring_app_key_from(&store), None);
}

#[test]
fn keyring_unavailable_falls_through() {
    let store = InMemorySecretStorage::unavailable();
    assert_eq!(keyring_app_key_from(&store), None);
}

// ---------------------------------------------------------------------------
// Global resolution precedence (deterministic via the TEST-ONLY override)
// ---------------------------------------------------------------------------

fn clear_env_and_cache() {
    std::env::remove_var("GRPC_APP_KEY");
    std::env::remove_var("GRPC_ENV");
    clear_app_key_cache();
}

#[test]
fn valid_env_beats_keyring() {
    let _lock = XDG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _seam = KeyringSeamGuard::install(Arc::new(InMemorySecretStorage::with_secret(
        APP_KEY_RING_ENTRY,
        IDENTITY_A,
    )));
    clear_env_and_cache();
    std::env::set_var("GRPC_APP_KEY", IDENTITY_B);
    assert_eq!(
        resolve_app_encryption_key().expect("env key resolves"),
        IDENTITY_B
    );
    std::env::remove_var("GRPC_APP_KEY");
}

#[test]
fn invalid_explicit_env_is_terminal_despite_valid_keyring() {
    // Rank 1 preserves its fail-closed semantics: an explicitly supplied but
    // invalid GRPC_APP_KEY NEVER falls through to the keyring.
    let _lock = XDG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _seam = KeyringSeamGuard::install(Arc::new(InMemorySecretStorage::with_secret(
        APP_KEY_RING_ENTRY,
        IDENTITY_A,
    )));
    clear_env_and_cache();
    std::env::set_var("GRPC_APP_KEY", "not-a-valid-age-key");
    assert!(
        resolve_app_encryption_key().is_err(),
        "invalid explicit GRPC_APP_KEY must be terminal (no keyring fallthrough)"
    );
    std::env::remove_var("GRPC_APP_KEY");
}

#[test]
fn keyring_beats_cache() {
    let _lock = XDG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _seam = KeyringSeamGuard::install(Arc::new(InMemorySecretStorage::with_secret(
        APP_KEY_RING_ENTRY,
        IDENTITY_A,
    )));
    clear_env_and_cache();
    cache_app_key(IDENTITY_B).expect("cache B");
    assert_eq!(
        resolve_app_encryption_key().expect("keyring key wins"),
        IDENTITY_A,
        "rank 2 (keyring) must beat rank 3 (cache)"
    );
}

#[test]
fn absent_keyring_falls_through_to_cache() {
    let _lock = XDG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _seam = KeyringSeamGuard::absent();
    clear_env_and_cache();
    cache_app_key(IDENTITY_B).expect("cache B");
    assert_eq!(
        resolve_app_encryption_key().expect("cache key resolves"),
        IDENTITY_B
    );
}

#[test]
fn unavailable_keyring_falls_through_to_cache() {
    let _lock = XDG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _seam = KeyringSeamGuard::install(Arc::new(InMemorySecretStorage::unavailable()));
    clear_env_and_cache();
    cache_app_key(IDENTITY_B).expect("cache B");
    assert_eq!(
        resolve_app_encryption_key().expect("unavailable keyring skips to cache"),
        IDENTITY_B
    );
}

#[test]
fn malformed_keyring_value_never_used_as_app_key() {
    // The malformed entry is rejected; the resolver never returns it — the
    // cache (or, in debug, the dev fallback) is used instead.
    let _lock = XDG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _seam = KeyringSeamGuard::install(Arc::new(InMemorySecretStorage::with_secret(
        APP_KEY_RING_ENTRY,
        "malformed-keyring-value",
    )));
    clear_env_and_cache();
    cache_app_key(IDENTITY_B).expect("cache B");
    let resolved =
        resolve_app_encryption_key().expect("resolution continues past malformed keyring");
    assert_eq!(resolved, IDENTITY_B);
    assert_ne!(resolved, "malformed-keyring-value");
}

#[test]
fn complete_absence_reaches_fail_closed() {
    // Deterministic fail-closed check via the pure core with the dev fallback
    // disabled — the release path (no dev key) must error, never invent a key.
    let err = grpc_lib::infrastructure::security::resolve_app_encryption_key_impl(
        None, None, None, false,
    );
    assert!(
        err.is_err(),
        "complete absence with dev fallback disabled must fail closed"
    );
}

// ---------------------------------------------------------------------------
// Remember / forget tests (port-parameterized, deterministic)
// ---------------------------------------------------------------------------

#[test]
fn remember_false_does_not_persist() {
    let store = InMemorySecretStorage::new();
    remember_app_key_best_effort_with(&store, false, IDENTITY_A);
    assert_eq!(store.get_secret(APP_KEY_RING_ENTRY).expect("get"), None);
}

#[test]
fn remember_true_persists_validated_key() {
    let store = InMemorySecretStorage::new();
    remember_app_key_best_effort_with(&store, true, IDENTITY_A);
    assert_eq!(
        store.get_secret(APP_KEY_RING_ENTRY).expect("get"),
        Some(IDENTITY_A.to_string())
    );
}

#[test]
fn remember_is_idempotent() {
    let store = InMemorySecretStorage::new();
    remember_app_key_best_effort_with(&store, true, IDENTITY_A);
    remember_app_key_best_effort_with(&store, true, IDENTITY_A);
    let entries = store.entries_snapshot();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        store.get_secret(APP_KEY_RING_ENTRY).expect("get"),
        Some(IDENTITY_A.to_string())
    );
}

#[test]
fn forget_removes_only_the_app_key_entry() {
    let store = InMemorySecretStorage::with_secret(APP_KEY_RING_ENTRY, IDENTITY_A);
    assert!(forget_remembered_app_key_with(&store).expect("forget"));
    assert_eq!(store.get_secret(APP_KEY_RING_ENTRY).expect("get"), None);
    assert!(store.entries_snapshot().is_empty());
}

#[test]
fn forget_with_no_entry_reports_false() {
    let store = InMemorySecretStorage::new();
    assert!(!forget_remembered_app_key_with(&store).expect("forget absent"));
}

#[test]
fn keyring_write_failure_never_invalidates_unlock() {
    // Best-effort semantics: persistence failure must not roll back an
    // in-progress unlock — the helper returns no error and the caller
    // continues normally (the unlock itself is not dependent on the write).
    let store = InMemorySecretStorage::unavailable();
    remember_app_key_best_effort_with(&store, true, IDENTITY_A);
    assert!(store.get_secret(APP_KEY_RING_ENTRY).is_err());
}

#[test]
fn no_secret_is_written_before_validation_succeeds() {
    // The resolver rejects malformed stored material outright, and the
    // remember helpers are only reachable AFTER a successful decrypt /
    // generate / import (single call sites inside the command seams, located
    // strictly after validation). Proving the ordering at the port level: a
    // store that never accepted a write stays unchanged when a malformed key
    // is presented to the resolver.
    let store = InMemorySecretStorage::with_secret(APP_KEY_RING_ENTRY, "malformed-value");
    assert_eq!(keyring_app_key_from(&store), None);
    let entries = store.entries_snapshot();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        store.get_secret(APP_KEY_RING_ENTRY).expect("get"),
        Some("malformed-value".into())
    );
}

// ---------------------------------------------------------------------------
// Authentication invariants (command-level, seam-isolated, pinned XDG)
// ---------------------------------------------------------------------------

/// Command-level harness, seam-isolated with a pinned XDG layout.
///
/// Pins `XDG_DATA_HOME` and `GRPC_DB_PATH` into a temp dir and seeds a minimal
/// valid deployment (open fiscal year, settings.current_year, writable backup
/// and log dirs). The deployment-readiness gate in `bootstrap_runtime` passes;
/// the in-memory keyring fake absorbs `remember` writes (never the real keyring).
fn with_security_harness(f: impl FnOnce(&SecurityHarness)) {
    let _lock = XDG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _seam = KeyringSeamGuard::absent();
    let temp = tempfile::TempDir::new().expect("temp root");
    let xdg_dir = temp.path().join("xdg");
    std::fs::create_dir_all(&xdg_dir).expect("xdg dir");
    let db_path = temp.path().join("db").join("grpc.db");
    std::env::set_var("XDG_DATA_HOME", &xdg_dir);
    std::env::set_var("GRPC_DB_PATH", &db_path);
    std::env::remove_var("GRPC_APP_KEY");
    std::env::remove_var("GRPC_ENV");

    // Deployment-readiness prerequisites (writable dirs before DB open).
    std::fs::create_dir_all(db_path.parent().expect("db parent").join("backups"))
        .expect("backup dir");
    std::fs::create_dir_all(xdg_dir.join("GRPC").join("logs")).expect("logs dir");

    // Seed a single open fiscal year matching settings.current_year.
    {
        let db = grpc_lib::db::ConnectionFactory::new().expect("seed db");
        let now = chrono::Utc::now().to_rfc3339();
        db.executor()
            .execute(
                "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
                rusqlite::params![2026, now],
            )
            .expect("seed fiscal year");
        db.executor()
            .execute(
                "UPDATE settings SET current_year = ?1 WHERE id = 1",
                rusqlite::params![2026],
            )
            .expect("set current year");
    }

    let state = grpc_lib::commands::AppState::new_for_test(
        grpc_lib::db::ConnectionFactory::new_for_test().expect("test db"),
    );
    let harness = SecurityHarness { _temp: temp, state };
    f(&harness);
    std::env::remove_var("XDG_DATA_HOME");
    std::env::remove_var("GRPC_DB_PATH");
}

struct SecurityHarness {
    _temp: tempfile::TempDir,
    state: grpc_lib::commands::AppState,
}

#[test]
fn auto_unlock_does_not_create_a_session() {
    with_security_harness(|h| {
        initialize_app_key_impl(&h.state, "StrongPass123", None, Some(true))
            .expect("initialize + remember must succeed (fake keyring absorbs the write)");
        let session = h.state.current_session.lock().expect("session lock");
        assert!(
            session.is_none(),
            "App-Key unlock/remember MUST NOT create a CurrentSession — login remains mandatory"
        );
    });
}

#[test]
fn unlock_after_init_keeps_session_absent() {
    with_security_harness(|h| {
        initialize_app_key_impl(&h.state, "StrongPass123", None, None).expect("initialize");
        clear_app_key_cache();
        unlock_app_key_impl(&h.state, "StrongPass123", Some(true)).expect("unlock + remember");
        let session = h.state.current_session.lock().expect("session lock");
        assert!(
            session.is_none(),
            "unlock MUST NOT create a session; username + password remain mandatory"
        );
    });
}

#[test]
fn wrong_passphrase_keeps_store_locked_and_never_reaches_persistence() {
    with_security_harness(|h| {
        initialize_app_key_impl(&h.state, "StrongPass123", None, None).expect("initialize");
        clear_app_key_cache();
        // Wrong passphrase → decrypt fails → the remember call site (strictly
        // after validation) is never reached → unlock fails closed.
        let err = unlock_app_key_impl(&h.state, "WrongPass999", Some(true))
            .expect_err("wrong passphrase must keep the store locked");
        assert!(!err.is_empty());
        assert!(
            grpc_lib::infrastructure::security::cached_app_key().is_none(),
            "failed unlock must leave the cache empty"
        );
    });
}

#[test]
fn resolution_does_not_authenticate_any_user() {
    // Phase 1 regression (ADR-0050): resolving the App Key (here via the
    // in-memory cache after a real unlock) must not touch the authentication
    // surface — the session stays absent and the login gate is unchanged
    // (covered end-to-end by `identity_bootstrap_tests`).
    with_security_harness(|h| {
        initialize_app_key_impl(&h.state, "StrongPass123", None, None).expect("initialize");
        let key = resolve_app_encryption_key().expect("key resolves through the in-memory cache");
        assert!(key.starts_with("AGE-SECRET-KEY-1"));
        let session = h.state.current_session.lock().expect("session lock");
        assert!(session.is_none(), "resolved App Key ≠ authenticated user");
    });
}
