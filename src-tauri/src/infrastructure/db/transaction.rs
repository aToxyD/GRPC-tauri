use crate::domain::events::{EventBuffer, EventContext};
use crate::errors::AppError;
use crate::repositories::DbExecutor;
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
}
