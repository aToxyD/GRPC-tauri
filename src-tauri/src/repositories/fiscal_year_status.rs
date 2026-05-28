//! SQL-only access to `fiscal_year_status`.

use crate::errors::AppError;
use crate::models::FiscalYearStatus;
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

pub struct FiscalYearStatusRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalYearStatusRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn get_by_year(&self, year: i32) -> Result<Option<FiscalYearStatus>, AppError> {
        self.executor
            .query_row_optional(
                "SELECT year, status, opened_at, closed_at, closed_by, archived FROM fiscal_year_status WHERE year = ?1",
                params![year],
                |row| {
                    Ok(FiscalYearStatus {
                        year: row.get(0)?,
                        status: row.get(1)?,
                        opened_at: row.get(2)?,
                        closed_at: row.get(3)?,
                        closed_by: row.get(4)?,
                        archived: row.get::<_, i32>(5)? == 1,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn update_status(
        &self,
        year: i32,
        status: &str,
        closed_at: Option<&str>,
        closed_by: Option<&str>,
    ) -> Result<(), AppError> {
        crate::application::services::FiscalHistoricalGuard::new(self.executor)
            .assert_fiscal_status_mutation_allowed(year)?;
        self.executor
            .execute(
                "UPDATE fiscal_year_status SET status = ?1, closed_at = ?2, closed_by = ?3 WHERE year = ?4",
                params![status, closed_at, closed_by, year],
            )
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn seed_year(&self, year: i32, status: &str, opened_at: &str) -> Result<(), AppError> {
        self.executor
            .execute(
                "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1, ?2, ?3)",
                params![year, status, opened_at],
            )
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn assert_open(&self, year: i32) -> Result<(), AppError> {
        let status = self.get_by_year(year)?;
        match status {
            Some(s) if s.status == "open" => Ok(()),
            Some(_) => Err(AppError::BusinessLogic(
                crate::errors::BusinessLogicError::FiscalYearClosed { year },
            )),
            None => Err(AppError::BusinessLogic(
                crate::errors::BusinessLogicError::FiscalYearNotFound { year },
            )),
        }
    }

    pub fn is_year_archived(&self, year: i32) -> Result<bool, AppError> {
        let archived: Option<i32> = self.executor.query_row_optional(
            "SELECT archived FROM fiscal_year_status WHERE year = ?1",
            params![year],
            |r| r.get(0),
        )?;
        Ok(archived.map(|v| v == 1).unwrap_or(false))
    }

    pub fn max_archived_year(&self) -> Result<Option<i32>, AppError> {
        self.executor
            .query_row_optional(
                "SELECT MAX(year) FROM fiscal_year_status WHERE archived = 1",
                [],
                |r| r.get::<_, Option<i32>>(0),
            )
            .map(|opt| opt.flatten())
            .map_err(Into::into)
    }

    pub fn count_open_years(&self) -> Result<i64, AppError> {
        self.executor
            .query_row(
                "SELECT COUNT(*) FROM fiscal_year_status WHERE status = 'open'",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub fn get_open_year(&self) -> Result<Option<i32>, AppError> {
        self.executor
            .query_row_optional(
                "SELECT year FROM fiscal_year_status WHERE status = 'open' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub fn count_archived(&self) -> Result<i64, AppError> {
        self.executor
            .query_row(
                "SELECT COUNT(*) FROM fiscal_year_status WHERE archived = 1",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub fn count_closed(&self) -> Result<i64, AppError> {
        self.executor
            .query_row(
                "SELECT COUNT(*) FROM fiscal_year_status WHERE status = 'closed'",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub fn get_all_years(&self) -> Result<Vec<i32>, AppError> {
        self.executor
            .query_all(
                "SELECT year FROM fiscal_year_status ORDER BY year",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub fn count_invariant_violations(&self) -> Result<(i64, i64, i64), AppError> {
        let v1: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM fiscal_year_status WHERE archived = 1 AND status != 'closed'",
            [],
            |r| r.get(0),
        )?;
        let v2: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM fiscal_year_status WHERE archived = 1 AND closed_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        let v3: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM fiscal_year_status WHERE archived = 1 AND status = 'open'",
            [],
            |r| r.get(0),
        )?;
        Ok((v1, v2, v3))
    }

    pub fn archive_year(&self, year: i32) -> Result<(), AppError> {
        self.executor
            .execute(
                "UPDATE fiscal_year_status SET archived = 1 WHERE year = ?1 AND status = 'closed'",
                params![year],
            )
            .map(|_| ())
            .map_err(Into::into)
    }
}
