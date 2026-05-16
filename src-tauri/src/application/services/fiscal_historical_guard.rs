//! Fail-closed guards for archived fiscal years and historical immutability.
//!
//! Archived years are operationally immutable: no imports, no snapshot mutation,
//! no status changes, and no restore that would regress past archived state.

use crate::errors::{AppError, BusinessLogicError};
use crate::repositories::executor::DbExecutor;
use std::path::Path;

pub struct FiscalHistoricalGuard<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalHistoricalGuard<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn is_year_archived(&self, year: i32) -> Result<bool, AppError> {
        use crate::repositories::RepositoryProvider;
        self.executor.fiscal_year_status().is_year_archived(year)
    }

    pub fn max_archived_year(&self) -> Result<Option<i32>, AppError> {
        use crate::repositories::RepositoryProvider;
        self.executor.fiscal_year_status().max_archived_year()
    }

    pub fn assert_year_not_archived(&self, year: i32) -> Result<(), AppError> {
        if self.is_year_archived(year)? {
            log::warn!(
                target: "grpc::fiscal",
                "[FISCAL_ARCHIVED_REJECT] year={} operation=mutation",
                year
            );
            return Err(AppError::BusinessLogic(
                BusinessLogicError::FiscalYearArchived { year },
            ));
        }
        Ok(())
    }

    pub fn assert_import_year_allowed(&self, incoming_year: i32) -> Result<(), AppError> {
        self.assert_year_not_archived(incoming_year)
    }

    pub fn assert_opening_snapshot_mutable(&self, fiscal_year: i32) -> Result<(), AppError> {
        self.assert_year_not_archived(fiscal_year)
    }

    pub fn assert_fiscal_status_mutation_allowed(&self, year: i32) -> Result<(), AppError> {
        self.assert_year_not_archived(year)
    }

    pub fn assert_export_snapshot_mutable(&self, fiscal_year: i32) -> Result<(), AppError> {
        self.assert_year_not_archived(fiscal_year)
    }

    /// Reject restore when the backup's newest archived year is older than live max archived.
    pub fn assert_restore_would_not_regress_archived_state(
        &self,
        backup_path: &Path,
        backup_port: &dyn crate::domain::ports::backup::BackupPort,
    ) -> Result<(), AppError> {
        let live_max = self.max_archived_year()?;
        let Some(live_max) = live_max else {
            return Ok(());
        };

        let backup_max = backup_port.get_backup_max_archived_year(backup_path)?;
        let Some(backup_max) = backup_max else {
            return Ok(());
        };

        if backup_max < live_max {
            let msg = format!(
                "backup newest archived year ({}) is older than live archived state ({}); restore refused",
                backup_max, live_max
            );
            log::warn!(
                target: "grpc::backup",
                "[BACKUP_RESTORE_REJECTED] reason=archived_state_regression live_max={} backup_max={}",
                live_max,
                backup_max
            );
            return Err(AppError::BusinessLogic(
                BusinessLogicError::RestoreWouldRegressArchivedState { message: msg },
            ));
        }
        Ok(())
    }
}

impl crate::architecture::Service for FiscalHistoricalGuard<'_> {}
