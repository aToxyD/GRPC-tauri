//! Fiscal-year read-only reporting service.
//!
//! Serves read projections of fiscal-year status.
//! Must NOT contain mutation, validation, or orchestration logic.

use crate::errors::AppError;
use crate::models::FiscalYearStatus;
use crate::repositories::{DbExecutor, RepositoryProvider};

pub struct FiscalReportingService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalReportingService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn get_status(&self, year: i32) -> Result<Option<FiscalYearStatus>, AppError> {
        self.executor.fiscal_year_status().get_by_year(year)
    }
}

impl crate::architecture::Service for FiscalReportingService<'_> {}
