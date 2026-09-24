use crate::errors::{AppError, BusinessLogicError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFailureReason {
    ReplayRejected,
    IntegrityMismatch,
    UnsupportedSchema,
    InvalidPackageProvenance,
    TargetRejected,
    ImportTransactionFailed,
}

impl ImportFailureReason {
    pub const fn code(self) -> &'static str {
        match self {
            ImportFailureReason::ReplayRejected => "REPLAY_REJECTED",
            ImportFailureReason::IntegrityMismatch => "INTEGRITY_MISMATCH",
            ImportFailureReason::UnsupportedSchema => "UNSUPPORTED_SCHEMA",
            ImportFailureReason::InvalidPackageProvenance => "INVALID_PACKAGE_PROVENANCE",
            ImportFailureReason::TargetRejected => "TARGET_REJECTED",
            ImportFailureReason::ImportTransactionFailed => "IMPORT_TRANSACTION_FAILED",
        }
    }

    pub fn classify(error: &AppError) -> Self {
        match error {
            AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage { .. }) => {
                ImportFailureReason::ReplayRejected
            }
            AppError::Validation(e) => {
                let msg = e.to_string();
                if msg.contains("PACKAGE_TOO_OLD")
                    || msg.contains("PACKAGE_TOO_NEW")
                    || msg.contains("schema_version")
                {
                    ImportFailureReason::UnsupportedSchema
                } else if msg.contains("integrity_hash") {
                    ImportFailureReason::IntegrityMismatch
                } else if msg.contains("target_node_id") || msg.contains("export_mode") {
                    ImportFailureReason::TargetRejected
                } else if msg.contains("source_node_id") {
                    ImportFailureReason::InvalidPackageProvenance
                } else {
                    ImportFailureReason::ImportTransactionFailed
                }
            }
            _ => ImportFailureReason::ImportTransactionFailed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::ValidationError;

    fn invalid(field: &str, message: &str) -> AppError {
        AppError::Validation(ValidationError::InvalidFormat {
            field: field.into(),
            message: message.into(),
        })
    }

    #[test]
    fn target_rejection_is_classified_for_target_node_id_window() {
        let err = invalid("target_node_id", "هدف الحزمة لا يطابق الوحدة المحلية");
        assert_eq!(
            ImportFailureReason::classify(&err),
            ImportFailureReason::TargetRejected
        );
        assert_eq!(
            ImportFailureReason::TargetRejected.code(),
            "TARGET_REJECTED"
        );
    }

    #[test]
    fn target_rejection_is_classified_for_export_mode_window() {
        let err = invalid("export_mode", "الحزمة لا تحمل نمط تصدير موثّقًا");
        assert_eq!(
            ImportFailureReason::classify(&err),
            ImportFailureReason::TargetRejected
        );
    }

    #[test]
    fn source_provenance_window_is_not_misclassified_as_target() {
        let err = invalid("source_node_id", "مصدر الحزمة غير مطابق");
        assert_eq!(
            ImportFailureReason::classify(&err),
            ImportFailureReason::InvalidPackageProvenance
        );
    }
}
