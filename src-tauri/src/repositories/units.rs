//! Units Repository Module
//!
//! Handles unit-related database operations.

use crate::errors::AppError;
use crate::models::{CreateUnitRequest, Unit};
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

/// Repository for unit-related database operations
pub struct UnitRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> UnitRepository<'a> {
    /// Create a new UnitRepository with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Insert a new unit row (SQL-only).
    /// Note: For atomic unit creation with user, use with_transaction at Database level.
    pub fn insert_unit(
        &self,
        id: &str,
        req: &CreateUnitRequest,
        wilaya_code: &str,
        user_id: &str,
        created_at: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO units (id, code, name, wilaya_code, user_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, &req.code, &req.name, wilaya_code, user_id, created_at],
        )?;
        Ok(())
    }

    /// Insert or replace raw unit data from imported package
    pub fn upsert_raw_unit(
        &self,
        id: &str,
        code: &str,
        name: &str,
        wilaya_code: &str,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT OR REPLACE INTO units (id, code, name, wilaya_code, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, code, name, wilaya_code, now],
        )?;
        Ok(())
    }

    /// Update a unit's user_id
    pub fn update_unit_user(&self, unit_id: &str, user_id: &str) -> Result<(), AppError> {
        self.executor.execute(
            "UPDATE units SET user_id = ?1 WHERE id = ?2",
            rusqlite::params![user_id, unit_id],
        )?;
        Ok(())
    }

    /// Get unit by ID
    pub fn get_unit(&self, unit_id: &str) -> Result<Option<Unit>, AppError> {
        let result = self.executor.query_row_optional(
            "SELECT id, code, name, wilaya_code, user_id, created_at FROM units WHERE id = ?1",
            [unit_id],
            |row| {
                let created_at_str: String = row.get(5)?;
                let created_at =
                    crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(Unit {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    name: row.get(2)?,
                    wilaya_code: row.get(3)?,
                    user_id: row.get(4)?,
                    created_at,
                })
            },
        )?;
        Ok(result)
    }

    /// Get unit by code
    pub fn get_unit_by_code(&self, code: &str) -> Result<Option<Unit>, AppError> {
        let result = self.executor.query_row_optional(
            "SELECT id, code, name, wilaya_code, user_id, created_at FROM units WHERE code = ?1",
            [code],
            |row| {
                let created_at_str: String = row.get(5)?;
                let created_at =
                    crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(Unit {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    name: row.get(2)?,
                    wilaya_code: row.get(3)?,
                    user_id: row.get(4)?,
                    created_at,
                })
            },
        )?;
        Ok(result)
    }

    /// Resolve canonical `units.id` when the local DB has a row for this display name + wilaya (UNIT nodes).
    pub fn find_unit_id_by_name_and_wilaya(
        &self,
        name: &str,
        wilaya_code: &str,
    ) -> Result<Option<String>, AppError> {
        self.executor
            .query_row_optional(
                "SELECT id FROM units WHERE name = ?1 AND wilaya_code = ?2 LIMIT 1",
                [name, wilaya_code],
                |row| row.get(0),
            )
            .map_err(AppError::from)
    }

    /// Get unit by associated user ID (for IDOR protection in password changes)
    pub fn get_unit_by_user_id(&self, user_id: &str) -> Result<Option<Unit>, AppError> {
        let result = self.executor.query_row_optional(
            "SELECT id, code, name, wilaya_code, user_id, created_at FROM units WHERE user_id = ?1",
            [user_id],
            |row| {
                let created_at_str: String = row.get(5)?;
                let created_at =
                    crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(Unit {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    name: row.get(2)?,
                    wilaya_code: row.get(3)?,
                    user_id: row.get(4)?,
                    created_at,
                })
            },
        )?;
        Ok(result)
    }

    /// List all units for a wilaya
    pub fn list_units(&self, wilaya_code: &str) -> Result<Vec<Unit>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, code, name, wilaya_code, user_id, created_at FROM units WHERE wilaya_code = ?1 ORDER BY code",
            [wilaya_code],
            |row| {
                let created_at_str: String = row.get(5)?;
                let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(Unit {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    name: row.get(2)?,
                    wilaya_code: row.get(3)?,
                    user_id: row.get(4)?,
                    created_at,
                })
            },
        )?)
    }

    /// List all units across all wilayas (for cross-unit benchmarks).
    pub fn list_all_units(&self) -> Result<Vec<Unit>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, code, name, wilaya_code, user_id, created_at FROM units ORDER BY code",
            [],
            |row| {
                let created_at_str: String = row.get(5)?;
                let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(Unit {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    name: row.get(2)?,
                    wilaya_code: row.get(3)?,
                    user_id: row.get(4)?,
                    created_at,
                })
            },
        )?)
    }

    /// Delete a unit and its associated user
    /// Note: For atomic deletion, use with_transaction at Database level
    pub fn delete_unit(&self, unit_id: &str) -> Result<Option<String>, AppError> {
        // Get the user_id associated with this unit
        let user_id: Option<String> = self.executor.query_row_optional(
            "SELECT user_id FROM units WHERE id = ?1",
            [unit_id],
            |row| row.get(0),
        )?;

        // Delete the unit
        self.executor
            .execute("DELETE FROM units WHERE id = ?1", [unit_id])?;

        Ok(user_id)
    }

    /// Update unit information
    pub fn update_unit(
        &self,
        unit_id: &str,
        code: &str,
        name: &str,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "UPDATE units SET code = ?1, name = ?2, updated_at = ?3 WHERE id = ?4",
            params![code, name, now, unit_id],
        )?;

        Ok(())
    }

    /// Get user ID associated with a unit
    pub fn get_unit_user_id(&self, unit_id: &str) -> Result<Option<String>, AppError> {
        let user_id: Option<String> = self.executor.query_row_optional(
            "SELECT user_id FROM units WHERE id = ?1",
            [unit_id],
            |row| row.get(0),
        )?;
        Ok(user_id)
    }
}
