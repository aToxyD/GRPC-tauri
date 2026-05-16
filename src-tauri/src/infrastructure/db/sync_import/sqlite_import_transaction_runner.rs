use crate::application::sync::ImportTransactionRunner;
use crate::db::Database;
use crate::errors::AppResult;
use crate::repositories::DbExecutor;

pub struct SqliteImportTransactionRunner<'a> {
    db: &'a mut Database,
}

impl<'a> SqliteImportTransactionRunner<'a> {
    pub fn new(db: &'a mut Database) -> Self {
        Self { db }
    }
}

impl ImportTransactionRunner for SqliteImportTransactionRunner<'_> {
    fn run<T, F>(&mut self, operation: F) -> AppResult<T>
    where
        F: FnOnce(DbExecutor<'_>) -> AppResult<T>,
    {
        self.db.with_transaction(operation)
    }
}
