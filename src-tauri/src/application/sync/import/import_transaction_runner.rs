use crate::errors::AppResult;
use crate::repositories::DbExecutor;

pub trait ImportTransactionRunner {
    fn run<T, F>(&mut self, operation: F) -> AppResult<T>
    where
        F: FnOnce(DbExecutor<'_>) -> AppResult<T>;
}
