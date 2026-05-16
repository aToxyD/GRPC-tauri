//! System Diagnostics Service
//! Provides tools for system health, database integrity, and operational forensics.

use crate::errors::AppError;
use crate::models::ImportAuditEvent;
use crate::repositories::{DbExecutor, RepositoryProvider};

pub struct SystemDiagnosticsService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> SystemDiagnosticsService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Check database integrity using PRAGMA integrity_check
    pub fn check_database_integrity(&self) -> Result<Vec<String>, AppError> {
        let repo = self.executor.system();
        repo.check_integrity()
    }

    /// Get recent import audit events for diagnostics
    pub fn get_import_diagnostics(&self, limit: usize) -> Result<Vec<ImportAuditEvent>, AppError> {
        let repo = self.executor.import_audit_events();
        repo.list_events(limit)
    }
}
