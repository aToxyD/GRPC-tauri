//! Report Models
//!
//! Daily reports, monthly summaries, and consumption tracking

use chrono::Datelike;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::models::product::Product;

/// Daily consumption report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReport {
    pub id: String,
    pub date: NaiveDate,
    pub personnel_count: i32,
    pub guest_count: i32,
    pub total_meals_cost: f64,
    pub actual_meal_rate: f64,
    pub unit_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub fiscal_year: i32,
}

impl DailyReport {
    /// Calculate total meals served
    pub fn total_meals(&self) -> i32 {
        self.personnel_count + self.guest_count
    }

    /// Check if report has any consumption
    pub fn has_consumption(&self) -> bool {
        self.total_meals() > 0
    }

    /// Get cost per meal (if any meals served)
    pub fn cost_per_meal(&self) -> Option<f64> {
        let meals = self.total_meals();
        if meals > 0 {
            Some(self.total_meals_cost / meals as f64)
        } else {
            None
        }
    }
}

/// Individual consumption item in a daily report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyConsumptionItem {
    pub id: String,
    pub daily_report_id: String,
    pub product_id: String,
    pub product_name: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub total_cost: f64,
}

/// Input for daily consumption report creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyConsumptionInput {
    pub date: NaiveDate,
    pub personnel_count: i32,
    pub guest_count: i32,
    pub items: Vec<ConsumptionItemInput>,
}

/// Single consumption item input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumptionItemInput {
    pub product_id: String,
    pub quantity: f64,
}

impl ConsumptionItemInput {
    /// Calculate cost with given product
    pub fn calculate_cost(&self, product: &Product) -> f64 {
        self.quantity * product.base_price
    }
}

/// Daily report with its items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportResult {
    pub report: DailyReport,
    pub items: Vec<DailyConsumptionItem>,
}

/// Monthly summary of consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlySummary {
    pub month: i32,
    pub year: i32,
    pub total_personnel: i32,
    pub total_guests: i32,
    pub total_meals: i32,
    pub total_consumption_value: f64,
    pub average_meal_rate: f64,
    pub report_count: i32,
}

impl MonthlySummary {
    /// Create a summary from daily reports
    pub fn from_reports(reports: &[DailyReport]) -> Option<Self> {
        if reports.is_empty() {
            return None;
        }

        let first_date = reports.first()?.date;
        let month = first_date.month() as i32;
        let year = first_date.year();

        let total_personnel: i32 = reports.iter().map(|r| r.personnel_count).sum();
        let total_guests: i32 = reports.iter().map(|r| r.guest_count).sum();
        let total_meals = total_personnel + total_guests;
        let total_consumption_value: f64 = reports.iter().map(|r| r.total_meals_cost).sum();

        let average_meal_rate = if total_meals > 0 {
            total_consumption_value / total_meals as f64
        } else {
            0.0
        };

        Some(Self {
            month,
            year,
            total_personnel,
            total_guests,
            total_meals,
            total_consumption_value,
            average_meal_rate,
            report_count: reports.len() as i32,
        })
    }

    /// Format meal rate as currency
    pub fn formatted_meal_rate(&self) -> String {
        format!("{:.2} DA", self.average_meal_rate)
    }

    /// Format total consumption as currency
    pub fn formatted_total(&self) -> String {
        format!("{:.2} DA", self.total_consumption_value)
    }
}

/// Monthly report imported from UNIT (stored as-is from sync package)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyReport {
    pub id: String,
    pub unit_id: String,
    pub report_year: i32,
    pub report_month: i32,
    pub total_personnel: i32,
    pub total_guests: i32,
    pub total_meals: i32,
    pub total_consumption_value: f64,
    pub average_meal_rate: f64,
    pub report_count: i32,
    pub imported_at: DateTime<Utc>,
    pub imported_by: String,
    pub file_hash: Option<String>,
}

/// System metrics report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub database_size: u64,
    pub backup_count: u32,
    pub uptime: u64,
    pub memory_usage: u64,
    pub total_products: u32,
    pub units_count: u32,

    pub last_backup: Option<String>,
    pub daily_reports: u32,
    pub monthly_reports: u32,
    pub today_orders: u32,
    pub active_users: u32,
}

/// Wilaya report list enum for flexible reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WilayaReportList {
    Daily(Vec<DailyReport>),
    Monthly(Vec<MonthlySummary>),
    Stock(Vec<crate::models::inventory::StockMovement>),
}

/// Summary report for all units in a Wilaya
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WilayaReportSummary {
    pub year: i32,
    pub month: i32,
    pub reports: Vec<WilayaUnitReport>,
}

/// Individual unit status in a Wilaya report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WilayaUnitReport {
    pub unit_id: String,
    pub unit_name: String,
    pub total_personnel: i32,
    pub total_guests: i32,
    pub total_cost: f64,
    pub avg_meal_rate: f64,
    pub is_imported: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_daily_report_total_meals() {
        let report = DailyReport {
            personnel_count: 100,
            guest_count: 20,
            ..Default::default()
        };
        assert_eq!(report.total_meals(), 120);
    }

    #[test]
    fn test_monthly_summary_calculation() {
        let reports = vec![
            DailyReport {
                date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                personnel_count: 100,
                guest_count: 10,
                total_meals_cost: 5000.0,
                ..Default::default()
            },
            DailyReport {
                date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                personnel_count: 100,
                guest_count: 20,
                total_meals_cost: 6000.0,
                ..Default::default()
            },
        ];

        let summary = MonthlySummary::from_reports(&reports).unwrap();
        assert_eq!(summary.total_meals, 230);
        assert_eq!(summary.total_consumption_value, 11000.0);
        assert_eq!(summary.report_count, 2);
    }
}

impl Default for DailyReport {
    fn default() -> Self {
        Self {
            id: String::new(),
            date: Utc::now().date_naive(),
            personnel_count: 0,
            guest_count: 0,
            total_meals_cost: 0.0,
            actual_meal_rate: 0.0,
            unit_id: None,
            created_at: Utc::now(),
            fiscal_year: Utc::now().year(),
        }
    }
}

/// Result of daily/monthly report import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportImportResult {
    pub report_count: i32,
    pub item_count: i32,
    pub unit_id: Option<String>,
    pub file_hash: String,
    pub imported_by: String,
    pub timestamp: String,
}
