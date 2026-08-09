//! Data Transfer Objects (DTOs)
//!
//! Request and response types for API operations
//! Separated from domain entities to prevent data leakage

use serde::{Deserialize, Serialize};

// ============================================================================
// Request DTOs
// ============================================================================

/// Password change request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    pub user_id: String,
    pub old_password: Option<String>, // None for admin reset
    pub new_password: String,
}

/// Product filter request
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProductFilterRequest {
    pub year: Option<i32>,
    pub name_contains: Option<String>,
    pub supplier: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

/// Order filter request
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrderFilterRequest {
    pub status: Option<String>,
    pub supplier_name: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

/// Report filter request
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReportFilterRequest {
    pub unit_id: Option<String>,
    pub year: Option<i32>,
    pub month: Option<i32>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

// ============================================================================
// Response DTOs
// ============================================================================

/// API operation result wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ErrorDetail>,
    pub message: Option<String>,
}

impl<T> OperationResult<T> {
    /// Create a successful result
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            message: None,
        }
    }

    /// Create a failed result
    pub fn failure(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(ErrorDetail {
                code: "ERROR".to_string(),
                message: error,
            }),
            message: None,
        }
    }

    /// Create a result with message
    pub fn with_message(data: T, message: String) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            message: Some(message),
        }
    }
}

/// Error detail for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}

/// Paginated response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: usize,
    pub page_size: usize,
    pub has_more: bool,
}

impl<T> PaginatedResponse<T> {
    /// Create a paginated response
    pub fn new(items: Vec<T>, total: i64, page: usize, page_size: usize) -> Self {
        Self {
            has_more: (page * page_size) < total as usize,
            items,
            total,
            page,
            page_size,
        }
    }

    /// Calculate total pages
    pub fn total_pages(&self) -> usize {
        if self.page_size == 0 {
            1
        } else {
            ((self.total as f64) / (self.page_size as f64)).ceil() as usize
        }
    }
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub status: String,
    pub version: String,
    pub timestamp: String,
    pub components: Vec<ComponentHealth>,
}

/// Individual component health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: String, // "healthy", "degraded", "unhealthy"
    pub message: Option<String>,
}

// ============================================================================
// Import/Export DTOs
// ============================================================================

/// Package import request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageImportRequest {
    pub file_path: String,
    pub file_hash: String,
    pub options: Option<PackageImportOptions>,
}

/// Package import options
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PackageImportOptions {
    pub skip_header: bool,
    pub delimiter: Option<String>,
    pub encoding: Option<String>,
}

/// Import result summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub total_records: usize,
    pub imported: usize,
    pub failed: usize,
    pub errors: Vec<ImportError>,
}

/// Individual import error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportError {
    pub line: usize,
    pub field: String,
    pub message: String,
}

/// Backup info response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfoResponse {
    pub id: String,
    pub created_at: String,
    pub size: u64,
    pub path: String,
    pub hash: String,
}

/// Excel export result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XlsxExportResult {
    pub success: bool,
    pub file_path: String,
    pub record_count: usize,
    pub message: String,
}

impl XlsxExportResult {
    pub fn success(file_path: String, record_count: usize) -> Self {
        Self {
            success: true,
            file_path: file_path.clone(),
            record_count,
            message: format!("تم تصدير {} سجل إلى ملف Excel: {}", record_count, file_path),
        }
    }
}

/// Package export result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageExportResult {
    pub success: bool,
    pub file_path: String,
    pub record_count: usize,
    pub encryption: String,
    pub file_hash: String,
    pub message: String,
}

impl PackageExportResult {
    pub fn success(file_path: String, record_count: usize, encryption: String) -> Self {
        Self {
            success: true,
            file_path: file_path.clone(),
            record_count,
            encryption,
            file_hash: String::new(), // To be filled if needed
            message: format!("تم تصدير {} سجل إلى {}", record_count, file_path),
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PackageImportResult {
    pub added: usize,
    pub updated: usize,
    pub deleted: usize,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TrustPackageImportResult {
    pub certificate_count: usize,
    pub revocation_count: usize,
    pub package_id: String,
    pub imported_by: String,
    pub timestamp: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RegistryPackageImportResult {
    pub snapshot_version: u64,
    pub unit_count: usize,
    pub package_id: String,
    pub imported_by: String,
    pub timestamp: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IdentityAccessPackageImportResult {
    pub admin_updated: bool,
    pub user_updated: bool,
    pub user_renamed: bool,
    pub package_id: String,
    pub imported_by: String,
    pub timestamp: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_result() {
        let success: OperationResult<i32> = OperationResult::success(42);
        assert!(success.success);
        assert_eq!(success.data, Some(42));

        let failure = OperationResult::<i32>::failure("Not found".to_string());
        assert!(!failure.success);
        assert!(failure.error.is_some());
    }

    #[test]
    fn test_paginated_response() {
        let items = vec![1, 2, 3, 4, 5];
        let response = PaginatedResponse::new(items, 100, 1, 5);
        assert_eq!(response.total, 100);
        assert_eq!(response.total_pages(), 20);
        assert!(response.has_more);
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UnitNodePackageImportResult {
    pub unit_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSecurityDiagnostics {
    pub production_mode: bool,
    pub has_app_key_env: bool,
    pub has_package_signing_key_env: bool,
    pub bootstrap_would_fail: bool,
    pub active_signing_key_id: String,
    pub accepted_verification_key_ids: Vec<String>,
    pub deprecated_signing_key_ids: Vec<String>,
    pub deprecation_deadline_utc: Option<String>,
    pub enforce_trusted_signers: bool,
    pub trusted_signer_ids: Vec<String>,
}

// ============================================================================
// Application-key provisioning (ADR-0041)
// ============================================================================

/// Live app-key provisioning status (ADR-0041 §9 `get_security_status`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppKeyStatus {
    /// `appkey.age` store exists on disk.
    pub provisioned: bool,
    /// An app key resolves now (env, unlocked store cache, or dev fallback).
    pub unlocked: bool,
    /// Absolute path of the store file (`appkey.age`).
    pub store_path: String,
    /// `env` | `store` | `dev` | `none` — the active resolution source.
    pub source: String,
    /// Backend projection: the operator must act (unlock or first-run setup).
    pub requires_action: bool,
}

/// Result of `initialize_app_key` (ADR-0041 §6, §9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppKeyInitializeResult {
    pub provisioned: bool,
    pub unlocked: bool,
    pub store_path: String,
    /// Whether an offline backup of the raw identity was exported (opt-in).
    pub exported_backup: bool,
}

/// Result of `unlock_app_key` (ADR-0041 §4 `Unlocked`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppKeyUnlockResult {
    pub provisioned: bool,
    pub unlocked: bool,
    pub store_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPreflightCheck {
    pub status: String, // ok | warn | fail
    pub reasons: Vec<String>,
    pub reason_messages_ar: Vec<String>,
    pub diagnostics: SyncSecurityDiagnostics,
}
