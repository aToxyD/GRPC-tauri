//! Read-side SQL for daily reports (parent + meal sections)

use chrono::{Datelike, NaiveDate};

use crate::domain::numeric::legacy_float;
use crate::domain::numeric::Money;
use crate::errors::{AppError, ValidationError};
use crate::models::{DailyReport, DailyReportMeal, MealType};
use crate::repositories::numeric_row;
use crate::repositories::DbExecutor;

#[derive(Clone, Copy, Debug)]
pub struct MonthlyWindow {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

#[derive(Clone, Debug)]
pub struct MonthlySummaryDailyRowProjection {
    pub date: NaiveDate,
    pub total_daily_cost: f64,
    pub total_daily_beneficiaries: i32,
    pub breakfast_average: f64,
    pub lunch_average: f64,
    pub dinner_average: f64,
    pub daily_average: f64,
}

#[derive(Clone, Debug)]
pub struct MonthlySummaryProjection {
    pub year: i32,
    pub month: i32,
    pub total_cost: f64,
    pub total_beneficiaries: i32,
    pub breakfast_average: f64,
    pub lunch_average: f64,
    pub dinner_average: f64,
    pub daily_average: f64,
    pub report_count: i32,
    pub daily_rows: Vec<MonthlySummaryDailyRowProjection>,
}

#[derive(Clone, Debug)]
pub struct WilayaReportProjection {
    pub unit_id: String,
    pub unit_name: String,
    pub total_beneficiaries: i32,
    pub total_cost: f64,
    pub daily_average: f64,
    pub is_imported: bool,
}

/// Returns the full calendar-year window for a given fiscal year.
/// Fiscal years are calendar-aligned (Jan 1 – Dec 31).
pub fn fiscal_year_window(year: i32) -> Result<MonthlyWindow, AppError> {
    let start = NaiveDate::from_ymd_opt(year, 1, 1).ok_or_else(|| {
        AppError::Validation(ValidationError::OutOfRange {
            field: "year".to_string(),
            value: year.to_string(),
        })
    })?;
    let end = NaiveDate::from_ymd_opt(year, 12, 31).ok_or_else(|| {
        AppError::Validation(ValidationError::OutOfRange {
            field: "year".to_string(),
            value: year.to_string(),
        })
    })?;
    Ok(MonthlyWindow { start, end })
}

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

const REPORT_SELECT: &str =
    "SELECT id, date, unit_id, total_daily_cost, total_daily_average, total_daily_beneficiaries, created_at, fiscal_year FROM daily_reports";

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
        total_daily_cost: numeric_row::money_col(3, row.get::<_, i64>(3)?)?,
        total_daily_average: row.get(4)?,
        total_daily_beneficiaries: row.get(5)?,
        created_at,
        fiscal_year: row.get(7)?,
    })
}

const MEAL_SELECT: &str = "SELECT id, daily_report_id, meal_type, staff_24h_count, staff_8h_count, reservation_count, mission_count, guest_count, total_beneficiaries, total_meal_cost, meal_average FROM daily_report_meals";

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
        total_meal_cost: numeric_row::money_col(9, row.get::<_, i64>(9)?)?,
        meal_average: row.get(10)?,
    })
}

pub fn list_daily_reports_by_month(
    executor: DbExecutor<'_>,
    year: i32,
    month: u32,
    unit_id: Option<&str>,
) -> Result<Vec<DailyReport>, AppError> {
    let win = monthly_window(year, month)?;
    let sql = format!(
        "{REPORT_SELECT} WHERE date >= ?1 AND date <= ?2 AND (?3 IS NULL OR unit_id = ?3) ORDER BY date ASC"
    );
    Ok(executor.query_all(
        &sql,
        rusqlite::params![win.start.to_string(), win.end.to_string(), unit_id],
        map_report_row,
    )?)
}

pub fn count_daily_reports_by_month(
    executor: DbExecutor<'_>,
    year: i32,
    month: u32,
    unit_id: Option<&str>,
) -> Result<u32, AppError> {
    let win = monthly_window(year, month)?;
    let count: i64 = executor.query_row(
        "SELECT COUNT(*) FROM daily_reports WHERE date >= ?1 AND date <= ?2 AND (?3 IS NULL OR unit_id = ?3)",
        rusqlite::params![win.start.to_string(), win.end.to_string(), unit_id],
        |row| row.get(0),
    )?;
    Ok(count as u32)
}

