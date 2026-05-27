//! Reports Repository — daily report parent + meal sections + meal items

use crate::errors::{AppError, ValidationError};
use crate::models::{
    DailyReport, DailyReportMeal, DailyReportMealItem, MealSectionInput, MealType, MonthlyReport,
};
use crate::repositories::executor::DbExecutor;
use chrono::NaiveDate;
use rusqlite::params;

const REPORT_SELECT: &str = "SELECT id, date, unit_id, total_daily_cost, total_daily_average, total_daily_beneficiaries, created_at, fiscal_year FROM daily_reports";

const MEAL_SELECT: &str = "SELECT id, daily_report_id, meal_type, staff_24h_count, staff_8h_count, reservation_count, mission_count, guest_count, total_beneficiaries, total_meal_cost, meal_average FROM daily_report_meals";

fn map_report_row(row: &rusqlite::Row<'_>) -> Result<DailyReport, rusqlite::Error> {
    let date_str: String = row.get(1)?;
    let created_at_str: String = row.get(6)?;
    let date = crate::errors::parse_naive_date(&date_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(DailyReport {
        id: row.get(0)?,
        date,
        unit_id: row.get(2)?,
        total_daily_cost: row.get(3)?,
        total_daily_average: row.get(4)?,
        total_daily_beneficiaries: row.get(5)?,
        created_at,
        fiscal_year: row.get(7)?,
    })
}

fn map_meal_row(row: &rusqlite::Row<'_>) -> Result<DailyReportMeal, rusqlite::Error> {
    let meal_type_str: String = row.get(2)?;
    let meal_type = MealType::from_str_custom(&meal_type_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid meal_type",
            )),
        )
    })?;
    Ok(DailyReportMeal {
        id: row.get(0)?,
        daily_report_id: row.get(1)?,
        meal_type,
        staff_24h_count: row.get(3)?,
        staff_8h_count: row.get(4)?,
        reservation_count: row.get(5)?,
        mission_count: row.get(6)?,
        guest_count: row.get(7)?,
        total_beneficiaries: row.get(8)?,
        total_meal_cost: row.get(9)?,
        meal_average: row.get(10)?,
    })
}

