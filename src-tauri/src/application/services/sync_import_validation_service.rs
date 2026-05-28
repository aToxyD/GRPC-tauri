//! Sync-import validation service.
//!
//! Handles fiscal-integrity checks and provenance validation for incoming
//! sync packages.  Does NOT mutate any state.

use crate::errors::AppError;
use crate::repositories::DbExecutor;

pub struct SyncImportValidationService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> SyncImportValidationService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Validate fiscal constraints for an incoming package import.
    /// Checks: fiscal year is open, year is within allowed import horizon,
    /// and non-historical packages match the current year.
    pub fn validate_import_fiscal_year(
        &self,
        incoming_year: i32,
        package_type: &str,
        source_node: &str,
        package_id: &str,
    ) -> Result<(), AppError> {
        let current_year: i32 =
            self.executor
                .query_row("SELECT current_year FROM settings WHERE id=1", [], |r| {
                    r.get(0)
                })?;
        crate::application::services::FiscalHistoricalGuard::new(self.executor)
            .assert_import_year_allowed(incoming_year)?;
        crate::application::services::FiscalValidationService::new(self.executor)
            .assert_fiscal_year_open(incoming_year)?;
        if package_type != "historical_import" && incoming_year != current_year {
            log::warn!(target:"grpc::sync","[FISCAL_IMPORT_REJECTED] source_node={} package_id={} incoming_year={} current_year={}",source_node,package_id,incoming_year,current_year);
            return Err(AppError::Internal(
                "Fiscal year mismatch during import".into(),
            ));
        }
        Ok(())
    }

    pub fn get_current_node_id(db: &crate::db::Database) -> String {
        match crate::application::services::SettingsService::new(db.executor()).get_settings() {
            Ok(s) => s.unit_name.unwrap_or_else(|| "WILAYA".to_string()),
            Err(_) => "unknown".to_string(),
        }
    }
}

/// Pure-function timestamp comparison — determines whether an incoming record
/// is newer than the existing one.
///
/// Used by product sync to avoid overwriting with stale data.
pub fn is_incoming_newer(incoming: &str, existing: &str) -> bool {
    match (
        chrono::DateTime::parse_from_rfc3339(incoming),
        chrono::DateTime::parse_from_rfc3339(existing),
    ) {
        (Ok(incoming_dt), Ok(existing_dt)) => {
            incoming_dt.with_timezone(&chrono::Utc) > existing_dt.with_timezone(&chrono::Utc)
        }
        _ => incoming > existing,
    }
}

impl crate::architecture::Service for SyncImportValidationService<'_> {}
