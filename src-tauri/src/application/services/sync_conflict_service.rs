//! Sync Conflict Intelligence Service
//!
//! Detects, records and manages sync conflicts between nodes.
//! Implements conflict detection logic without touching business data.

use crate::errors::AppError;
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Domain Types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConflictType {
    StaleImport,
    DuplicatePackage,
    OutOfOrderReport,
    DivergentStockState,
    ConflictingInventoryMutation,
    ReplayAttempt,
    UnknownSourceNode,
}

impl ConflictType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StaleImport => "STALE_IMPORT",
            Self::DuplicatePackage => "DUPLICATE_PACKAGE",
            Self::OutOfOrderReport => "OUT_OF_ORDER_REPORT",
            Self::DivergentStockState => "DIVERGENT_STOCK_STATE",
            Self::ConflictingInventoryMutation => "CONFLICTING_INVENTORY_MUTATION",
            Self::ReplayAttempt => "REPLAY_ATTEMPT",
            Self::UnknownSourceNode => "UNKNOWN_SOURCE_NODE",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::StaleImport => "استيراد قديم",
            Self::DuplicatePackage => "حزمة مكررة",
            Self::OutOfOrderReport => "تقرير خارج الترتيب",
            Self::DivergentStockState => "تباين في حالة المخزون",
            Self::ConflictingInventoryMutation => "تعارض في تحديث المخزون",
            Self::ReplayAttempt => "محاولة إعادة تشغيل",
            Self::UnknownSourceNode => "عقدة مصدر غير معروفة",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConflictSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl ConflictSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
            Self::Critical => "CRITICAL",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictResolutionSuggestion {
    pub action: String,
    pub description: String,
    pub requires_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub id: String,
    pub package_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub conflict_type: String,
    pub conflict_type_display: String,
    pub severity: String,
    pub description: String,
    pub suggested_resolution: Option<ConflictResolutionSuggestion>,
    pub resolved: bool,
    pub resolution_note: Option<String>,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictSummary {
    pub total: i64,
    pub unresolved: i64,
    pub by_severity: Vec<SeverityCount>,
    pub by_type: Vec<TypeCount>,
    pub recent_conflicts: Vec<SyncConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeverityCount {
    pub severity: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeCount {
    pub conflict_type: String,
    pub conflict_type_display: String,
    pub count: i64,
}

// ─── Service ─────────────────────────────────────────────────────────────────

pub struct SyncConflictService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> SyncConflictService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Record a new conflict detected during sync/import operations.
    #[allow(clippy::too_many_arguments)]
    pub fn record_conflict(
        &self,
        package_id: &str,
        source_node_id: &str,
        target_node_id: &str,
        conflict_type: ConflictType,
        severity: ConflictSeverity,
        description: &str,
        suggested_resolution: Option<ConflictResolutionSuggestion>,
    ) -> Result<String, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let resolution_json = suggested_resolution
            .as_ref()
            .map(|r| serde_json::to_string(r).unwrap_or_default());

        self.executor.sync_conflicts().insert_conflict(
            &id,
            package_id,
            source_node_id,
            target_node_id,
            conflict_type.as_str(),
            severity.as_str(),
            description,
            resolution_json.as_deref(),
            &now,
        )?;

        Ok(id)
    }

    /// List all conflicts with optional filter for unresolved only.
    pub fn list_conflicts(&self, unresolved_only: bool) -> Result<Vec<SyncConflict>, AppError> {
        self.executor
            .sync_conflicts()
            .list_conflicts(unresolved_only)
            .map(|rows| rows.into_iter().map(row_to_conflict).collect())
    }

    /// Get a summary of conflict statistics.
    pub fn get_conflict_summary(&self) -> Result<ConflictSummary, AppError> {
        let repo = self.executor.sync_conflicts();
        let total = repo.count_total()?;
        let unresolved = repo.count_unresolved()?;
        let by_severity_rows = repo.count_by_severity()?;
        let by_type_rows = repo.count_by_type()?;
        let recent = repo.list_recent(10)?;

        let by_severity = by_severity_rows
            .into_iter()
            .map(|(sev, count)| SeverityCount {
                severity: sev,
                count,
            })
            .collect();

        let by_type = by_type_rows
            .into_iter()
            .map(|(ctype, count)| {
                let display = conflict_type_display(&ctype);
                TypeCount {
                    conflict_type: ctype,
                    conflict_type_display: display.to_string(),
                    count,
                }
            })
            .collect();

        let recent_conflicts = recent.into_iter().map(row_to_conflict).collect();

        Ok(ConflictSummary {
            total,
            unresolved,
            by_severity,
            by_type,
            recent_conflicts,
        })
    }

    /// Mark a conflict as resolved with a note.
    pub fn resolve_conflict(
        &self,
        conflict_id: &str,
        resolved_by: &str,
        note: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        self.executor
            .sync_conflicts()
            .resolve_conflict(conflict_id, resolved_by, note, &now)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn conflict_type_display(ctype: &str) -> &'static str {
    match ctype {
        "STALE_IMPORT" => "استيراد قديم",
        "DUPLICATE_PACKAGE" => "حزمة مكررة",
        "OUT_OF_ORDER_REPORT" => "تقرير خارج الترتيب",
        "DIVERGENT_STOCK_STATE" => "تباين في حالة المخزون",
        "CONFLICTING_INVENTORY_MUTATION" => "تعارض في تحديث المخزون",
        "REPLAY_ATTEMPT" => "محاولة إعادة تشغيل",
        "UNKNOWN_SOURCE_NODE" => "عقدة مصدر غير معروفة",
        _ => "تعارض غير معروف",
    }
}

fn row_to_conflict(r: crate::repositories::sync_conflicts::SyncConflictRow) -> SyncConflict {
    let suggested_resolution = r
        .suggested_resolution
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());

    let display = conflict_type_display(&r.conflict_type).to_string();

    SyncConflict {
        id: r.id,
        package_id: r.package_id,
        source_node_id: r.source_node_id,
        target_node_id: r.target_node_id,
        conflict_type_display: display,
        conflict_type: r.conflict_type,
        severity: r.severity,
        description: r.description,
        suggested_resolution,
        resolved: r.resolved,
        resolution_note: r.resolution_note,
        resolved_by: r.resolved_by,
        resolved_at: r.resolved_at,
        created_at: r.created_at,
    }
}
