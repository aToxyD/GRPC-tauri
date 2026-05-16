use crate::errors::AppError;
use crate::repositories::DbExecutor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEventDbRow {
    pub id: String,
    pub event_type: String,
    pub outcome: String,
    pub duration_ms: Option<i64>,
    pub timestamp: String,
    pub metadata: Option<String>,
    pub user_id: Option<String>,
    pub schema_version: i64,
}

pub struct TelemetryRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> TelemetryRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_event(
        &self,
        id: &str,
        event_type: &str,
        outcome: &str,
        duration_ms: Option<i64>,
        metadata: Option<String>,
        user_id: Option<&str>,
        schema_version: u16,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO telemetry_events (id, event_type, outcome, duration_ms, metadata, user_id, schema_version)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                id,
                event_type,
                outcome,
                duration_ms,
                metadata,
                user_id,
                schema_version
            ],
        )?;
        Ok(())
    }

    pub fn fetch_recent(&self, limit: u32) -> Result<Vec<TelemetryEventDbRow>, AppError> {
        let rows = self.executor.query_map(
            "SELECT id, event_type, outcome, duration_ms, timestamp, metadata, user_id, schema_version 
             FROM telemetry_events 
             ORDER BY timestamp DESC 
             LIMIT ?",
            rusqlite::params![limit],
            |row| {
                Ok(TelemetryEventDbRow {
                    id: row.get(0)?,
                    event_type: row.get(1)?,
                    outcome: row.get(2)?,
                    duration_ms: row.get(3)?,
                    timestamp: row.get(4)?,
                    metadata: row.get(5)?,
                    user_id: row.get(6)?,
                    schema_version: row.get(7)?,
                })
            },
        )?;
        Ok(rows)
    }

    pub fn delete_older_than_days(&self, days: u32) -> Result<usize, AppError> {
        let deleted = self.executor.execute(
            "DELETE FROM telemetry_events WHERE timestamp < datetime('now', '-' || ? || ' days')",
            rusqlite::params![days],
        )?;
        Ok(deleted)
    }
}
