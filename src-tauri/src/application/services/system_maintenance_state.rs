//! Explicit in-process maintenance mode — no background coordination, no distributed locks.

use crate::errors::{AppError, BusinessLogicError};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SystemMaintenanceState {
    Normal,
    MaintenanceLocked,
    RestoreInProgress,
    MigrationInProgress,
    IntegrityRecovery,
}

impl SystemMaintenanceState {
    pub fn blocks_operational_mutations(self) -> bool {
        !matches!(self, Self::Normal)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::MaintenanceLocked => "MAINTENANCE_LOCKED",
            Self::RestoreInProgress => "RESTORE_IN_PROGRESS",
            Self::MigrationInProgress => "MIGRATION_IN_PROGRESS",
            Self::IntegrityRecovery => "INTEGRITY_RECOVERY",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenanceBlockedOperation {
    Import,
    StockWrite,
    ReportWrite,
    FiscalClose,
    Archive,
    Sync,
}

impl MaintenanceBlockedOperation {
    fn code(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::StockWrite => "stock_write",
            Self::ReportWrite => "report_write",
            Self::FiscalClose => "fiscal_close",
            Self::Archive => "archive",
            Self::Sync => "sync",
        }
    }
}

pub struct SystemMaintenanceHandle {
    inner: Arc<Mutex<SystemMaintenanceState>>,
}

impl SystemMaintenanceHandle {
    pub fn new(initial: SystemMaintenanceState) -> Self {
        Self {
            inner: Arc::new(Mutex::new(initial)),
        }
    }

    pub fn get(&self) -> Result<SystemMaintenanceState, AppError> {
        self.inner
            .lock()
            .map(|g| *g)
            .map_err(|e| AppError::Internal(format!("maintenance state lock: {}", e)))
    }

    pub fn set(&self, state: SystemMaintenanceState) -> Result<(), AppError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| AppError::Internal(format!("maintenance state lock: {}", e)))?;
        log::info!(
            target: "grpc::maintenance",
            "[MAINTENANCE_STATE] {} -> {}",
            guard.label(),
            state.label()
        );
        *guard = state;
        Ok(())
    }

    pub fn assert_allows(&self, operation: MaintenanceBlockedOperation) -> Result<(), AppError> {
        let state = self.get()?;
        if state.blocks_operational_mutations() {
            log::warn!(
                target: "grpc::maintenance",
                "[MAINTENANCE_BLOCKED] state={} op={}",
                state.label(),
                operation.code()
            );
            return Err(AppError::BusinessLogic(
                BusinessLogicError::MaintenanceModeBlocked {
                    state: state.label().to_string(),
                    operation: operation.code().to_string(),
                },
            ));
        }
        Ok(())
    }
}

impl Default for SystemMaintenanceHandle {
    fn default() -> Self {
        Self::new(SystemMaintenanceState::Normal)
    }
}

impl crate::architecture::Service for SystemMaintenanceHandle {}
