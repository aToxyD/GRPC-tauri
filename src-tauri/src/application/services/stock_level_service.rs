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

    /// Get stock summary, optionally scoped to a specific fiscal year.
    ///
    /// When `fiscal_year` is `Some`, movement statistics (total_in, total_out,
    /// movement_count) include only movements belonging to that fiscal year.
    /// The `current_quantity` field always reflects the real inventory stock.
    pub fn get_stock_summary(
        &self,
        fiscal_year: Option<i32>,
    ) -> Result<Vec<StockSummary>, AppError> {
        self.executor.inventory().get_stock_summary(fiscal_year)
    }

    /// Get stock summary with automatic fiscal-year scoping for Unit nodes.
    ///
    /// For Unit nodes (`unit_node = true`), the open fiscal year is resolved
    /// from `FiscalYearStatusRepository` and movement statistics are scoped.
    /// For Wilaya nodes (`unit_node = false`), no fiscal-year filter is applied.
    /// `current_quantity` always reflects the real inventory stock.
    pub fn get_stock_summary_scoped(&self, unit_node: bool) -> Result<Vec<StockSummary>, AppError> {
        let fiscal_year = if unit_node {
            self.executor.fiscal_year_status().get_open_year()?
        } else {
            None
        };
        self.get_stock_summary(fiscal_year)
    }

    /// Authoritative consumption-unit key for a product (SEC-087 Phase 5),
    /// resolved from backend product configuration — the single source of truth
    /// for stock identity (AGENTS.md A1/A3).
    ///
    /// A configured product resolves to its authoritative `consumption_unit`
    /// code; an existing-but-unconfigured/legacy product resolves to the legacy
    /// NULL stock identity. This helper resolves the product's configuration,
    /// NOT its existence: `get_product_config_codes()` returns `None` for both
    /// a missing product and an unconfigured one, so the `None` output must
    /// never be interpreted as a product-existence verdict. Missing products
    /// cannot expose inventory rows regardless of the resolved key, because
    /// `get_stock_typed`/`get_stock_summary` look stock up through the
    /// structural `products` lookup plus the FK relationship
    /// (`inventory_stocks.product_id → products.id`, ON DELETE CASCADE), which
    /// makes a stock row for an unknown product unmatchable. Unit codes are
    /// never derived from purchase_unit, FIFO layers, or UI input here.
    fn resolve_consumption_unit(&self, product_id: &str) -> Result<Option<i32>, AppError> {
        Ok(self
            .executor
            .products()
            .get_product_config_codes(product_id)?
            .and_then(|c| c.consumption_unit))
    }

    pub fn get_stock(&self, product_id: &str) -> Result<Option<InventoryStock>, AppError> {
        if product_id.trim().is_empty() {
            return Err(AppError::Internal("product_id is required".to_string()));
        }
        let key = self.resolve_consumption_unit(product_id)?;
        self.executor.inventory().get_stock_typed(product_id, key)
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
            let key = self.resolve_consumption_unit(&item.product_id)?;
            let stock = self
                .executor
                .inventory()
                .get_stock_typed(&item.product_id, key)?;
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
    ///
    /// SEC-087 Phase 5: advisory snapshot-coverage warnings surface stock rows
    /// that exist without a persisted purchase snapshot (legacy/manual stock).
    /// These are display-only and never block operations. The "vice versa"
    /// direction (purchase snapshot with fully-consumed stock) is by-construction
    /// normal for FIFO layers and is intentionally NOT reported.
    pub fn get_inventory_fifo_view(
        &self,
        unit_id: &str,
    ) -> Result<crate::models::InventoryStockPageView, AppError> {
        use crate::models::{
            InventoryCoverageWarning, InventoryLayerView, InventoryProductView,
            InventoryStockPageView,
        };
        use std::collections::HashMap;

        let rows = self
            .executor
            .fifo_layers()
            .get_inventory_fifo_view(unit_id)?;

        struct RowAcc {
            product_name: Option<String>,
            layers: Vec<InventoryLayerView>,
            total_qty: f64,
            oldest: Option<String>,
            order_without_snapshot: bool,
        }

        let mut product_map: HashMap<String, RowAcc> = HashMap::new();

        for row in rows {
            let entry = product_map.entry(row.product_id).or_insert_with(|| RowAcc {
                product_name: None,
                layers: Vec::new(),
                total_qty: 0.0,
                oldest: None,
                order_without_snapshot: false,
            });

            let received_at = row.received_at.clone();
            let source_type = row.source_type.clone();
            entry.layers.push(InventoryLayerView {
                layer_id: row.layer_id,
                source_type: Some(source_type.clone()),
                received_at: received_at.clone(),
                qty_remaining: row.qty_remaining,
                unit_cost: row.unit_cost,
                layer_value: row.qty_remaining * row.unit_cost,
            });
            entry.total_qty += row.qty_remaining;
            if !entry.order_without_snapshot
                && source_type == "ORDER"
                && row.purchase_unit_cost.is_none()
            {
                entry.order_without_snapshot = true;
            }
            if entry.product_name.is_none() {
                entry.product_name = Some(row.product_name);
            }

            match &entry.oldest {
                None => entry.oldest = Some(received_at),
                Some(old) => {
                    if received_at < *old {
                        entry.oldest = Some(received_at);
                    }
                }
            }
        }

        let mut products = Vec::new();
        let mut warnings = Vec::new();
        let mut total_value = 0.0f64;
        let mut total_layers = 0usize;

        for (pid, acc) in product_map {
            let product_value: f64 = acc.layers.iter().map(|l| l.layer_value).sum();
            total_value += product_value;
            let layer_count = acc.layers.len();
            total_layers += layer_count;

            if acc.order_without_snapshot {
                warnings.push(InventoryCoverageWarning {
                    code: "STOCK_WITHOUT_PURCHASE_SNAPSHOT".to_string(),
                    product_id: pid.clone(),
                    product_name: acc.product_name.clone().unwrap_or_default(),
                    message: "المخزون موجود دون سند شراء مسجّل (طريقة قديمة أو إدخال يدوي)"
                        .to_string(),
                });
            }

            products.push(InventoryProductView {
                product_id: pid,
                product_name: acc.product_name.unwrap_or_default(),
                total_quantity: acc.total_qty,
                total_value: product_value,
                oldest_layer_date: acc.oldest,
                layer_count,
                layers: acc.layers,
            });
        }

        products.sort_by(|a, b| a.product_name.cmp(&b.product_name));

        Ok(InventoryStockPageView {
            total_inventory_value: total_value,
            total_products: products.len(),
            total_active_layers: total_layers,
            products,
            warnings,
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
