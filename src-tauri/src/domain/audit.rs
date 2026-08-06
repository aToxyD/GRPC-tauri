//! Audit Trail System for GRPC Application
//!
//! This module provides comprehensive audit logging for all operations
//! including user actions, data changes, and system events.

use serde::{Deserialize, Serialize};

/// Types of audit actions that can be logged
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuditAction {
    // Auth
    Login,
    LoginFailed,
    Logout,
    PasswordChange,
    UnauthorizedAccess,
    // Products
    CreateProduct,
    UpdateProduct,
    DeleteProduct,
    // Orders
    CreateOrder,
    UpdateOrder,
    ConfirmOrder,
    DeleteOrder,
    // Daily Reports
    CreateDailyReport,
    UpdateDailyReport,
    DeleteDailyReport,
    // Units
    CreateUnit,
    UpdateUnit,
    DeleteUnit,
    ResolveConflict,
    UnitNodeImport,
    // Settings
    UpdateSettings,
    // Backup
    CreateBackup,
    RestoreBackup,
    BackupCheckpointFailed,
    BackupSnapshotValidationFailed,
    BackupSnapshotTooSmall,
    BackupSnapshotIntegrityFailed,
    // Inventory Snapshots
    CreateSnapshot,
    // Users
    CreateUser,
    UpdateUser,
    // Test/Generic
    Create,
    // Granular Imports (for new services)
    ImportProducts,
    ImportDailyReport,
    ImportMonthlyReport,
    ImportStockMovements,
    ImportNodePackage,
    ImportTrustPackage,
    ImportRegistryPackage,
    // Fiscal Lifecycle
    FiscalYearOpened,
    FiscalYearClosed,
    FiscalYearArchived,
    OpeningBalancesGenerated,
    CarryForwardExecuted,
    CarryForwardRejected,
    FiscalWriteRejected,
    // Fiscal Closure Package
    FiscalClosurePackageExported,
    FiscalClosurePackageApplied,
    // Identity rotation (B7)
    IdentityRotated,
    IdentityReissued,
}

