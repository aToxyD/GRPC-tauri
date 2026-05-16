use crate::application::sync::SYNC_PACKAGE_SCHEMA_VERSION;
use crate::errors::AppError;
use crate::repositories::DbExecutor;
use crate::repositories::RepositoryProvider;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TelemetryEventType {
    Backup,
    Restore,
    SyncImport,
    SyncExport,
    IntegrityCheck,
    WalCheckpoint,
    RuntimeStartup,
}

impl TelemetryEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Backup => "BACKUP",
            Self::Restore => "RESTORE",
            Self::SyncImport => "SYNC_IMPORT",
            Self::SyncExport => "SYNC_EXPORT",
            Self::IntegrityCheck => "INTEGRITY_CHECK",
            Self::WalCheckpoint => "WAL_CHECKPOINT",
            Self::RuntimeStartup => "RUNTIME_STARTUP",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "BACKUP" => Self::Backup,
            "RESTORE" => Self::Restore,
            "SYNC_IMPORT" => Self::SyncImport,
            "SYNC_EXPORT" => Self::SyncExport,
            "INTEGRITY_CHECK" => Self::IntegrityCheck,
            "WAL_CHECKPOINT" => Self::WalCheckpoint,
            "RUNTIME_STARTUP" => Self::RuntimeStartup,
            _ => Self::IntegrityCheck, // Fallback
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TelemetryOutcome {
    Success,
    Failure,
}

impl TelemetryOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Failure => "FAILURE",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "SUCCESS" => Self::Success,
            _ => Self::Failure,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub id: String,
    pub event_type: TelemetryEventType,
    pub outcome: TelemetryOutcome,
    pub duration_ms: Option<i64>,
    pub timestamp: String,
    pub metadata: Option<serde_json::Value>,
    pub user_id: Option<String>,
    pub schema_version: u16,
}

pub struct TelemetryService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> TelemetryService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn record_event(
        &self,
        event_type: TelemetryEventType,
        outcome: TelemetryOutcome,
        duration_ms: Option<i64>,
        metadata: Option<serde_json::Value>,
        user_id: Option<&str>,
    ) -> Result<String, AppError> {
        let id = Uuid::new_v4().to_string();
        let schema_version = SYNC_PACKAGE_SCHEMA_VERSION.as_u16();

        let metadata_str = metadata
            .as_ref()
            .and_then(|m| serde_json::to_string(m).ok());

        self.executor.telemetry().insert_event(
            &id,
            event_type.as_str(),
            outcome.as_str(),
            duration_ms,
            metadata_str,
            user_id,
            schema_version,
        )?;

        // Structured logging
        let target = match event_type {
            TelemetryEventType::Backup => "grpc::backup",
            TelemetryEventType::Restore => "grpc::restore",
            TelemetryEventType::RuntimeStartup => "grpc::runtime",
            _ => "grpc::telemetry",
        };

        let duration_display = match duration_ms {
            Some(d) => format!("{}ms", d),
            None => "N/A".to_string(),
        };

        log::info!(
            target: target,
            "telemetry event recorded: type={}, outcome={}, duration={}, id={}",
            event_type.as_str(),
            outcome.as_str(),
            duration_display,
            id
        );

        Ok(id)
    }

    pub fn get_recent_events(&self, limit: u32) -> Result<Vec<TelemetryEvent>, AppError> {
        let rows = self.executor.telemetry().fetch_recent(limit)?;

        let events = rows
            .into_iter()
            .map(|r| TelemetryEvent {
                id: r.id,
                event_type: TelemetryEventType::parse(&r.event_type),
                outcome: TelemetryOutcome::parse(&r.outcome),
                duration_ms: r.duration_ms,
                timestamp: r.timestamp,
                metadata: r.metadata.and_then(|s| serde_json::from_str(&s).ok()),
                user_id: r.user_id,
                schema_version: r.schema_version as u16,
            })
            .collect();

        Ok(events)
    }

    pub fn cleanup_old_events(&self, days: u32) -> Result<usize, AppError> {
        self.executor.telemetry().delete_older_than_days(days)
    }
}
