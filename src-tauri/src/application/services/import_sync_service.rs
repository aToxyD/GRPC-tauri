//! Import Sync Service — facade.
//!
//! Delegates to the semantically decomposed services:
//!   - SyncImportValidationService  (validate_import_fiscal_year, get_current_node_id)
//!   - SyncImportExecutionService   (import_daily_reports, import_monthly_report,
//!     import_products_sync, import_stock_movements)
//!
//! Conflict detection is handled by SyncConflictService (already decomposed).
//!
//! This facade preserves backward compatibility for existing callers.
//! New callers should use the individual services directly.

pub use super::sync_import_execution_service::SyncImportExecutionService;
pub use super::sync_import_validation_service::SyncImportValidationService;
pub use super::sync_import_validation_service::is_incoming_newer;

use crate::errors::AppError;
use crate::models::{DailyReportResult, ProductSyncRecord};
use crate::repositories::DbExecutor;

pub struct ImportSyncService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> ImportSyncService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn get_current_node_id(db: &crate::db::Database) -> String {
        SyncImportValidationService::get_current_node_id(db)
    }

    pub fn import_daily_reports(&self, reports: Vec<DailyReportResult>) -> Result<usize, AppError> {
        SyncImportExecutionService::new(self.executor).import_daily_reports(reports)
    }

    pub fn import_monthly_report(
        &self,
        unit_id: &str,
        year: i32,
        month: u32,
        reports: Vec<DailyReportResult>,
    ) -> Result<usize, AppError> {
        SyncImportExecutionService::new(self.executor)
            .import_monthly_report(unit_id, year, month, reports)
    }

    pub fn import_products_sync(
        &self,
        records: &[ProductSyncRecord],
        current_year: i32,
        current_node_id: &str,
    ) -> Result<(usize, usize, usize), AppError> {
        SyncImportExecutionService::new(self.executor)
            .import_products_sync(records, current_year, current_node_id)
    }

    pub fn import_stock_movements(
        &self,
        movements: Vec<crate::models::inventory::StockMovement>,
        unit_id: Option<&str>,
    ) -> Result<usize, AppError> {
        SyncImportExecutionService::new(self.executor)
            .import_stock_movements(movements, unit_id)
    }
}

impl crate::architecture::Service for ImportSyncService<'_> {}
