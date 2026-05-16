//! Read-side SQL for reports (month slices, projections). Writes stay in [`crate::repositories::reports::ReportRepository`].

use chrono::{Datelike, NaiveDate};

use crate::errors::{AppError, ValidationError};
use crate::models::DailyReport;
use crate::repositories::DbExecutor;

/// Inclusive calendar bounds for filtering `daily_reports.date` stored as `YYYY-MM-DD` text.
#[derive(Clone, Copy, Debug)]
pub struct MonthlyWindow {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

#[derive(Clone, Debug)]
pub struct MonthlySummaryDailyRowProjection {
    pub date: NaiveDate,
    pub cost: f64,
    pub personnel: i32,
    pub guests: i32,
    pub actual_meal_rate: f64,
}

#[derive(Clone, Debug)]
pub struct MonthlySummaryProjection {
    pub year: i32,
    pub month: i32,
    pub total_cost: f64,
    pub total_personnel: i32,
    pub total_guests: i32,
    pub total_meals: i32,
    pub average_meal_rate: f64,
    pub report_count: i32,
    pub daily_rows: Vec<MonthlySummaryDailyRowProjection>,
}

/// Read-oriented denormalized projection for wilaya reporting/export.
/// Not a domain entity and not intended for mutation workflows.
#[derive(Clone, Debug)]
pub struct WilayaReportProjection {
    pub unit_id: String,
    pub unit_name: String,
    pub total_personnel: i32,
    pub total_guests: i32,
    pub total_cost: f64,
    pub avg_meal_rate: f64,
    pub is_imported: bool,
}

/// Resolve first/last day of `(year, month)` for SQL range filters (not domain rules — query boundary only).
pub fn monthly_window(year: i32, month: u32) -> Result<MonthlyWindow, AppError> {
    let start = NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(|| {
        AppError::Validation(ValidationError::OutOfRange {
            field: "month".to_string(),
            value: format!("{year}-{month:02}"),
        })
    })?;
    let (ny, nm) = if month == 12 {
        (year + 1, 1u32)
    } else {
        (year, month + 1)
    };
    let end = NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|d| d.pred_opt())
        .ok_or_else(|| {
            AppError::Internal(format!(
                "monthly_window: could not compute last day for {year}-{month:02}"
            ))
        })?;
    Ok(MonthlyWindow { start, end })
}

/// Daily reports in `[year-month]` range, optionally scoped to a unit (same semantics as legacy repository method).
pub fn list_daily_reports_by_month(
    executor: DbExecutor<'_>,
    year: i32,
    month: u32,
    unit_id: Option<&str>,
) -> Result<Vec<DailyReport>, AppError> {
    let win = monthly_window(year, month)?;
    let start_date = win.start.to_string();
    let end_date = win.end.to_string();

    Ok(executor.query_all(
        "SELECT id, date, personnel_count, guest_count, total_meals_cost, actual_meal_rate, unit_id, created_at, fiscal_year
         FROM daily_reports
         WHERE date >= ?1 AND date <= ?2
           AND (?3 IS NULL OR unit_id = ?3)
         ORDER BY date ASC",
        rusqlite::params![start_date, end_date, unit_id],
        |row| {
            let date_str: String = row.get(1)?;
            let created_at_str: String = row.get(7)?;
            let date = crate::errors::parse_naive_date(&date_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
            })?;
            let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(e))
            })?;
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

pub fn count_daily_reports_by_month(
    executor: DbExecutor<'_>,
    year: i32,
    month: u32,
    unit_id: Option<&str>,
) -> Result<u32, AppError> {
    let win = monthly_window(year, month)?;
    let start_date = win.start.to_string();
    let end_date = win.end.to_string();

    let count: i64 = executor.query_row(
        "SELECT COUNT(*) FROM daily_reports WHERE date >= ?1 AND date <= ?2 AND (?3 IS NULL OR unit_id = ?3)",
        rusqlite::params![start_date, end_date, unit_id],
        |row| row.get(0),
    )?;
    Ok(count as u32)
}

