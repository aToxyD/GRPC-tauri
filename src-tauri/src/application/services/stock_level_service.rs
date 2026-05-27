//! Stock Level Service Module
//!
//! Handles querying and updating stock levels directly.

use crate::errors::AppError;
use crate::models::{ConsumptionItemInput, InventoryStock, StockCheckResult, StockSummary};
use crate::repositories::{DbExecutor, RepositoryProvider};

/// Service for handling current stock levels
pub struct StockLevelService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> StockLevelService<'a> {
    /// Create a new StockLevelService
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn get_stock_summary(&self) -> Result<Vec<StockSummary>, AppError> {
        self.executor.inventory().get_stock_summary()
    }

    pub fn get_stock(&self, product_id: &str) -> Result<Option<InventoryStock>, AppError> {
        if product_id.trim().is_empty() {
            return Err(AppError::Internal("product_id is required".to_string()));
        }
        self.executor.inventory().get_stock(product_id)
    }

    pub fn get_all_stocks(&self) -> Result<Vec<InventoryStock>, AppError> {
        self.executor.inventory().get_all_stocks()
    }

    pub fn update_stock(&self, product_id: &str, quantity: f64) -> Result<(), AppError> {
        if product_id.trim().is_empty() {
            return Err(AppError::Internal("product_id is required".to_string()));
        }
        if quantity < 0.0 {
            return Err(AppError::Internal(
                "quantity cannot be negative".to_string(),
            ));
        }
        self.executor.inventory().update_stock(product_id, quantity)
    }

    pub fn check_stock_availability(
        &self,
        items: Vec<ConsumptionItemInput>,
    ) -> Result<Vec<StockCheckResult>, AppError> {
        let mut results = Vec::new();
        for item in items {
            let stock = self.executor.inventory().get_stock(&item.product_id)?;
            let (available_qty, product_name) = match stock {
                Some(s) => (s.quantity, s.product_name),
                None => (0.0, "غير معروف".to_string()),
            };
            let available = available_qty >= item.quantity;
            let deficit = if available {
                0.0
            } else {
                item.quantity - available_qty
            };
            results.push(StockCheckResult {
                available,
                product_id: item.product_id,
                product_name,
                requested: item.quantity,
                available_stock: available_qty,
                deficit,
            });
        }
        Ok(results)
    }

    pub fn get_total_inventory_value(&self) -> Result<f64, AppError> {
        let settings = self.executor.settings().get_settings_row()?;
        if settings.node_type != crate::models::NodeType::Wilaya {
            return Err(AppError::BusinessLogic(
                crate::errors::BusinessLogicError::OperationNotPermitted {
                    message: "get_total_inventory_value is only available on WILAYA nodes"
                        .to_string(),
                },
            ));
        }
        self.executor.inventory().get_total_inventory_value()
    }

    pub fn get_remaining_layers(
        &self,
        unit_id: &str,
        product_id: &str,
    ) -> Result<Vec<crate::models::FifoStockLayer>, AppError> {
        self.executor
            .fifo_layers()
            .get_remaining_layers(unit_id, product_id)
    }

    /// Full inventory FIFO view for the UNIT StockPage.
    pub fn get_inventory_fifo_view(
        &self,
        unit_id: &str,
    ) -> Result<crate::models::InventoryStockPageView, AppError> {
        use crate::models::{InventoryLayerView, InventoryProductView, InventoryStockPageView};
        use std::collections::HashMap;

        let rows = self
            .executor
            .fifo_layers()
            .get_inventory_fifo_view(unit_id)?;

        let mut product_map: HashMap<
            String,
            (String, Vec<InventoryLayerView>, f64, Option<String>),
        > = HashMap::new();

        for row in rows {
            let entry = product_map
                .entry(row.product_id)
                .or_insert_with(|| (row.product_name, Vec::new(), 0.0, None));

            let received_at = row.received_at;
            entry.1.push(InventoryLayerView {
                layer_id: row.layer_id,
                source_type: Some(row.source_type),
                received_at: received_at.clone(),
                qty_remaining: row.qty_remaining,
                unit_cost: row.unit_cost,
                layer_value: row.qty_remaining * row.unit_cost,
            });
            entry.2 += row.qty_remaining;

            match &entry.3 {
                None => entry.3 = Some(received_at),
                Some(old) => {
                    if received_at < *old {
                        entry.3 = Some(received_at);
                    }
                }
            }
        }

        let mut products = Vec::new();
        let mut total_value = 0.0f64;
        let mut total_layers = 0usize;

        for (_pid, (pname, layers, total_qty, oldest)) in product_map {
            let product_value: f64 = layers.iter().map(|l| l.layer_value).sum();
            total_value += product_value;
            let layer_count = layers.len();
            total_layers += layer_count;
            products.push(InventoryProductView {
                product_id: _pid,
                product_name: pname,
                total_quantity: total_qty,
                total_value: product_value,
                oldest_layer_date: oldest,
                layer_count,
                layers,
            });
        }

        products.sort_by(|a, b| a.product_name.cmp(&b.product_name));

        Ok(InventoryStockPageView {
            total_inventory_value: total_value,
            total_products: products.len(),
            total_active_layers: total_layers,
            products,
        })
    }

    pub fn get_consumption_history(
        &self,
        movement_id: &str,
    ) -> Result<Vec<crate::models::InventoryLayerConsumption>, AppError> {
        self.executor
            .fifo_layers()
            .get_consumption_history(movement_id)
    }
}
