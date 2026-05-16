//! Audit Models (Re-exports from Domain)
//!
//! This module re-exports audit types from the domain layer to the models layer
//! for use in DTOs and frontend communication.

pub use crate::domain::audit::{
    AuditAction, AuditEntry, AuditFilters, AuditLogResponse, AuditStats, AuditStatus,
    DailyOperationCount, EntityType, OperationCount, UserActivitySummary,
};

use serde::{Deserialize, Serialize};

/// User activity summary (UI-specific if different from domain)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserActivity {
    pub user_id: String,
    pub username: String,
    pub login_count: i32,
    pub last_login: Option<String>,
    pub actions_today: i32,
    pub actions_this_week: i32,
}

/// Backup audit info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupAuditInfo {
    pub id: String,
    pub created_at: String,
    pub size: u64,
    pub path: String,
    pub hash: String,
}
