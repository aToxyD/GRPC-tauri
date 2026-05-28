//! Meal-level FIFO cost engine — shared by preview and save paths.
//!
//! Sorting, allocation, and cost computation live here so that
//! `DailyReportService` and `FifoPreviewService` always agree.
//!
//! The `consume` callback is injected so the engine works with both
//! real DB consumption and in-memory simulation — zero fork risk.

use crate::errors::AppError;
use crate::domain::accounting::fifo::ConsumedLayerPortion;
use crate::models::{MealSectionInput, MealType};

/// Cost breakdown for one product within a meal.
#[derive(Debug, Clone)]
pub struct ComputedProductCost {
    pub product_id: String,
    pub quantity: f64,
    pub total_cost: f64,
    pub portions: Vec<ConsumedLayerPortion>,
}

/// Cost breakdown for one meal section.
#[derive(Debug, Clone)]
pub struct ComputedMealCost {
    pub meal_type: MealType,
    pub total_cost: f64,
    pub products: Vec<ComputedProductCost>,
}

/// Full meal cost computation result.
#[derive(Debug, Clone)]
pub struct MealCostComputation {
    pub meals: Vec<ComputedMealCost>,
    pub total_cost: f64,
}

/// Compute FIFO costs for a full daily report's meals.
///
/// Meals are processed in operational order (Breakfast → Lunch → Dinner)
/// regardless of payload order.  `consume` is called once per (product_id,
/// quantity) pair and must return the FIFO layer portions consumed.
pub fn compute_meal_fifo_costs<F>(
    meals: &[MealSectionInput],
    mut consume: F,
) -> Result<MealCostComputation, AppError>
where
    F: FnMut(&str, f64) -> Result<Vec<ConsumedLayerPortion>, AppError>,
{
    let mut sorted = meals.to_vec();
    sorted.sort_by_key(|m| match m.meal_type {
        MealType::Breakfast => 0u8,
        MealType::Lunch => 1,
        MealType::Dinner => 2,
    });

    let mut computed_meals = Vec::new();
    let mut total_cost = 0.0f64;

    for section in &sorted {
        let mut products = Vec::new();
        let mut meal_cost = 0.0f64;

        for item in &section.items {
            if item.quantity <= 0.0 {
                continue;
            }

            let portions = consume(&item.product_id, item.quantity)?;
            let item_cost: f64 = portions.iter().map(|p| p.total_cost).sum();
            meal_cost += item_cost;

            products.push(ComputedProductCost {
                product_id: item.product_id.clone(),
                quantity: item.quantity,
                total_cost: item_cost,
                portions,
            });
        }

        total_cost += meal_cost;
        computed_meals.push(ComputedMealCost {
            meal_type: section.meal_type,
            total_cost: meal_cost,
            products,
        });
    }

    Ok(MealCostComputation {
        meals: computed_meals,
        total_cost,
    })
}
