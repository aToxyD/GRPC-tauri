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

    pub fn get_consumption_history(
        &self,
        movement_id: &str,
    ) -> Result<Vec<crate::models::InventoryLayerConsumption>, AppError> {
        self.executor
            .fifo_layers()
            .get_consumption_history(movement_id)
    }
}
