//! FIFO consumption preview — dry-run using the same engine as execution.

use crate::domain::accounting::InventoryValue;
use crate::domain::fifo_engine::{simulate_fifo_consumption, FifoLayerRow};
use crate::domain::meal_cost_engine::compute_meal_fifo_costs;
use crate::domain::validation::validate_daily_report_input;
use crate::errors::AppError;
use crate::models::{
    ConsumedLayerPortion, ConsumptionItemInput, DailyConsumptionSummary,
    DailyFifoConsumptionPreview, DailyReportInput, DailyReportMeal, MealFifoPreview,
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
            predicted_remaining_inventory_value:
                crate::domain::numeric::legacy_float::money_to_f64(&remaining_value.value())?,
            meal_previews: vec![],
            daily_summary: DailyConsumptionSummary::default(),
        })
    }

    /// Preview a full daily report — simulates meal-level FIFO matching `create_daily_report`.
    /// Uses the shared engine so preview and save always allocate identically.
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

        // Use the shared engine with an in-memory simulation callback.
        // This guarantees the same allocation as create_daily_report.
        let computation = compute_meal_fifo_costs(&input.meals, |pid, qty| {
            let layers = remaining_layers.get_mut(pid).ok_or_else(|| {
                AppError::Internal(format!("No active layers for product {}", pid))
            })?;
            let portions = simulate_fifo_consumption(pid, layers, qty)?;
            for portion in &portions {
                for (lid, _, qty) in layers.iter_mut() {
                    if *lid == portion.layer_id {
                        *qty -= portion.quantity;
                        break;
                    }
                }
            }
            layers.retain(|(_, _, qty)| *qty > 0.0);
            Ok(portions)
        })?;

        // Build meal-level previews from the engine result
        let mut meal_previews = Vec::new();
        let mut total_consumed: HashMap<String, f64> = HashMap::new();
        let mut total_portions: HashMap<String, Vec<ConsumedLayerPortion>> = HashMap::new();

        let section_by_type: HashMap<&str, &MealSectionInput> = input
            .meals
            .iter()
            .map(|s| (s.meal_type.as_str(), s))
            .collect();

        for computed_meal in &computation.meals {
            let mut product_previews = Vec::new();

            for product in &computed_meal.products {
                *total_consumed
                    .entry(product.product_id.clone())
                    .or_insert(0.0) += product.quantity;
                total_portions
                    .entry(product.product_id.clone())
                    .or_default()
                    .extend(product.portions.clone());

                let unit_cost = if product.quantity > 0.0 {
                    product.total_cost / product.quantity
                } else {
                    0.0
                };
                product_previews.push(ProductFifoPreview {
                    product_id: product.product_id.clone(),
                    quantity: product.quantity,
                    predicted_fifo_cost: product.total_cost,
                    unit_cost,
                    predicted_consumption_layers: product.portions.clone(),
                });
            }

            let section = section_by_type.get(computed_meal.meal_type.as_str());
            let total_beneficiaries = match section {
                Some(s) => DailyReportMeal::compute_total_beneficiaries(
                    s.staff_24h_count,
                    s.staff_8h_count,
                    s.reservation_count,
                    s.mission_count,
                    s.guest_count,
                ),
                None => 0,
            };
            let meal_average = DailyReportMeal::compute_meal_average(
                computed_meal.total_cost,
                total_beneficiaries,
            );
            meal_previews.push(MealFifoPreview {
                meal_type: computed_meal.meal_type.as_str().to_string(),
                predicted_fifo_cost: computed_meal.total_cost,
                total_beneficiaries,
                meal_average,
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
            let unit_cost = if qty > 0.0 { cost / qty } else { 0.0 };
            product_previews.push(ProductFifoPreview {
                product_id,
                quantity: qty,
                predicted_fifo_cost: cost,
                unit_cost,
                predicted_consumption_layers: portions,
            });
        }

        let remaining_value = self.remaining_value_after_preview(unit_id, &product_previews)?;

        // Build daily summary from meal previews (same semantics as DailyConsumptionSummary::from_meal_sections)
        let mut daily_summary = DailyConsumptionSummary::default();
        for mp in &meal_previews {
            match mp.meal_type.as_str() {
                "breakfast" => {
                    daily_summary.breakfast_beneficiaries = mp.total_beneficiaries;
                    daily_summary.breakfast_cost = mp.predicted_fifo_cost;
                    daily_summary.breakfast_average = mp.meal_average;
                }
                "lunch" => {
                    daily_summary.lunch_beneficiaries = mp.total_beneficiaries;
                    daily_summary.lunch_cost = mp.predicted_fifo_cost;
                    daily_summary.lunch_average = mp.meal_average;
                }
                "dinner" => {
                    daily_summary.dinner_beneficiaries = mp.total_beneficiaries;
                    daily_summary.dinner_cost = mp.predicted_fifo_cost;
                    daily_summary.dinner_average = mp.meal_average;
                }
                _ => {}
            }
        }
        daily_summary.total_daily_beneficiaries = daily_summary.breakfast_beneficiaries
            + daily_summary.lunch_beneficiaries
            + daily_summary.dinner_beneficiaries;
        daily_summary.total_daily_cost =
            daily_summary.breakfast_cost + daily_summary.lunch_cost + daily_summary.dinner_cost;
        daily_summary.daily_average = daily_summary.breakfast_average
            + daily_summary.lunch_average
            + daily_summary.dinner_average;

        Ok(DailyFifoConsumptionPreview {
            predicted_fifo_cost: computation.total_cost,
            predicted_consumption_layers: product_previews,
            predicted_remaining_inventory_value:
                crate::domain::numeric::legacy_float::money_to_f64(&remaining_value.value())?,
            meal_previews,
            daily_summary,
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
            let unit_cost = if *quantity > 0.0 {
                cost / *quantity
            } else {
                0.0
            };
            previews.push(ProductFifoPreview {
                product_id: product_id.clone(),
                quantity: *quantity,
                predicted_fifo_cost: cost,
                unit_cost,
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
        // Presentation preview: the remaining-value estimate is bounded at zero
        // for display; the Money newtype receives it at this boundary only.
        Ok(InventoryValue::new(
            crate::domain::numeric::legacy_float::money_from_f64(
                (current_value - consumed_total).max(0.0),
            )?,
        ))
    }
}

impl crate::architecture::Service for FifoPreviewService<'_> {}