/// Enforce the SEC-057 monthly completeness gate: a monthly summary for
/// `(year, month)` — scoped to `unit_id` when `Some` — is admissible ONLY for
/// a full calendar month (every day of the month has a daily report). A
/// partial month is rejected on both the UNIT export path and the WILAYA
/// import path, so a monthly total can never be silently built from
/// incomplete daily data.
pub fn assert_complete_calendar_month(
    executor: DbExecutor<'_>,
    year: i32,
    month: u32,
    unit_id: Option<&str>,
) -> Result<(), AppError> {
    let win = monthly_window(year, month)?;
    let expected_days = win.end.day();
    let actual = count_daily_reports_by_month(executor, year, month, unit_id)?;
    if actual != expected_days {
        return Err(AppError::Validation(
            crate::errors::ValidationError::InvalidFormat {
                field: "month".into(),
                message: format!(
                    "البيانات الشهرية غير مكتملة: يتطلب شهرًا كاملًا ({expected_days} يومًا) لكن وُجد {actual} تقريرًا يوميًا ({year:04}-{month:02})"
                ),
            },
        ));
    }
    Ok(())
}

pub fn load_monthly_summary_projection(
    executor: DbExecutor<'_>,
    window: MonthlyWindow,
    unit_id: Option<&str>,
) -> Result<MonthlySummaryProjection, AppError> {
    let start_date = window.start.to_string();
    let end_date = window.end.to_string();

    let reports = executor.query_all(
        &format!(
            "{REPORT_SELECT} WHERE date >= ?1 AND date <= ?2 AND (?3 IS NULL OR unit_id = ?3)"
        ),
        rusqlite::params![start_date, end_date, unit_id],
        map_report_row,
    )?;

    // Authoritative monthly consumption value: aggregate the per-day Money
    // values exactly (no accounting arithmetic in f64), convert once at the
    // wire boundary. Each per-day value is already a cent-scale wire f64.
    let mut total_cost = Money::zero();
    for r in &reports {
        let daily = legacy_float::money_from_f64(r.total_daily_cost)?;
        total_cost = total_cost.checked_add(daily)?;
    }
    let total_cost = legacy_float::money_to_f64(&total_cost)?;
    let total_beneficiaries: i32 = reports.iter().map(|r| r.total_daily_beneficiaries).sum();

    let mut breakfast_avgs: Vec<f64> = Vec::new();
    let mut lunch_avgs: Vec<f64> = Vec::new();
    let mut dinner_avgs: Vec<f64> = Vec::new();

    let mut daily_rows = Vec::with_capacity(reports.len());

    for report in &reports {
        let meals = executor.query_all(
            &format!("{MEAL_SELECT} WHERE daily_report_id = ?1"),
            [&report.id],
            map_meal_row,
        )?;

        let mut breakfast_average = 0.0;
        let mut lunch_average = 0.0;
        let mut dinner_average = 0.0;

        for m in &meals {
            match m.meal_type {
                MealType::Breakfast => breakfast_average = m.meal_average,
                MealType::Lunch => lunch_average = m.meal_average,
                MealType::Dinner => dinner_average = m.meal_average,
            }
        }

        breakfast_avgs.push(breakfast_average);
        lunch_avgs.push(lunch_average);
        dinner_avgs.push(dinner_average);

        daily_rows.push(MonthlySummaryDailyRowProjection {
            date: report.date,
            total_daily_cost: report.total_daily_cost,
            total_daily_beneficiaries: report.total_daily_beneficiaries,
            breakfast_average,
            lunch_average,
            dinner_average,
            daily_average: report.total_daily_average,
        });
    }

    let avg = |v: &[f64]| -> f64 {
        if v.is_empty() {
            0.0
        } else {
            v.iter().sum::<f64>() / v.len() as f64
        }
    };

    let breakfast_average = avg(&breakfast_avgs);
    let lunch_average = avg(&lunch_avgs);
    let dinner_average = avg(&dinner_avgs);
    let daily_average = breakfast_average + lunch_average + dinner_average;

    Ok(MonthlySummaryProjection {
        year: window.start.year(),
        month: window.start.month() as i32,
        total_cost,
        total_beneficiaries,
        breakfast_average,
        lunch_average,
        dinner_average,
        daily_average,
        report_count: reports.len() as i32,
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
                  COALESCE(mr.total_beneficiaries, 0),
                  COALESCE(mr.total_consumption_value, 0),
                  COALESCE(mr.daily_average, 0.0),
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
                total_beneficiaries: row.get(2)?,
                total_cost: numeric_row::money_col(3, row.get::<_, i64>(3)?)?,
                daily_average: row.get(4)?,
                is_imported: row.get::<_, i32>(5)? != 0,
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
                total_beneficiaries: 88,
                total_consumption_value: 1234.5,
                breakfast_average: 10.0,
                lunch_average: 12.0,
                dinner_average: 14.0,
                daily_average: 36.0,
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
        assert_eq!(rows[1].unit_name, "Bravo Unit");
        assert!(rows[1].is_imported);
    }
}
