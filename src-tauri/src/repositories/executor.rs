//! DbExecutor - Database execution abstraction
//!
//! This module provides an abstraction over rusqlite's Connection and Transaction types,
//! allowing repositories to work in both transactional and non-transactional contexts
//! without needing direct Database access.
//!
//! ARCHITECTURAL CONSTRAINTS:
//! - Repositories MUST NOT call db.get_connection() directly
//! - All SQL execution goes through DbExecutor
//! - Cross-repository calls MUST reuse the same executor instance

use rusqlite::{Connection, Params, Result as SqliteResult, Row, Statement, Transaction};

/// Abstraction over Connection and Transaction to allow repositories
/// to work in both contexts without needing direct Database access.
#[derive(Clone, Copy)]
pub enum DbExecutor<'a> {
    Conn(&'a Connection),
    Tx(&'a Transaction<'a>),
}

impl<'a> DbExecutor<'a> {
    /// Execute a SQL statement with parameters
    pub fn execute<P: Params>(&self, sql: &str, params: P) -> SqliteResult<usize> {
        match self {
            DbExecutor::Conn(conn) => conn.execute(sql, params),
            DbExecutor::Tx(tx) => tx.execute(sql, params),
        }
    }

    /// Execute of SQL statements
    pub fn execute_script(&self, sql: &str) -> SqliteResult<()> {
        match self {
            DbExecutor::Conn(conn) => conn.execute_batch(sql),
            DbExecutor::Tx(tx) => tx.execute_batch(sql),
        }
    }

    /// Prepare a SQL statement
    pub fn prepare(&self, sql: &str) -> SqliteResult<Statement<'_>> {
        match self {
            DbExecutor::Conn(conn) => conn.prepare(sql),
            DbExecutor::Tx(tx) => tx.prepare(sql),
        }
    }

    /// Query for a single row
    pub fn query_row<T, P, F>(&self, sql: &str, params: P, f: F) -> SqliteResult<T>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> SqliteResult<T>,
    {
        match self {
            DbExecutor::Conn(conn) => conn.query_row(sql, params, f),
            DbExecutor::Tx(tx) => tx.query_row(sql, params, f),
        }
    }

    /// Query for an optional single row
    pub fn query_row_optional<T, P, F>(&self, sql: &str, params: P, f: F) -> SqliteResult<Option<T>>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> SqliteResult<T>,
    {
        use rusqlite::OptionalExtension;
        match self {
            DbExecutor::Conn(conn) => conn.query_row(sql, params, f).optional(),
            DbExecutor::Tx(tx) => tx.query_row(sql, params, f).optional(),
        }
    }

    /// Query and map ALL rows into a Vec.
    ///
    /// Architectural purpose: repositories must stay SQL-only (no loops/iterators).
    /// Row iteration lives here (infrastructure), while repositories provide only
    /// the SQL string + row-mapping closure.
    pub fn query_all<T, P, F>(&self, sql: &str, params: P, f: F) -> SqliteResult<Vec<T>>
    where
        P: Params,
        F: FnMut(&Row<'_>) -> SqliteResult<T>,
    {
        let mut stmt = self.prepare(sql)?;
        let mut rows = stmt.query(params)?;
        crate::db::map_rows_all(&mut rows, f)
    }

    /// Query and map rows (alias for query_all)
    pub fn query_map<T, P, F>(&self, sql: &str, params: P, f: F) -> SqliteResult<Vec<T>>
    where
        P: Params,
        F: FnMut(&Row<'_>) -> SqliteResult<T>,
    {
        self.query_all(sql, params, f)
    }

    /// Query and process rows using an iterator-like closure (bounded peak RAM).
    pub fn query_iter<T, P, F, R>(
        &self,
        sql: &str,
        params: P,
        mut mapping: F,
        mut consumer: R,
    ) -> SqliteResult<()>
    where
        P: Params,
        F: FnMut(&Row<'_>) -> SqliteResult<T>,
        R: FnMut(T) -> Result<(), String>,
    {
        let mut stmt = self.prepare(sql)?;
        let mut rows = stmt.query(params)?;
        while let Some(row) = rows.next()? {
            let item = mapping(row)?;
            consumer(item).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
        }
        Ok(())
    }

    /// Get the last insert rowid
    pub fn last_insert_rowid(&self) -> i64 {
        match self {
            DbExecutor::Conn(conn) => conn.last_insert_rowid(),
            DbExecutor::Tx(tx) => tx.last_insert_rowid(),
        }
    }

    /// Get the number of rows changed by the last statement
    pub fn changes(&self) -> u64 {
        match self {
            DbExecutor::Conn(conn) => conn.changes(),
            DbExecutor::Tx(tx) => tx.changes(),
        }
    }
}

/// Trait for types that can provide a DbExecutor
///
/// This allows both Database and Transaction contexts to be used
/// interchangeably with repositories.
pub trait ExecutorProvider {
    fn executor(&self) -> DbExecutor<'_>;
}

impl ExecutorProvider for Connection {
    fn executor(&self) -> DbExecutor<'_> {
        DbExecutor::Conn(self)
    }
}
