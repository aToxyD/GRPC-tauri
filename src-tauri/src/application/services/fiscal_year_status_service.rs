use crate::errors::AppError;
use crate::models::FiscalYearStatus;
use crate::repositories::{DbExecutor, RepositoryProvider};

pub struct FiscalYearStatusService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalYearStatusService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn get_year(&self, year: i32) -> Result<Option<FiscalYearStatus>, AppError> {
        self.executor.fiscal_year_status().get_by_year(year)
    }
}

impl crate::architecture::Service for FiscalYearStatusService<'_> {}
