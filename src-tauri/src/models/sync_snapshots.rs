//! Protocol-agnostic read models for sync snapshots.
//!
//! These types track **interchange schema**, not domain invariants; they may evolve independently of
//! `DailyReport` / aggregates in `report.rs`.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyConsumptionSyncLine {
    pub product_id: String,
    pub product_name: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub total_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportSyncSnapshot {
    pub report_id: String,
    pub date: NaiveDate,
    pub personnel_count: i32,
    pub guest_count: i32,
    pub total_meals_cost: f64,
    pub actual_meal_rate: f64,
    pub items: Vec<DailyConsumptionSyncLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyDetailSyncSnapshot {
    pub date: NaiveDate,
    pub personnel_count: i32,
    pub guest_count: i32,
    pub total_meals_cost: f64,
    pub actual_meal_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductSyncRecord {
    pub id: String,
    pub name: String,
    pub base_price: f64,
    pub tva: f64,
    pub supplier_name: Option<String>,
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
