//! Sync interchange models for daily reports (one report, meals[])

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::models::MealType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyConsumptionSyncLine {
    pub product_id: String,
    pub product_name: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub total_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MealSectionSyncSnapshot {
    pub meal_type: MealType,
    pub staff_24h_count: i32,
    pub staff_8h_count: i32,
    pub reservation_count: i32,
    pub mission_count: i32,
    pub guest_count: i32,
    pub total_beneficiaries: i32,
    pub total_meal_cost: f64,
    pub meal_average: f64,
    pub items: Vec<DailyConsumptionSyncLine>,
}

/// One daily consumption report for sync (official operational document)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportSyncSnapshot {
    pub report_id: String,
    pub date: NaiveDate,
    pub total_daily_cost: f64,
    pub total_daily_average: f64,
    pub total_daily_beneficiaries: i32,
    pub meals: Vec<MealSectionSyncSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyDetailSyncSnapshot {
    pub date: NaiveDate,
    pub total_daily_beneficiaries: i32,
    pub total_daily_cost: f64,
    pub breakfast_average: f64,
    pub lunch_average: f64,
    pub dinner_average: f64,
    pub daily_average: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductSyncRecord {
    pub id: String,
    pub name: String,
    pub base_price: f64,
    pub year: i32,
    pub created_at: String,
    pub updated_at: String,
    pub node_id: String,
    pub deleted: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportAuditEvent {
    pub id: i64,
    pub event_type: String,
    pub package_id: String,
    pub package_kind: String,
    pub source_node_id: Option<String>,
    pub reason_code: Option<String>,
    pub occurred_at: String,
}
