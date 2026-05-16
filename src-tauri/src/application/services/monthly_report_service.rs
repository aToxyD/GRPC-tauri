//! Monthly Report Service Module
//!
//! Handles importing, listing, and retrieving monthly reports.

use crate::errors::AppError;
use crate::models::MonthlyReport;
use crate::repositories::{DbExecutor, RepositoryProvider};

/// Service for handling monthly reports
pub struct MonthlyReportService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> MonthlyReportService<'a> {
    /// Create a new MonthlyReportService
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// تخزين تقرير شهري مستورد (أو تحديثه إذا كان موجوداً)
    pub fn import_monthly_report(&self, report: &MonthlyReport) -> Result<(), AppError> {
        let repo = self.executor.reports();
        repo.upsert_monthly_report_summary(report)
    }

    /// جلب التقارير الشهرية مع فلترة اختيارية
    pub fn list_monthly_reports(
        &self,
        unit_id: Option<&str>,
        year: Option<i32>,
        month: Option<i32>,
    ) -> Result<Vec<MonthlyReport>, AppError> {
        let repo = self.executor.reports();
        repo.list_monthly_reports(unit_id, year, month)
    }

    /// جلب تقرير شهري محدد
    pub fn get_monthly_report(
        &self,
        unit_id: &str,
        year: i32,
        month: i32,
    ) -> Result<Option<MonthlyReport>, AppError> {
        let repo = self.executor.reports();
        repo.get_monthly_report(unit_id, year, month)
    }
}
