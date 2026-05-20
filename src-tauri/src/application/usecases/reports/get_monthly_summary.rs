use crate::errors::AppError;
use crate::models::MonthlySummary;
use crate::repositories::DbExecutor;

/// Usecase: Get monthly summary.
///
/// Key rule: لا pull-all ثم filtering في Rust.
/// نستخدم query شهرية مستهدفة عبر `ReportRepository::list_daily_reports_by_month`.
pub fn execute<'a>(
    executor: DbExecutor<'a>,
    year: i32,
    month: i32,
    effective_unit_id: Option<&str>,
) -> Result<MonthlySummary, AppError> {
    let window = crate::infrastructure::db::read::reports::monthly_window(year, month as u32)?;
    let projection = crate::infrastructure::db::read::reports::load_monthly_summary_projection(
        executor,
        window,
        effective_unit_id,
    )?;
    Ok(MonthlySummary {
        month: projection.month,
        year: projection.year,
        total_beneficiaries: projection.total_beneficiaries,
        total_consumption_value: projection.total_cost,
        breakfast_average: projection.breakfast_average,
        lunch_average: projection.lunch_average,
        dinner_average: projection.dinner_average,
        daily_average: projection.daily_average,
        report_count: projection.report_count,
    })
}
