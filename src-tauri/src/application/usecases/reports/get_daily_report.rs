use crate::application::usecases::reports::types::ReportScope;
use crate::errors::{AppError, AppResult};
use crate::models::DailyReportResult;
use crate::repositories::{DbExecutor, ReportRepository};

pub fn execute<'a>(
    executor: DbExecutor<'a>,
    scope: ReportScope,
    report_id: &str,
) -> AppResult<DailyReportResult> {
    let repo = ReportRepository::new(executor);

    let report = match scope {
        ReportScope::Global => repo
            .get_daily_report(report_id)?
            .ok_or_else(|| AppError::Internal("Report not found".to_string()))?,
        ReportScope::Unit(unit_id) => repo
            .get_daily_report_scoped(report_id, unit_id.as_str())?
            .ok_or_else(|| AppError::Internal("Report not found or access denied".to_string()))?,
    };

    let items = repo.get_daily_report_items(report_id)?;
    Ok(DailyReportResult { report, items })
}
