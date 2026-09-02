use crate::errors::AppError;
use crate::models::{DailyReportMeal, MonthlySummary, WilayaReportList};
use crate::repositories::RepositoryProvider;
use chrono::NaiveDate;

pub struct ReportCalculationService<'a> {
    executor: crate::repositories::DbExecutor<'a>,
}

impl<'a> ReportCalculationService<'a> {
    pub fn new(executor: crate::repositories::DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn calculate_meal_cost(items: Vec<(f64, f64)>) -> f64 {
        items
            .into_iter()
            .fold(0.0, |acc, (qty, price)| acc + qty * price)
    }

    pub fn calculate_meal_rate(
        total_cost: f64,
        staff_24h: i32,
        staff_8h: i32,
        reservation: i32,
        mission: i32,
        guest: i32,
    ) -> f64 {
        let total_beneficiaries = DailyReportMeal::compute_total_beneficiaries(
            staff_24h,
            staff_8h,
            reservation,
            mission,
            guest,
        );
        DailyReportMeal::compute_meal_average(total_cost, total_beneficiaries)
    }

    pub fn calculate_product_price_with_tva(base_price: f64, tva: f64) -> f64 {
        crate::domain::pricing::price::price_with_tva(base_price, tva)
    }

    pub fn build_wilaya_reports(
        &self,
        unit_id: Option<String>,
        report_type: String,
        year: Option<i32>,
        month: Option<i32>,
    ) -> Result<WilayaReportList, AppError> {
        let settings =
            crate::application::services::SettingsService::new(self.executor).get_settings()?;
        if settings.node_type != crate::models::NodeType::Wilaya {
            return Err(AppError::Internal(
                "غير مصرح: هذه العملية متاحة فقط لعقد WILAYA".to_string(),
            ));
        }

        match report_type.as_str() {
            "daily" => {
                let start = year
                    .zip(month)
                    .and_then(|(y, m)| NaiveDate::from_ymd_opt(y, m as u32, 1));

                let end = year.and_then(|y| {
                    month.and_then(|m| {
                        NaiveDate::from_ymd_opt(y, m as u32, 31)
                            .or_else(|| NaiveDate::from_ymd_opt(y, m as u32, 30))
                            .or_else(|| NaiveDate::from_ymd_opt(y, m as u32, 29))
                            .or_else(|| NaiveDate::from_ymd_opt(y, m as u32, 28))
                    })
                });

                let repo = self.executor.reports();
                let reports = match &unit_id {
                    Some(uid) => repo.list_daily_reports_scoped(start, end, uid)?,
                    None => repo.list_daily_reports(start, end)?,
                };

                Ok(WilayaReportList::Daily(reports))
            }
            "monthly" => {
                let target_year = match year {
                    Some(y) => y,
                    None => crate::application::services::fiscal_scope::resolve_active_fiscal_year(
                        self.executor,
                    )?,
                };
                let monthly_reports =
                    crate::application::services::MonthlyReportService::new(self.executor)
                        .list_monthly_reports(unit_id.as_deref(), Some(target_year), month)?;

                let mut summaries: Vec<MonthlySummary> = Vec::with_capacity(monthly_reports.len());
                for r in monthly_reports {
                    summaries.push(MonthlySummary {
                        month: r.report_month,
                        year: r.report_year,
                        total_beneficiaries: r.total_beneficiaries,
                        total_consumption_value: r.total_consumption_value,
                        breakfast_average: r.breakfast_average,
                        lunch_average: r.lunch_average,
                        dinner_average: r.dinner_average,
                        daily_average: r.daily_average,
                        report_count: r.report_count,
                    });
                }

                Ok(WilayaReportList::Monthly(summaries))
            }
            "stock" => {
                let start_date = year
                    .zip(month)
                    .and_then(|(y, m)| NaiveDate::from_ymd_opt(y, m as u32, 1))
                    .map(|d| d.to_string());

                let end_date = year
                    .and_then(|y| {
                        month.and_then(|m| {
                            NaiveDate::from_ymd_opt(y, m as u32, 31)
                                .or_else(|| NaiveDate::from_ymd_opt(y, m as u32, 30))
                                .or_else(|| NaiveDate::from_ymd_opt(y, m as u32, 29))
                                .or_else(|| NaiveDate::from_ymd_opt(y, m as u32, 28))
                        })
                    })
                    .map(|d| d.to_string());

                let filters = crate::models::StockMovementFilters {
                    unit_id: unit_id.clone(),
                    start_date,
                    end_date,
                    ..Default::default()
                };
                let response =
                    crate::application::services::StockMovementService::new(self.executor)
                        .get_stock_movements(&filters, 0, 1000)?;

                Ok(WilayaReportList::Stock(response.movements))
            }
            _ => Err(AppError::Validation(
                crate::errors::ValidationError::InvalidFormat {
                    field: "report_type".to_string(),
                    message: "نوع التقرير غير صالح: يجب أن يكون 'daily' أو 'monthly' أو 'stock'"
                        .to_string(),
                },
            )),
        }
    }
}
