use crate::application::usecases::reports::types::{DailyReportFilters, ReportScope};
use crate::errors::AppResult;
use crate::models::DailyReport;
use crate::repositories::{DbExecutor, ReportRepository};

pub fn execute<'a>(
    executor: DbExecutor<'a>,
    scope: ReportScope,
    filters: DailyReportFilters,
) -> AppResult<Vec<DailyReport>> {
    let repo = ReportRepository::new(executor);

    // Fiscal year filter (Unit scope only; Wilaya ignores it)
    if let ReportScope::Unit(unit_id) = &scope {
        if filters.fiscal_year.is_some() {
            return repo.list_daily_reports_by_fiscal_year(
                filters.fiscal_year,
                filters.month,
                unit_id.as_str(),
            );
        }
    }

    match scope {
        ReportScope::Global => repo.list_daily_reports(filters.start_date, filters.end_date),
        ReportScope::Unit(unit_id) => {
            repo.list_daily_reports_scoped(filters.start_date, filters.end_date, unit_id.as_str())
        }
    }
}
