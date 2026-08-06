use crate::errors::AppError;
use crate::infrastructure::security::{NodeIdentityProvider, SettingsNodeIdentityProvider};
use crate::models::UserRole;
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::Utc;
use uuid::Uuid;

use crate::domain::security::PasswordHashPort;

pub struct UserService<'a> {
    executor: DbExecutor<'a>,
    password_port: &'a dyn PasswordHashPort,
}

impl<'a> UserService<'a> {
    pub fn new(executor: DbExecutor<'a>, password_port: &'a dyn PasswordHashPort) -> Self {
        Self {
            executor,
            password_port,
        }
    }

    pub fn create_user(
        &self,
        username: &str,
        password: &str,
        role: UserRole,
    ) -> Result<String, AppError> {
        crate::domain::validation::validate_change_password(password)?;

        let node_id = SettingsNodeIdentityProvider::new(self.executor).current_node_id()?;

        let password_hash = self
            .password_port
            .hash_password(password, &node_id)
            .map_err(crate::errors::AppError::Internal)?;

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.executor
            .users()
            .upsert_user(&id, username, &password_hash, role, &node_id, &now)?;
        Ok(id)
    }

    pub fn change_password(&self, user_id: &str, new_password: &str) -> Result<(), AppError> {
        crate::domain::validation::validate_change_password(new_password)?;

        let user_repo = self.executor.users();
        let node_id = user_repo.get_user_node_id(user_id)?;

        let password_hash = self
            .password_port
            .hash_password(new_password, &node_id)
            .map_err(crate::errors::AppError::Internal)?;

        let now = chrono::Utc::now().to_rfc3339();
        user_repo.change_password(user_id, &password_hash, &now)
    }

    /// Seed the default administrator on first launch only.
    ///
    /// Bootstrap invariant: startup is idempotent with respect to
    /// administrator credentials. When an `admin` user already exists
    /// (including a soft-deleted one) no credential material is created,
    /// hashed, or overwritten. The atomic `insert_user_if_absent` is the
    /// authoritative guard against concurrent seeding.
    //
    // [arch:allow-bootstrap-admin] see ADR-0038 — temporary legacy path, B5
    // deprecation window only; production seeding is removed in B6-A (RFC
    // 2026-08-04-node-identity-trust).
    pub fn create_default_admin(&self) -> Result<(), AppError> {
        let existing = self.executor.users().get_user_by_username("admin")?;
        if existing.is_some() {
            return Ok(());
        }

        let node_id = SettingsNodeIdentityProvider::new(self.executor)
            .current_node_id()
            .unwrap_or_else(|_| "WILAYA".to_string());
        let password_hash = self
            .password_port
            .hash_password("admin", &node_id) // [arch:allow-bootstrap-admin] see ADR-0038 — B5 legacy window
            .map_err(crate::errors::AppError::Internal)?;

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.executor.users().insert_user_if_absent(
            &id,
            "admin",
            &password_hash,
            UserRole::Admin,
            &node_id,
            &now,
        )?;
        Ok(())
    }

    pub fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<crate::models::User>, AppError> {
        self.executor.users().get_user_by_username(username)
    }

    pub fn get_user_by_id(&self, id: &str) -> Result<Option<crate::models::User>, AppError> {
        self.executor.users().get_user_by_id(id)
    }
}