impl AuditAction {
    /// Convert action to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditAction::Login => "Login",
            AuditAction::LoginFailed => "LoginFailed",
            AuditAction::Logout => "Logout",
            AuditAction::PasswordChange => "PasswordChange",
            AuditAction::UnauthorizedAccess => "UnauthorizedAccess",
            AuditAction::CreateProduct => "CreateProduct",
            AuditAction::UpdateProduct => "UpdateProduct",
            AuditAction::DeleteProduct => "DeleteProduct",
            AuditAction::CreateOrder => "CreateOrder",
            AuditAction::UpdateOrder => "UpdateOrder",
            AuditAction::ConfirmOrder => "ConfirmOrder",
            AuditAction::DeleteOrder => "DeleteOrder",
            AuditAction::CreateDailyReport => "CreateDailyReport",
            AuditAction::UpdateDailyReport => "UpdateDailyReport",
            AuditAction::DeleteDailyReport => "DeleteDailyReport",
            AuditAction::CreateUnit => "CreateUnit",
            AuditAction::UpdateUnit => "UpdateUnit",
            AuditAction::DeleteUnit => "DeleteUnit",
            AuditAction::ResolveConflict => "ResolveConflict",
            AuditAction::UnitNodeImport => "UnitNodeImport",
            AuditAction::UpdateSettings => "UpdateSettings",
            AuditAction::CreateBackup => "CreateBackup",
            AuditAction::RestoreBackup => "RestoreBackup",
            AuditAction::BackupCheckpointFailed => "BackupCheckpointFailed",
            AuditAction::BackupSnapshotValidationFailed => "BackupSnapshotValidationFailed",
            AuditAction::BackupSnapshotTooSmall => "BackupSnapshotTooSmall",
            AuditAction::BackupSnapshotIntegrityFailed => "BackupSnapshotIntegrityFailed",
            AuditAction::CreateSnapshot => "CreateSnapshot",
            AuditAction::CreateUser => "CreateUser",
            AuditAction::UpdateUser => "UpdateUser",
            AuditAction::Create => "Create",
            AuditAction::ImportProducts => "ImportProducts",
            AuditAction::ImportDailyReport => "ImportDailyReport",
            AuditAction::ImportMonthlyReport => "ImportMonthlyReport",
            AuditAction::ImportStockMovements => "ImportStockMovements",
            AuditAction::ImportNodePackage => "ImportNodePackage",
            AuditAction::ImportTrustPackage => "ImportTrustPackage",
            AuditAction::ImportRegistryPackage => "ImportRegistryPackage",
            AuditAction::FiscalYearOpened => "FiscalYearOpened",
            AuditAction::FiscalYearClosed => "FiscalYearClosed",
            AuditAction::FiscalYearArchived => "FiscalYearArchived",
            AuditAction::OpeningBalancesGenerated => "OpeningBalancesGenerated",
            AuditAction::CarryForwardExecuted => "CarryForwardExecuted",
            AuditAction::CarryForwardRejected => "CarryForwardRejected",
            AuditAction::FiscalWriteRejected => "FiscalWriteRejected",
            AuditAction::FiscalClosurePackageExported => "FiscalClosurePackageExported",
            AuditAction::FiscalClosurePackageApplied => "FiscalClosurePackageApplied",
            AuditAction::IdentityRotated => "IdentityRotated",
            AuditAction::IdentityReissued => "IdentityReissued",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Login" => Some(AuditAction::Login),
            "LoginFailed" => Some(AuditAction::LoginFailed),
            "Logout" => Some(AuditAction::Logout),
            "PasswordChange" => Some(AuditAction::PasswordChange),
            "UnauthorizedAccess" => Some(AuditAction::UnauthorizedAccess),
            "CreateProduct" => Some(AuditAction::CreateProduct),
            "UpdateProduct" => Some(AuditAction::UpdateProduct),
            "DeleteProduct" => Some(AuditAction::DeleteProduct),
            "CreateOrder" => Some(AuditAction::CreateOrder),
            "UpdateOrder" => Some(AuditAction::UpdateOrder),
            "ConfirmOrder" => Some(AuditAction::ConfirmOrder),
            "DeleteOrder" => Some(AuditAction::DeleteOrder),
            "CreateDailyReport" => Some(AuditAction::CreateDailyReport),
            "UpdateDailyReport" => Some(AuditAction::UpdateDailyReport),
            "DeleteDailyReport" => Some(AuditAction::DeleteDailyReport),
            "CreateUnit" => Some(AuditAction::CreateUnit),
            "UpdateUnit" => Some(AuditAction::UpdateUnit),
            "DeleteUnit" => Some(AuditAction::DeleteUnit),
            "ResolveConflict" => Some(AuditAction::ResolveConflict),
            "UnitNodeImport" => Some(AuditAction::UnitNodeImport),
            "UpdateSettings" => Some(AuditAction::UpdateSettings),
            "CreateBackup" => Some(AuditAction::CreateBackup),
            "RestoreBackup" => Some(AuditAction::RestoreBackup),
            "BackupCheckpointFailed" => Some(AuditAction::BackupCheckpointFailed),
            "BackupSnapshotValidationFailed" => Some(AuditAction::BackupSnapshotValidationFailed),
            "BackupSnapshotTooSmall" => Some(AuditAction::BackupSnapshotTooSmall),
            "BackupSnapshotIntegrityFailed" => Some(AuditAction::BackupSnapshotIntegrityFailed),
            "CreateSnapshot" => Some(AuditAction::CreateSnapshot),
            "CreateUser" => Some(AuditAction::CreateUser),
            "UpdateUser" => Some(AuditAction::UpdateUser),
            "Create" => Some(AuditAction::Create),
            "ImportProducts" => Some(AuditAction::ImportProducts),
            "ImportDailyReport" => Some(AuditAction::ImportDailyReport),
            "ImportMonthlyReport" => Some(AuditAction::ImportMonthlyReport),
            "ImportStockMovements" => Some(AuditAction::ImportStockMovements),
            "ImportNodePackage" => Some(AuditAction::ImportNodePackage),
            "ImportTrustPackage" => Some(AuditAction::ImportTrustPackage),
            "ImportRegistryPackage" => Some(AuditAction::ImportRegistryPackage),
            "FiscalYearOpened" => Some(AuditAction::FiscalYearOpened),
            "FiscalYearClosed" => Some(AuditAction::FiscalYearClosed),
            "FiscalYearArchived" => Some(AuditAction::FiscalYearArchived),
            "OpeningBalancesGenerated" => Some(AuditAction::OpeningBalancesGenerated),
            "CarryForwardExecuted" => Some(AuditAction::CarryForwardExecuted),
            "CarryForwardRejected" => Some(AuditAction::CarryForwardRejected),
            "FiscalWriteRejected" => Some(AuditAction::FiscalWriteRejected),
            "FiscalClosurePackageExported" => Some(AuditAction::FiscalClosurePackageExported),
            "FiscalClosurePackageApplied" => Some(AuditAction::FiscalClosurePackageApplied),
            "IdentityRotated" => Some(AuditAction::IdentityRotated),
            "IdentityReissued" => Some(AuditAction::IdentityReissued),
            _ => None,
        }
    }

    pub fn display_arabic(&self) -> &'static str {
        match self {
            AuditAction::Login => "تسجيل دخول",
            AuditAction::LoginFailed => "فشل تسجيل الدخول",
            AuditAction::Logout => "تسجيل خروج",
            AuditAction::PasswordChange => "تغيير كلمة المرور",
            AuditAction::UnauthorizedAccess => "وصول غير مصرح",
            AuditAction::CreateProduct => "إنشاء منتج",
            AuditAction::UpdateProduct => "تحديث منتج",
            AuditAction::DeleteProduct => "حذف منتج",
            AuditAction::CreateOrder => "إنشاء طلبية",
            AuditAction::UpdateOrder => "تحديث طلبية",
            AuditAction::ConfirmOrder => "تأكيد طلبية",
            AuditAction::DeleteOrder => "حذف طلبية",
            AuditAction::CreateDailyReport => "إنشاء تقرير يومي",
            AuditAction::UpdateDailyReport => "تحديث تقرير يومي",
            AuditAction::DeleteDailyReport => "حذف تقرير يومي",
            AuditAction::CreateUnit => "إنشاء وحدة",
            AuditAction::UpdateUnit => "تحديث وحدة",
            AuditAction::DeleteUnit => "حذف وحدة",
            AuditAction::ResolveConflict => "حل تعارض مزامنة",
            AuditAction::UnitNodeImport => "استيراد وحدة",
            AuditAction::UpdateSettings => "تحديث الإعدادات",
            AuditAction::CreateBackup => "إنشاء نسخة احتياطية",
            AuditAction::RestoreBackup => "استعادة نسخة احتياطية",
            AuditAction::BackupCheckpointFailed => "فشل نقطة التحقق من النسخة الاحتياطية",
            AuditAction::BackupSnapshotValidationFailed => "فشل التحقق من لقطة النسخة الاحتياطية",
            AuditAction::BackupSnapshotTooSmall => "لقطة النسخة الاحتياطية صغيرة جداً",
            AuditAction::BackupSnapshotIntegrityFailed => {
                "فشل التحقق من سلامة لقطة النسخة الاحتياطية"
            }
            AuditAction::CreateSnapshot => "إنشاء لقطة مخزون",
            AuditAction::CreateUser => "إنشاء مستخدم",
            AuditAction::UpdateUser => "تحديث مستخدم",
            AuditAction::Create => "إنشاء",
            AuditAction::ImportProducts => "استيراد منتجات",
            AuditAction::ImportDailyReport => "استيراد تقرير يومي",
            AuditAction::ImportMonthlyReport => "استيراد تقرير شهري",
            AuditAction::ImportStockMovements => "استيراد حركات المخزون",
            AuditAction::ImportNodePackage => "استيراد حزمة وحدة",
            AuditAction::ImportTrustPackage => "استيراد حزمة الثقة",
            AuditAction::ImportRegistryPackage => "استيراد حزمة السجل",
            AuditAction::FiscalYearOpened => "فتح سنة مالية",
            AuditAction::FiscalYearClosed => "إغلاق سنة مالية",
            AuditAction::FiscalYearArchived => "أرشفة سنة مالية",
            AuditAction::OpeningBalancesGenerated => "توليد أرصدة افتتاحية",
            AuditAction::CarryForwardExecuted => "تنفيذ ترحيل الأرصدة",
            AuditAction::CarryForwardRejected => "رفض ترحيل الأرصدة",
            AuditAction::FiscalWriteRejected => "رفض كتابة مالية",
            AuditAction::FiscalClosurePackageExported => "تصدير حزمة إغلاق السنة",
            AuditAction::FiscalClosurePackageApplied => "تطبيق حزمة إغلاق السنة",
            AuditAction::IdentityRotated => "تدوير هوية",
            AuditAction::IdentityReissued => "إعادة إصدار هوية",
        }
    }

    pub fn default_entity_type(&self) -> EntityType {
        match self {
            AuditAction::Login
            | AuditAction::LoginFailed
            | AuditAction::Logout
            | AuditAction::PasswordChange
            | AuditAction::UnauthorizedAccess => EntityType::User,
            AuditAction::CreateProduct
            | AuditAction::UpdateProduct
            | AuditAction::DeleteProduct
            | AuditAction::ImportProducts => EntityType::Product,
            AuditAction::CreateOrder
            | AuditAction::UpdateOrder
            | AuditAction::ConfirmOrder
            | AuditAction::DeleteOrder => EntityType::Order,
            AuditAction::CreateDailyReport
            | AuditAction::UpdateDailyReport
            | AuditAction::DeleteDailyReport
            | AuditAction::ImportDailyReport => EntityType::DailyReport,
            AuditAction::CreateUnit
            | AuditAction::UpdateUnit
            | AuditAction::DeleteUnit
            | AuditAction::UnitNodeImport
            | AuditAction::ImportNodePackage => EntityType::Unit,
            AuditAction::UpdateSettings => EntityType::Settings,
            AuditAction::CreateBackup
            | AuditAction::RestoreBackup
            | AuditAction::BackupCheckpointFailed
            | AuditAction::BackupSnapshotValidationFailed
            | AuditAction::BackupSnapshotTooSmall
            | AuditAction::BackupSnapshotIntegrityFailed => EntityType::System,
            AuditAction::CreateSnapshot
            | AuditAction::ImportMonthlyReport
            | AuditAction::ImportStockMovements => EntityType::DailyReport,
            AuditAction::CreateUser | AuditAction::UpdateUser => EntityType::User,
            AuditAction::Create | AuditAction::ResolveConflict => EntityType::System,
            // B4 trust distribution (Identity Store) and registry fleet-state
            // snapshots are system-level facts, not single-entity updates.
            AuditAction::ImportTrustPackage | AuditAction::ImportRegistryPackage => {
                EntityType::System
            }
            AuditAction::FiscalYearOpened
            | AuditAction::FiscalYearClosed
            | AuditAction::FiscalYearArchived
            | AuditAction::OpeningBalancesGenerated
            | AuditAction::CarryForwardExecuted
            | AuditAction::CarryForwardRejected
            | AuditAction::FiscalWriteRejected
            | AuditAction::FiscalClosurePackageExported
            | AuditAction::FiscalClosurePackageApplied => EntityType::Financial,
            // Identity rotation is a node/credential lifecycle fact (B7).
            AuditAction::IdentityRotated | AuditAction::IdentityReissued => EntityType::System,
        }
    }
}

