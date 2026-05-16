use crate::errors::AppResult;
use crate::repositories::executor::DbExecutor;

pub struct SystemRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> SystemRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn check_integrity(&self) -> AppResult<Vec<String>> {
        self.executor
            .query_map("PRAGMA integrity_check", [], |row| {
                let msg: String = row.get(0)?;
                Ok(msg)
            })
            .map_err(Into::into)
    }

    pub fn get_page_stats(&self) -> AppResult<(i64, i64)> {
        let page_count: i64 = self
            .executor
            .query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size: i64 = self
            .executor
            .query_row("PRAGMA page_size", [], |r| r.get(0))?;
        Ok((page_count, page_size))
    }

    pub fn get_max_schema_version(&self) -> AppResult<i32> {
        let version: i32 = self.executor.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )?;
        Ok(version)
    }
}
