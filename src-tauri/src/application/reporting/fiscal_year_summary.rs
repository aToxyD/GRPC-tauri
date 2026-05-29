use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::repositories::DbExecutor;
use crate::repositories::RepositoryProvider;

use super::{Report, ReportEnvelope, ReportMetadata};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiscalYearSummaryInput {
    pub fiscal_year: i32,
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
            unit_id: None,
            limit: 0,
            offset: 0,
        };

        let mut total_movements_in: i64 = 0;
        let mut total_movements_out: i64 = 0;
        let mut total_consumption_value: f64 = 0.0;
        let mut total_opening_value: f64 = 0.0;

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
                            total_consumption_value =
                                super::round_money(total_consumption_value + cost * row.quantity);
                        }
                    }
                    "OPENING" => {
                        if let Some(cost) = row.unit_cost {
                            total_opening_value =
                                super::round_money(total_opening_value + cost * row.quantity);
                        }
                    }
                    _ => {}
                }
            }
        }

        let reports = executor.reports();
        let daily_report_count = reports.count_active_reports_by_year(year)?;

        let mut total_beneficiaries: i64 = 0;
        let all_daily = reports.list_daily_reports(None, None)?;
        for r in &all_daily {
            if r.fiscal_year == year {
                total_beneficiaries += r.total_daily_beneficiaries as i64;
            }
        }

        let ending_inventory_value: f64 = executor
            .query_row(
                "SELECT COALESCE(SUM(qty_remaining * unit_cost), 0.0) FROM fifo_stock_layers WHERE origin_fiscal_year = ?1 AND qty_remaining > 0",
                rusqlite::params![year],
                |row| row.get::<_, f64>(0),
            )
            .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?;

        let layer_count: i64 = executor
            .query_row(
                "SELECT COUNT(*) FROM fifo_stock_layers WHERE origin_fiscal_year = ?1 AND qty_remaining > 0",
                rusqlite::params![year],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| FiscalYearSummaryError::Internal(e.to_string()))?;

        let metadata = ReportMetadata::new(Self::slug(), Self::version(), Some(year))
            .with_snapshot_source("fifo_stock_layers + stock_movements + daily_reports".into());

        Ok(ReportEnvelope {
            metadata,
            data: FiscalYearSummaryOutput {
                fiscal_year: year,
                status: status.status,
                total_movements_in,
                total_movements_out,
                total_consumption_value,
                total_opening_value,
                daily_report_count,
                total_beneficiaries,
                ending_inventory_value: super::round_money(ending_inventory_value),
                layer_count,
            },
        })
    }
}
