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
    // SEC-057 monthly completeness gate: a UNIT-scoped export must represent a
    // full calendar month (no partial month). A WILAYA-scoped export has no
    // single unit whose daily completeness is being aggregated, so the gate
    // applies only when an effective unit scope is present.
    if let Some(unit_id) = effective_unit_id {
        crate::infrastructure::db::read::reports::assert_complete_calendar_month(
            executor,
            year,
            month as u32,
            Some(unit_id),
        )?;
    }
    let projection = crate::infrastructure::db::read::reports::load_monthly_summary_projection(
        executor,
        window,
        effective_unit_id,
    )?;
    let summary = crate::models::MonthlySummary {
        month: projection.month,
        year: projection.year,
        total_beneficiaries: projection.total_beneficiaries,
        total_consumption_value: projection.total_cost,
        breakfast_average: projection.breakfast_average,
        lunch_average: projection.lunch_average,
        dinner_average: projection.dinner_average,
        daily_average: projection.daily_average,
        report_count: projection.report_count,
    };
    let daily_detail_rows: Vec<DailyDetailSyncSnapshot> = projection
        .daily_rows
        .into_iter()
        .map(|r| DailyDetailSyncSnapshot {
            date: r.date,
            total_daily_beneficiaries: r.total_daily_beneficiaries,
            total_daily_cost: r.total_daily_cost,
            breakfast_average: r.breakfast_average,
            lunch_average: r.lunch_average,
            dinner_average: r.dinner_average,
            daily_average: r.daily_average,
        })
        .collect();

    Ok(MonthlySummaryExportDataset {
        summary,
        daily_detail_rows,
    })
}
