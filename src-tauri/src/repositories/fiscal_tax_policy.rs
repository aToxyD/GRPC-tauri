//! Fiscal Year Tax Policy Repository Module
//!
//! Handles per-fiscal-year TVA rate persistence (ADR-0055 / SEC-087-F).
//! One immutable rate per fiscal year; once established it cannot be changed;
//! set `frozen = 1` at fiscal closure.
//! ARCHITECTURE: SQL only — no loops, no calculations, no cross-repo calls.

use crate::errors::AppError;
use crate::models::{FiscalYearTaxPolicy, SetTaxPolicyRequest};
use crate::repositories::executor::DbExecutor;
use crate::repositories::numeric_row;
use rusqlite::params;

fn map_tax_policy_row(row: &rusqlite::Row<'_>) -> Result<FiscalYearTaxPolicy, rusqlite::Error> {
    Ok(FiscalYearTaxPolicy {
        fiscal_year: row.get(0)?,
        tva_rate: numeric_row::rate_col(1, row.get::<_, i64>(1)?)?,
        frozen: row.get(2)?,
        set_by: row.get(3)?,
        created_at: row.get(4)?,
    })
}

/// Repository for fiscal-year tax policy operations
pub struct FiscalYearTaxPolicyRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalYearTaxPolicyRepository<'a> {
    /// Create a new FiscalYearTaxPolicyRepository with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn insert_policy(
        &self,
        req: &SetTaxPolicyRequest,
        set_by: &str,
        created_at: &str,
    ) -> Result<(), AppError> {
        let tva_rate_scaled = numeric_row::rate_scaled(req.tva_rate)?;
        self.executor.execute(
            "INSERT INTO fiscal_year_tax_policy (fiscal_year, tva_rate, frozen, set_by, created_at) VALUES (?1, ?2, 0, ?3, ?4)",
            params![req.fiscal_year, tva_rate_scaled, set_by, created_at],
        )?;
        Ok(())
    }

    pub fn get_policy(&self, fiscal_year: i32) -> Result<Option<FiscalYearTaxPolicy>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT fiscal_year, tva_rate, frozen, set_by, created_at FROM fiscal_year_tax_policy WHERE fiscal_year = ?1",
                [fiscal_year],
                map_tax_policy_row,
            )?;
        Ok(result)
    }

    pub fn list_policies(&self) -> Result<Vec<FiscalYearTaxPolicy>, AppError> {
        Ok(self.executor.query_all(
            "SELECT fiscal_year, tva_rate, frozen, set_by, created_at FROM fiscal_year_tax_policy ORDER BY fiscal_year DESC",
            [],
            map_tax_policy_row,
        )?)
    }

    pub fn set_frozen(&self, fiscal_year: i32, frozen: bool) -> Result<usize, AppError> {
        let n = self.executor.execute(
            "UPDATE fiscal_year_tax_policy SET frozen = ?1 WHERE fiscal_year = ?2",
            params![frozen, fiscal_year],
        )?;
        Ok(n)
    }

    pub fn policy_exists(&self, fiscal_year: i32) -> Result<bool, AppError> {
        let existing: Option<i32> = self.executor.query_row_optional(
            "SELECT fiscal_year FROM fiscal_year_tax_policy WHERE fiscal_year = ?1",
            [fiscal_year],
            |row| row.get(0),
        )?;
        Ok(existing.is_some())
    }

    /// WILAYA-authoritative tax-policy upsert (ContractCatalog V2, WILAYA →
    /// UNIT read-only projection). `tva_rate` / `frozen` are WILAYA-owned;
    /// `set_by` / `created_at` are kept from whichever writer established the
    /// year (they are provenance-only on a UNIT).
    pub fn upsert_sync_tax_policy(&self, policy: &FiscalYearTaxPolicy) -> Result<(), AppError> {
        let tva_rate_scaled = numeric_row::rate_scaled(policy.tva_rate)?;
        self.executor.execute(
            "INSERT INTO fiscal_year_tax_policy (fiscal_year, tva_rate, frozen, set_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(fiscal_year) DO UPDATE SET
                tva_rate = excluded.tva_rate,
                frozen = excluded.frozen",
            params![
                policy.fiscal_year,
                tva_rate_scaled,
                policy.frozen as i64,
                policy.set_by,
                policy.created_at,
            ],
        )?;
        Ok(())
    }
}