#[cfg(test)]
mod audit_action_tests {
    use super::AuditAction;

    #[test]
    fn resolve_conflict_round_trip_as_str_parse() {
        let a = AuditAction::ResolveConflict;
        assert_eq!(a.as_str(), "ResolveConflict");
        assert_eq!(AuditAction::parse("ResolveConflict"), Some(a.clone()));
        assert_eq!(AuditAction::parse("resolveconflict"), None);
    }

    #[test]
    fn resolve_conflict_serde_json_round_trip() {
        let a = AuditAction::ResolveConflict;
        let j = serde_json::to_string(&a).unwrap();
        assert_eq!(j, "\"ResolveConflict\"");
        let back: AuditAction = serde_json::from_str(&j).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn resolve_conflict_display_arabic() {
        assert_eq!(
            AuditAction::ResolveConflict.display_arabic(),
            "حل تعارض مزامنة"
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EntityType {
    Product,
    Order,
    DailyReport,
    Unit,
    Settings,
    User,
    System,
    Backup,
    Financial,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Product => "Product",
            EntityType::Order => "Order",
            EntityType::DailyReport => "DailyReport",
            EntityType::Unit => "Unit",
            EntityType::Settings => "Settings",
            EntityType::User => "User",
            EntityType::System => "System",
            EntityType::Backup => "Backup",
            EntityType::Financial => "Financial",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Product" => Some(EntityType::Product),
            "Order" => Some(EntityType::Order),
            "DailyReport" => Some(EntityType::DailyReport),
            "Unit" => Some(EntityType::Unit),
            "Settings" => Some(EntityType::Settings),
            "User" => Some(EntityType::User),
            "System" => Some(EntityType::System),
            "Backup" => Some(EntityType::Backup),
            "Financial" => Some(EntityType::Financial),
            _ => None,
        }
    }

    pub fn display_arabic(&self) -> &'static str {
        match self {
            EntityType::Product => "منتج",
            EntityType::Order => "طلبية",
            EntityType::DailyReport => "تقرير يومي",
            EntityType::Unit => "وحدة",
            EntityType::Settings => "إعدادات",
            EntityType::User => "مستخدم",
            EntityType::System => "نظام",
            EntityType::Backup => "نسخة احتياطية",
            EntityType::Financial => "مالي",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuditStatus {
    Success,
    Failed,
    Partial,
}

impl AuditStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditStatus::Success => "Success",
            AuditStatus::Failed => "Failed",
            AuditStatus::Partial => "Partial",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Success" => Some(AuditStatus::Success),
            "Failed" => Some(AuditStatus::Failed),
            "Partial" => Some(AuditStatus::Partial),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub action: AuditAction,
    pub action_display: String,
    pub entity_type: EntityType,
    pub entity_type_display: String,
    pub entity_id: Option<String>,
    pub entity_name: Option<String>,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
    pub session_id: Option<String>,
    pub timestamp: String,
    pub status: AuditStatus,
    pub error_message: Option<String>,
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub previous_hash: Option<String>,
    #[serde(default)]
    pub entry_hash: Option<String>,
}

pub struct AuditEntryDbRow {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub entity_name: Option<String>,
    pub old_value_json: Option<String>,
    pub new_value_json: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: String,
    pub status: String,
    pub error_message: Option<String>,
    pub metadata_json: Option<String>,
    pub previous_hash: Option<String>,
    pub entry_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAuditEntry {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub entity_name: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: String,
    pub status: String,
    pub error_message: Option<String>,
    pub metadata: Option<String>,
    pub previous_hash: Option<String>,
    pub entry_hash: Option<String>,
    // Dual-write structured columns (NULL for legacy rows)
    pub event_type: Option<String>,
    pub actor_id: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub fiscal_year: Option<i32>,
    pub before_snapshot: Option<String>,
    pub after_snapshot: Option<String>,
    pub node_id: Option<String>,
    pub details: Option<String>,
}

pub struct AuditQuery {
    pub user_id: Option<String>,
    pub action: Option<String>,
    pub entity_type: Option<String>,
    pub start_timestamp: Option<String>,
    pub end_timestamp: Option<String>,
    pub status: Option<String>,
    pub search_like: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuditFilters {
    pub user_id: Option<String>,
    pub action: Option<String>,
    pub entity_type: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub status: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogResponse {
    pub entries: Vec<AuditEntry>,
    pub total_count: i64,
    pub page: usize,
    pub page_size: usize,
    pub has_more: bool,
}

pub struct UserActivitySummaryRow {
    pub user_id: String,
    pub username: String,
    pub total_operations: i64,
    pub failed_operations: i64,
    pub last_activity: String,
}

pub struct OperationCountRow {
    pub action: String,
    pub count: i64,
}

pub struct DailyOperationCountRow {
    pub date: String,
    pub total: i64,
    pub failed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditStats {
    pub total_operations: i64,
    pub failed_operations: i64,
    pub success_rate: f64,
    pub most_active_users: Vec<UserActivitySummary>,
    pub operations_by_type: Vec<OperationCount>,
    pub operations_by_day: Vec<DailyOperationCount>,
    pub period_start: String,
    pub period_end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserActivitySummary {
    pub user_id: String,
    pub username: String,
    pub total_operations: i64,
    pub failed_operations: i64,
    pub last_activity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationCount {
    pub action: String,
    pub action_display: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyOperationCount {
    pub date: String,
    pub total: i64,
    pub failed: i64,
}

// =============================================================================
// Phase 4 — Audit Schema Evolution types
// =============================================================================

/// Structured audit event type classification.
/// Maps from AuditAction to a high-level category.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuditEventType {
    UserAction,
    SystemEvent,
    FiscalEvent,
    SyncEvent,
    IntegrityEvent,
}

impl AuditEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditEventType::UserAction => "UserAction",
            AuditEventType::SystemEvent => "SystemEvent",
            AuditEventType::FiscalEvent => "FiscalEvent",
            AuditEventType::SyncEvent => "SyncEvent",
            AuditEventType::IntegrityEvent => "IntegrityEvent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "UserAction" => Some(AuditEventType::UserAction),
            "SystemEvent" => Some(AuditEventType::SystemEvent),
            "FiscalEvent" => Some(AuditEventType::FiscalEvent),
            "SyncEvent" => Some(AuditEventType::SyncEvent),
            "IntegrityEvent" => Some(AuditEventType::IntegrityEvent),
            _ => None,
        }
    }
}

/// Row shape for the projection layer — includes all legacy + structured columns.
/// Used by `to_audit_event()` to reconstruct `AuditEvent` from either format.
#[derive(Debug, Clone)]
pub struct AuditEventRow {
    // Legacy columns (always present)
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub entity_name: Option<String>,
    pub old_value_json: Option<String>,
    pub new_value_json: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: String,
    pub status: String,
    pub error_message: Option<String>,
    pub metadata_json: Option<String>,
    pub previous_hash: Option<String>,
    pub entry_hash: Option<String>,
    // Structured columns (NULL for legacy rows)
    pub event_type: Option<String>,
    pub actor_id: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub fiscal_year: Option<i32>,
    pub before_snapshot_json: Option<String>,
    pub after_snapshot_json: Option<String>,
    pub node_id: Option<String>,
    pub details_json: Option<String>,
}

/// Structured audit event — the canonical governance-grade audit record.
/// Can be reconstructed from either new structured columns or legacy flat columns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub event_type: AuditEventType,
    pub actor_id: Option<String>,
    pub actor_name: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub fiscal_year: Option<i32>,
    pub before_snapshot: Option<serde_json::Value>,
    pub after_snapshot: Option<serde_json::Value>,
    pub node_id: Option<String>,
    pub details: serde_json::Value,
    pub action: String,
    pub action_display_arabic: String,
    pub entity_type: String,
    pub entity_type_display_arabic: String,
    pub entity_name: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: String,
    pub status: String,
    pub error_message: Option<String>,
    pub previous_hash: Option<String>,
    pub entry_hash: Option<String>,
}

/// Reconstructs an `AuditEvent` from a database row.
/// Priority: structured columns → details JSON → legacy flat columns.
pub fn to_audit_event(r: AuditEventRow) -> Result<AuditEvent, String> {
    let action = r.action.clone();
    let entity_type = r.entity_type.clone();

    // Attempt reconstruction from structured columns first (new rows)
    if let Some(ref et) = r.event_type {
        let event_type = AuditEventType::parse(et).unwrap_or(AuditEventType::SystemEvent);

        let details: serde_json::Value = r
            .details_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::Value::Null);

        let status = r.status.clone();

        let action_display = AuditAction::parse(&action)
            .map(|a| a.display_arabic().to_string())
            .unwrap_or_default();
        let entity_display = EntityType::parse(&entity_type)
            .map(|e| e.display_arabic().to_string())
            .unwrap_or_default();

        Ok(AuditEvent {
            id: r.id,
            event_type,
            actor_id: r.actor_id,
            actor_name: Some(r.username),
            target_type: r.target_type,
            target_id: r.target_id,
            fiscal_year: r.fiscal_year,
            before_snapshot: parse_json_str(r.before_snapshot_json.as_deref()),
            after_snapshot: parse_json_str(r.after_snapshot_json.as_deref()),
            node_id: r.node_id,
            details,
            action,
            action_display_arabic: action_display,
            entity_type,
            entity_type_display_arabic: entity_display,
            entity_name: r.entity_name,
            session_id: r.session_id,
            timestamp: r.timestamp,
            status,
            error_message: r.error_message,
            previous_hash: r.previous_hash,
            entry_hash: r.entry_hash,
        })
    } else if let Some(ref d) = r.details_json {
        // Reconstruct from details JSON (new row that only has details populated)
        let parsed: serde_json::Value = serde_json::from_str(d)
            .map_err(|e| format!("Failed to parse audit details JSON: {e}"))?;

        let action_parsed = parsed
            .get("action")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let status_str = parsed
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("Success")
            .to_string();
        let entity_type_str = parsed
            .get("entity_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let a_action = action_parsed.as_deref().and_then(AuditAction::parse);
        let a_entity = entity_type_str.as_deref().and_then(EntityType::parse);

        let event_type = a_action
            .as_ref()
            .map(audit_action_to_event_type)
            .unwrap_or(AuditEventType::SystemEvent);
        let actor_id = parsed
            .get("user_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let actor_name = parsed
            .get("username")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let target_id = parsed
            .get("entity_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let entity_name = parsed
            .get("entity_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let session_id = parsed
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let error_msg = parsed
            .get("error_message")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let before = parsed
            .get("old_value")
            .and_then(|v| parse_json_value(v.clone()));
        let after = parsed
            .get("new_value")
            .and_then(|v| parse_json_value(v.clone()));
        let node = parsed
            .get("node_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let action_str = action_parsed.unwrap_or_default();
        let entity_str = entity_type_str.unwrap_or_default();

        Ok(AuditEvent {
            id: r.id,
            event_type,
            actor_id,
            actor_name,
            target_type: Some(entity_str.clone()),
            target_id,
            fiscal_year: None,
            before_snapshot: before,
            after_snapshot: after,
            node_id: node,
            details: parsed,
            action: action_str,
            action_display_arabic: a_action
                .map(|a| a.display_arabic().to_string())
                .unwrap_or_default(),
            entity_type: entity_str,
            entity_type_display_arabic: a_entity
                .map(|e| e.display_arabic().to_string())
                .unwrap_or_default(),
            entity_name,
            session_id,
            timestamp: r.timestamp,
            status: status_str,
            error_message: error_msg,
            previous_hash: r.previous_hash,
            entry_hash: r.entry_hash,
        })
    } else {
        // Legacy row — reconstruct from flat columns
        let a_action =
            AuditAction::parse(&action).ok_or_else(|| format!("Unknown audit action: {action}"))?;
        let a_entity = EntityType::parse(&entity_type)
            .ok_or_else(|| format!("Unknown entity type: {entity_type}"))?;
        let a_status = AuditStatus::parse(&r.status)
            .ok_or_else(|| format!("Unknown audit status: {}", r.status))?;

        let details = build_details_json(&r);
        let action_display = a_action.display_arabic().to_string();
        let entity_display = a_entity.display_arabic().to_string();

        Ok(AuditEvent {
            id: r.id,
            event_type: audit_action_to_event_type(&a_action),
            actor_id: Some(r.user_id),
            actor_name: Some(r.username),
            target_type: Some(entity_type.clone()),
            target_id: r.entity_id,
            fiscal_year: None,
            before_snapshot: parse_json_str(r.old_value_json.as_deref()),
            after_snapshot: parse_json_str(r.new_value_json.as_deref()),
            node_id: None,
            details,
            action,
            action_display_arabic: action_display,
            entity_type,
            entity_type_display_arabic: entity_display,
            entity_name: r.entity_name,
            session_id: r.session_id,
            timestamp: r.timestamp,
            status: a_status.as_str().to_string(),
            error_message: r.error_message,
            previous_hash: r.previous_hash,
            entry_hash: r.entry_hash,
        })
    }
}

/// Map an AuditAction to its high-level AuditEventType.
pub fn audit_action_to_event_type(action: &AuditAction) -> AuditEventType {
    match action {
        // Auth + CRUD operations → UserAction
        AuditAction::Login
        | AuditAction::LoginFailed
        | AuditAction::Logout
        | AuditAction::PasswordChange
        | AuditAction::UnauthorizedAccess
        | AuditAction::CreateProduct
        | AuditAction::UpdateProduct
        | AuditAction::DeleteProduct
        | AuditAction::CreateOrder
        | AuditAction::UpdateOrder
        | AuditAction::ConfirmOrder
        | AuditAction::DeleteOrder
        | AuditAction::CreateDailyReport
        | AuditAction::UpdateDailyReport
        | AuditAction::DeleteDailyReport
        | AuditAction::CreateUnit
        | AuditAction::UpdateUnit
        | AuditAction::DeleteUnit
        | AuditAction::ResolveConflict
        | AuditAction::UpdateSettings
        | AuditAction::CreateBackup
        | AuditAction::RestoreBackup
        | AuditAction::CreateSnapshot
        | AuditAction::CreateUser
        | AuditAction::UpdateUser
        | AuditAction::Create => AuditEventType::UserAction,

        // Fiscal lifecycle events → FiscalEvent
        AuditAction::FiscalYearOpened
        | AuditAction::FiscalYearClosed
        | AuditAction::FiscalYearArchived
        | AuditAction::OpeningBalancesGenerated
        | AuditAction::CarryForwardExecuted
        | AuditAction::CarryForwardRejected
        | AuditAction::FiscalWriteRejected
        | AuditAction::FiscalClosurePackageExported
        | AuditAction::FiscalClosurePackageApplied => AuditEventType::FiscalEvent,

        // Sync/import events → SyncEvent
        AuditAction::ImportProducts
        | AuditAction::ImportDailyReport
        | AuditAction::ImportMonthlyReport
        | AuditAction::ImportStockMovements
        | AuditAction::ImportNodePackage
        | AuditAction::ImportTrustPackage
        | AuditAction::ImportRegistryPackage
        | AuditAction::UnitNodeImport => AuditEventType::SyncEvent,

        // Integrity/backup failures → IntegrityEvent
        AuditAction::BackupCheckpointFailed
        | AuditAction::BackupSnapshotValidationFailed
        | AuditAction::BackupSnapshotTooSmall
        | AuditAction::BackupSnapshotIntegrityFailed => AuditEventType::IntegrityEvent,

        // Identity rotation (B7) → SystemEvent (node/credential lifecycle)
        AuditAction::IdentityRotated | AuditAction::IdentityReissued => AuditEventType::SystemEvent,
    }
}

/// Build a legacy-format details JSON from flat struct fields.
fn build_details_json(r: &AuditEventRow) -> serde_json::Value {
    serde_json::json!({
        "id": r.id,
        "action": r.action,
        "entity_type": r.entity_type,
        "entity_id": r.entity_id,
        "entity_name": r.entity_name,
        "username": r.username,
        "old_value": r.old_value_json,
        "new_value": r.new_value_json,
        "session_id": r.session_id,
        "status": r.status,
        "error_message": r.error_message,
        "metadata": r.metadata_json,
        "previous_hash": r.previous_hash,
        "entry_hash": r.entry_hash,
    })
}

/// Build a canonical details JSON from a NewAuditEntry's legacy fields.
/// Used during dual-write insertion.
pub fn build_details_from_entry(entry: &NewAuditEntry) -> Option<String> {
    let obj = serde_json::json!({
        "id": entry.id,
        "action": entry.action,
        "entity_type": entry.entity_type,
        "entity_id": entry.entity_id,
        "entity_name": entry.entity_name,
        "username": entry.username,
        "old_value": entry.old_value,
        "new_value": entry.new_value,
        "session_id": entry.session_id,
        "status": entry.status,
        "error_message": entry.error_message,
        "metadata": entry.metadata,
    });
    Some(obj.to_string())
}

fn parse_json_str(s: Option<&str>) -> Option<serde_json::Value> {
    s.and_then(|raw| serde_json::from_str(raw).ok())
}

fn parse_json_value(v: serde_json::Value) -> Option<serde_json::Value> {
    match v {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => serde_json::from_str(&s)
            .ok()
            .or(Some(serde_json::Value::String(s))),
        other => Some(other),
    }
}
