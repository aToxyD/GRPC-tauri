use crate::application::reporting::fiscal_year_summary::{
    FiscalYearSummaryInput, FiscalYearSummaryOutput, FiscalYearSummaryReport,
};
use crate::application::reporting::inventory_valuation::{
    InventoryValuationInput, InventoryValuationOutput, InventoryValuationReport,
};
use crate::application::reporting::stock_movement_ledger::{
    StockMovementLedgerInput, StockMovementLedgerOutput, StockMovementLedgerReport,
};
use crate::application::reporting::Report;
use crate::repositories::DbExecutor;

/// Read-only context exposing canonical report outputs.
///
/// KPIs consume report data through this context only.
/// No SQL, no repositories, no DbExecutor access from KPI implementations.
pub struct ReportsContext<'a> {
    executor: DbExecutor<'a>,
    fiscal_year: i32,
}

impl<'a> ReportsContext<'a> {
    pub fn new(executor: DbExecutor<'a>, fiscal_year: i32) -> Self {
        Self {
            executor,
            fiscal_year,
        }
    }

    pub fn fiscal_year(&self) -> i32 {
        self.fiscal_year
    }

    /// Returns the FiscalYearSummary for this context's fiscal year.
    /// Returns Ok(None) if the year doesn't exist (no data yet).
    /// Returns Err for actual DB/query failures.
    pub fn fiscal_year_summary(
        &self,
    ) -> Result<Option<FiscalYearSummaryOutput>, String>
    {
        match FiscalYearSummaryReport::compute(self.executor, FiscalYearSummaryInput {
            fiscal_year: self.fiscal_year,
        }) {
            Ok(e) => Ok(Some(e.data)),
            Err(crate::application::reporting::fiscal_year_summary::FiscalYearSummaryError::YearNotFound(_)) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Returns the InventoryValuation for an optional fiscal year and unit.
    pub fn inventory_valuation(
        &self,
        fiscal_year: Option<i32>,
        unit_id: Option<String>,
    ) -> Result<
        InventoryValuationOutput,
        crate::application::reporting::inventory_valuation::InventoryValuationError,
    > {
        InventoryValuationReport::compute(self.executor, InventoryValuationInput {
            fiscal_year,
            unit_id,
        })
        .map(|e| e.data)
    }

    /// Returns stock movements filtered by the given parameters.
    pub fn stock_movement_ledger(
        &self,
        input: StockMovementLedgerInput,
    ) -> Result<
        StockMovementLedgerOutput,
        crate::application::reporting::stock_movement_ledger::StockMovementLedgerError,
    > {
        StockMovementLedgerReport::compute(self.executor, input).map(|e| e.data)
    }
}
