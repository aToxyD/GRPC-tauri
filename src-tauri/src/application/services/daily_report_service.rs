//! Daily Report Service — one daily report containing breakfast, lunch, dinner

use crate::domain::validation::validate_daily_report_input;
use crate::errors::AppError;
use crate::models::{
    DailyReport, DailyReportInput, DailyReportMeal, DailyReportResult, MealSectionInput,
    MealSectionResult, MonthlySummary, NewStockMovement, StockMovementType,
};
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::Datelike;
use std::collections::HashMap;
use uuid::Uuid;

type ComputedMeal = (
    MealSectionInput,
    i32,
    f64,
    f64,
    Vec<(String, f64, f64, f64, Option<String>)>,
);

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

        // ── Phase 1: aggregate total quantity per product across all meals ───
        let mut stock_by_product: HashMap<String, f64> = HashMap::new();
        for section in &input.meals {
            for item in &section.items {
                if item.quantity > 0.0 {
                    *stock_by_product
                        .entry(item.product_id.clone())
                        .or_insert(0.0) += item.quantity;
                }
            }
        }

        // ── Phase 2: FIFO consumption first, then OUT movement ───────────────
        // This order ensures InsufficientStock aborts before any ledger write.
        struct ProductConsumption {
            movement_id: String,
            portions: Vec<crate::models::ConsumedLayerPortion>,
        }
        let mut consumption_map: HashMap<String, ProductConsumption> = HashMap::new();

        for (product_id, quantity) in &stock_by_product {
            let product_name = product_repo
                .get_product(product_id)?
                .map(|p| p.name)
                .unwrap_or_default();

            // Step A: consume FIFO (fails fast with InsufficientStock if needed)
            let portions = fifo_repo.consume_fifo(unit_id_str, product_id, *quantity);

            // Step B: if FIFO fails → abort entirely (transaction rolls back)
            let portions = portions?;

            // Step C: create OUT movement with weighted-average FIFO cost
            let total_qty: f64 = portions.iter().map(|p| p.quantity).sum();
            let total_cost: f64 = portions.iter().map(|p| p.total_cost).sum();
            let weighted_unit_cost = if total_qty > 0.0 {
                total_cost / total_qty
            } else {
                0.0
            };

            let movement = NewStockMovement {
                product_id: product_id.clone(),
                movement_type: StockMovementType::Out,
                quantity: *quantity,
                reference_type: Some("Consumption".to_string()),
                reference_id: Some(report_id.clone()),
                notes: Some(format!("استهلاك يومي - {} - {}", date_str, product_name)),
                user_id: user_id.to_string(),
                username: username.to_string(),
                unit_id: unit_id.map(|u| u.to_string()),
                unit_cost: Some(weighted_unit_cost),
            };
            let movement_id = stock_repo.record_stock_movement(&movement)?;

            consumption_map.insert(
                product_id.clone(),
                ProductConsumption {
                    movement_id,
                    portions,
                },
            );
        }

        // ── Phase 3: record inventory_layer_consumptions for each movement ───
        // (must happen after movement exists for FK to pass)
        for (product_id, cons) in &consumption_map {
            for portion in &cons.portions {
                fifo_repo.create_consumption_record(
                    unit_id_str,
                    &cons.movement_id,
                    &portion.layer_id,
                    portion.quantity,
                    portion.unit_cost,
                    &now,
                )?;
            }
            let _ = product_id; // used implicitly via consumption_map key
        }

        // ── Phase 4: compute meal totals and persist ─────────────────────────
        let mut computed_meals: Vec<ComputedMeal> = Vec::new();

        for section in &input.meals {
            let mut item_costs: Vec<(String, f64, f64, f64, Option<String>)> = Vec::new();
            let mut total_cost = 0.0f64;

            for item in &section.items {
                if item.quantity <= 0.0 {
                    continue;
                }

                let cons = consumption_map.get(&item.product_id).ok_or_else(|| {
                    AppError::Internal(format!(
                        "No consumption record for product {}",
                        item.product_id
                    ))
                })?;

                // NEW — one line per FIFO layer, each at its real unit_cost
                let total_qty_for_product =
                    *stock_by_product.get(&item.product_id).ok_or_else(|| {
                        AppError::Internal(format!(
                            "Product {} not found in stock_by_product",
                            item.product_id
                        ))
                    })?;

                for portion in &cons.portions {
                    let ratio = if total_qty_for_product > 0.0 {
                        item.quantity / total_qty_for_product
                    } else {
                        0.0
                    };
                    let portion_qty_in_meal = portion.quantity * ratio;
                    let portion_cost_in_meal = portion.total_cost * ratio;

                    total_cost += portion_cost_in_meal;

                    item_costs.push((
                        item.product_id.clone(),
                        portion_qty_in_meal,
                        portion.unit_cost, // real layer price
                        portion_cost_in_meal,
                        Some(portion.layer_id.clone()), // direct layer reference
                    ));
                }
            }

            let total_beneficiaries = DailyReportMeal::compute_total_beneficiaries(
                section.staff_24h_count,
                section.staff_8h_count,
                section.reservation_count,
                section.mission_count,
                section.guest_count,
            );
            let meal_average =
                DailyReportMeal::compute_meal_average(total_cost, total_beneficiaries);

            computed_meals.push((
                section.clone(),
                total_beneficiaries,
                total_cost,
                meal_average,
                item_costs,
            ));
        }

        let total_daily_cost: f64 = computed_meals.iter().map(|(_, _, c, _, _)| c).sum();
        let total_daily_beneficiaries: i32 = computed_meals.iter().map(|(_, b, _, _, _)| b).sum();
        let total_daily_average: f64 = computed_meals.iter().map(|(_, _, _, a, _)| a).sum();

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

        for (section, total_beneficiaries, total_cost, meal_average, item_costs) in computed_meals {
            let meal_id = Uuid::new_v4().to_string();
            report_repo.insert_meal_section(
                &meal_id,
                &report_id,
                &section,
                total_beneficiaries,
                total_cost,
                meal_average,
            )?;

            for (product_id, quantity, unit_price, item_cost, layer_id) in item_costs {
                let item_id = Uuid::new_v4().to_string();
                report_repo.insert_meal_item(
                    &item_id,
                    &meal_id,
                    &product_id,
                    quantity,
                    unit_price,
                    item_cost,
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
