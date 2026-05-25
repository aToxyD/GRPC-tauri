//! FIFO consumption preview — dry-run using the same engine as execution.

use crate::domain::accounting::InventoryValue;
use crate::domain::validation::validate_daily_report_input;
use crate::errors::AppError;
use crate::models::{
    ConsumptionItemInput, DailyFifoConsumptionPreview, DailyReportInput, MealFifoPreview,
    MealSectionInput, ProductFifoPreview,
};
use crate::repositories::{DbExecutor, RepositoryProvider};
use std::collections::HashMap;

pub struct FifoPreviewService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FifoPreviewService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Preview FIFO cost for a list of items (single aggregate per product).
    pub fn preview_items(
        &self,
        unit_id: &str,
        items: &[ConsumptionItemInput],
    ) -> Result<DailyFifoConsumptionPreview, AppError> {
        let mut stock_by_product: HashMap<String, f64> = HashMap::new();
        for item in items {
            if item.quantity > 0.0 {
                *stock_by_product
                    .entry(item.product_id.clone())
                    .or_insert(0.0) += item.quantity;
            }
        }

        let product_previews = self.preview_products(unit_id, &stock_by_product)?;
        let predicted_fifo_cost: f64 = product_previews.iter().map(|p| p.predicted_fifo_cost).sum();

        let remaining_value = self.remaining_value_after_preview(unit_id, &product_previews)?;

        Ok(DailyFifoConsumptionPreview {
            predicted_fifo_cost,
            predicted_consumption_layers: product_previews,
            predicted_remaining_inventory_value: remaining_value.value(),
            meal_previews: vec![],
        })
    }

    /// Preview a full daily report input — aggregates per product like `DailyReportService`.
    pub fn preview_daily_report(
        &self,
        input: &DailyReportInput,
        unit_id: &str,
    ) -> Result<DailyFifoConsumptionPreview, AppError> {
        validate_daily_report_input(input)?;

        let mut stock_by_product: HashMap<String, f64> = HashMap::new();
        for section in &input.meals {
            for item in &section.items {
                if item.quantity > 0.0 {
                    *stock_by_product
                        .entry(item.product_id.clone())
                        .or_insert(0.0) += item.quantity;
                }
            }
        }

        let consumption_map: HashMap<String, Vec<crate::models::ConsumedLayerPortion>> =
            self.simulate_product_consumptions(unit_id, &stock_by_product)?;

        let mut meal_previews = Vec::new();
        let mut total_cost = 0.0f64;

        for section in &input.meals {
            let (meal_cost, product_previews) = self.meal_preview_from_consumption_map(
                section,
                &stock_by_product,
                &consumption_map,
            )?;
            total_cost += meal_cost;
            meal_previews.push(MealFifoPreview {
                meal_type: section.meal_type.as_str().to_string(),
                predicted_fifo_cost: meal_cost,
                product_previews,
            });
        }

        let product_previews: Vec<ProductFifoPreview> = consumption_map
            .iter()
            .map(|(product_id, portions)| {
                let qty = stock_by_product
                    .get(product_id)
                    .copied()
                    .expect("consumption map keys are subset of stock aggregate");
                let cost: f64 = portions.iter().map(|p| p.total_cost).sum();
                ProductFifoPreview {
                    product_id: product_id.clone(),
                    quantity: qty,
                    predicted_fifo_cost: cost,
                    predicted_consumption_layers: portions.clone(),
                }
            })
            .collect();

        let remaining_value = self.remaining_value_after_preview(unit_id, &product_previews)?;

        Ok(DailyFifoConsumptionPreview {
            predicted_fifo_cost: total_cost,
            predicted_consumption_layers: product_previews,
            predicted_remaining_inventory_value: remaining_value.value(),
            meal_previews,
        })
    }

    fn preview_products(
        &self,
        unit_id: &str,
        stock_by_product: &HashMap<String, f64>,
    ) -> Result<Vec<ProductFifoPreview>, AppError> {
        let fifo_repo = self.executor.fifo_layers();
        let mut previews = Vec::new();

        for (product_id, quantity) in stock_by_product {
            let portions = fifo_repo.preview_consume_fifo(unit_id, product_id, *quantity)?;
            let cost: f64 = portions.iter().map(|p| p.total_cost).sum();
            previews.push(ProductFifoPreview {
                product_id: product_id.clone(),
                quantity: *quantity,
                predicted_fifo_cost: cost,
                predicted_consumption_layers: portions,
            });
        }

        Ok(previews)
    }

    fn simulate_product_consumptions(
        &self,
        unit_id: &str,
        stock_by_product: &HashMap<String, f64>,
    ) -> Result<HashMap<String, Vec<crate::models::ConsumedLayerPortion>>, AppError> {
        let fifo_repo = self.executor.fifo_layers();
        let mut map = HashMap::new();
        for (product_id, quantity) in stock_by_product {
            let portions = fifo_repo.preview_consume_fifo(unit_id, product_id, *quantity)?;
            map.insert(product_id.clone(), portions);
        }
        Ok(map)
    }

    fn meal_preview_from_consumption_map(
        &self,
        section: &MealSectionInput,
        stock_by_product: &HashMap<String, f64>,
        consumption_map: &HashMap<String, Vec<crate::models::ConsumedLayerPortion>>,
    ) -> Result<(f64, Vec<ProductFifoPreview>), AppError> {
        let mut meal_cost = 0.0f64;
        let mut product_previews = Vec::new();

        for item in &section.items {
            if item.quantity <= 0.0 {
                continue;
            }

            let portions = consumption_map.get(&item.product_id).ok_or_else(|| {
                AppError::Internal(format!(
                    "No consumption preview for product {}",
                    item.product_id
                ))
            })?;

            let total_qty_for_product =
                *stock_by_product.get(&item.product_id).ok_or_else(|| {
                    AppError::Internal(format!(
                        "Product {} not found in daily stock aggregate",
                        item.product_id
                    ))
                })?;

            let mut item_cost = 0.0f64;
            let mut item_portions = Vec::new();

            for portion in portions {
                let ratio = if total_qty_for_product > 0.0 {
                    item.quantity / total_qty_for_product
                } else {
                    0.0
                };
                let portion_qty = portion.quantity * ratio;
                let portion_cost = portion.total_cost * ratio;
                item_cost += portion_cost;
                if portion_qty > 0.0 {
                    item_portions.push(crate::models::ConsumedLayerPortion {
                        layer_id: portion.layer_id.clone(),
                        quantity: portion_qty,
                        unit_cost: portion.unit_cost,
                        total_cost: portion_cost,
                    });
                }
            }

            meal_cost += item_cost;
            product_previews.push(ProductFifoPreview {
                product_id: item.product_id.clone(),
                quantity: item.quantity,
                predicted_fifo_cost: item_cost,
                predicted_consumption_layers: item_portions,
            });
        }

        Ok((meal_cost, product_previews))
    }

    fn remaining_value_after_preview(
        &self,
        unit_id: &str,
        product_previews: &[ProductFifoPreview],
    ) -> Result<InventoryValue, AppError> {
        let fifo_repo = self.executor.fifo_layers();
        let current_value = fifo_repo.get_inventory_value_fifo(unit_id)?;
        let consumed_total: f64 = product_previews.iter().map(|p| p.predicted_fifo_cost).sum();
        Ok(InventoryValue::new(
            (current_value - consumed_total).max(0.0),
        ))
    }
}

impl crate::architecture::Service for FifoPreviewService<'_> {}
