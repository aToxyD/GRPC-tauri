//! Deterministic local safeguards for critical operator actions.
//!
//! - Execution tokens bound to a stable DB state fingerprint (stale → reject)
//! - Short-window double-execution prevention (in-process only)
//! - Integrity preconditions (fail-closed on Corrupted/Critical)
//! - Archive blocked when CRITICAL operational anomalies are present

use crate::application::services::system_integrity_state_service::SystemIntegrityState;
use crate::application::services::OperationalAnomalyService;
use crate::errors::{AppError, BusinessLogicError};
use crate::repositories::executor::DbExecutor;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const THROTTLE_WINDOW: Duration = Duration::from_millis(2000);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GuardedOperation {
    FiscalClose { year: i32, next_year: i32 },
    ArchiveYear { year: i32 },
    RestoreBackup,
    ImportHistorical,
}

impl GuardedOperation {
    fn discriminator(&self) -> String {
        match self {
            GuardedOperation::FiscalClose { year, next_year } => {
                format!("close:{}:{}", year, next_year)
            }
            GuardedOperation::ArchiveYear { year } => format!("archive:{}", year),
            GuardedOperation::RestoreBackup => "restore".to_string(),
            GuardedOperation::ImportHistorical => "import-historical".to_string(),
        }
    }

    pub fn throttle_key(&self) -> String {
        self.discriminator()
    }
}

pub struct OperationExecutionGuard {
    last_executions: Mutex<HashMap<String, Instant>>,
}

impl Default for OperationExecutionGuard {
    fn default() -> Self {
        Self {
            last_executions: Mutex::new(HashMap::new()),
        }
    }
}

impl OperationExecutionGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Deterministic fingerprint of fiscal + integrity signals used for tokens.
    pub fn state_fingerprint(executor: DbExecutor<'_>) -> Result<String, AppError> {
        let open_year: i32 = executor.query_row(
            "SELECT year FROM fiscal_year_status WHERE status = 'open' LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        let current_year: i32 =
            executor.query_row("SELECT current_year FROM settings WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        let max_archived: i32 = executor.query_row(
            "SELECT COALESCE(MAX(year), 0) FROM fiscal_year_status WHERE archived = 1",
            [],
            |r| r.get(0),
        )?;
        let critical_findings: i64 = executor.query_row(
            "SELECT COUNT(*) FROM operational_findings_log WHERE severity = 'CRITICAL'",
            [],
            |r| r.get(0),
        )?;
        let integrity = SystemIntegrityState::resolve_from_executor(executor)?;
        Ok(format!(
            "open={}|current={}|archived_max={}|critical_findings={}|integrity={:?}",
            open_year, current_year, max_archived, critical_findings, integrity
        ))
    }

    pub fn issue_execution_token(
        executor: DbExecutor<'_>,
        operation: GuardedOperation,
    ) -> Result<String, AppError> {
        let fp = Self::state_fingerprint(executor)?;
        Ok(compute_token(&operation, &fp))
    }

    pub fn validate_execution_token(
        executor: DbExecutor<'_>,
        operation: GuardedOperation,
        provided: &str,
    ) -> Result<(), AppError> {
        let expected = Self::issue_execution_token(executor, operation)?;
        if provided.trim().is_empty() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::StaleExecutionToken {
                    operation: operation.discriminator(),
                },
            ));
        }
        if provided.trim() != expected {
            log::warn!(
                target: "grpc::operator_safety",
                "[EXECUTION_TOKEN_REJECTED] op={} stale_or_invalid=true",
                operation.discriminator()
            );
            return Err(AppError::BusinessLogic(
                BusinessLogicError::StaleExecutionToken {
                    operation: operation.discriminator(),
                },
            ));
        }
        log::info!(
            target: "grpc::operator_safety",
            "[EXECUTION_TOKEN_ACCEPTED] op={}",
            operation.discriminator()
        );
        Ok(())
    }

    pub fn assert_not_throttled(&self, operation: GuardedOperation) -> Result<(), AppError> {
        let key = operation.throttle_key();
        let map = self
            .last_executions
            .lock()
            .map_err(|e| AppError::Internal(format!("execution guard lock: {}", e)))?;
        if let Some(last) = map.get(&key) {
            if last.elapsed() < THROTTLE_WINDOW {
                log::warn!(
                    target: "grpc::operator_safety",
                    "[EXECUTION_THROTTLED] op={} window_ms={}",
                    key,
                    THROTTLE_WINDOW.as_millis()
                );
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::OperationThrottled { operation: key },
                ));
            }
        }
        Ok(())
    }

    pub fn record_execution(&self, operation: GuardedOperation) {
        if let Ok(mut map) = self.last_executions.lock() {
            map.insert(operation.throttle_key(), Instant::now());
        }
    }

    pub fn assert_integrity_allows_restore_or_archive(
        executor: DbExecutor<'_>,
    ) -> Result<(), AppError> {
        let state = SystemIntegrityState::resolve_from_executor(executor)?;
        if state.blocks_financial_operations() {
            log::warn!(
                target: "grpc::integrity",
                "[INTEGRITY_OPERATION_BLOCKED] state={:?} ops=restore,archive",
                state
            );
            return Err(AppError::BusinessLogic(
                BusinessLogicError::RestoreBlockedByIntegrity {
                    state: format!("{:?}", state),
                },
            ));
        }
        Ok(())
    }

    pub fn assert_archive_no_critical_anomalies(
        executor: DbExecutor<'_>,
        year: i32,
    ) -> Result<(), AppError> {
        let report = OperationalAnomalyService::new(executor).analyze()?;
        let critical = report
            .findings
            .iter()
            .any(|f| f.severity == crate::application::services::FindingSeverity::Critical);
        if critical {
            log::warn!(
                target: "grpc::operational_anomaly",
                "[ARCHIVE_REJECTED] year={} reason=critical_anomaly_present",
                year
            );
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "لا يمكن أرشفة السنة المالية {} في وجود شذوذ تشغيلي حرج. يرجى حل الشذوذ أولاً.",
                        year
                    ),
                },
            ));
        }
        Ok(())
    }
}

fn compute_token(operation: &GuardedOperation, fingerprint: &str) -> String {
    let mut h = Sha256::new();
    h.update(operation.discriminator().as_bytes());
    h.update(b"|");
    h.update(fingerprint.as_bytes());
    format!("{:x}", h.finalize())
}

impl crate::architecture::Service for OperationExecutionGuard {}
