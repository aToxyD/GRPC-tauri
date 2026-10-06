//! User Repository Module
//!
//! Handles all user-related database operations including:
//! - User creation and authentication
//! - Password hashing and verification (Argon2)
//! - User listing and management
//!
//! ARCHITECTURAL NOTE:
//! - All SQL is executed via DbExecutor (supports both Connection and Transaction)
//! - No direct Database access — only DbExecutor

use crate::errors::AppError;
use crate::models::{User, UserRole};
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

/// Repository for user-related database operations
pub struct UserRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> UserRepository<'a> {
    /// Create a new UserRepository with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Get user by username within a node scope (ADR-0052).
    ///
    /// Usernames are unique per `(username, node_id)`, never globally: every
    /// UNIT operator is canonically named `user`, so a bare lookup would be
    /// ambiguous on a WILAYA database holding one shadow operator per unit.
    /// The scope is the local node identity (`"WILAYA"` on WILAYA nodes, the
    /// local unit code on UNIT nodes). Only active (`deleted = 0`) rows are
    /// returned — disabled accounts (`deleted = 1`) are rejected at the source
    /// (B8 account status).
    pub fn get_user_by_username(
        &self,
        username: &str,
        node_scope: &str,
    ) -> Result<Option<User>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT id, username, password_hash, role, created_at, COALESCE(node_id, 'WILAYA'), deleted, must_change_password FROM users WHERE username = ?1 AND COALESCE(node_id, 'WILAYA') = ?2 AND deleted = 0",
                [username, node_scope],
                |row| {
                    let created_at_str: String = row.get(4)?;
                    let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?;
                    Ok(User {
                        id: row.get(0)?,
                        username: row.get(1)?,
                        password_hash: row.get(2)?,
                        role: UserRole::from(row.get::<_, String>(3)?),
                        created_at,
                        node_id: row.get(5)?,
                        deleted: row.get(6)?,
                        must_change_password: row.get(7)?,
                    })
                },
            )
            ?;
        Ok(result)
    }

    /// Get user by username within a node scope, including disabled
    /// (`deleted = 1`) rows (ADR-0052 scoping).
    /// Used by the account-status and identity-access export paths where
    /// re-enabling and lockout propagation require visibility of soft-deleted
    /// accounts. Never used by the login path.
    pub fn get_user_by_username_raw(
        &self,
        username: &str,
        node_scope: &str,
    ) -> Result<Option<User>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT id, username, password_hash, role, created_at, COALESCE(node_id, 'WILAYA'), deleted, must_change_password FROM users WHERE username = ?1 AND COALESCE(node_id, 'WILAYA') = ?2",
                [username, node_scope],
                |row| {
                    let created_at_str: String = row.get(4)?;
                    let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?;
                    Ok(User {
                        id: row.get(0)?,
                        username: row.get(1)?,
                        password_hash: row.get(2)?,
                        role: UserRole::from(row.get::<_, String>(3)?),
                        created_at,
                        node_id: row.get(5)?,
                        deleted: row.get(6)?,
                        must_change_password: row.get(7)?,
                    })
                },
            )
            ?;
        Ok(result)
    }

    /// Get a user by node binding, including disabled rows.
    /// B8 apply path: resolves the local unit-bound user to canonicalize its
    /// name before upserting the synchronized accounts.
    pub fn get_user_by_node_id(&self, node_id: &str) -> Result<Option<User>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT id, username, password_hash, role, created_at, COALESCE(node_id, 'WILAYA'), deleted, must_change_password FROM users WHERE node_id = ?1 LIMIT 1",
                [node_id],
                |row| {
                    let created_at_str: String = row.get(4)?;
                    let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?;
                    Ok(User {
                        id: row.get(0)?,
                        username: row.get(1)?,
                        password_hash: row.get(2)?,
                        role: UserRole::from(row.get::<_, String>(3)?),
                        created_at,
                        node_id: row.get(5)?,
                        deleted: row.get(6)?,
                        must_change_password: row.get(7)?,
                    })
                },
            )
            ?;
        Ok(result)
    }

    /// Get user by ID
    pub fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT id, username, password_hash, role, created_at, COALESCE(node_id, 'WILAYA'), deleted, must_change_password FROM users WHERE id = ?1",
                [user_id],
                |row| {
                    let created_at_str: String = row.get(4)?;
                    let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?;
                    Ok(User {
                        id: row.get(0)?,
                        username: row.get(1)?,
                        password_hash: row.get(2)?,
                        role: UserRole::from(row.get::<_, String>(3)?),
                        created_at,
                        node_id: row.get(5)?,
                        deleted: row.get(6)?,
                        must_change_password: row.get(7)?,
                    })
                },
            )?;
        Ok(result)
    }

    /// Create a new user
    ///
    /// The password_hash must be already hashed with node binding.
    ///
    /// `must_change_password` (ADR-0063) persists the forced credential state
    /// on the created row. It is written only on insert: an ON CONFLICT update
    /// never rewrites an existing account's forced state, because clearing it
    /// belongs to the self-change / reset paths, not to account provisioning.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_user(
        &self,
        id: &str,
        username: &str,
        password_hash: &str,
        role: UserRole,
        node_id: &str,
        must_change_password: bool,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO users (id, username, password_hash, role, created_at, node_id, updated_at, must_change_password) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(username, node_id) DO UPDATE SET
                 password_hash = excluded.password_hash,
                 role = excluded.role,
                 updated_at = excluded.updated_at",
            params![id, username, password_hash, &role.to_string(), now, node_id, now, must_change_password as i64],
        )?;

        Ok(())
    }

    /// Insert a user only when the `(username, node_id)` pair does not already
    /// exist (ADR-0052 node-scoped uniqueness).
    ///
    /// Returns `true` when a new row was inserted, `false` when the scoped
    /// account already existed. Never overwrites existing credentials or
    /// credential material. Used by the startup admin-seeding path so that
    /// repeated launches are idempotent with respect to credentials.
    pub fn insert_user_if_absent(
        &self,
        id: &str,
        username: &str,
        password_hash: &str,
        role: UserRole,
        node_id: &str,
        now: &str,
    ) -> Result<bool, AppError> {
        let affected = self.executor.execute(
            "INSERT INTO users (id, username, password_hash, role, created_at, node_id, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(username, node_id) DO NOTHING",
            params![id, username, password_hash, &role.to_string(), now, node_id, now],
        )?;
        Ok(affected > 0)
    }

    /// Insert raw user data from imported package (pre-hashed password).
    /// `node_id` is mandatory — accounts are always node-scoped (ADR-0052).
    pub fn insert_raw_user(
        &self,
        id: &str,
        username: &str,
        password_hash: &str,
        role: &str,
        node_id: &str,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO users (id, username, password_hash, role, created_at, node_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, username, password_hash, role, now, node_id],
        )?;
        Ok(())
    }

    /// Insert or update raw user data from an imported package (pre-hashed
    /// password), keyed by the node-scoped account identity `(username,
    /// node_id)` — explicit deterministic upsert, never REPLACE semantics.
    pub fn upsert_raw_user(
        &self,
        id: &str,
        username: &str,
        password_hash: &str,
        role: &str,
        node_id: &str,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO users (id, username, password_hash, role, created_at, node_id, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(username, node_id) DO UPDATE SET
                 password_hash = excluded.password_hash,
                 role = excluded.role,
                 updated_at = excluded.updated_at",
            // ADR-0052: row identity is immutable — `id` is never rewritten on
            // conflict so child references (units.user_id) stay valid.
            rusqlite::params![id, username, password_hash, role, now, node_id, now],
        )?;
        Ok(())
    }

    /// Change user password
    pub fn change_password(
        &self,
        user_id: &str,
        password_hash: &str,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE id = ?3",
            params![password_hash, now, user_id],
        )?;

        Ok(())
    }

    /// ADR-0063 §6.6 — change a password **and** clear the forced credential
    /// state as one fail-closed unit.
    ///
    /// Two statements executed on the caller's [`DbExecutor`], which is a
    /// transaction executor when the command wraps this call in
    /// `AuditTxService::execute_with_audit`: a failure of either statement
    /// surfaces as an `Err` and therefore rolls back both. Every statement's
    /// affected-row count is verified — a zero-row write means the target row
    /// does not exist (or vanished mid-transaction), so the operation fails
    /// closed instead of reporting success.
    ///
    /// Deliberately separate from [`Self::change_password`]: the administrative
    /// reset paths must keep their existing semantics and must not clear the
    /// forced flag (ADR-0063 §7.2 is out of scope for this operation).
    pub fn change_password_and_clear_forced_state(
        &self,
        user_id: &str,
        password_hash: &str,
        now: &str,
    ) -> Result<(), AppError> {
        let updated = self.executor.execute(
            "UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE id = ?3",
            params![password_hash, now, user_id],
        )?;
        if updated == 0 {
            return Err(AppError::Internal(
                "password change target does not exist".to_string(),
            ));
        }

        let cleared = self.executor.execute(
            "UPDATE users SET must_change_password = 0 WHERE id = ?1",
            params![user_id],
        )?;
        if cleared == 0 {
            return Err(AppError::Internal(
                "forced credential state could not be cleared".to_string(),
            ));
        }

        Ok(())
    }

    /// Enable (`deleted = 0`) or disable (`deleted = 1`) an account.
    /// Disabling is a soft-delete: the row is preserved and re-enableable.
    pub fn set_deleted(&self, user_id: &str, deleted: bool, now: &str) -> Result<(), AppError> {
        self.executor.execute(
            "UPDATE users SET deleted = ?1, updated_at = ?2 WHERE id = ?3",
            params![deleted as i64, now, user_id],
        )?;

        Ok(())
    }

    /// Upsert the canonical synchronized `user` account (B8 — Identity & Access
    /// Synchronization). Username is fixed to `user`, role to `User`; the row
    /// is keyed by `(username, node_id)` (ADR-0052), so the collision scope is
    /// the local unit identity.
    pub fn upsert_synced_user(
        &self,
        id: &str,
        password_hash: &str,
        node_id: &str,
        deleted: bool,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO users (id, username, password_hash, role, created_at, node_id, deleted, updated_at) VALUES (?1, 'user', ?2, 'User', ?3, ?4, ?5, ?6)
             ON CONFLICT(username, node_id) DO UPDATE SET
                 password_hash = excluded.password_hash,
                 role = excluded.role,
                 deleted = excluded.deleted,
                 updated_at = excluded.updated_at",
            params![id, password_hash, now, node_id, deleted as i64, now],
        )?;

        Ok(())
    }

    /// Upsert the canonical synchronized `admin` account (B8 — Identity &
    /// Access Synchronization). Username is fixed to `admin`, role to `Admin`;
    /// the row is keyed by `(username, node_id)` (ADR-0052). A single
    /// fleet-wide hash (admin derivation domain) authenticates this account on
    /// every node. Disabling is expressed through the `deleted` column
    /// (soft-delete, never a hard row removal).
    pub fn upsert_synced_admin(
        &self,
        id: &str,
        password_hash: &str,
        node_id: &str,
        deleted: bool,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO users (id, username, password_hash, role, created_at, node_id, deleted, updated_at) VALUES (?1, 'admin', ?2, 'Admin', ?3, ?4, ?5, ?6)
             ON CONFLICT(username, node_id) DO UPDATE SET
                 password_hash = excluded.password_hash,
                 role = excluded.role,
                 deleted = excluded.deleted,
                 updated_at = excluded.updated_at",
            params![id, password_hash, now, node_id, deleted as i64, now],
        )?;

        Ok(())
    }

    /// List all users ordered by creation date (newest first)
    pub fn list_users(&self) -> Result<Vec<User>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, username, password_hash, role, created_at, COALESCE(node_id, 'WILAYA'), deleted, must_change_password FROM users ORDER BY created_at DESC",
            [],
            |row| {
                let created_at_str: String = row.get(4)?;
                let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    password_hash: row.get(2)?,
                    role: UserRole::from(row.get::<_, String>(3)?),
                    created_at,
                    node_id: row.get(5)?,
                    deleted: row.get(6)?,
                    must_change_password: row.get(7)?,
                })
            },
        )?)
    }

    /// Delete a user by ID
    pub fn delete_user(&self, user_id: &str) -> Result<(), AppError> {
        self.executor
            .execute("DELETE FROM users WHERE id = ?1", [user_id])?;
        Ok(())
    }

    /// Count active users (SQL-only, no logic)
    pub fn count_active_users(&self) -> Result<i64, AppError> {
        let count = self.executor.query_row(
            "SELECT COUNT(*) FROM users WHERE deleted = 0 AND role != 'System'",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Count active Admin accounts (B8 bootstrap predicate: a fresh UNIT has
    /// no canonical Admin until the first identity_access import).
    pub fn count_active_admins(&self) -> Result<i64, AppError> {
        let count = self.executor.query_row(
            "SELECT COUNT(*) FROM users WHERE deleted = 0 AND role = 'Admin'",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// ADR-0063 §8.1 — existence of the canonical local UNIT `admin` row.
    ///
    /// The durable initialization latch for the `admin_access` first-import
    /// exemption: `username = 'admin'` bound to the local UNIT `node_id`,
    /// evaluated WITHOUT any `deleted` or status filter. An admin disable is
    /// soft (`deleted = 1`) and must not re-open the bootstrap exemption
    /// (D5, F15). SQL-only, no logic.
    pub fn admin_exists_for_node(&self, node_code: &str) -> Result<bool, AppError> {
        let exists = self.executor.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE username = 'admin' AND node_id = ?1)",
            params![node_code],
            |row| row.get::<_, i64>(0).map(|v| v == 1),
        )?;
        Ok(exists)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ConnectionFactory;
    use crate::db::Database;

    fn make_executor(db: &Database) -> DbExecutor<'_> {
        db.executor()
    }

    #[test]
    fn test_get_user_by_username() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = UserRepository::new(make_executor(&db));

        // Use insert_raw_user to create a test user
        repo.insert_raw_user(
            "test-id",
            "testuser",
            "hash",
            "Admin",
            "WILAYA",
            "2024-01-01T00:00:00Z",
        )
        .unwrap();

        let result = repo.get_user_by_username("testuser", "WILAYA").unwrap();
        assert!(result.is_some());

        let user = result.unwrap();
        assert_eq!(user.username, "testuser");
        assert_eq!(user.role, UserRole::Admin);

        // ADR-0052: a foreign scope must not observe the row.
        assert!(repo
            .get_user_by_username("testuser", "UNIT-X")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_list_users() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = UserRepository::new(make_executor(&db));

        repo.insert_raw_user(
            "test-1",
            "user1",
            "hash",
            "Admin",
            "WILAYA",
            "2024-01-01T00:00:00Z",
        )
        .unwrap();
        repo.insert_raw_user(
            "test-2",
            "user2",
            "hash",
            "User",
            "WILAYA",
            "2024-01-01T00:00:01Z",
        )
        .unwrap();

        let users = repo.list_users().unwrap();
        assert!(users.len() >= 2);
    }

    #[test]
    fn test_disabled_account_is_rejected_at_login_source() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = UserRepository::new(make_executor(&db));

        repo.insert_raw_user(
            "test-1",
            "user1",
            "hash",
            "User",
            "WILAYA",
            "2024-01-01T00:00:00Z",
        )
        .unwrap();
        assert!(repo
            .get_user_by_username("user1", "WILAYA")
            .unwrap()
            .is_some());

        repo.set_deleted("test-1", true, "2024-01-02T00:00:00Z")
            .unwrap();
        assert!(
            repo.get_user_by_username("user1", "WILAYA")
                .unwrap()
                .is_none(),
            "disabled account must not be returned by the login lookup"
        );

        repo.set_deleted("test-1", false, "2024-01-03T00:00:00Z")
            .unwrap();
        assert!(repo
            .get_user_by_username("user1", "WILAYA")
            .unwrap()
            .is_some());
    }

    /// ADR-0052 core invariant: multiple UNIT operators canonically named
    /// `user` coexist in one database because uniqueness is node-scoped.
    #[test]
    fn test_canonical_unit_operators_coexist_across_nodes() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = UserRepository::new(make_executor(&db));

        for (id, scope, ts) in [
            ("op-a", "UNIT-A", "2024-01-01T00:00:00Z"),
            ("op-b", "UNIT-B", "2024-01-01T00:00:01Z"),
        ] {
            repo.upsert_synced_user(id, "hash", scope, false, ts)
                .unwrap();
        }

        let a = repo
            .get_user_by_username("user", "UNIT-A")
            .unwrap()
            .expect("UNIT-A operator present");
        let b = repo
            .get_user_by_username("user", "UNIT-B")
            .unwrap()
            .expect("UNIT-B operator present");
        assert_eq!((a.id.as_str(), b.id.as_str()), ("op-a", "op-b"));
        // The WILAYA scope observes neither shadow operator.
        assert!(repo
            .get_user_by_username("user", "WILAYA")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_upsert_synced_user_is_canonical() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = UserRepository::new(make_executor(&db));

        repo.upsert_synced_user("test-1", "hash-v1", "UNIT-1", false, "2024-01-01T00:00:00Z")
            .unwrap();
        let user = repo
            .get_user_by_username("user", "UNIT-1")
            .unwrap()
            .expect("canonical user present");
        assert_eq!(user.username, "user");
        assert_eq!(user.role, UserRole::User);
        assert_eq!(user.node_id, "UNIT-1");
        assert_eq!(user.password_hash, "hash-v1");

        repo.upsert_synced_user("test-1", "hash-v2", "UNIT-1", true, "2024-01-02T00:00:00Z")
            .unwrap();
        assert!(
            repo.get_user_by_username("user", "UNIT-1")
                .unwrap()
                .is_none(),
            "disabled canonical user is rejected"
        );

        repo.upsert_synced_user("test-1", "hash-v2", "UNIT-1", false, "2024-01-03T00:00:00Z")
            .unwrap();
        let user = repo
            .get_user_by_username("user", "UNIT-1")
            .unwrap()
            .expect("re-enabled canonical user present");
        assert_eq!(user.password_hash, "hash-v2");
    }

    #[test]
    fn test_upsert_synced_admin_updates_seeded_row() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = UserRepository::new(make_executor(&db));

        assert!(repo
            .get_user_by_username("admin", "WILAYA")
            .unwrap()
            .is_some());
        repo.upsert_synced_admin(
            "other-id",
            "fleet-hash",
            "UNIT-1",
            false,
            "2024-01-01T00:00:00Z",
        )
        .unwrap();
        let admin = repo
            .get_user_by_username("admin", "UNIT-1")
            .unwrap()
            .expect("canonical admin present");
        assert_eq!(admin.role, UserRole::Admin);
        assert_eq!(admin.password_hash, "fleet-hash");
    }

    /// ADR-0063: `upsert_user` persists the forced credential state on the
    /// inserted row, and every read path surfaces it.
    #[test]
    fn test_upsert_user_persists_forced_change_state() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = UserRepository::new(make_executor(&db));

        repo.upsert_user(
            "forced-1",
            "user",
            "hash-forced",
            UserRole::User,
            "UNIT-77",
            true,
            "2024-01-01T00:00:00Z",
        )
        .unwrap();

        // Every user read path observes the flag.
        assert!(
            repo.get_user_by_username("user", "UNIT-77")
                .unwrap()
                .expect("row present")
                .must_change_password
        );
        assert!(
            repo.get_user_by_username_raw("user", "UNIT-77")
                .unwrap()
                .expect("row present")
                .must_change_password
        );
        assert!(
            repo.get_user_by_id("forced-1")
                .unwrap()
                .expect("row present")
                .must_change_password
        );
        assert!(
            repo.get_user_by_node_id("UNIT-77")
                .unwrap()
                .expect("row present")
                .must_change_password
        );
        assert!(
            repo.list_users()
                .unwrap()
                .iter()
                .find(|u| u.id == "forced-1")
                .expect("row listed")
                .must_change_password
        );
    }

    /// ADR-0063: accounts created outside the canonical UNIT bootstrap path
    /// default to the safe, non-forced state — the schema default and the
    /// explicit `false` are equivalent, so existing flows are unaffected.
    #[test]
    fn test_accounts_created_outside_bootstrap_default_to_not_forced() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = UserRepository::new(make_executor(&db));

        // Schema-default path: a raw insert that never names the column.
        repo.insert_raw_user(
            "raw-1",
            "imported",
            "hash",
            "User",
            "UNIT-88",
            "2024-01-01T00:00:00Z",
        )
        .unwrap();
        assert!(
            !repo
                .get_user_by_username("imported", "UNIT-88")
                .unwrap()
                .expect("row present")
                .must_change_password
        );

        // Explicit path used by the sync/admin provisioning flows.
        repo.upsert_synced_user("synced-1", "hash", "UNIT-89", false, "2024-01-01T00:00:00Z")
            .unwrap();
        assert!(
            !repo
                .get_user_by_username("user", "UNIT-89")
                .unwrap()
                .expect("row present")
                .must_change_password
        );

        repo.upsert_synced_admin("admin-1", "hash", "UNIT-90", false, "2024-01-01T00:00:00Z")
            .unwrap();
        assert!(
            !repo
                .get_user_by_username("admin", "UNIT-90")
                .unwrap()
                .expect("row present")
                .must_change_password
        );

        // The seeded fleet admin is untouched by the new column.
        assert!(
            !repo
                .get_user_by_username("admin", "WILAYA")
                .unwrap()
                .expect("seeded admin present")
                .must_change_password
        );
    }
}
