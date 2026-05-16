use crate::application::usecases::exports::types::{
    DailyReportExportDataset, DailyReportExportInput,
};
use crate::application::usecases::reports::get_daily_report;
use crate::application::usecases::reports::types::ReportScope;
use crate::errors::AppResult;
use crate::models::{DailyConsumptionSyncLine, DailyReportSyncSnapshot};
use crate::repositories::DbExecutor;

pub fn execute<'a>(
    executor: DbExecutor<'a>,
    scope: ReportScope,
    input: DailyReportExportInput,
) -> AppResult<DailyReportExportDataset> {
    let result = get_daily_report::execute(executor, scope, &input.report_id)?;
    let items: Vec<DailyConsumptionSyncLine> = result
        .items
        .iter()
        .map(|i| DailyConsumptionSyncLine {
            product_id: i.product_id.clone(),
            product_name: i.product_name.clone(),
            quantity: i.quantity,
            unit_price: i.unit_price,
            total_cost: i.total_cost,
        })
        .collect();
    let r = result.report;
    let snapshot = DailyReportSyncSnapshot {
        report_id: r.id,
        date: r.date,
        personnel_count: r.personnel_count,
        guest_count: r.guest_count,
        total_meals_cost: r.total_meals_cost,
        actual_meal_rate: r.actual_meal_rate,
        items,
    };
    Ok(DailyReportExportDataset { snapshot })
}