pub fn load_monthly_summary_projection(
    executor: DbExecutor<'_>,
    window: MonthlyWindow,
    unit_id: Option<&str>,
) -> Result<MonthlySummaryProjection, AppError> {
    let start_date = window.start.to_string();
    let end_date = window.end.to_string();

    let (total_cost, total_personnel, total_guests, average_meal_rate, report_count): (
        f64,
        i32,
        i32,
        f64,
        i32,
    ) = executor.query_row(
        "SELECT
            COALESCE(SUM(total_meals_cost), 0.0),
            COALESCE(SUM(personnel_count), 0),
            COALESCE(SUM(guest_count), 0),
            COALESCE(AVG(actual_meal_rate), 0.0),
            COALESCE(COUNT(*), 0)
         FROM daily_reports
         WHERE date >= ?1 AND date <= ?2
           AND (?3 IS NULL OR unit_id = ?3)",
        rusqlite::params![start_date, end_date, unit_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )?;

    let daily_rows = executor.query_all(
        "SELECT date, total_meals_cost, personnel_count, guest_count, actual_meal_rate
         FROM daily_reports
         WHERE date >= ?1 AND date <= ?2
           AND (?3 IS NULL OR unit_id = ?3)
         ORDER BY date ASC",
        rusqlite::params![window.start.to_string(), window.end.to_string(), unit_id],
        |row| {
            let date_str: String = row.get(0)?;
            let date = crate::errors::parse_naive_date(&date_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            Ok(MonthlySummaryDailyRowProjection {
                date,
                cost: row.get(1)?,
                personnel: row.get(2)?,
                guests: row.get(3)?,
                actual_meal_rate: row.get(4)?,
            })
        },
    )?;

    let total_meals = total_personnel + total_guests;
    Ok(MonthlySummaryProjection {
        year: window.start.year(),
        month: window.start.month() as i32,
        total_cost,
        total_personnel,
        total_guests,
        total_meals,
        average_meal_rate,
        report_count,
        daily_rows,
    })
}

pub fn load_wilaya_reports_projection(
    executor: DbExecutor<'_>,
    year: i32,
    month: i32,
) -> Result<Vec<WilayaReportProjection>, AppError> {
    Ok(executor.query_all(
        r#"SELECT u.id, u.name,
                  COALESCE(mr.total_personnel, 0),
                  COALESCE(mr.total_guests, 0),
                  COALESCE(mr.total_consumption_value, 0.0),
                  COALESCE(mr.average_meal_rate, 0.0),
                  CASE WHEN mr.id IS NOT NULL THEN 1 ELSE 0 END as is_imported
           FROM units u
           LEFT JOIN monthly_reports mr
             ON u.id = mr.unit_id AND mr.report_year = ?1 AND mr.report_month = ?2
           ORDER BY u.name"#,
        rusqlite::params![year, month],
        |row| {
            Ok(WilayaReportProjection {
                unit_id: row.get(0)?,
                unit_name: row.get(1)?,
                total_personnel: row.get(2)?,
                total_guests: row.get(3)?,
                total_cost: row.get(4)?,
                avg_meal_rate: row.get(5)?,
                is_imported: row.get::<_, i32>(6)? != 0,
            })
        },
    )?)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::load_wilaya_reports_projection;

    #[test]
    fn wilaya_projection_returns_ordered_denormalized_rows() {
        let db = crate::db::ConnectionFactory::new_for_test().expect("create test db");
        let now = Utc::now().to_rfc3339();
        let unit_a = Uuid::new_v4().to_string();
        let unit_b = Uuid::new_v4().to_string();

        let unit_repo = crate::repositories::UnitRepository::new(db.executor());
        unit_repo
            .upsert_raw_unit(&unit_b, "U2", "Bravo Unit", "01", &now)
            .expect("insert unit bravo");
        unit_repo
            .upsert_raw_unit(&unit_a, "U1", "Alpha Unit", "01", &now)
            .expect("insert unit alpha");

        let report_repo = crate::repositories::ReportRepository::new(db.executor());
        report_repo
            .upsert_monthly_report_summary(&crate::models::MonthlyReport {
                id: Uuid::new_v4().to_string(),
                unit_id: unit_b.clone(),
                report_year: 2024,
                report_month: 3,
                total_personnel: 77,
                total_guests: 11,
                total_meals: 88,
                total_consumption_value: 1234.5,
                average_meal_rate: 14.0,
                report_count: 4,
                imported_at: Utc::now(),
                imported_by: "tester".to_string(),
                file_hash: None,
            })
            .expect("insert monthly report");

        let rows = load_wilaya_reports_projection(db.executor(), 2024, 3).expect("load projection");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].unit_name, "Alpha Unit");
        assert!(!rows[0].is_imported);
        assert_eq!(rows[0].total_personnel, 0);
        assert_eq!(rows[1].unit_name, "Bravo Unit");
        assert!(rows[1].is_imported);
        assert_eq!(rows[1].total_personnel, 77);
        assert_eq!(rows[1].total_guests, 11);
        assert!((rows[1].total_cost - 1234.5).abs() < 0.0001);
    }
}
