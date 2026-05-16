use crate::application::sync::{ImportAuditEvent, ImportAuditLogger};
use crate::errors::AppResult;
use crate::repositories::{DbExecutor, ImportAuditEventsRepository};

pub struct SqliteImportAuditLogger<'a> {
    repo: ImportAuditEventsRepository<'a>,
}

impl<'a> SqliteImportAuditLogger<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self {
            repo: ImportAuditEventsRepository::new(executor),
        }
    }
}

impl ImportAuditLogger for SqliteImportAuditLogger<'_> {
    fn log(&self, event: &ImportAuditEvent) -> AppResult<()> {
        self.repo.insert_event(
            event.event_type.as_str(),
            event.package_id.as_str(),
            event.package_kind.as_str(),
            event.source_node_id.as_deref(),
            event.reason_code.as_deref(),
        )
    }
}
