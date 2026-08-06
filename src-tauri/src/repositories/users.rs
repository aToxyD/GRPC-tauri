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

    /// Get user by username.
    /// Only active (`deleted = 0`) rows are returned — disabled accounts
    /// (`deleted = 1`) are rejected at the source (B8 account status).
    pub fn get_user_by_username(&self, username: &str) -> Result<Option<User>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT id, username, password_hash, role, created_at, COALESCE(node_id, 'WILAYA'), deleted FROM users WHERE username = ?1 AND deleted = 0",
                [username],
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
                    })
                },
            )
            ?;
        Ok(result)
    }

    /// Get user by username including disabled (`deleted = 1`) rows.
    /// Used by the account-status and identity-access export paths where
    /// re-enabling and lockout propagation require visibility of soft-deleted
    /// accounts. Never used by the login path.
    pub fn get_user_by_username_raw(&self, username: &str) -> Result<Option<User>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT id, username, password_hash, role, created_at, COALESCE(node_id, 'WILAYA'), deleted FROM users WHERE username = ?1",
                [username],
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
                "SELECT id, username, password_hash, role, created_at, COALESCE(node_id, 'WILAYA'), deleted FROM users WHERE node_id = ?1 LIMIT 1",
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
                "SELECT id, username, password_hash, role, created_at, COALESCE(node_id, 'WILAYA'), deleted FROM users WHERE id = ?1",
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
                    })
                },
            )?;
        Ok(result)
    }

    /// Create a new user
    ///
    /// The password_hash must be already hashed with node binding
    pub fn upsert_user(
        &self,
        id: &str,
        username: &str,
        password_hash: &str,
        role: UserRole,
        node_id: &str,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO users (id, username, password_hash, role, created_at, node_id, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(username) DO UPDATE SET
                 password_hash = excluded.password_hash,
                 role = excluded.role,
                 node_id = excluded.node_id,
                 updated_at = excluded.updated_at",
            params![id, username, password_hash, &role.to_string(), now, node_id, now],
        )?;

        Ok(())
    }

    /// Insert a user only when the username does not already exist.
    ///
    /// Returns `true` when a new row was inserted, `false` when the username
    /// already existed. Never overwrites existing credentials or credential
    /// material. Used by the startup admin-seeding path so that repeated
    /// launches are idempotent with respect to credentials.
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
             ON CONFLICT(username) DO NOTHING",
            params![id, username, password_hash, &role.to_string(), now, node_id, now],
        )?;
        Ok(affected > 0)
    }

    /// Insert raw user data from imported package (pre-hashed password)
    pub fn insert_raw_user(
        &self,
        id: &str,
        username: &str,
        password_hash: &str,
        role: &str,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, username, password_hash, role, now],
        )?;
        Ok(())
    }

    /// Insert or replace raw user data from imported package (pre-hashed password)
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
            "INSERT OR REPLACE INTO users (id, username, password_hash, role, created_at, node_id, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
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

    /// Enable (`deleted = 0`) or disable (`deleted = 1`) an account.
    /// Disabling is a soft-delete: the row is preserved and re-enableable.
    pub fn set_deleted(
        &self,
        user_id: &str,
        deleted: bool,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "UPDATE users SET deleted = ?1, updated_at = ?2 WHERE id = ?3",
            params![deleted as i64, now, user_id],
        )?;

        Ok(())
    }

    /// Upsert the canonical synchronized `user` account (B8 — Identity & Access
    /// Synchronization). Username is fixed to `user`, role to `User`.
    ///
    /// The unit-side user row is owned by this canonical contract; a name
    /// collision (legacy username) must be renamed by the caller *before*
    /// invoking this helper so the `username` UNIQUE key is not violated.
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
             ON CONFLICT(username) DO UPDATE SET
                 password_hash = excluded.password_hash,
                 role = excluded.role,
                 node_id = excluded.node_id,
                 deleted = excluded.deleted,
                 updated_at = excluded.updated_at",
            params![id, password_hash, now, node_id, deleted as i64, now],
        )?;

        Ok(())
    }

    /// Upsert the canonical synchronized `admin` account (B8 — Identity &
    /// Access Synchronization). Username is fixed to `admin`, role to `Admin`.
    ///
    /// A single fleet-wide hash (admin derivation domain) authenticates this
    /// account on every node. Disabling is expressed through the `deleted`
    /// column (soft-delete, never a hard row removal).
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
             ON CONFLICT(username) DO UPDATE SET
                 password_hash = excluded.password_hash,
                 role = excluded.role,
                 node_id = excluded.node_id,
                 deleted = excluded.deleted,
                 updated_at = excluded.updated_at",
            params![id, password_hash, now, node_id, deleted as i64, now],
        )?;

        Ok(())
    }

    /// List all users ordered by creation date (newest first)
    pub fn list_users(&self) -> Result<Vec<User>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, username, password_hash, role, created_at, COALESCE(node_id, 'WILAYA'), deleted FROM users ORDER BY created_at DESC",
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

    /// Update a user's username.
    /// Called by UnitService when a unit's credentials change.
    pub fn update_username(
        &self,
        user_id: &str,
        new_username: &str,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "UPDATE users SET username = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![new_username, now, user_id],
        )?;
        Ok(())
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
            "2024-01-01T00:00:00Z",
        )
        .unwrap();

        let result = repo.get_user_by_username("testuser").unwrap();
        assert!(result.is_some());

        let user = result.unwrap();
        assert_eq!(user.username, "testuser");
        assert_eq!(user.role, UserRole::Admin);
    }

    #[test]
    fn test_list_users() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = UserRepository::new(make_executor(&db));

        repo.insert_raw_user("test-1", "user1", "hash", "Admin", "2024-01-01T00:00:00Z")
            .unwrap();
        repo.insert_raw_user("test-2", "user2", "hash", "User", "2024-01-01T00:00:01Z")
            .unwrap();

        let users = repo.list_users().unwrap();
        assert!(users.len() >= 2);
    }

    #[test]
    fn test_disabled_account_is_rejected_at_login_source() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = UserRepository::new(make_executor(&db));

        repo.insert_raw_user("test-1", "user1", "hash", "User", "2024-01-01T00:00:00Z")
            .unwrap();
        assert!(repo.get_user_by_username("user1").unwrap().is_some());

        repo.set_deleted("test-1", true, "2024-01-02T00:00:00Z")
            .unwrap();
        assert!(
            repo.get_user_by_username("user1").unwrap().is_none(),
            "disabled account must not be returned by the login lookup"
        );

        repo.set_deleted("test-1", false, "2024-01-03T00:00:00Z")
            .unwrap();
        assert!(repo.get_user_by_username("user1").unwrap().is_some());
    }

    #[test]
    fn test_upsert_synced_user_is_canonical() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = UserRepository::new(make_executor(&db));

        repo.upsert_synced_user("test-1", "hash-v1", "UNIT-1", false, "2024-01-01T00:00:00Z")
            .unwrap();
        let user = repo
            .get_user_by_username("user")
            .unwrap()
            .expect("canonical user present");
        assert_eq!(user.username, "user");
        assert_eq!(user.role, UserRole::User);
        assert_eq!(user.node_id, "UNIT-1");
        assert_eq!(user.password_hash, "hash-v1");

        repo.upsert_synced_user("test-1", "hash-v2", "UNIT-1", true, "2024-01-02T00:00:00Z")
            .unwrap();
        assert!(
            repo.get_user_by_username("user").unwrap().is_none(),
            "disabled canonical user is rejected"
        );

        repo.upsert_synced_user("test-1", "hash-v2", "UNIT-1", false, "2024-01-03T00:00:00Z")
            .unwrap();
        let user = repo
            .get_user_by_username("user")
            .unwrap()
            .expect("re-enabled canonical user present");
        assert_eq!(user.password_hash, "hash-v2");
    }

    #[test]
    fn test_upsert_synced_admin_updates_seeded_row() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = UserRepository::new(make_executor(&db));

        assert!(repo.get_user_by_username("admin").unwrap().is_some());
        repo.upsert_synced_admin("other-id", "fleet-hash", "UNIT-1", false, "2024-01-01T00:00:00Z")
            .unwrap();
        let admin = repo
            .get_user_by_username("admin")
            .unwrap()
            .expect("canonical admin present");
        assert_eq!(admin.role, UserRole::Admin);
        assert_eq!(admin.password_hash, "fleet-hash");
    }
}