pub struct ReportRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> ReportRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn upsert_monthly_report_summary(&self, report: &MonthlyReport) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO monthly_reports (id, unit_id, report_year, report_month, total_beneficiaries, total_consumption_value, breakfast_average, lunch_average, dinner_average, daily_average, report_count, imported_at, imported_by, file_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(unit_id, report_year, report_month) DO UPDATE SET
                total_beneficiaries = excluded.total_beneficiaries,
                total_consumption_value = excluded.total_consumption_value,
                breakfast_average = excluded.breakfast_average,
                lunch_average = excluded.lunch_average,
                dinner_average = excluded.dinner_average,
                daily_average = excluded.daily_average,
                report_count = excluded.report_count,
                imported_at = excluded.imported_at,
                imported_by = excluded.imported_by,
                file_hash = excluded.file_hash",
            rusqlite::params![
                report.id,
                report.unit_id,
                report.report_year,
                report.report_month,
                report.total_beneficiaries,
                report.total_consumption_value,
                report.breakfast_average,
                report.lunch_average,
                report.dinner_average,
                report.daily_average,
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
    ) -> Result<Vec<MonthlyReport>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, unit_id, report_year, report_month, total_beneficiaries, total_consumption_value, breakfast_average, lunch_average, dinner_average, daily_average, report_count, imported_at, imported_by, file_hash
             FROM monthly_reports
             WHERE (?1 IS NULL OR unit_id = ?1) AND (?2 IS NULL OR report_year = ?2) AND (?3 IS NULL OR report_month = ?3)
             ORDER BY report_year DESC, report_month DESC",
            rusqlite::params![unit_id, year, month],
            |row| {
                let imported_at_str: String = row.get(11)?;
                let imported_at = crate::errors::parse_datetime_rfc3339(&imported_at_str)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(MonthlyReport {
                    id: row.get(0)?,
                    unit_id: row.get(1)?,
                    report_year: row.get(2)?,
                    report_month: row.get(3)?,
                    total_beneficiaries: row.get(4)?,
                    total_consumption_value: row.get(5)?,
                    breakfast_average: row.get(6)?,
                    lunch_average: row.get(7)?,
                    dinner_average: row.get(8)?,
                    daily_average: row.get(9)?,
                    report_count: row.get(10)?,
                    imported_at,
                    imported_by: row.get(12)?,
                    file_hash: row.get(13)?,
                })
            },
        )?)
    }

    pub fn get_monthly_report(
        &self,
        unit_id: &str,
        year: i32,
        month: i32,
    ) -> Result<Option<MonthlyReport>, AppError> {
        let result = self.executor.query_row_optional(
            "SELECT id, unit_id, report_year, report_month, total_beneficiaries, total_consumption_value, breakfast_average, lunch_average, dinner_average, daily_average, report_count, imported_at, imported_by, file_hash FROM monthly_reports WHERE unit_id = ?1 AND report_year = ?2 AND report_month = ?3",
            rusqlite::params![unit_id, year, month],
            |row| {
                let imported_at_str: String = row.get(11)?;
                let imported_at = crate::errors::parse_datetime_rfc3339(&imported_at_str)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(MonthlyReport {
                    id: row.get(0)?,
                    unit_id: row.get(1)?,
                    report_year: row.get(2)?,
                    report_month: row.get(3)?,
                    total_beneficiaries: row.get(4)?,
                    total_consumption_value: row.get(5)?,
                    breakfast_average: row.get(6)?,
                    lunch_average: row.get(7)?,
                    dinner_average: row.get(8)?,
                    daily_average: row.get(9)?,
                    report_count: row.get(10)?,
                    imported_at,
                    imported_by: row.get(12)?,
                    file_hash: row.get(13)?,
                })
            },
        )?;
        Ok(result)
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

    pub fn daily_report_exists_for_date_global(&self, date: &str) -> Result<bool, AppError> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM daily_reports WHERE date = ?1 AND unit_id IS NULL",
            [date],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_daily_report_header(
        &self,
        id: &str,
        date_str: &str,
        unit_id: Option<&str>,
        total_daily_cost: f64,
        total_daily_average: f64,
        total_daily_beneficiaries: i32,
        fiscal_year: i32,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO daily_reports (id, date, unit_id, total_daily_cost, total_daily_average, total_daily_beneficiaries, created_at, fiscal_year) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, date_str, unit_id, total_daily_cost, total_daily_average, total_daily_beneficiaries, now, fiscal_year],
        )?;
        Ok(())
    }

    pub fn insert_meal_section(
        &self,
        meal_id: &str,
        daily_report_id: &str,
        section: &MealSectionInput,
        total_beneficiaries: i32,
        total_meal_cost: f64,
        meal_average: f64,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO daily_report_meals (id, daily_report_id, meal_type, staff_24h_count, staff_8h_count, reservation_count, mission_count, guest_count, total_beneficiaries, total_meal_cost, meal_average) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                meal_id,
                daily_report_id,
                section.meal_type.as_str(),
                section.staff_24h_count,
                section.staff_8h_count,
                section.reservation_count,
                section.mission_count,
                section.guest_count,
                total_beneficiaries,
                total_meal_cost,
                meal_average,
            ],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_meal_item(
        &self,
        item_id: &str,
        meal_id: &str,
        product_id: &str,
        quantity: f64,
        unit_price: f64,
        total_cost: f64,
        fifo_layer_id: Option<&str>,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO daily_report_meal_items (id, meal_id, product_id, quantity, unit_price, total_cost, fifo_layer_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![item_id, meal_id, product_id, quantity, unit_price, total_cost, fifo_layer_id],
        )?;
        Ok(())
    }

    pub fn get_daily_report(&self, report_id: &str) -> Result<Option<DailyReport>, AppError> {
        let sql = format!("{REPORT_SELECT} WHERE id = ?1");
        Ok(self
            .executor
            .query_row_optional(&sql, [report_id], map_report_row)?)
    }

    pub fn get_daily_report_scoped(
        &self,
        report_id: &str,
        unit_id: &str,
    ) -> Result<Option<DailyReport>, AppError> {
        let sql = format!("{REPORT_SELECT} WHERE id = ?1 AND unit_id = ?2");
        Ok(self
            .executor
            .query_row_optional(&sql, [report_id, unit_id], map_report_row)?)
    }

    pub fn get_daily_report_by_date(
        &self,
        date: &str,
        unit_id: Option<&str>,
    ) -> Result<Option<DailyReport>, AppError> {
        let sql = match unit_id {
            Some(_) => format!("{REPORT_SELECT} WHERE date = ?1 AND unit_id = ?2"),
            None => format!("{REPORT_SELECT} WHERE date = ?1 AND unit_id IS NULL"),
        };
        Ok(match unit_id {
            Some(uid) => self.executor.query_row_optional(
                &sql,
                rusqlite::params![date, uid],
                map_report_row,
            )?,
            None => {
                self.executor
                    .query_row_optional(&sql, rusqlite::params![date], map_report_row)?
            }
        })
    }

    pub fn list_meals_for_report(&self, report_id: &str) -> Result<Vec<DailyReportMeal>, AppError> {
        let sql = format!("{MEAL_SELECT} WHERE daily_report_id = ?1 ORDER BY meal_type ASC");
        Ok(self.executor.query_all(&sql, [report_id], map_meal_row)?)
    }

    pub fn get_meal_items(&self, meal_id: &str) -> Result<Vec<DailyReportMealItem>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT mi.id, mi.meal_id, mi.product_id, p.name, mi.quantity, mi.unit_price, mi.total_cost, mi.fifo_layer_id
               FROM daily_report_meal_items mi
               JOIN products p ON mi.product_id = p.id
               WHERE mi.meal_id = ?1
               ORDER BY mi.product_id ASC, mi.id ASC"#,
            [meal_id],
            |row| {
                Ok(DailyReportMealItem {
                    id: row.get(0)?,
                    meal_id: row.get(1)?,
                    product_id: row.get(2)?,
                    product_name: row.get(3)?,
                    quantity: row.get(4)?,
                    unit_price: row.get(5)?,
                    total_cost: row.get(6)?,
                    fifo_layer_id: row.get(7)?,
                })
            },
        )?)
    }

    pub fn list_daily_reports(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> Result<Vec<DailyReport>, AppError> {
        let sql = format!(
            "{REPORT_SELECT} WHERE (?1 IS NULL OR date >= ?1) AND (?2 IS NULL OR date <= ?2) ORDER BY date DESC"
        );
        Ok(self.executor.query_all(
            &sql,
            rusqlite::params![
                start_date.map(|d| d.to_string()),
                end_date.map(|d| d.to_string())
            ],
            map_report_row,
        )?)
    }

    pub fn list_daily_reports_scoped(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
        unit_id: &str,
    ) -> Result<Vec<DailyReport>, AppError> {
        let sql = format!(
            "{REPORT_SELECT} WHERE unit_id = ?1 AND (?2 IS NULL OR date >= ?2) AND (?3 IS NULL OR date <= ?3) ORDER BY date DESC"
        );
        Ok(self.executor.query_all(
            &sql,
            rusqlite::params![
                unit_id,
                start_date.map(|d| d.to_string()),
                end_date.map(|d| d.to_string())
            ],
            map_report_row,
        )?)
    }

    /// List daily reports filtered by fiscal_year and optional month.
    /// When month is Some, filters by that month (1..=12).
    /// When month is None, returns all months in the fiscal year.
    /// Validates that month is in 1..=12 range.
    pub fn list_daily_reports_by_fiscal_year(
        &self,
        fiscal_year: Option<i32>,
        month: Option<u32>,
        unit_id: &str,
    ) -> Result<Vec<DailyReport>, AppError> {
        if let Some(m) = month {
            if !(1..=12).contains(&m) {
                return Err(AppError::Validation(ValidationError::OutOfRange {
                    field: "month".to_string(),
                    value: m.to_string(),
                }));
            }
        }
        let sql = format!(
            "{} WHERE unit_id = ?1 AND (?2 IS NULL OR fiscal_year = ?2) AND (?3 IS NULL OR CAST(strftime('%m', date) AS INTEGER) = ?3) ORDER BY date DESC",
            REPORT_SELECT
        );
        Ok(self.executor.query_all(
            &sql,
            rusqlite::params![unit_id, fiscal_year, month.map(|m| m as i64)],
            map_report_row,
        )?)
    }

    /// Returns all distinct fiscal years available across
    /// daily_reports and fiscal_year_status, sorted descending.
    pub fn list_available_fiscal_years(&self) -> Result<Vec<i32>, AppError> {
        Ok(self.executor.query_all(
            "SELECT DISTINCT year FROM (
                SELECT fiscal_year AS year FROM daily_reports
                UNION ALL
                SELECT year FROM fiscal_year_status
            ) ORDER BY year DESC",
            [],
            |row| row.get(0),
        )?)
    }

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
                total_beneficiaries: p.total_beneficiaries,
                total_cost: p.total_cost,
                daily_average: p.daily_average,
                is_imported: p.is_imported,
            })
            .collect();
        Ok(crate::models::WilayaReportSummary {
            year,
            month,
            reports,
        })
    }

    pub fn insert_raw_daily_report(
        &self,
        id: &str,
        report: &DailyReport,
        now: &str,
    ) -> Result<(), AppError> {
        self.insert_daily_report_header(
            id,
            &report.date.to_string(),
            report.unit_id.as_deref(),
            report.total_daily_cost,
            report.total_daily_average,
            report.total_daily_beneficiaries,
            report.fiscal_year,
            now,
        )
    }

    pub fn insert_raw_meal(&self, meal: &DailyReportMeal) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO daily_report_meals (id, daily_report_id, meal_type, staff_24h_count, staff_8h_count, reservation_count, mission_count, guest_count, total_beneficiaries, total_meal_cost, meal_average) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                &meal.id,
                &meal.daily_report_id,
                meal.meal_type.as_str(),
                meal.staff_24h_count,
                meal.staff_8h_count,
                meal.reservation_count,
                meal.mission_count,
                meal.guest_count,
                meal.total_beneficiaries,
                meal.total_meal_cost,
                meal.meal_average,
            ],
        )?;
        Ok(())
    }

    pub fn insert_raw_meal_item(&self, item: &DailyReportMealItem) -> Result<(), AppError> {
        self.insert_meal_item(
            &item.id,
            &item.meal_id,
            &item.product_id,
            item.quantity,
            item.unit_price,
            item.total_cost,
            item.fifo_layer_id.as_deref(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_or_replace_raw_daily_report(
        &self,
        id: &str,
        report: &DailyReport,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT OR REPLACE INTO daily_reports (id, date, unit_id, total_daily_cost, total_daily_average, total_daily_beneficiaries, created_at, fiscal_year) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                report.date.to_string(),
                report.unit_id,
                report.total_daily_cost,
                report.total_daily_average,
                report.total_daily_beneficiaries,
                now,
                report.fiscal_year,
            ],
        )?;
        Ok(())
    }

    // Legacy stubs used by old code paths during transition
    pub fn meal_report_exists_for_date_unit(
        &self,
        date: &str,
        _meal_type: &str,
        unit_id: &str,
    ) -> Result<bool, AppError> {
        self.daily_report_exists_for_date_unit(date, unit_id)
    }

    pub fn meal_report_exists_for_date_global(
        &self,
        date: &str,
        _meal_type: &str,
    ) -> Result<bool, AppError> {
        self.daily_report_exists_for_date_global(date)
    }
}
