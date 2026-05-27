//! Daily Report Service — one daily report containing breakfast, lunch, dinner
//!
//! FIFO consumption is now performed at MEAL LEVEL, not aggregated per product.
//! Breakfast consumes FIFO layers first, then Lunch, then Dinner.
//! Each meal item's cost reflects authentic FIFO layer portions.

use crate::domain::meal_cost_engine::compute_meal_fifo_costs;
use crate::domain::validation::validate_daily_report_input;
use crate::errors::AppError;
use crate::models::{
    DailyReport, DailyReportInput, DailyReportMeal, DailyReportResult, MealSectionInput,
    MealSectionResult, MealType, MonthlySummary, NewStockMovement, StockMovementType,
};
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::Datelike;
use std::collections::HashMap;
use uuid::Uuid;

pub struct DailyReportService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> DailyReportService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn daily_report_exists_for_date(
        &self,
        date: &str,
        unit_id: Option<&str>,
    ) -> Result<bool, AppError> {
        let repo = self.executor.reports();
        match unit_id {
            Some(uid) => repo.daily_report_exists_for_date_unit(date, uid),
            None => repo.daily_report_exists_for_date_global(date),
        }
    }

    /// Returns all distinct fiscal years available across
    /// daily_reports and fiscal_year_status, sorted descending.
    pub fn list_available_fiscal_years(&self) -> Result<Vec<i32>, AppError> {
        self.executor.reports().list_available_fiscal_years()
    }

    pub fn create_daily_report(
        &self,
        input: &DailyReportInput,
        unit_id: Option<&str>,
        user_id: &str,
        username: &str,
    ) -> Result<String, AppError> {
        validate_daily_report_input(input)?;

        let date_str = input.date.to_string();
        if self.daily_report_exists_for_date(&date_str, unit_id)? {
            return Err(AppError::BusinessLogic(
                crate::errors::BusinessLogicError::ReportAlreadyExists { date: date_str },
            ));
        }

        let unit_id_str = unit_id.ok_or_else(|| {
            AppError::BusinessLogic(crate::errors::BusinessLogicError::OperationNotPermitted {
                message: "unit_id is required to create a daily report".to_string(),
            })
        })?;

        let fiscal_year = input.date.year();
        crate::application::services::FiscalYearService::new(self.executor)
            .assert_fiscal_year_open(fiscal_year)?;

        let product_repo = self.executor.products();
        let report_repo = self.executor.reports();
        let stock_repo = crate::application::services::StockMovementService::new(self.executor);
        let fifo_repo = self.executor.fifo_layers();
        let now = chrono::Utc::now().to_rfc3339();
        let report_id = Uuid::new_v4().to_string();

        // ── Phase 1: FIFO consumption via shared engine ─────────────────────
        // The engine handles meal ordering (Breakfast → Lunch → Dinner)
        // and delegates actual consumption to the injected callback.
        // No ratio-based redistribution. No weighted-average across meals.

        let computation = compute_meal_fifo_costs(&input.meals, |pid, qty| {
            fifo_repo.consume_fifo(unit_id_str, pid, qty)
        })?;

        // ── Phase 2: create movements and consumption records ────────────
        struct MealComputed {
            section: MealSectionInput,
            beneficiaries: i32,
            total_cost: f64,
            average: f64,
            item_costs: Vec<(String, f64, f64, f64, Option<String>)>,
        }
        let mut computed_meals: Vec<MealComputed> = Vec::new();

        let section_by_type: HashMap<MealType, &MealSectionInput> =
            input.meals.iter().map(|s| (s.meal_type, s)).collect();

        for computed_meal in &computation.meals {
            let section = *section_by_type
                .get(&computed_meal.meal_type)
                .ok_or_else(|| {
                    AppError::Internal(format!("Missing section for {:?}", computed_meal.meal_type))
                })?;

            let mut item_costs: Vec<(String, f64, f64, f64, Option<String>)> = Vec::new();

            for product in &computed_meal.products {
                let product_name = product_repo
                    .get_product(&product.product_id)?
                    .map(|p| p.name)
                    .unwrap_or_default();

                let weighted_unit_cost = if product.quantity > 0.0 {
                    product.total_cost / product.quantity
                } else {
                    0.0
                };

                let movement = NewStockMovement {
                    product_id: product.product_id.clone(),
                    movement_type: StockMovementType::Out,
                    quantity: product.quantity,
                    reference_type: Some("Consumption".to_string()),
                    reference_id: Some(report_id.clone()),
                    notes: Some(format!(
                        "استهلاك يومي - {} - {:?} - {}",
                        date_str, computed_meal.meal_type, product_name
                    )),
                    user_id: user_id.to_string(),
                    username: username.to_string(),
                    unit_id: Some(unit_id_str.to_string()),
                    unit_cost: Some(weighted_unit_cost),
                };
                let movement_id = stock_repo.record_stock_movement(&movement)?;

                for portion in &product.portions {
                    fifo_repo.create_consumption_record(
                        unit_id_str,
                        &movement_id,
                        &portion.layer_id,
                        portion.quantity,
                        portion.unit_cost,
                        &now,
                    )?;
                    item_costs.push((
                        product.product_id.clone(),
                        portion.quantity,
                        portion.unit_cost,
                        portion.total_cost,
                        Some(portion.layer_id.clone()),
                    ));
                }
            }

            let beneficiaries = DailyReportMeal::compute_total_beneficiaries(
                section.staff_24h_count,
                section.staff_8h_count,
                section.reservation_count,
                section.mission_count,
                section.guest_count,
            );
            let average =
                DailyReportMeal::compute_meal_average(computed_meal.total_cost, beneficiaries);

            computed_meals.push(MealComputed {
                section: section.clone(),
                beneficiaries,
                total_cost: computed_meal.total_cost,
                average,
                item_costs,
            });
        }

        // ── Phase 2: persist report header + meals + items ──────────────────
        let total_daily_cost: f64 = computed_meals.iter().map(|m| m.total_cost).sum();
        let total_daily_beneficiaries: i32 = computed_meals.iter().map(|m| m.beneficiaries).sum();
        let total_daily_average: f64 = computed_meals.iter().map(|m| m.average).sum();

        report_repo.insert_daily_report_header(
            &report_id,
            &date_str,
            unit_id,
            total_daily_cost,
            total_daily_average,
            total_daily_beneficiaries,
            fiscal_year,
            &now,
        )?;

        for meal in &computed_meals {
            let meal_id = Uuid::new_v4().to_string();
            report_repo.insert_meal_section(
                &meal_id,
                &report_id,
                &meal.section,
                meal.beneficiaries,
                meal.total_cost,
                meal.average,
            )?;

            for (product_id, quantity, unit_price, item_cost, layer_id) in &meal.item_costs {
                let item_id = Uuid::new_v4().to_string();
                report_repo.insert_meal_item(
                    &item_id,
                    &meal_id,
                    product_id,
                    *quantity,
                    *unit_price,
                    *item_cost,
                    layer_id.as_deref(),
                )?;
            }
        }

        Ok(report_id)
    }

    pub fn load_daily_report_result(
        &self,
        report_id: &str,
    ) -> Result<Option<DailyReportResult>, AppError> {
        let report = self.executor.reports().get_daily_report(report_id)?;
        let Some(report) = report else {
            return Ok(None);
        };

        let meal_rows = self.executor.reports().list_meals_for_report(report_id)?;
        let mut meals = Vec::with_capacity(meal_rows.len());
        for meal in meal_rows {
            let items = self.executor.reports().get_meal_items(&meal.id)?;
            meals.push(MealSectionResult { meal, items });
        }

        Ok(Some(DailyReportResult { report, meals }))
    }

    pub fn get_daily_consumption_view(
        &self,
        date: &str,
        unit_id: Option<&str>,
    ) -> Result<Option<DailyReportResult>, AppError> {
        let report = self
            .executor
            .reports()
            .get_daily_report_by_date(date, unit_id)?;
        match report {
            Some(r) => self.load_daily_report_result(&r.id),
            None => Ok(None),
        }
    }

    pub fn get_daily_report(&self, id: &str) -> Result<Option<DailyReport>, AppError> {
        self.executor.reports().get_daily_report(id)
    }

    pub fn get_monthly_summary(
        &self,
        year: i32,
        month: i32,
        effective_unit_id: Option<&str>,
    ) -> Result<MonthlySummary, AppError> {
        let projection = crate::infrastructure::db::read::reports::load_monthly_summary_projection(
            self.executor,
            crate::infrastructure::db::read::reports::monthly_window(year, month as u32)?,
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

    pub fn get_wilaya_monthly_report(
        &self,
        year: i32,
        month: i32,
    ) -> Result<crate::models::WilayaReportSummary, AppError> {
        self.executor
            .reports()
            .get_wilaya_monthly_report(year, month)
    }

    pub fn list_daily_reports_by_month(
        &self,
        year: i32,
        month: u32,
        unit_id: Option<&str>,
    ) -> Result<Vec<DailyReport>, AppError> {
        self.executor
            .reports()
            .list_daily_reports_by_month(year, month, unit_id)
    }

    pub fn calculate_meal_cost(&self, items: Vec<(f64, f64)>) -> f64 {
        items.into_iter().map(|(qty, price)| qty * price).sum()
    }

    pub fn calculate_meal_rate(
        &self,
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

    pub fn calculate_product_price_with_tva(&self, base_price: f64, tva: f64) -> f64 {
        base_price * (1.0 + tva / 100.0)
    }
}
