//! Fiscal-year lifecycle service — facade.
//!
//! Delegates to the semantically decomposed services:
//!   - FiscalClosingService    (close_year, archive_year)
//!   - FiscalValidationService  (assert_fiscal_year_open)
//!   - FiscalReportingService   (get_status)
//!
//! This facade preserves backward compatibility for existing callers.
//! New callers should use the individual services directly.

pub use super::fiscal_closing_service::FiscalClosingService;
pub use super::fiscal_reporting_service::FiscalReportingService;
pub use super::fiscal_validation_service::FiscalValidationService;
pub use super::fiscal_validation_service::validate_fiscal_state;

use crate::errors::AppError;
use crate::repositories::DbExecutor;

pub struct FiscalYearService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalYearService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn close_year(
        &self,
        year: i32,
        next_year: i32,
        user_id: &str,
        username: &str,
        unit_id: Option<&str>,
    ) -> Result<usize, AppError> {
        FiscalClosingService::new(self.executor)
            .close_year(year, next_year, user_id, username, unit_id)
    }

    pub fn get_status(&self, year: i32) -> Result<Option<crate::models::FiscalYearStatus>, AppError> {
        FiscalReportingService::new(self.executor).get_status(year)
    }

    pub fn assert_fiscal_year_open(&self, year: i32) -> Result<(), AppError> {
        FiscalValidationService::new(self.executor).assert_fiscal_year_open(year)
    }

    pub fn archive_year(&self, year: i32, user_id: &str, username: &str) -> Result<(), AppError> {
        FiscalClosingService::new(self.executor).archive_year(year, user_id, username)
    }
}

impl crate::architecture::Service for FiscalYearService<'_> {}
