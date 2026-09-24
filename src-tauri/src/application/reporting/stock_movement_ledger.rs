use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::repositories::numeric_row;
use crate::repositories::DbExecutor;

use super::{Report, ReportEnvelope, ReportMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StockMovementLedgerInput {
    pub fiscal_year: Option<i32>,
    pub product_id: Option<String>,
    pub movement_type: Option<String>,
    pub unit_id: Option<String>,
    pub start_timestamp: Option<String>,
    pub end_timestamp: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StockMovementLedgerRow {
    pub id: String,
    pub product_id: String,
    pub product_name: Option<String>,
    pub movement_type: String,
    pub quantity: f64,
    pub balance_before: f64,
    pub balance_after: f64,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub notes: Option<String>,
    pub timestamp: String,
    pub user_id: String,
    pub unit_id: Option<String>,
    pub fiscal_year: Option<i32>,
    pub unit_cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StockMovementLedgerOutput {
    pub rows: Vec<StockMovementLedgerRow>,
    pub total_count: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Error)]
pub enum StockMovementLedgerError {
    #[error("{0}")]
    Internal(String),
}

impl From<crate::errors::AppError> for StockMovementLedgerError {
    fn from(e: crate::errors::AppError) -> Self {
        Self::Internal(e.to_string())
    }
}

pub struct StockMovementLedgerReport;

impl Report for StockMovementLedgerReport {
    type Input = StockMovementLedgerInput;
    type Output = StockMovementLedgerOutput;
    type Error = StockMovementLedgerError;

    fn slug() -> &'static str {
        "stock-movement-ledger"
    }

    fn version() -> u32 {
        1
    }

    fn compute(
        executor: DbExecutor<'_>,
        input: Self::Input,
    ) -> Result<ReportEnvelope<Self::Output>, Self::Error> {
        let total_count: i64 = executor
            .query_row(
                r#"SELECT COUNT(*)
                   FROM stock_movements sm
                   WHERE (?1 IS NULL OR sm.product_id = ?1)
                     AND (?2 IS NULL OR sm.movement_type = ?2)
                     AND (?3 IS NULL OR sm.unit_id = ?3)
                     AND (?4 IS NULL OR sm.timestamp >= ?4)
                     AND (?5 IS NULL OR sm.timestamp <= ?5)
                     AND (?6 IS NULL OR sm.fiscal_year = ?6)"#,
                rusqlite::params![
                    input.product_id,
                    input.movement_type,
                    input.unit_id,
                    input.start_timestamp,
                    input.end_timestamp,
                    input.fiscal_year,
                ],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| StockMovementLedgerError::Internal(e.to_string()))?;

        let rows: Vec<StockMovementLedgerRow> = executor
            .query_all(
                r#"SELECT sm.id, sm.product_id, p.name as product_name, sm.movement_type,
                          sm.quantity, sm.balance_before, sm.balance_after, sm.reference_type,
                          sm.reference_id, sm.notes, sm.timestamp, sm.user_id, sm.unit_id,
                          sm.fiscal_year, sm.unit_cost
                   FROM stock_movements sm
                   LEFT JOIN products p ON sm.product_id = p.id
                   WHERE (?1 IS NULL OR sm.product_id = ?1)
                     AND (?2 IS NULL OR sm.movement_type = ?2)
                     AND (?3 IS NULL OR sm.unit_id = ?3)
                     AND (?4 IS NULL OR sm.timestamp >= ?4)
                     AND (?5 IS NULL OR sm.timestamp <= ?5)
                     AND (?6 IS NULL OR sm.fiscal_year = ?6)
                   ORDER BY sm.timestamp ASC, sm.id ASC
                   LIMIT ?7 OFFSET ?8"#,
                rusqlite::params![
                    input.product_id,
                    input.movement_type,
                    input.unit_id,
                    input.start_timestamp,
                    input.end_timestamp,
                    input.fiscal_year,
                    input.limit,
                    input.offset,
                ],
                |row| {
                    Ok(StockMovementLedgerRow {
                        id: row.get(0)?,
                        product_id: row.get(1)?,
                        product_name: row.get(2)?,
                        movement_type: row.get(3)?,
                        quantity: numeric_row::qty_col(4, row.get::<_, i64>(4)?)?,
                        balance_before: numeric_row::qty_col(5, row.get::<_, i64>(5)?)?,
                        balance_after: numeric_row::qty_col(6, row.get::<_, i64>(6)?)?,
                        reference_type: row.get(7)?,
                        reference_id: row.get(8)?,
                        notes: row.get(9)?,
                        timestamp: row.get(10)?,
                        user_id: row.get(11)?,
                        unit_id: row.get(12)?,
                        fiscal_year: row.get(13)?,
                        unit_cost: match row.get::<_, Option<i64>>(14)? {
                            Some(scaled) => Some(numeric_row::money_col(14, scaled)?),
                            None => None,
                        },
                    })
                },
            )
            .map_err(|e| StockMovementLedgerError::Internal(e.to_string()))?;

        let metadata = ReportMetadata::new(Self::slug(), Self::version(), input.fiscal_year)
            .with_snapshot_source("stock_movements".into());

        Ok(ReportEnvelope {
            metadata,
            data: StockMovementLedgerOutput {
                rows,
                total_count,
                limit: input.limit,
                offset: input.offset,
            },
        })
    }
}
