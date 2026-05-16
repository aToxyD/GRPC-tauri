//! Settings Repository Module
//!
//! Handles all settings-related database operations including:
//! - Application configuration
//! - Wilaya node configuration
//! - Current year management
//! - Unit ID retrieval

use crate::errors::AppError;
use crate::models::{Settings, WilayaNodeConfiguration};
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

/// Repository for settings-related database operations
pub struct SettingsRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> SettingsRepository<'a> {
    /// Create a new SettingsRepository with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Get the application settings row (SQL-only).
    pub fn get_settings_row(&self) -> Result<Settings, AppError> {
        let settings = self.executor.query_row(
            "SELECT id, node_type, unit_name, current_year, wilaya_code, wilaya_name, configured FROM settings WHERE id = 1",
            [],
            |row| {
                let node_type_str: String = row.get(1)?;
                let node_type = match node_type_str.parse() {
                    Ok(nt) => nt,
                    Err(_) => crate::models::NodeType::Unit, // Fallback
                };

                Ok(Settings {
                    id: row.get(0)?,
                    node_type,
                    unit_name: row.get(2)?,
                    unit_code: None, // Placeholder
                    current_year: row.get(3)?,
                    wilaya_code: row.get(4)?,
                    wilaya_name: row.get(5)?,
                    configured: row.get(6)?,
                })
            },
        )?;
        Ok(settings)
    }

    /// Fetch the first unit code (SQL-only). Used by SettingsService for UNIT nodes.
    pub fn get_first_unit_code(&self) -> Result<Option<String>, AppError> {
        Ok(self
            .executor
            .query_row_optional("SELECT code FROM units LIMIT 1", [], |row| row.get(0))?)
    }

    /// Configure the wilaya node settings
    pub fn configure_wilaya(&self, config: &WilayaNodeConfiguration) -> Result<(), AppError> {
        self.executor.execute(
            "UPDATE settings SET node_type = ?1, unit_name = NULL, wilaya_code = ?2, wilaya_name = ?3, configured = 1 WHERE id = 1",
            params![config.node_type.to_string(), &config.wilaya_code, &config.wilaya_name],
        )?;
        Ok(())
    }

    /// Configure UNIT node settings from imported package
    pub fn update_unit_node_settings(
        &self,
        unit_name: &str,
        wilaya_code: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "UPDATE settings SET node_type = 'UNIT', unit_name = ?1, wilaya_code = ?2, wilaya_name = NULL, configured = 1 WHERE id = 1",
            rusqlite::params![unit_name, wilaya_code],
        )?;
        Ok(())
    }

    /// Set the current year
    pub fn set_current_year(&self, year: i32) -> Result<(), AppError> {
        self.executor
            .execute("UPDATE settings SET current_year = ?1 WHERE id = 1", [year])?;
        Ok(())
    }

    /// Get the current year from settings
    pub fn get_current_year(&self) -> Result<i32, AppError> {
        let year: i32 =
            self.executor
                .query_row("SELECT current_year FROM settings WHERE id = 1", [], |r| {
                    r.get(0)
                })?;
        Ok(year)
    }

    /// Get the current unit_id for UNIT nodes
    /// Returns None for WILAYA nodes or if no unit is configured
    /// NOTE: Returns unit_name (from settings) not id (from units table) for consistency
    pub fn get_current_unit_id(&self) -> Result<Option<String>, AppError> {
        Ok(self.executor.query_row_optional(
            "SELECT unit_name FROM settings WHERE id = 1 AND node_type = 'UNIT'",
            [],
            |row| row.get(0),
        )?)
    }

    /// Check if the application is configured
    pub fn is_configured(&self) -> Result<bool, AppError> {
        let configured: i32 =
            self.executor
                .query_row("SELECT configured FROM settings WHERE id = 1", [], |row| {
                    row.get(0)
                })?;
        Ok(configured != 0)
    }

    /// Get the node type (WILAYA or UNIT)
    pub fn get_node_type(&self) -> Result<String, AppError> {
        Ok(self
            .executor
            .query_row("SELECT node_type FROM settings WHERE id = 1", [], |row| {
                row.get(0)
            })?)
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
    fn test_get_settings() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = SettingsRepository::new(make_executor(&db));

        // Should get default settings
        let settings = repo.get_settings_row().unwrap();
        assert_eq!(settings.id, 1);
    }

    #[test]
    fn test_set_current_year() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = SettingsRepository::new(make_executor(&db));

        repo.set_current_year(2025).unwrap();
        let settings = repo.get_settings_row().unwrap();
        assert_eq!(settings.current_year, 2025);
    }
}
