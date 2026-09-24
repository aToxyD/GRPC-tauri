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

    /// Total meal cost from `(quantity, unit_price)` wire pairs (ADR-0048).
    /// Each term is exact `money × quantity`; the sum is exact `Decimal` and is
    /// rounded to `f64` exactly once at this boundary.
    pub fn calculate_meal_cost(items: Vec<(f64, f64)>) -> Result<f64, AppError> {
        use crate::domain::numeric::legacy_float;
        let mut total = crate::domain::numeric::Money::zero();
        for (qty, price) in items {
            let quantity = legacy_float::quantity_from_f64(qty)?;
            let unit_price = legacy_float::money_from_f64(price)?;
            let line = unit_price.checked_mul_quantity(&quantity)?;
            total = total.checked_add(line)?;
        }
        legacy_float::money_to_f64(&total).map_err(Into::into)
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

    /// IPC-facing TVA calculation (SEC-087 / ADR-0048): the `f64` inputs are wire
    /// values, converted to exact `Decimal` at this boundary; the arithmetic is
    /// the canonical tax-term chain owned by `domain::pricing::price` and the
    /// TTC result is rounded exactly once back to `f64`. Transitional legacy
    /// display helper — NOT an authoritative pricing source.
    pub fn calculate_product_price_with_tva(base_price: f64, tva: f64) -> Result<f64, AppError> {
        let base = crate::domain::numeric::legacy_float::money_from_f64(base_price)?;
        let rate = crate::domain::numeric::legacy_float::rate_from_f64(tva)?;
        let breakdown = crate::domain::pricing::price::compute_contract_fiscal(&base, &rate)?;
        crate::domain::numeric::legacy_float::money_to_f64(&breakdown.price_ttc).map_err(Into::into)
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
