//! Daily Report Service Module
//!
//! Orchestrates daily consumption report creation.
//! Business logic (cost calculation, meal rate) lives here.
//! All SQL is delegated to ReportRepository, ProductRepository, and StockMovementRepository.

use crate::errors::AppError;
use crate::models::{
    DailyConsumptionInput, DailyReport, MonthlySummary, NewStockMovement, StockMovementType,
};
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::Datelike;

/// Service for daily report business logic
pub struct DailyReportService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> DailyReportService<'a> {
    /// Create a new DailyReportService with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Check if a daily report exists for a given date (and optional unit scope).
    ///
    /// Logic (choosing scoped vs global query) belongs to the service layer.
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

    /// Create a daily consumption report:
    /// 1. Calculate total_cost and meal_rate (business logic — belongs in service)
    /// 2. Persist the report header via ReportRepository
    /// 3. Persist each consumption item via ReportRepository
    /// 4. Record an OUT stock movement per item via StockMovementRepository
    pub fn create_daily_report(
        &self,
        input: &DailyConsumptionInput,
        unit_id: Option<&str>,
        user_id: &str,
        username: &str,
    ) -> Result<(String, f64, f64), AppError> {
        let dt = input.date;
        // FALLBACK: report lacks explicit fiscal_year — infer from date field.
        let fiscal_year = dt.year();
        log::warn!(
            target: "grpc::fiscal",
            "FISCAL FALLBACK: daily report fiscal_year inferred from date year={}",
            fiscal_year
        );

        // 1. Fiscal Guard
        crate::application::services::FiscalYearService::new(self.executor)
            .assert_fiscal_year_open(fiscal_year)?;

        let product_repo = self.executor.products();
        let report_repo = self.executor.reports();
        let stock_repo = crate::application::services::StockMovementService::new(self.executor);
        let date_str = input.date.to_string();

        // ─── Business Logic: compute costs ──────────────────────────────────
        let mut total_cost = 0.0f64;
        let mut item_costs: Vec<(String, f64, f64, f64)> = Vec::new(); // (product_id, qty, unit_price, item_cost)

        for item in &input.items {
            let product = product_repo
                .get_product(&item.product_id)
                .map_err(|e| {
                    log::error!(
                        "Financial integrity: DB error fetching product '{}': {:?}",
                        item.product_id,
                        e
                    );
                    e
                })?
                .ok_or_else(|| {
                    crate::errors::AppError::BusinessLogic(
                        crate::errors::BusinessLogicError::ResourceNotFound {
                            resource: "Product".to_string(),
                            id: item.product_id.clone(),
                        },
                    )
                })?;
            let item_cost = item.quantity * product.base_price;
            total_cost += item_cost;
            item_costs.push((
                item.product_id.clone(),
                item.quantity,
                product.base_price,
                item_cost,
            ));
        }

        let total_meals = input.personnel_count + input.guest_count;
        let meal_rate = if total_meals > 0 {
            total_cost / total_meals as f64
        } else {
            0.0
        };
        // ────────────────────────────────────────────────────────────────────

        // Persist report header
        let report_id =
            report_repo.insert_report_header(input, unit_id, total_cost, meal_rate, fiscal_year)?;

        // Persist items + stock movements
        for (product_id, quantity, unit_price, item_cost) in item_costs {
            // Get product name for the movement note
            let product_name = product_repo
                .get_product(&product_id)?
                .map(|p| p.name)
                .unwrap_or_default();

            report_repo.insert_consumption_item(
                &report_id,
                &product_id,
                quantity,
                unit_price,
                item_cost,
            )?;

            let movement = NewStockMovement {
                product_id: product_id.clone(),
                movement_type: StockMovementType::Out,
                quantity,
                reference_type: Some("Consumption".to_string()),
                reference_id: Some(report_id.clone()),
                notes: Some(format!("استهلاك يومي - {} - {}", date_str, product_name)),
                user_id: user_id.to_string(),
                username: username.to_string(),
                unit_id: unit_id.map(|u| u.to_string()),
            };
            stock_repo.record_stock_movement(&movement)?;
        }

        Ok((report_id, total_cost, meal_rate))
    }

    pub fn calculate_meal_cost(&self, items: Vec<(f64, f64)>) -> f64 {
        items.into_iter().map(|(qty, price)| qty * price).sum()
    }

    pub fn calculate_meal_rate(
        &self,
        total_cost: f64,
        personnel_count: i32,
        guest_count: i32,
    ) -> f64 {
        let total_meals = personnel_count + guest_count;
        if total_meals > 0 {
            total_cost / total_meals as f64
        } else {
            0.0
        }
    }

    pub fn calculate_product_price_with_tva(&self, base_price: f64, tva: f64) -> f64 {
        base_price * (1.0 + tva / 100.0)
    }

    pub fn get_monthly_summary(
        &self,
        year: i32,
        month: i32,
        effective_unit_id: Option<&str>,
    ) -> Result<MonthlySummary, AppError> {
        let filtered = self.list_daily_reports_by_month(year, month as u32, effective_unit_id)?;

        let total_personnel: i32 = filtered.iter().map(|r| r.personnel_count).sum();
        let total_guests: i32 = filtered.iter().map(|r| r.guest_count).sum();
        let total_meals = total_personnel + total_guests;
        let total_consumption: f64 = filtered.iter().map(|r| r.total_meals_cost).sum();
        let average_rate = if filtered.is_empty() {
            0.0
        } else {
            filtered.iter().map(|r| r.actual_meal_rate).sum::<f64>() / filtered.len() as f64
        };

        Ok(MonthlySummary {
            month,
            year,
            total_personnel,
            total_guests,
            total_meals,
            total_consumption_value: total_consumption,
            average_meal_rate: average_rate,
            report_count: filtered.len() as i32,
        })
    }

    pub fn get_daily_report(&self, id: &str) -> Result<Option<DailyReport>, AppError> {
        self.executor.reports().get_daily_report(id)
    }

    pub fn get_daily_report_items(
        &self,
        id: &str,
    ) -> Result<Vec<crate::models::DailyConsumptionItem>, AppError> {
        self.executor.reports().get_daily_report_items(id)
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
}
