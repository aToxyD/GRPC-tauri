use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::numeric::legacy_float::{money_to_f64, quantity_to_f64};
use crate::domain::numeric::{Money, Quantity};
use crate::repositories::numeric_row;
use crate::repositories::DbExecutor;

use super::{Report, ReportEnvelope, ReportMetadata};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InventoryValuationInput {
    pub fiscal_year: Option<i32>,
    pub unit_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProductValuationRow {
    pub product_id: String,
    pub product_name: String,
    pub total_quantity: f64,
    pub weighted_avg_unit_cost: f64,
    pub total_value: f64,
    pub layer_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InventoryValuationOutput {
    pub total_inventory_value: f64,
    pub product_count: usize,
    pub active_layer_count: usize,
    pub products: Vec<ProductValuationRow>,
}

#[derive(Debug, Error)]
pub enum InventoryValuationError {
    #[error("{0}")]
    Internal(String),
}

impl From<crate::errors::AppError> for InventoryValuationError {
    fn from(e: crate::errors::AppError) -> Self {
        Self::Internal(e.to_string())
    }
}

pub struct InventoryValuationReport;

impl Report for InventoryValuationReport {
    type Input = InventoryValuationInput;
    type Output = InventoryValuationOutput;
    type Error = InventoryValuationError;

    fn slug() -> &'static str {
        "inventory-valuation"
    }

    fn version() -> u32 {
        1
    }

    fn compute(
        executor: DbExecutor<'_>,
        input: Self::Input,
    ) -> Result<ReportEnvelope<Self::Output>, Self::Error> {
        // INTEGER boundary: `SUM(f.qty_remaining)` is scale-3 quantity, and
        // `SUM(f.qty_remaining * f.unit_cost)` equals value_DA × 100000.
        // All valuation arithmetic below is exact Money/Quantity.
        let rows: Vec<(String, String, i64, i64, i64)> = executor
            .query_all(
                r#"SELECT f.product_id, p.name,
                          SUM(f.qty_remaining), SUM(f.qty_remaining * f.unit_cost), COUNT(*)
                   FROM fifo_stock_layers f
                   JOIN products p ON f.product_id = p.id
                   WHERE f.qty_remaining > 0
                     AND (?1 IS NULL OR f.unit_id = ?1)
                     AND (?2 IS NULL OR f.origin_fiscal_year = ?2)
                   GROUP BY f.product_id, p.name
                   ORDER BY p.name COLLATE NOCASE"#,
                rusqlite::params![input.unit_id, input.fiscal_year],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .map_err(|e| InventoryValuationError::Internal(e.to_string()))?;

        let mut products = Vec::with_capacity(rows.len());
        let mut total_value = Money::zero();
        let mut total_layers: usize = 0;

        for (product_id, product_name, qty_scaled, value_times_100000, layer_count) in rows {
            let qty = Quantity::from_scaled_i64(qty_scaled)
                .map_err(|e| InventoryValuationError::Internal(e.to_string()))?;
            let value = numeric_row::money_sum(value_times_100000)
                .map_err(|e| InventoryValuationError::Internal(e.to_string()))?;

            // Weighted average unit cost = FIFO value ÷ remaining quantity
            // (dedicated Money÷Quantity operation). Zero/empty quantity → zero.
            let weighted_avg_unit_cost = if qty.is_positive() {
                value
                    .checked_div_quantity(&qty)
                    .and_then(|cost| money_to_f64(&cost))
                    .map_err(|e| InventoryValuationError::Internal(e.to_string()))?
            } else {
                0.0
            };

            let row_value = money_to_f64(&value)
                .map_err(|e| InventoryValuationError::Internal(e.to_string()))?;
            total_value = total_value
                .checked_add(value)
                .map_err(|e| InventoryValuationError::Internal(e.to_string()))?;
            total_layers += layer_count as usize;
            products.push(ProductValuationRow {
                product_id,
                product_name,
                total_quantity: quantity_to_f64(&qty)
                    .map_err(|e| InventoryValuationError::Internal(e.to_string()))?,
                weighted_avg_unit_cost,
                total_value: row_value,
                layer_count: layer_count as usize,
            });
        }

        let metadata = ReportMetadata::new(Self::slug(), Self::version(), input.fiscal_year)
            .with_snapshot_source("fifo_stock_layers".into());

        Ok(ReportEnvelope {
            metadata,
            data: InventoryValuationOutput {
                total_inventory_value: money_to_f64(&total_value)
                    .map_err(|e| InventoryValuationError::Internal(e.to_string()))?,
                product_count: products.len(),
                active_layer_count: total_layers,
                products,
            },
        })
    }
}
