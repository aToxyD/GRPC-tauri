use crate::application::sync::import::ImportAuditEvent;
use crate::errors::AppResult;

pub trait ImportAuditLogger {
    fn log(&self, event: &ImportAuditEvent) -> AppResult<()>;
}
