//! Reports Repository Module
//!
//! Handles daily report and monthly report operations.
//! ARCHITECTURE: SQL only — no cross-repo calls, no business logic.

use crate::errors::AppError;
use crate::models::{DailyConsumptionInput, DailyConsumptionItem, DailyReport};
use crate::repositories::executor::DbExecutor;
use chrono::{NaiveDate, Utc};
use rusqlite::params;
use uuid::Uuid;

/// Repository for report-related database operations
pub struct ReportRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> ReportRepository<'a> {
    /// Create a new ReportRepository with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn upsert_monthly_report_summary(
        &self,
        report: &crate::models::MonthlyReport,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO monthly_reports (id, unit_id, report_year, report_month, total_personnel, total_guests, total_meals, total_consumption_value, average_meal_rate, report_count, imported_at, imported_by, file_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(unit_id, report_year, report_month) DO UPDATE SET
                total_personnel = excluded.total_personnel,
                total_guests = excluded.total_guests,
                total_meals = excluded.total_meals,
                total_consumption_value = excluded.total_consumption_value,
                average_meal_rate = excluded.average_meal_rate,
                report_count = excluded.report_count,
                imported_at = excluded.imported_at,
                imported_by = excluded.imported_by,
                file_hash = excluded.file_hash",
            rusqlite::params![
                report.id,
                report.unit_id,
                report.report_year,
                report.report_month,
                report.total_personnel,
                report.total_guests,
                report.total_meals,
                report.total_consumption_value,
                report.average_meal_rate,
                report.report_count,
                report.imported_at.to_rfc3339(),
                report.imported_by,
                report.file_hash,
            ],
        )?;
        Ok(())
    }

    pub fn list_monthly_reports(
        &self,
        unit_id: Option<&str>,
        year: Option<i32>,
        month: Option<i32>,
    ) -> Result<Vec<crate::models::MonthlyReport>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, unit_id, report_year, report_month, total_personnel, total_guests, total_meals, total_consumption_value, average_meal_rate, report_count, imported_at, imported_by, file_hash
             FROM monthly_reports
             WHERE (?1 IS NULL OR unit_id = ?1)
               AND (?2 IS NULL OR report_year = ?2)
               AND (?3 IS NULL OR report_month = ?3)
             ORDER BY report_year DESC, report_month DESC",
            rusqlite::params![unit_id, year, month],
            |row| {
                let imported_at_str: String = row.get(10)?;
                let imported_at = crate::errors::parse_datetime_rfc3339(&imported_at_str)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(crate::models::MonthlyReport {
                    id: row.get(0)?,
                    unit_id: row.get(1)?,
                    report_year: row.get(2)?,
                    report_month: row.get(3)?,
                    total_personnel: row.get(4)?,
                    total_guests: row.get(5)?,
                    total_meals: row.get(6)?,
                    total_consumption_value: row.get(7)?,
                    average_meal_rate: row.get(8)?,
                    report_count: row.get(9)?,
                    imported_at,
                    imported_by: row.get(11)?,
                    file_hash: row.get(12)?,
                })
            },
        )?)
    }

    pub fn get_monthly_report(
        &self,
        unit_id: &str,
        year: i32,
        month: i32,
    ) -> Result<Option<crate::models::MonthlyReport>, AppError> {
        let result = self.executor.query_row_optional(
            "SELECT id, unit_id, report_year, report_month, total_personnel, total_guests, total_meals, total_consumption_value, average_meal_rate, report_count, imported_at, imported_by, file_hash FROM monthly_reports WHERE unit_id = ?1 AND report_year = ?2 AND report_month = ?3",
            rusqlite::params![unit_id, year, month],
            |row| {
                let imported_at_str: String = row.get(10)?;
                let imported_at = crate::errors::parse_datetime_rfc3339(&imported_at_str)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(crate::models::MonthlyReport {
                    id: row.get(0)?,
                    unit_id: row.get(1)?,
                    report_year: row.get(2)?,
                    report_month: row.get(3)?,
                    total_personnel: row.get(4)?,
                    total_guests: row.get(5)?,
                    total_meals: row.get(6)?,
                    total_consumption_value: row.get(7)?,
                    average_meal_rate: row.get(8)?,
                    report_count: row.get(9)?,
                    imported_at,
                    imported_by: row.get(11)?,
                    file_hash: row.get(12)?,
                })
            }
        )?;
        Ok(result)
    }

    /// Check if a daily report exists for a specific date
    pub fn daily_report_exists_for_date_global(&self, date: &str) -> Result<bool, AppError> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM daily_reports WHERE date = ?1",
            [date],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn daily_report_exists_for_date_unit(
        &self,
        date: &str,
        unit_id: &str,
    ) -> Result<bool, AppError> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM daily_reports WHERE date = ?1 AND unit_id = ?2",
            [date, unit_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Insert the daily report header row. Returns the new report id.
    /// Called by DailyReportService after it has computed total_cost and meal_rate.
    pub fn insert_report_header(
        &self,
        input: &DailyConsumptionInput,
        unit_id: Option<&str>,
        total_cost: f64,
        meal_rate: f64,
        fiscal_year: i32,
    ) -> Result<String, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let date_str = input.date.to_string();
        self.executor.execute(
            "INSERT INTO daily_reports (id, date, personnel_count, guest_count, total_meals_cost, actual_meal_rate, unit_id, created_at, fiscal_year) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![&id, &date_str, &input.personnel_count, &input.guest_count, &total_cost, &meal_rate, unit_id, &now, fiscal_year],
        )?;
        Ok(id)
    }

    /// Insert a single consumption item row.
    /// Called by DailyReportService once per item after computing costs.
    pub fn insert_consumption_item(
        &self,
        report_id: &str,
        product_id: &str,
        quantity: f64,
        unit_price: f64,
        total_cost: f64,
    ) -> Result<(), AppError> {
        let item_id = Uuid::new_v4().to_string();
        self.executor.execute(
            "INSERT INTO daily_consumption_items (id, daily_report_id, product_id, quantity, unit_price, total_cost) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![&item_id, report_id, product_id, &quantity, &unit_price, &total_cost],
        )?;
        Ok(())
    }

    /// Insert a raw daily report
    pub fn insert_raw_daily_report(
        &self,
        id: &str,
        report: &crate::models::DailyReport,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO daily_reports (id, date, personnel_count, guest_count, total_meals_cost, actual_meal_rate, unit_id, created_at, fiscal_year) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![id, &report.date.to_string(), &report.personnel_count, &report.guest_count, &report.total_meals_cost, &report.actual_meal_rate, &report.unit_id, now, report.fiscal_year],
        )?;
        Ok(())
    }

    /// Insert a raw consumption item
    pub fn insert_raw_consumption_item(
        &self,
        report_id: &str,
        item: &crate::models::DailyConsumptionItem,
    ) -> Result<(), AppError> {
        let item_id = Uuid::new_v4().to_string();
        self.executor.execute(
            "INSERT INTO daily_consumption_items (id, daily_report_id, product_id, quantity, unit_price, total_cost) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![&item_id, report_id, &item.product_id, &item.quantity, &item.unit_price, &item.total_cost],
        )?;
        Ok(())
    }

    /// Get daily report by ID
    pub fn get_daily_report(&self, report_id: &str) -> Result<Option<DailyReport>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT id, date, personnel_count, guest_count, total_meals_cost, actual_meal_rate, unit_id, created_at, fiscal_year FROM daily_reports WHERE id = ?1",
                [report_id],
                |row| {
                    let date_str: String = row.get(1)?;
                    let created_at_str: String = row.get(7)?;
                    let date = crate::errors::parse_naive_date(&date_str)
                        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
                    let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(e)))?;
                    Ok(DailyReport {
                        id: row.get(0)?,
                        date,
                        personnel_count: row.get(2)?,
                        guest_count: row.get(3)?,
                        total_meals_cost: row.get(4)?,
                        actual_meal_rate: row.get(5)?,
                        unit_id: row.get(6)?,
                        created_at,
                        fiscal_year: row.get(8)?,
                    })
                },
            )?;
        Ok(result)
    }

    /// Get daily report with unit scope validation (IDOR prevention)
    pub fn get_daily_report_scoped(
        &self,
        report_id: &str,
        unit_id: &str,
    ) -> Result<Option<DailyReport>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT id, date, personnel_count, guest_count, total_meals_cost, actual_meal_rate, unit_id, created_at, fiscal_year 
                 FROM daily_reports 
                 WHERE id = ?1 AND unit_id = ?2",
                [report_id, unit_id],
                |row| {
                    let date_str: String = row.get(1)?;
                    let created_at_str: String = row.get(7)?;
                    let date = crate::errors::parse_naive_date(&date_str)
                        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
                    let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(e)))?;
                    Ok(DailyReport {
                        id: row.get(0)?,
                        date,
                        personnel_count: row.get(2)?,
                        guest_count: row.get(3)?,
                        total_meals_cost: row.get(4)?,
                        actual_meal_rate: row.get(5)?,
                        unit_id: row.get(6)?,
                        created_at,
                        fiscal_year: row.get(8)?,
                    })
                },
            )?;
        Ok(result)
    }

    /// Get daily report items
    pub fn get_daily_report_items(
        &self,
        report_id: &str,
    ) -> Result<Vec<DailyConsumptionItem>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT ci.id, ci.daily_report_id, ci.product_id, p.name, ci.quantity, ci.unit_price, ci.total_cost
               FROM daily_consumption_items ci
               JOIN products p ON ci.product_id = p.id
               WHERE ci.daily_report_id = ?1"#,
            [report_id],
            |row| {
                Ok(DailyConsumptionItem {
                    id: row.get(0)?,
                    daily_report_id: row.get(1)?,
                    product_id: row.get(2)?,
                    product_name: row.get(3)?,
                    quantity: row.get(4)?,
                    unit_price: row.get(5)?,
                    total_cost: row.get(6)?,
                })
            },
        )?)
    }

    /// List daily reports with optional date filter
    pub fn list_daily_reports(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> Result<Vec<DailyReport>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, date, personnel_count, guest_count, total_meals_cost, actual_meal_rate, unit_id, created_at, fiscal_year
             FROM daily_reports
             WHERE (?1 IS NULL OR date >= ?1)
               AND (?2 IS NULL OR date <= ?2)
             ORDER BY date DESC",
            rusqlite::params![start_date.map(|d| d.to_string()), end_date.map(|d| d.to_string())],
            |row| {
                let date_str: String = row.get(1)?;
                let created_at_str: String = row.get(7)?;
                let date = crate::errors::parse_naive_date(&date_str)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
                let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(DailyReport {
                    id: row.get(0)?,
                    date,
                    personnel_count: row.get(2)?,
                    guest_count: row.get(3)?,
                    total_meals_cost: row.get(4)?,
                    actual_meal_rate: row.get(5)?,
                    unit_id: row.get(6)?,
                    created_at,
                    fiscal_year: row.get(8)?,
                })
            },
        )?)
    }

    /// List daily reports filtered by unit_id (for IDOR protection)
    pub fn list_daily_reports_scoped(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
        unit_id: &str,
    ) -> Result<Vec<DailyReport>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, date, personnel_count, guest_count, total_meals_cost, actual_meal_rate, unit_id, created_at, fiscal_year
             FROM daily_reports
             WHERE unit_id = ?1
               AND (?2 IS NULL OR date >= ?2)
               AND (?3 IS NULL OR date <= ?3)
             ORDER BY date DESC",
            rusqlite::params![unit_id, start_date.map(|d| d.to_string()), end_date.map(|d| d.to_string())],
            |row| {
                let date_str: String = row.get(1)?;
                let created_at_str: String = row.get(7)?;
                let date = crate::errors::parse_naive_date(&date_str)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
                let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(DailyReport {
                    id: row.get(0)?,
                    date,
                    personnel_count: row.get(2)?,
                    guest_count: row.get(3)?,
                    total_meals_cost: row.get(4)?,
                    actual_meal_rate: row.get(5)?,
                    unit_id: row.get(6)?,
                    created_at,
                    fiscal_year: row.get(8)?,
                })
            },
        )?)
    }

    /// List daily reports for a specific month
    pub fn list_daily_reports_by_month(
        &self,
        year: i32,
        month: u32,
        unit_id: Option<&str>,
    ) -> Result<Vec<DailyReport>, AppError> {
        crate::infrastructure::db::read::reports::list_daily_reports_by_month(
            self.executor,
            year,
            month,
            unit_id,
        )
    }

    pub fn count_active_reports_by_year(&self, year: i32) -> Result<i64, AppError> {
        let count = self.executor.query_row(
            "SELECT COUNT(*) FROM daily_reports WHERE fiscal_year = ?1 AND deleted = 0",
            params![year],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn count_daily_reports_by_month(
        &self,
        year: i32,
        month: u32,
        unit_id: Option<&str>,
    ) -> Result<u32, AppError> {
        crate::infrastructure::db::read::reports::count_daily_reports_by_month(
            self.executor,
            year,
            month,
            unit_id,
        )
    }

    pub fn get_wilaya_monthly_report(
        &self,
        year: i32,
        month: i32,
    ) -> Result<crate::models::WilayaReportSummary, AppError> {
        let projections = crate::infrastructure::db::read::reports::load_wilaya_reports_projection(
            self.executor,
            year,
            month,
        )?;
        let reports = projections
            .into_iter()
            .map(|p| crate::models::WilayaUnitReport {
                unit_id: p.unit_id,
                unit_name: p.unit_name,
                total_personnel: p.total_personnel,
                total_guests: p.total_guests,
                total_cost: p.total_cost,
                avg_meal_rate: p.avg_meal_rate,
                is_imported: p.is_imported,
            })
            .collect();
        Ok(crate::models::WilayaReportSummary {
            year,
            month,
            reports,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_or_replace_raw_daily_report(
        &self,
        id: &str,
        date_str: &str,
        personnel_count: i32,
        guest_count: i32,
        total_meals_cost: f64,
        actual_meal_rate: f64,
        unit_id: &str,
        now: &str,
        fiscal_year: i32,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT OR REPLACE INTO daily_reports (id, date, personnel_count, guest_count, total_meals_cost, actual_meal_rate, unit_id, created_at, fiscal_year) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, date_str, personnel_count, guest_count, total_meals_cost, actual_meal_rate, unit_id, now, fiscal_year],
        )?;
        Ok(())
    }
}
