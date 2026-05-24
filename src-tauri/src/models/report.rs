//! Report Models — one daily report containing breakfast, lunch, and dinner sections

use chrono::Datelike;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::models::product::Product;

/// Meal type (section within a daily report)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MealType {
    Breakfast,
    Lunch,
    Dinner,
}

impl MealType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MealType::Breakfast => "breakfast",
            MealType::Lunch => "lunch",
            MealType::Dinner => "dinner",
        }
    }

    pub fn from_str_custom(s: &str) -> Option<Self> {
        match s {
            "breakfast" => Some(MealType::Breakfast),
            "lunch" => Some(MealType::Lunch),
            "dinner" => Some(MealType::Dinner),
            _ => None,
        }
    }

    pub fn all() -> [MealType; 3] {
        [MealType::Breakfast, MealType::Lunch, MealType::Dinner]
    }
}

/// Daily consumption report header (one per date + unit)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReport {
    pub id: String,
    pub date: NaiveDate,
    pub unit_id: Option<String>,
    pub total_daily_cost: f64,
    pub total_daily_average: f64,
    pub total_daily_beneficiaries: i32,
    pub created_at: DateTime<Utc>,
    pub fiscal_year: i32,
}

/// Meal section within a daily report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportMeal {
    pub id: String,
    pub daily_report_id: String,
    pub meal_type: MealType,
    pub staff_24h_count: i32,
    pub staff_8h_count: i32,
    pub reservation_count: i32,
    pub mission_count: i32,
    pub guest_count: i32,
    pub total_beneficiaries: i32,
    pub total_meal_cost: f64,
    pub meal_average: f64,
}

/// Product line on a meal section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportMealItem {
    pub id: String,
    pub meal_id: String,
    pub product_id: String,
    pub product_name: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub total_cost: f64,
    pub fifo_layer_id: Option<String>,
}

pub struct BeneficiaryCounts {
    pub staff_24h: i32,
    pub staff_8h: i32,
    pub reservation: i32,
    pub mission: i32,
    pub guest: i32,
}

impl BeneficiaryCounts {
    pub fn total(&self) -> i32 {
        self.staff_24h + self.staff_8h + self.reservation + self.mission + self.guest
    }
}

impl DailyReportMeal {
    pub fn compute_total_beneficiaries(
        staff_24h: i32,
        staff_8h: i32,
        reservation: i32,
        mission: i32,
        guest: i32,
    ) -> i32 {
        staff_24h + staff_8h + reservation + mission + guest
    }

    pub fn compute_meal_average(total_cost: f64, total_beneficiaries: i32) -> f64 {
        if total_beneficiaries > 0 {
            let avg = total_cost / total_beneficiaries as f64;
            (avg * 100.0).round() / 100.0
        } else {
            0.0
        }
    }
}

/// Input for one meal section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MealSectionInput {
    pub meal_type: MealType,
    pub staff_24h_count: i32,
    pub staff_8h_count: i32,
    pub reservation_count: i32,
    pub mission_count: i32,
    pub guest_count: i32,
    pub items: Vec<ConsumptionItemInput>,
}

/// Input for creating a full daily report (all meal sections)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportInput {
    pub date: NaiveDate,
    pub meals: Vec<MealSectionInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumptionItemInput {
    pub product_id: String,
    pub quantity: f64,
}

impl ConsumptionItemInput {
    pub fn calculate_cost(&self, product: &Product) -> f64 {
        self.quantity * product.base_price
    }
}

/// Meal section with items (read model)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MealSectionResult {
    pub meal: DailyReportMeal,
    pub items: Vec<DailyReportMealItem>,
}

/// Full daily report with all meal sections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportResult {
    pub report: DailyReport,
    pub meals: Vec<MealSectionResult>,
}

impl DailyReportResult {
    pub fn daily_summary(&self) -> DailyConsumptionSummary {
        DailyConsumptionSummary::from_meal_sections(&self.meals)
    }
}

/// Denormalized daily summary for UI / sync
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DailyConsumptionSummary {
    pub breakfast_beneficiaries: i32,
    pub lunch_beneficiaries: i32,
    pub dinner_beneficiaries: i32,
    pub breakfast_cost: f64,
    pub lunch_cost: f64,
    pub dinner_cost: f64,
    pub breakfast_average: f64,
    pub lunch_average: f64,
    pub dinner_average: f64,
    pub total_daily_beneficiaries: i32,
    pub total_daily_cost: f64,
    pub daily_average: f64,
}

