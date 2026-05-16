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
            AuditAction::FiscalYearOpened => "FiscalYearOpened",
            AuditAction::FiscalYearClosed => "FiscalYearClosed",
            AuditAction::FiscalYearArchived => "FiscalYearArchived",
            AuditAction::OpeningBalancesGenerated => "OpeningBalancesGenerated",
            AuditAction::CarryForwardExecuted => "CarryForwardExecuted",
            AuditAction::CarryForwardRejected => "CarryForwardRejected",
            AuditAction::FiscalWriteRejected => "FiscalWriteRejected",
            AuditAction::FiscalClosurePackageExported => "FiscalClosurePackageExported",
            AuditAction::FiscalClosurePackageApplied => "FiscalClosurePackageApplied",
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
            "FiscalYearOpened" => Some(AuditAction::FiscalYearOpened),
            "FiscalYearClosed" => Some(AuditAction::FiscalYearClosed),
            "FiscalYearArchived" => Some(AuditAction::FiscalYearArchived),
            "OpeningBalancesGenerated" => Some(AuditAction::OpeningBalancesGenerated),
            "CarryForwardExecuted" => Some(AuditAction::CarryForwardExecuted),
            "CarryForwardRejected" => Some(AuditAction::CarryForwardRejected),
            "FiscalWriteRejected" => Some(AuditAction::FiscalWriteRejected),
            "FiscalClosurePackageExported" => Some(AuditAction::FiscalClosurePackageExported),
            "FiscalClosurePackageApplied" => Some(AuditAction::FiscalClosurePackageApplied),
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
            AuditAction::FiscalYearOpened => "فتح سنة مالية",
            AuditAction::FiscalYearClosed => "إغلاق سنة مالية",
            AuditAction::FiscalYearArchived => "أرشفة سنة مالية",
            AuditAction::OpeningBalancesGenerated => "توليد أرصدة افتتاحية",
            AuditAction::CarryForwardExecuted => "تنفيذ ترحيل الأرصدة",
            AuditAction::CarryForwardRejected => "رفض ترحيل الأرصدة",
            AuditAction::FiscalWriteRejected => "رفض كتابة مالية",
            AuditAction::FiscalClosurePackageExported => "تصدير حزمة إغلاق السنة",
            AuditAction::FiscalClosurePackageApplied => "تطبيق حزمة إغلاق السنة",
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
            AuditAction::CreateOrder | AuditAction::ConfirmOrder | AuditAction::DeleteOrder => {
                EntityType::Order
            }
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
            AuditAction::FiscalYearOpened
            | AuditAction::FiscalYearClosed
            | AuditAction::FiscalYearArchived
            | AuditAction::OpeningBalancesGenerated
            | AuditAction::CarryForwardExecuted
            | AuditAction::CarryForwardRejected
            | AuditAction::FiscalWriteRejected
            | AuditAction::FiscalClosurePackageExported
            | AuditAction::FiscalClosurePackageApplied => EntityType::Financial,
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
