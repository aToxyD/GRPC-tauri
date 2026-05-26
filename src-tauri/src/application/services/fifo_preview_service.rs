//! FIFO consumption preview — dry-run using the same engine as execution.

use crate::domain::accounting::InventoryValue;
use crate::domain::fifo_engine::{simulate_fifo_consumption, FifoLayerRow};
use crate::domain::validation::validate_daily_report_input;
use crate::errors::AppError;
use crate::models::{
    ConsumedLayerPortion, ConsumptionItemInput, DailyFifoConsumptionPreview, DailyReportInput,
    MealFifoPreview, MealType, ProductFifoPreview,
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

    /// Preview a full daily report — simulates meal-level FIFO matching `create_daily_report`.
    /// Meals are processed in operational order (Breakfast → Lunch → Dinner),
    /// consuming layers sequentially per meal item using a local in-memory layer state.
    pub fn preview_daily_report(
        &self,
        input: &DailyReportInput,
        unit_id: &str,
    ) -> Result<DailyFifoConsumptionPreview, AppError> {
        validate_daily_report_input(input)?;

        let fifo_repo = self.executor.fifo_layers();

        // Collect unique product IDs
        let mut product_ids: Vec<&str> = Vec::new();
        for section in &input.meals {
            for item in &section.items {
                if item.quantity > 0.0 && !product_ids.contains(&item.product_id.as_str()) {
                    product_ids.push(&item.product_id);
                }
            }
        }

        // Fetch active layers per product (in-memory mutable working copy)
        let mut remaining_layers: HashMap<String, Vec<FifoLayerRow>> = HashMap::new();
        for pid in &product_ids {
            let layers = fifo_repo.fetch_active_layers(unit_id, pid)?;
            remaining_layers.insert(pid.to_string(), layers);
        }

        // Sort meals to operational order (match create_daily_report)
        let mut meals = input.meals.clone();
        meals.sort_by_key(|m| match m.meal_type {
            MealType::Breakfast => 0u8,
            MealType::Lunch => 1,
            MealType::Dinner => 2,
        });

        // Track aggregated per-product consumption for daily view
        let mut total_consumed: HashMap<String, f64> = HashMap::new();
        let mut total_portions: HashMap<String, Vec<ConsumedLayerPortion>> = HashMap::new();
        let mut total_cost = 0.0f64;
        let mut meal_previews = Vec::new();

        for section in &meals {
            let mut meal_cost = 0.0f64;
            let mut product_previews = Vec::new();

            for item in &section.items {
                if item.quantity <= 0.0 {
                    continue;
                }

                let layers = remaining_layers.get_mut(&item.product_id).ok_or_else(|| {
                    AppError::Internal(format!("No active layers for product {}", item.product_id))
                })?;

                let portions = simulate_fifo_consumption(&item.product_id, layers, item.quantity)?;

                // Deduct consumed quantities from the in-memory layer state
                for portion in &portions {
                    for (lid, _, qty) in layers.iter_mut() {
                        if *lid == portion.layer_id {
                            *qty -= portion.quantity;
                            break;
                        }
                    }
                }
                // Remove exhausted layers so subsequent meals don't see zero-qty entries
                layers.retain(|(_, _, qty)| *qty > 0.0);

                let item_cost: f64 = portions.iter().map(|p| p.total_cost).sum();
                meal_cost += item_cost;

                *total_consumed.entry(item.product_id.clone()).or_insert(0.0) += item.quantity;
                total_portions
                    .entry(item.product_id.clone())
                    .or_default()
                    .extend(portions.clone());

                product_previews.push(ProductFifoPreview {
                    product_id: item.product_id.clone(),
                    quantity: item.quantity,
                    predicted_fifo_cost: item_cost,
                    predicted_consumption_layers: portions,
                });
            }

            total_cost += meal_cost;
            meal_previews.push(MealFifoPreview {
                meal_type: section.meal_type.as_str().to_string(),
                predicted_fifo_cost: meal_cost,
                product_previews,
            });
        }

        // Build per-product aggregated view
        let mut product_previews = Vec::new();
        for (product_id, portions) in total_portions {
            let qty = total_consumed.remove(&product_id).ok_or_else(|| {
                AppError::Internal(format!(
                    "Missing total consumed quantity for product {}",
                    product_id
                ))
            })?;
            let cost: f64 = portions.iter().map(|p| p.total_cost).sum();
            product_previews.push(ProductFifoPreview {
                product_id,
                quantity: qty,
                predicted_fifo_cost: cost,
                predicted_consumption_layers: portions,
            });
        }

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
