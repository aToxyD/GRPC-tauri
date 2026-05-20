use crate::application::usecases::exports::types::{
    DailyReportExportDataset, DailyReportExportInput,
};
use crate::application::usecases::reports::get_daily_report;
use crate::application::usecases::reports::types::ReportScope;
use crate::errors::AppResult;
use crate::models::{DailyConsumptionSyncLine, DailyReportSyncSnapshot, MealSectionSyncSnapshot};
use crate::repositories::DbExecutor;

pub fn execute<'a>(
    executor: DbExecutor<'a>,
    scope: ReportScope,
    input: DailyReportExportInput,
) -> AppResult<DailyReportExportDataset> {
    let result = get_daily_report::execute(executor, scope, &input.report_id)?;
    let r = result.report;

    let meals: Vec<MealSectionSyncSnapshot> = result
        .meals
        .iter()
        .map(|m| {
            let items: Vec<DailyConsumptionSyncLine> = m
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
            MealSectionSyncSnapshot {
                meal_type: m.meal.meal_type,
                staff_24h_count: m.meal.staff_24h_count,
                staff_8h_count: m.meal.staff_8h_count,
                reservation_count: m.meal.reservation_count,
                mission_count: m.meal.mission_count,
                guest_count: m.meal.guest_count,
                total_beneficiaries: m.meal.total_beneficiaries,
                total_meal_cost: m.meal.total_meal_cost,
                meal_average: m.meal.meal_average,
                items,
            }
        })
        .collect();

    let snapshot = DailyReportSyncSnapshot {
        report_id: r.id,
        date: r.date,
        total_daily_cost: r.total_daily_cost,
        total_daily_average: r.total_daily_average,
        total_daily_beneficiaries: r.total_daily_beneficiaries,
        meals,
    };

    Ok(DailyReportExportDataset { snapshot })
}
