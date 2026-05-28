use crate::domain::events::{EventBuffer, EventContext};
use crate::errors::AppError;
use crate::repositories::DbExecutor;
use crate::repositories::DomainEventRepository;
use rusqlite::Connection;

/// Context for an active database transaction.
pub struct TxContext<'a> {
    pub executor: DbExecutor<'a>,
}

/// Transaction helper used by the DB facade.
pub struct TransactionService;

impl TransactionService {
    pub fn with_transaction<F, T>(conn: &mut Connection, f: F) -> Result<T, AppError>
    where
        F: FnOnce(DbExecutor<'_>) -> Result<T, AppError>,
    {
        let tx = conn.transaction()?;
        let executor = DbExecutor::Tx(&tx);
        let result = f(executor)?;
        tx.commit()?;
        Ok(result)
    }

    /// Execute within a transaction with domain event support.
    ///
    /// Creates an `EventContext` that provides both database access
    /// and an event emission API. On success returns `(result, buffer)`
    /// where `buffer` contains all emitted events in emission order.
    /// On rollback the buffer is discarded.
    ///
    /// Events are NOT automatically persisted. Use `with_event_persistence`
    /// for automatic persistence before commit.
    pub fn with_event_context<F, T>(
        conn: &mut Connection,
        f: F,
    ) -> Result<(T, EventBuffer), AppError>
    where
        F: FnOnce(&mut EventContext<'_>) -> Result<T, AppError>,
    {
        let tx = conn.transaction()?;
        let (result, buffer) = {
            let executor = DbExecutor::Tx(&tx);
            let mut ctx = EventContext::new(executor);
            let result = f(&mut ctx)?;
            (result, ctx.into_buffer())
        };
        tx.commit()?;
        Ok((result, buffer))
    }

    /// Execute within a transaction with automatic event persistence.
    ///
    /// Works like `with_event_context`, but additionally persists all
    /// buffered events to the `domain_events` table before committing.
    ///
    /// Persistence guarantees:
    /// - Events are persisted inside the same SQLite transaction
    /// - If the operation rolls back, events are discarded (no partial persistence)
    /// - If commit succeeds, events are atomically persisted with state changes
    /// - Events are inserted in emission order
    pub fn with_event_persistence<F, T>(
        conn: &mut Connection,
        f: F,
    ) -> Result<(T, EventBuffer), AppError>
    where
        F: FnOnce(&mut EventContext<'_>) -> Result<T, AppError>,
    {
        let tx = conn.transaction()?;
        let (result, buffer) = {
            let executor = DbExecutor::Tx(&tx);
            let mut ctx = EventContext::new(executor);

            // Run the service operation — emits events into the buffer
            let result = f(&mut ctx)?;

            // Consume context to release ownership of executor
            let buffer = ctx.into_buffer();

            // Persist buffered events before commit (same SQLite transaction)
            if !buffer.is_empty() {
                let repo = DomainEventRepository::new(DbExecutor::Tx(&tx));
                repo.insert_batch(buffer.events())?;
            }

            (result, buffer)
        };
        tx.commit()?;
        Ok((result, buffer))
    }
}
