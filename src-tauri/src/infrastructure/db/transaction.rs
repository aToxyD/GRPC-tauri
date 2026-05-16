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
}
