use serde::{Deserialize, Serialize};
use thiserror::Error;

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
        let rows: Vec<(String, String, f64, f64, i64)> = executor
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
                        row.get::<_, f64>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .map_err(|e| InventoryValuationError::Internal(e.to_string()))?;

        let mut products = Vec::with_capacity(rows.len());
        let mut total_value = 0.0_f64;
        let mut total_layers: usize = 0;

        for (product_id, product_name, qty, value, layer_count) in rows {
            let weighted_cost = if qty > 0.0 {
                super::round_money(value / qty)
            } else {
                0.0
            };
            let row_value = super::round_money(value);
            total_value = super::round_money(total_value + row_value);
            total_layers += layer_count as usize;
            products.push(ProductValuationRow {
                product_id,
                product_name,
                total_quantity: super::round_money(qty),
                weighted_avg_unit_cost: weighted_cost,
                total_value: row_value,
                layer_count: layer_count as usize,
            });
        }

        let metadata = ReportMetadata::new(Self::slug(), Self::version(), input.fiscal_year)
            .with_snapshot_source("fifo_stock_layers".into());

        Ok(ReportEnvelope {
            metadata,
            data: InventoryValuationOutput {
                total_inventory_value: super::round_money(total_value),
                product_count: products.len(),
                active_layer_count: total_layers,
                products,
            },
        })
    }
}
