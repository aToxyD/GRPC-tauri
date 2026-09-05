use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::numeric::legacy_float::{money_from_f64, money_to_f64, quantity_from_f64};
use crate::domain::numeric::Money;
use crate::repositories::DbExecutor;
use crate::repositories::RepositoryProvider;

use super::{Report, ReportEnvelope, ReportMetadata};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiscalYearSummaryInput {
    pub fiscal_year: i32,
    pub unit_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FiscalYearSummaryOutput {
    pub fiscal_year: i32,
    pub status: String,
    pub total_movements_in: i64,
    pub total_movements_out: i64,
    pub total_consumption_value: f64,
    pub total_opening_value: f64,
    pub daily_report_count: i64,
    pub total_beneficiaries: i64,
    pub ending_inventory_value: f64,
    pub layer_count: i64,
}

#[derive(Debug, Error)]
pub enum FiscalYearSummaryError {
    #[error("Fiscal year not found: {0}")]
    YearNotFound(i32),
    #[error("{0}")]
    Internal(String),
}

impl From<crate::errors::AppError> for FiscalYearSummaryError {
    fn from(e: crate::errors::AppError) -> Self {
        Self::Internal(e.to_string())
    }
}

pub struct FiscalYearSummaryReport;

impl Report for FiscalYearSummaryReport {
    type Input = FiscalYearSummaryInput;
    type Output = FiscalYearSummaryOutput;
    type Error = FiscalYearSummaryError;

    fn slug() -> &'static str {
        "fiscal-year-summary"
    }

    fn version() -> u32 {
        1
    }

    fn compute(
        executor: DbExecutor<'_>,
        input: Self::Input,
    ) -> Result<ReportEnvelope<Self::Output>, Self::Error> {
        let year = input.fiscal_year;
        let unit_id = input.unit_id;

        let status = executor
            .fiscal_year_status()
            .get_by_year(year)?
            .ok_or(FiscalYearSummaryError::YearNotFound(year))?;

        let movements = executor.stock_movements();
        let stock_movements_query = crate::models::StockMovementQuery {
            product_id: None,
            movement_type: None,
            start_timestamp: None,
            end_timestamp: None,
            reference_type: None,
            reference_id: None,
            unit_id: unit_id.clone(),
            limit: 0,
            offset: 0,
        };

        let mut total_movements_in: i64 = 0;
        let mut total_movements_out: i64 = 0;
        let mut total_consumption_value = Money::zero();
        let mut total_opening_value = Money::zero();

        let all_movements = movements.fetch_stock_movements(&stock_movements_query)?;

        for row in &all_movements {
            if row.fiscal_year == Some(year) || row.fiscal_year.is_none() {
                match row.movement_type.as_str() {
                    "IN" => {
                        total_movements_in += 1;
                    }
                    "OUT" => {
                        total_movements_out += 1;
                        if let Some(cost) = row.unit_cost {
                            // ADR-0048: OUT value = unit cost (Money) × quantity,
                            // summed exactly and rounded exactly once at output.
                            let unit = money_from_f64(cost)
                                .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?;
                            let qty = quantity_from_f64(row.quantity)
                                .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?;
                            total_consumption_value =
                                total_consumption_value
                                    .checked_add(unit.checked_mul_quantity(&qty).map_err(|e| {
                                        FiscalYearSummaryError::Internal(e.to_string())
                                    })?)
                                    .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?;
                        }
                    }
                    "OPENING" => {
                        if let Some(cost) = row.unit_cost {
                            let unit = money_from_f64(cost)
                                .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?;
                            let qty = quantity_from_f64(row.quantity)
                                .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?;
                            total_opening_value =
                                total_opening_value
                                    .checked_add(unit.checked_mul_quantity(&qty).map_err(|e| {
                                        FiscalYearSummaryError::Internal(e.to_string())
                                    })?)
                                    .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?;
                        }
                    }
                    _ => {}
                }
            }
        }

        let reports = executor.reports();
        let mut daily_report_count: i64 = 0;
        let mut total_beneficiaries: i64 = 0;
        let all_daily = reports.list_daily_reports(None, None)?;
        for r in &all_daily {
            if r.fiscal_year == year {
                let unit_match = unit_id.is_none() || r.unit_id.as_deref() == unit_id.as_deref();
                if unit_match {
                    daily_report_count += 1;
                    total_beneficiaries += r.total_daily_beneficiaries as i64;
                }
            }
        }

        let (ending_inventory_value, layer_count) = if let Some(ref uid) = unit_id {
            let inv: f64 = executor
                .query_row(
                    "SELECT COALESCE(SUM(qty_remaining * unit_cost), 0.0) FROM fifo_stock_layers WHERE origin_fiscal_year = ?1 AND qty_remaining > 0 AND unit_id = ?2",
                    rusqlite::params![year, uid],
                    |row| row.get::<_, f64>(0),
                )
                .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?;
            let cnt: i64 = executor
                .query_row(
                    "SELECT COUNT(*) FROM fifo_stock_layers WHERE origin_fiscal_year = ?1 AND qty_remaining > 0 AND unit_id = ?2",
                    rusqlite::params![year, uid],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?;
            (inv, cnt)
        } else {
            let inv: f64 = executor
                .query_row(
                    "SELECT COALESCE(SUM(qty_remaining * unit_cost), 0.0) FROM fifo_stock_layers WHERE origin_fiscal_year = ?1 AND qty_remaining > 0",
                    rusqlite::params![year],
                    |row| row.get::<_, f64>(0),
                )
                .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?;
            let cnt: i64 = executor
                .query_row(
                    "SELECT COUNT(*) FROM fifo_stock_layers WHERE origin_fiscal_year = ?1 AND qty_remaining > 0",
                    rusqlite::params![year],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?;
            (inv, cnt)
        };

        let metadata = ReportMetadata::new(Self::slug(), Self::version(), Some(year))
            .with_snapshot_source("fifo_stock_layers + stock_movements + daily_reports".into());

        Ok(ReportEnvelope {
            metadata,
            data: FiscalYearSummaryOutput {
                fiscal_year: year,
                status: status.status,
                total_movements_in,
                total_movements_out,
                // Money sums rounded exactly once at the scale-2 wire boundary.
                total_consumption_value: money_to_f64(&total_consumption_value)
                    .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?,
                total_opening_value: money_to_f64(&total_opening_value)
                    .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?,
                daily_report_count,
                total_beneficiaries,
                // SQL aggregate over REAL columns is a boundary value; the
                // scale-2 normalization happens here, not mid-arithmetic.
                ending_inventory_value: money_to_f64(
                    &money_from_f64(ending_inventory_value)
                        .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?,
                )
                .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?,
                layer_count,
            },
        })
    }
}
