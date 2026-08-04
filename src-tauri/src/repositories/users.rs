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

    /// Get user by username
    pub fn get_user_by_username(&self, username: &str) -> Result<Option<User>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT id, username, password_hash, role, created_at, COALESCE(node_id, 'WILAYA') FROM users WHERE username = ?1",
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
                "SELECT id, username, password_hash, role, created_at, COALESCE(node_id, 'WILAYA') FROM users WHERE id = ?1",
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

    /// Get user node ID
    pub fn get_user_node_id(&self, user_id: &str) -> Result<String, AppError> {
        let node_id: String = self.executor.query_row(
            "SELECT COALESCE(node_id, 'WILAYA') FROM users WHERE id = ?1",
            [user_id],
            |row| row.get(0),
        )?;
        Ok(node_id)
    }

    /// List all users ordered by creation date (newest first)
    pub fn list_users(&self) -> Result<Vec<User>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, username, password_hash, role, created_at, COALESCE(node_id, 'WILAYA') FROM users ORDER BY created_at DESC",
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
}
