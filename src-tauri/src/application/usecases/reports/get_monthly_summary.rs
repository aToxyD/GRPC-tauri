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
        total_personnel: projection.total_personnel,
        total_guests: projection.total_guests,
        total_meals: projection.total_meals,
        total_consumption_value: projection.total_cost,
        average_meal_rate: projection.average_meal_rate,
        report_count: projection.report_count,
    })
}
