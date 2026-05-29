use crate::domain::events::{DomainEvent, EventCategory, StoredEvent};
use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use rusqlite::params;
use uuid::Uuid;

/// A sequence gap detected in persisted domain events.
#[derive(Debug, Clone, PartialEq)]
pub struct EventSequenceGap {
    pub transaction_id: Uuid,
    pub expected_count: u64,
    pub actual_count: u64,
}

pub struct DomainEventRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> DomainEventRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Persist a batch of events inside the current transaction.
    ///
    /// Events are inserted in emission order to preserve deterministic
    /// ordering within the SQLite page structure. The `UNIQUE(transaction_id, sequence_number)`
    /// constraint prevents duplicate entries.
    pub fn insert_batch(&self, events: &[StoredEvent]) -> Result<(), AppError> {
        if events.is_empty() {
            return Ok(());
        }

        let mut stmt = self.executor.prepare(
            "INSERT INTO domain_events (transaction_id, sequence_number, category, event_type, event_body)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        for event in events {
            let tx_str = event.transaction_id.to_string();
            let body = event.event_body_json()?;
            stmt.execute(params![
                tx_str,
                event.sequence_number as i64,
                event.category.as_str(),
                event.event_type_str(),
                body,
            ])?;
        }

        Ok(())
    }

    fn row_to_stored_event(
        seq: i64,
        tx_str: String,
        cat_str: String,
        body: String,
    ) -> Result<StoredEvent, AppError> {
        let tx_id: Uuid = tx_str.parse()?;
        let category: EventCategory = cat_str.parse().map_err(AppError::Internal)?;
        let event: DomainEvent = serde_json::from_str(&body)?;
        Ok(StoredEvent {
            sequence_number: seq as u64,
            transaction_id: tx_id,
            category,
            event,
        })
    }

    /// Retrieve all events for a specific transaction, ordered by sequence_number.
    pub fn find_by_transaction_id(&self, tx_id: Uuid) -> Result<Vec<StoredEvent>, AppError> {
        let tx_str = tx_id.to_string();
        let rows = self.executor.query_all(
            "SELECT sequence_number, transaction_id, category, event_body
             FROM domain_events
             WHERE transaction_id = ?1
             ORDER BY sequence_number ASC",
            params![tx_str],
            |row| {
                let seq: i64 = row.get(0)?;
                let tx_str: String = row.get(1)?;
                let cat_str: String = row.get(2)?;
                let body: String = row.get(3)?;
                Self::row_to_stored_event(seq, tx_str, cat_str, body)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
            },
        )?;

        Ok(rows)
    }

    /// Retrieve all events ordered by the canonical replay ordering:
    /// `(transaction_id ASC, sequence_number ASC)`.
    pub fn find_all_ordered(&self) -> Result<Vec<StoredEvent>, AppError> {
        let rows = self.executor.query_all(
            "SELECT sequence_number, transaction_id, category, event_body
             FROM domain_events
             ORDER BY transaction_id ASC, sequence_number ASC",
            [],
            |row| {
                let seq: i64 = row.get(0)?;
                let tx_str: String = row.get(1)?;
                let cat_str: String = row.get(2)?;
                let body: String = row.get(3)?;
                Self::row_to_stored_event(seq, tx_str, cat_str, body)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
            },
        )?;

        Ok(rows)
    }

    /// Detect transactions with non-contiguous sequence numbers.
    ///
    /// Returns `Vec<EventSequenceGap>` describing every transaction where
    /// `MAX(sequence_number) - MIN(sequence_number) + 1 != COUNT(*)`.
    /// An empty vec means all persisted event sequences are contiguous.
    pub fn detect_sequence_gaps(&self) -> Result<Vec<EventSequenceGap>, AppError> {
        let gaps = self.executor.query_all(
            "SELECT transaction_id,
                    COUNT(*) as actual_count,
                    MAX(sequence_number) - MIN(sequence_number) + 1 as expected_count
             FROM domain_events
             GROUP BY transaction_id
             HAVING actual_count != expected_count",
            [],
            |row| {
                let tx_str: String = row.get(0)?;
                let actual: i64 = row.get(1)?;
                let expected: i64 = row.get(2)?;

                let tx_id: Uuid = tx_str
                    .parse()
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

                Ok(EventSequenceGap {
                    transaction_id: tx_id,
                    expected_count: expected as u64,
                    actual_count: actual as u64,
                })
            },
        )?;

        Ok(gaps)
    }
}
