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
            .hash_node(password, &node_id)
            .map_err(crate::errors::AppError::Internal)?;

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.executor
            .users()
            // Not the canonical UNIT bootstrap path (ADR-0063 §4): this generic
            // account-creation flow always starts outside the forced state.
            .upsert_user(&id, username, &password_hash, role, &node_id, false, &now)?;
        Ok(id)
    }

    pub fn get_user_by_username(
        &self,
        username: &str,
        node_scope: &str,
    ) -> Result<Option<crate::models::User>, AppError> {
        self.executor
            .users()
            .get_user_by_username(username, node_scope)
    }

    pub fn get_user_by_id(&self, id: &str) -> Result<Option<crate::models::User>, AppError> {
        self.executor.users().get_user_by_id(id)
    }
}
