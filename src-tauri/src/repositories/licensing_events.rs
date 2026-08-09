//! Licensing import/re-verification audit trail (SQL only).
//!
//! ADR-0042 §7 (P4). Append-only log of import and re-verification outcomes
//! (`verified` / `rejected` / `not-for-this-node`). Rows are never updated or
//! deleted by application code.

use crate::errors::{AppError, AppResult, ValidationError};

use super::executor::DbExecutor;

/// A single append-only licensing event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicensingEventRow {
    pub id: Option<i64>,
    pub event_type: String,
    pub outcome: String,
    pub license_id: Option<String>,
    pub artifact_id: Option<String>,
    pub key_id: Option<String>,
    pub message: Option<String>,
    pub recorded_at: String,
}

impl LicensingEventRow {
    /// New event before persistence (`id` is assigned by the store).
    pub fn new(
        event_type: &str,
        outcome: &str,
        license_id: Option<&str>,
        artifact_id: Option<&str>,
        key_id: Option<&str>,
        message: Option<&str>,
    ) -> Self {
        Self {
            id: None,
            event_type: event_type.to_string(),
            outcome: outcome.to_string(),
            license_id: license_id.map(str::to_string),
            artifact_id: artifact_id.map(str::to_string),
            key_id: key_id.map(str::to_string),
            message: message.map(str::to_string),
            recorded_at: String::new(),
        }
    }
}

pub struct LicensingEventsRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> LicensingEventsRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Append an event; `recorded_at` is stamped by the store. Returns the rowid.
    pub fn record(&self, event: &LicensingEventRow) -> AppResult<i64> {
        if event.event_type.trim().is_empty() || event.outcome.trim().is_empty() {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "event".into(),
                message: "event_type and outcome are required".into(),
            }));
        }
        self.executor
            .execute(
                r#"INSERT INTO licensing_events
                       (event_type, outcome, license_id, artifact_id, key_id, message, recorded_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))"#,
                rusqlite::params![
                    event.event_type,
                    event.outcome,
                    event.license_id,
                    event.artifact_id,
                    event.key_id,
                    event.message,
                ],
            )
            .map_err(AppError::from)?;
        Ok(self.executor.last_insert_rowid())
    }

    /// Most recent events, newest first.
    pub fn list_recent(&self, limit: u64) -> AppResult<Vec<LicensingEventRow>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.executor
            .query_all(
                "SELECT id, event_type, outcome, license_id, artifact_id, key_id, message, recorded_at \
                 FROM licensing_events ORDER BY id DESC LIMIT ?1",
                rusqlite::params![limit],
                map_row,
            )
            .map_err(AppError::from)
    }

    pub fn count(&self) -> AppResult<u64> {
        let n: i64 = self
            .executor
            .query_row("SELECT COUNT(*) FROM licensing_events", [], |r| r.get(0))
            .map_err(AppError::from)?;
        Ok(u64::try_from(n).unwrap_or(0))
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LicensingEventRow> {
    Ok(LicensingEventRow {
        id: Some(row.get(0)?),
        event_type: row.get(1)?,
        outcome: row.get(2)?,
        license_id: row.get(3)?,
        artifact_id: row.get(4)?,
        key_id: row.get(5)?,
        message: row.get(6)?,
        recorded_at: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ConnectionFactory, Database};

    fn repo(db: &Database) -> LicensingEventsRepository<'_> {
        LicensingEventsRepository::new(db.executor())
    }

    #[test]
    fn record_appends_and_newest_first() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let r = repo(&db);
        r.record(&LicensingEventRow::new(
            "import",
            "verified",
            Some("lic-1"),
            None,
            None,
            None,
        ))
        .unwrap();
        r.record(&LicensingEventRow::new(
            "import",
            "not-for-this-node",
            None,
            None,
            None,
            Some("subject mismatch"),
        ))
        .unwrap();
        assert_eq!(r.count().unwrap(), 2);

        let recent = r.list_recent(10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].outcome, "not-for-this-node");
        assert_eq!(recent[1].outcome, "verified");
        assert!(!recent[0].recorded_at.is_empty());
        assert!(recent[0].id.is_some());
    }

    #[test]
    fn list_recent_respects_limit() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let r = repo(&db);
        for i in 0..5 {
            r.record(&LicensingEventRow::new(
                "import",
                "verified",
                Some(&format!("l{i}")),
                None,
                None,
                None,
            ))
            .unwrap();
        }
        assert_eq!(r.list_recent(3).unwrap().len(), 3);
    }

    #[test]
    fn rejects_empty_event_or_outcome() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let r = repo(&db);
        assert!(r
            .record(&LicensingEventRow::new(
                "", "verified", None, None, None, None
            ))
            .is_err());
        assert!(r
            .record(&LicensingEventRow::new(
                "import", "", None, None, None, None
            ))
            .is_err());
    }
}
