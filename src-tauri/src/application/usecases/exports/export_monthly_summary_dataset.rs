use crate::application::usecases::exports::types::{
    MonthlySummaryExportDataset, MonthlySummaryExportInput,
};
use crate::errors::AppResult;
use crate::models::DailyDetailSyncSnapshot;
use crate::repositories::DbExecutor;

pub fn execute<'a>(
    executor: DbExecutor<'a>,
    input: MonthlySummaryExportInput,
    effective_unit_id: Option<&str>,
) -> AppResult<MonthlySummaryExportDataset> {
    let MonthlySummaryExportInput { year, month } = input;
    let window = crate::infrastructure::db::read::reports::monthly_window(year, month as u32)?;
    let projection = crate::infrastructure::db::read::reports::load_monthly_summary_projection(
        executor,
        window,
        effective_unit_id,
    )?;
    let summary = crate::models::MonthlySummary {
        month: projection.month,
        year: projection.year,
        total_personnel: projection.total_personnel,
        total_guests: projection.total_guests,
        total_meals: projection.total_meals,
        total_consumption_value: projection.total_cost,
        average_meal_rate: projection.average_meal_rate,
        report_count: projection.report_count,
    };
    let daily_detail_rows: Vec<DailyDetailSyncSnapshot> = projection
        .daily_rows
        .into_iter()
        .map(|r| DailyDetailSyncSnapshot {
            date: r.date,
            personnel_count: r.personnel,
            guest_count: r.guests,
            total_meals_cost: r.cost,
            actual_meal_rate: r.actual_meal_rate,
        })
        .collect();

    Ok(MonthlySummaryExportDataset {
        summary,
        daily_detail_rows,
    })
}
