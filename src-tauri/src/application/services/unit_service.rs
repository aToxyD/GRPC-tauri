//! Unit Service Module
//!
//! Business logic for unit and associated user management.
//! SQL is delegated to UnitRepository and UserRepository.

use crate::errors::AppError;
use crate::models::{CreateUnitRequest, Unit, UserRole};
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::Utc;
use uuid::Uuid;

use crate::domain::security::PasswordHashPort;

/// Service for unit management business logic
pub struct UnitService<'a> {
    executor: DbExecutor<'a>,
    password_port: &'a dyn PasswordHashPort,
}

impl<'a> UnitService<'a> {
    /// Create a new UnitService with the given executor
    pub fn new(executor: DbExecutor<'a>, password_port: &'a dyn PasswordHashPort) -> Self {
        Self {
            executor,
            password_port,
        }
    }

    /// Create a unit with its associated user.
    ///
    /// The password is hashed node-bound to the unit code.
    /// The caller is responsible for wrapping in a transaction:
    /// `db.with_transaction(|tx| UnitService::new(tx).create_unit(req, wilaya_code))`
    pub fn create_unit(
        &self,
        req: &CreateUnitRequest,
        wilaya_code: &str,
    ) -> Result<(Unit, String), AppError> {
        let user_repo = self.executor.users();
        let unit_repo = self.executor.units();

        // Business logic: use unit code as node_id for node-bound password hashing
        let node_id = &req.code;

        // Hash password with node binding
        let password_hash = self
            .password_port
            .hash_password(&req.password, node_id)
            .map_err(crate::errors::AppError::Internal)?;

        let user_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        user_repo.upsert_user(
            &user_id,
            &req.username,
            &password_hash,
            UserRole::User,
            node_id,
            &now,
        )?;

        let unit_id = Uuid::new_v4().to_string();
        let unit_created_at_raw = Utc::now().to_rfc3339();
        unit_repo.insert_unit(&unit_id, req, wilaya_code, &user_id, &unit_created_at_raw)?;
        let created_at = crate::errors::parse_datetime_rfc3339(&unit_created_at_raw)
            .map_err(|e| crate::errors::AppError::Internal(e.to_string()))?;

        let unit = Unit {
            id: unit_id,
            code: req.code.clone(),
            name: req.name.clone(),
            wilaya_code: wilaya_code.to_string(),
            user_id: Some(user_id.clone()),
            created_at,
        };

        Ok((unit, user_id))
    }

    /// Update unit info and its associated user credentials.
    ///
    /// Business rules:
    /// - Update code/name always
    /// - Update username of associated user always (if user exists)
    /// - Update password only if non-empty
    pub fn update_unit(&self, unit_id: &str, req: &CreateUnitRequest) -> Result<Unit, AppError> {
        let user_repo = self.executor.users();
        let unit_repo = self.executor.units();

        // Update unit fields
        let now = Utc::now().to_rfc3339();
        unit_repo.update_unit(unit_id, &req.code, &req.name, &now)?;

        // Update associated user if exists
        if let Some(user_id) = unit_repo.get_unit_user_id(unit_id)? {
            if user_repo.get_user_by_id(&user_id)?.is_some() {
                // Update username
                let now = Utc::now().to_rfc3339();
                user_repo.update_username(&user_id, &req.username, &now)?;

                // Update password only if a new one is provided
                if !req.password.is_empty() {
                    let node_id = &req.code;
                    let password_hash = self
                        .password_port
                        .hash_password(&req.password, node_id)
                        .map_err(crate::errors::AppError::Internal)?;
                    let now = Utc::now().to_rfc3339();
                    user_repo.change_password(&user_id, &password_hash, &now)?;
                }
            }
        }

        unit_repo
            .get_unit(unit_id)?
            .ok_or_else(|| AppError::Internal(format!("Unit not found: {}", unit_id)))
    }

    /// Delete unit and its associated user atomically.
    pub fn delete_unit(&self, unit_id: &str) -> Result<(), AppError> {
        let user_repo = self.executor.users();
        let unit_repo = self.executor.units();

        if let Some(user_id) = unit_repo.delete_unit(unit_id)? {
            user_repo.delete_user(&user_id)?;
        }
        Ok(())
    }

    pub fn get_unit(&self, id: &str) -> Result<Option<Unit>, AppError> {
        self.executor.units().get_unit(id)
    }

    pub fn list_units(&self, wilaya_code: &str) -> Result<Vec<Unit>, AppError> {
        self.executor.units().list_units(wilaya_code)
    }
}
