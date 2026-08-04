//! Admin bootstrap idempotency regression tests.
//!
//! Encodes the audit invariant (A-1):
//! **startup must be idempotent with respect to administrator credentials.**
//!
//! - fresh database -> default administrator is created
//! - restart existing database -> password hash is unchanged
//! - restart existing database after a password change -> the modified
//!   password persists and the original default no longer authenticates
//! - repeated seeding across launches is idempotent (single admin row)

use grpc_lib::application::services::UserService;
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::security::PasswordHashPort;
use grpc_lib::infrastructure::security::Argon2PasswordHashProvider;
use grpc_lib::models::{User, UserRole};
use grpc_lib::repositories::UserRepository;

const DEFAULT_ADMIN_PASSWORD: &str = "admin";
const MODIFIED_PASSWORD: &str = "Str0ng-Adm1n-Pass!";

fn seed(path: &std::path::Path) -> Database {
    ConnectionFactory::new_with_path(path).expect("bootstrap database")
}

fn admin_user(db: &Database) -> User {
    let port = Argon2PasswordHashProvider;
    UserService::new(db.executor(), &port)
        .get_user_by_username("admin")
        .expect("query admin")
        .expect("admin user must be seeded")
}

fn admin_count(db: &Database) -> usize {
    UserRepository::new(db.executor())
        .list_users()
        .expect("list users")
        .iter()
        .filter(|u| u.role == UserRole::Admin)
        .count()
}

fn verify(password: &str, user: &User) -> bool {
    Argon2PasswordHashProvider
        .verify_password(password, &user.node_id, &user.password_hash)
        .expect("verify password")
}

#[test]
fn fresh_database_seeds_default_admin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bootstrap.db");

    let db = seed(&path);
    let admin = admin_user(&db);
    assert_eq!(admin.username, "admin");
    assert_eq!(admin.role, UserRole::Admin);
    assert!(verify(DEFAULT_ADMIN_PASSWORD, &admin), "default admin logs in");
}

#[test]
fn restart_preserves_password_hash_and_metadata() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bootstrap.db");

    let (hash_before, updated_before) = {
        let db = seed(&path);
        let admin = admin_user(&db);
        (admin.password_hash, admin.created_at)
    };

    // Simulate application restart: connection is closed, then reopened.
    let db = seed(&path);
    let admin = admin_user(&db);
    assert_eq!(admin.password_hash, hash_before, "hash must survive restart");
    assert_eq!(admin.created_at, updated_before, "seed metadata must survive restart");
    assert!(verify(DEFAULT_ADMIN_PASSWORD, &admin), "default password still valid");
}

#[test]
fn modified_password_persists_across_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bootstrap.db");

    let (id, new_hash) = {
        let db = seed(&path);
        let port = Argon2PasswordHashProvider;
        let svc = UserService::new(db.executor(), &port);
        let admin = admin_user(&db);
        svc.change_password(&admin.id, MODIFIED_PASSWORD).expect("change password");
        let updated = admin_user(&db);
        (updated.id, updated.password_hash)
    };

    let db = seed(&path);
    let admin = admin_user(&db);
    assert_eq!(admin.id, id, "same admin row after restart");
    assert_eq!(
        admin.password_hash,
        new_hash,
        "modified hash must persist across restart"
    );
    assert!(
        verify(MODIFIED_PASSWORD, &admin),
        "modified password must authenticate after restart"
    );
    assert!(
        !verify(DEFAULT_ADMIN_PASSWORD, &admin),
        "default password must NOT authenticate after a change"
    );
}

#[test]
fn repeated_seeding_is_idempotent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bootstrap.db");

    let db = seed(&path);
    let port = Argon2PasswordHashProvider;
    let svc = UserService::new(db.executor(), &port);

    let hash_before = admin_user(&db).password_hash;

    svc.create_default_admin().expect("re-seed 1");
    svc.create_default_admin().expect("re-seed 2");
    let db2 = seed(&path);
    svc.create_default_admin().expect("re-seed after restart");

    let admin = admin_user(&db2);
    assert_eq!(
        admin.password_hash,
        hash_before,
        "repeated seeding must never overwrite the admin hash"
    );
    assert_eq!(
        admin_count(&db2),
        1,
        "repeated seeding must not create duplicate admins"
    );
}
