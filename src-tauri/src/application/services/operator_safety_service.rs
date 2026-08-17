//! Operator Safety Service
//!
//! Provides typed-confirmation safeguards for critical operations:
//!   - Fiscal close          → must type "<YEAR>"
//!   - Archive year          → must type "ARCHIVE"
//!   - Restore backup        → must type "RESTORE"
//!   - Import historical pkg → must type "IMPORT-HISTORICAL"
//!   - Older-trust restore   → must type "RESTORE-OLDER-TRUST"
//!
//! Hard constraints:
//!   - NO modal framework, NO workflow engine
//!   - Service is a pure deterministic function — same input → same output
//!   - Never mutates state
//!
//! The command layer calls `require_confirmation` BEFORE entering the critical
//! transaction. If the operator-provided string does not match the expected
//! token (case-sensitive, trimmed), the operation is refused with a
//! BusinessLogic error and nothing happens.

use crate::errors::{AppError, BusinessLogicError};

#[derive(Debug, Clone, Copy)]
pub enum CriticalOperation {
    FiscalClose { year: i32 },
    ArchiveYear,
    RestoreBackup,
    RestoreOlderTrust,
    ImportHistoricalPackage,
}

impl CriticalOperation {
    /// Returns the exact token the operator must type.
    pub fn expected_token(&self) -> String {
        match self {
            CriticalOperation::FiscalClose { year } => year.to_string(),
            CriticalOperation::ArchiveYear => "ARCHIVE".to_string(),
            CriticalOperation::RestoreBackup => "RESTORE".to_string(),
            CriticalOperation::RestoreOlderTrust => "RESTORE-OLDER-TRUST".to_string(),
            CriticalOperation::ImportHistoricalPackage => "IMPORT-HISTORICAL".to_string(),
        }
    }

    /// Human-readable label (English; UI may translate).
    pub fn label(&self) -> &'static str {
        match self {
            CriticalOperation::FiscalClose { .. } => "إغلاق السنة المالية",
            CriticalOperation::ArchiveYear => "أرشفة السنة",
            CriticalOperation::RestoreBackup => "استعادة النسخة الاحتياطية",
            CriticalOperation::RestoreOlderTrust => "استعادة حالة الثقة الأقدم",
            CriticalOperation::ImportHistoricalPackage => "استيراد الحزمة التاريخية",
        }
    }
}

pub struct OperatorSafetyService;

impl OperatorSafetyService {
    /// Validate that the operator typed the exact expected token.
    ///
    /// Returns `Ok(())` only if `provided.trim() == operation.expected_token()`
    /// (case-sensitive). Otherwise returns a `BusinessLogic` error.
    pub fn require_confirmation(
        operation: CriticalOperation,
        provided: &str,
    ) -> Result<(), AppError> {
        let expected = operation.expected_token();
        let trimmed = provided.trim();

        if trimmed.is_empty() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "التأكيد مطلوب: اكتب '{}' لتفويض عملية {}.",
                        expected,
                        operation.label()
                    ),
                },
            ));
        }

        if trimmed != expected {
            log::warn!(
                target: "grpc::operator_safety",
                "[CONFIRMATION_REJECTED] op={} expected_len={} provided_len={}",
                operation.label(),
                expected.len(),
                trimmed.len()
            );
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "عدم تطابق رمز التأكيد لعملية {}. المتوقع بالضبط: '{}'.",
                        operation.label(),
                        expected
                    ),
                },
            ));
        }

        log::info!(
            target: "grpc::operator_safety",
            "[CONFIRMATION_ACCEPTED] op={}",
            operation.label()
        );
        Ok(())
    }
}