impl DailyConsumptionSummary {
    pub fn from_meal_sections(meals: &[MealSectionResult]) -> Self {
        let mut s = Self::default();
        for m in meals {
            match m.meal.meal_type {
                MealType::Breakfast => {
                    s.breakfast_beneficiaries = m.meal.total_beneficiaries;
                    s.breakfast_cost = m.meal.total_meal_cost;
                    s.breakfast_average = m.meal.meal_average;
                }
                MealType::Lunch => {
                    s.lunch_beneficiaries = m.meal.total_beneficiaries;
                    s.lunch_cost = m.meal.total_meal_cost;
                    s.lunch_average = m.meal.meal_average;
                }
                MealType::Dinner => {
                    s.dinner_beneficiaries = m.meal.total_beneficiaries;
                    s.dinner_cost = m.meal.total_meal_cost;
                    s.dinner_average = m.meal.meal_average;
                }
            }
        }
        s.total_daily_beneficiaries =
            s.breakfast_beneficiaries + s.lunch_beneficiaries + s.dinner_beneficiaries;
        s.total_daily_cost = s.breakfast_cost + s.lunch_cost + s.dinner_cost;
        s.daily_average = s.breakfast_average + s.lunch_average + s.dinner_average;
        s
    }

    pub fn from_report_result(result: &DailyReportResult) -> Self {
        let mut s = Self::from_meal_sections(&result.meals);
        s.total_daily_beneficiaries = result.report.total_daily_beneficiaries;
        s.total_daily_cost = result.report.total_daily_cost;
        s.daily_average = result.report.total_daily_average;
        s
    }
}

/// Alias for date-based fetch (same as DailyReportResult)
pub type DailyConsumptionView = DailyReportResult;

// Legacy aliases
pub type MealConsumption = DailyReportMeal;
pub type MealConsumptionItem = DailyReportMealItem;
pub type MealConsumptionInput = MealSectionInput;
pub type MealConsumptionResult = MealSectionResult;
pub type DailyConsumptionInput = DailyReportInput;

/// Monthly summary of consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlySummary {
    pub month: i32,
    pub year: i32,
    pub total_beneficiaries: i32,
    pub total_consumption_value: f64,
    pub breakfast_average: f64,
    pub lunch_average: f64,
    pub dinner_average: f64,
    pub daily_average: f64,
    pub report_count: i32,
}

impl MonthlySummary {
    pub fn formatted_daily_average(&self) -> String {
        format!("{:.2} DA", self.daily_average)
    }

    pub fn formatted_total(&self) -> String {
        format!("{:.2} DA", self.total_consumption_value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyReport {
    pub id: String,
    pub unit_id: String,
    pub report_year: i32,
    pub report_month: i32,
    pub total_beneficiaries: i32,
    pub total_consumption_value: f64,
    pub breakfast_average: f64,
    pub lunch_average: f64,
    pub dinner_average: f64,
    pub daily_average: f64,
    pub report_count: i32,
    pub imported_at: DateTime<Utc>,
    pub imported_by: String,
    pub file_hash: Option<String>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WilayaReportList {
    Daily(Vec<DailyReport>),
    Monthly(Vec<MonthlySummary>),
    Stock(Vec<crate::models::inventory::StockMovement>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WilayaReportSummary {
    pub year: i32,
    pub month: i32,
    pub reports: Vec<WilayaUnitReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WilayaUnitReport {
    pub unit_id: String,
    pub unit_name: String,
    pub total_beneficiaries: i32,
    pub total_cost: f64,
    pub daily_average: f64,
    pub is_imported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportImportResult {
    pub report_count: i32,
    pub item_count: i32,
    pub unit_id: Option<String>,
    pub file_hash: String,
    pub imported_by: String,
    pub timestamp: String,
}

impl Default for DailyReportMeal {
    fn default() -> Self {
        Self {
            id: String::new(),
            daily_report_id: String::new(),
            meal_type: MealType::Breakfast,
            staff_24h_count: 0,
            staff_8h_count: 0,
            reservation_count: 0,
            mission_count: 0,
            guest_count: 0,
            total_beneficiaries: 0,
            total_meal_cost: 0.0,
            meal_average: 0.0,
        }
    }
}

impl Default for DailyReport {
    fn default() -> Self {
        Self {
            id: String::new(),
            date: Utc::now().date_naive(),
            unit_id: None,
            total_daily_cost: 0.0,
            total_daily_average: 0.0,
            total_daily_beneficiaries: 0,
            created_at: Utc::now(),
            fiscal_year: Utc::now().year(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beneficiary_sum() {
        assert_eq!(
            DailyReportMeal::compute_total_beneficiaries(1, 2, 3, 4, 5),
            15
        );
    }

    #[test]
    fn test_daily_summary_from_sections() {
        let meals = vec![
            MealSectionResult {
                meal: DailyReportMeal {
                    meal_type: MealType::Breakfast,
                    total_beneficiaries: 10,
                    total_meal_cost: 100.0,
                    meal_average: 10.0,
                    ..Default::default()
                },
                items: vec![],
            },
            MealSectionResult {
                meal: DailyReportMeal {
                    meal_type: MealType::Lunch,
                    total_beneficiaries: 20,
                    total_meal_cost: 300.0,
                    meal_average: 15.0,
                    ..Default::default()
                },
                items: vec![],
            },
        ];
        let s = DailyConsumptionSummary::from_meal_sections(&meals);
        assert_eq!(s.breakfast_cost, 100.0);
        assert_eq!(s.lunch_cost, 300.0);
        assert_eq!(s.daily_average, 25.0);
        assert_eq!(s.total_daily_beneficiaries, 30);
    }
}
