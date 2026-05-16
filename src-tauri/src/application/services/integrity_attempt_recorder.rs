//! Integrity Attempt Recorder
//!
//! Tiny append-only helper used by the various integrity verification paths
//! (inventory, fiscal drift, audit chain, backup file, database integrity) to
//! record each attempt's outcome.
//!
//! The recorder NEVER changes business state — it only writes one row into
//! `integrity_verification_attempts` so the OperationalAnomalyService and the
//! diagnostics UI can surface repeated failures (Anomaly E).
//!
//! Failures to record are logged but NEVER bubble up: recording is best-effort.

use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;

#[derive(Debug, Clone, Copy)]
pub enum VerificationType {
    Inventory,
    FiscalDrift,
    AuditChain,
    BackupFile,
    DatabaseIntegrity,
}

impl VerificationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            VerificationType::Inventory => "INVENTORY",
            VerificationType::FiscalDrift => "FISCAL_DRIFT",
            VerificationType::AuditChain => "AUDIT_CHAIN",
            VerificationType::BackupFile => "BACKUP_FILE",
            VerificationType::DatabaseIntegrity => "DATABASE_INTEGRITY",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum VerificationOutcome {
    Pass,
    Fail,
}

impl VerificationOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            VerificationOutcome::Pass => "PASS",
            VerificationOutcome::Fail => "FAIL",
        }
    }
}

pub struct IntegrityAttemptRecorder<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> IntegrityAttemptRecorder<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Record an integrity verification attempt. Best-effort; never propagates errors.
    pub fn record(
        &self,
        verification_type: VerificationType,
        outcome: VerificationOutcome,
        details: Option<&str>,
    ) {
        if let Err(e) = self.try_record(verification_type, outcome, details) {
            log::warn!(
                target: "grpc::integrity_recorder",
                "Failed to record integrity attempt ({}/{}): {:?}",
                verification_type.as_str(),
                outcome.as_str(),
                e
            );
        }
    }

    fn try_record(
        &self,
        verification_type: VerificationType,
        outcome: VerificationOutcome,
        details: Option<&str>,
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        use crate::repositories::RepositoryProvider;
        self.executor.integrity().record_attempt(
            &now,
            verification_type.as_str(),
            outcome.as_str(),
            details,
        )
    }
}

impl crate::architecture::Service for IntegrityAttemptRecorder<'_> {}
