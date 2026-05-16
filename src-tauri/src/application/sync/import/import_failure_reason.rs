use crate::errors::{AppError, BusinessLogicError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFailureReason {
    ReplayRejected,
    IntegrityMismatch,
    UnsupportedSchema,
    InvalidPackageProvenance,
    ImportTransactionFailed,
}

impl ImportFailureReason {
    pub const fn code(self) -> &'static str {
        match self {
            ImportFailureReason::ReplayRejected => "REPLAY_REJECTED",
            ImportFailureReason::IntegrityMismatch => "INTEGRITY_MISMATCH",
            ImportFailureReason::UnsupportedSchema => "UNSUPPORTED_SCHEMA",
            ImportFailureReason::InvalidPackageProvenance => "INVALID_PACKAGE_PROVENANCE",
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
