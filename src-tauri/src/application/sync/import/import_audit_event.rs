#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportAuditEventType {
    ImportStarted,
    ImportSucceeded,
    ImportRejected,
}

impl ImportAuditEventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            ImportAuditEventType::ImportStarted => "ImportStarted",
            ImportAuditEventType::ImportSucceeded => "ImportSucceeded",
            ImportAuditEventType::ImportRejected => "ImportRejected",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportAuditEvent {
    pub event_type: ImportAuditEventType,
    pub package_id: String,
    pub package_kind: String,
    pub source_node_id: Option<String>,
    pub reason_code: Option<String>,
}
