//! Meal-level FIFO cost engine — shared by preview and save paths.
//!
//! Sorting, allocation, and cost computation live here so that
//! `DailyReportService` and `FifoPreviewService` always agree.
//!
//! The `consume` callback is injected so the engine works with both
//! real DB consumption and in-memory simulation — zero fork risk.

use crate::domain::accounting::fifo::ConsumedLayerPortion;
use crate::domain::fifo_engine::TypedConsumedPortion;
use crate::domain::numeric::{legacy_float, Money, Quantity};
use crate::errors::AppError;
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

// --- Exact-typed core (ADR-0048) --------------------------------------------
// The typed engine performs all cost arithmetic on `Money`/`Quantity`. The
// public facade (wire DTOs) and the save path both delegate here; the only
// `f64` presence is the boundary conversions in the legacy adapter.

/// Exact-typed cost breakdown for one product within a meal.
#[derive(Debug, Clone)]
pub struct ComputedProductCostTyped {
    pub product_id: String,
    pub quantity: Quantity,
    pub total_cost: Money,
    pub portions: Vec<TypedConsumedPortion>,
}

/// Exact-typed cost breakdown for one meal section.
#[derive(Debug, Clone)]
pub struct ComputedMealCostTyped {
    pub meal_type: MealType,
    pub total_cost: Money,
    pub products: Vec<ComputedProductCostTyped>,
}

/// Exact-typed full meal cost computation result.
#[derive(Debug, Clone)]
pub struct MealCostComputationTyped {
    pub meals: Vec<ComputedMealCostTyped>,
    pub total_cost: Money,
}

/// Exact meal FIFO cost computation (ADR-0048).
///
/// Input quantities are wire values (converted at the boundary); all
/// aggregation is exact `Decimal`. `consume` is called once per
/// (product_id, quantity) pair and must return the exact FIFO portions.
pub(crate) fn compute_meal_fifo_costs_typed<F>(
    meals: &[MealSectionInput],
    mut consume: F,
) -> Result<MealCostComputationTyped, AppError>
where
    F: FnMut(&str, Quantity) -> Result<Vec<TypedConsumedPortion>, AppError>,
{
    let mut sorted = meals.to_vec();
    sorted.sort_by_key(|m| match m.meal_type {
        MealType::Breakfast => 0u8,
        MealType::Lunch => 1,
        MealType::Dinner => 2,
    });

    let mut computed_meals = Vec::new();
    let mut total_cost = Money::zero();

    for section in &sorted {
        let mut products = Vec::new();
        let mut meal_cost = Money::zero();

        for item in &section.items {
            let quantity = legacy_float::quantity_from_f64(item.quantity)?;
            if quantity.is_zero() {
                continue;
            }

            let portions = consume(&item.product_id, quantity)?;
            let mut item_cost = Money::zero();
            for portion in &portions {
                item_cost = item_cost.checked_add(portion.total_cost)?;
            }
            meal_cost = meal_cost.checked_add(item_cost)?;

            products.push(ComputedProductCostTyped {
                product_id: item.product_id.clone(),
                quantity,
                total_cost: item_cost,
                portions,
            });
        }

        total_cost = total_cost.checked_add(meal_cost)?;
        computed_meals.push(ComputedMealCostTyped {
            meal_type: section.meal_type,
            total_cost: meal_cost,
            products,
        });
    }

    Ok(MealCostComputationTyped {
        meals: computed_meals,
        total_cost,
    })
}

fn typed_to_wire(typed: &MealCostComputationTyped) -> Result<MealCostComputation, AppError> {
    let meals = typed
        .meals
        .iter()
        .map(|m| {
            let products = m
                .products
                .iter()
                .map(|p| {
                    let portions = p
                        .portions
                        .iter()
                        .map(|port| {
                            Ok(ConsumedLayerPortion {
                                layer_id: port.layer_id.clone(),
                                quantity: legacy_float::quantity_to_f64(&port.quantity)?,
                                unit_cost: legacy_float::money_to_f64(&port.unit_cost)?,
                                total_cost: legacy_float::money_to_f64(&port.total_cost)?,
                            })
                        })
                        .collect::<Result<Vec<ConsumedLayerPortion>, AppError>>()?;
                    Ok(ComputedProductCost {
                        product_id: p.product_id.clone(),
                        quantity: legacy_float::quantity_to_f64(&p.quantity)?,
                        total_cost: legacy_float::money_to_f64(&p.total_cost)?,
                        portions,
                    })
                })
                .collect::<Result<Vec<ComputedProductCost>, AppError>>()?;
            Ok(ComputedMealCost {
                meal_type: m.meal_type,
                total_cost: legacy_float::money_to_f64(&m.total_cost)?,
                products,
            })
        })
        .collect::<Result<Vec<ComputedMealCost>, AppError>>()?;
    Ok(MealCostComputation {
        meals,
        total_cost: legacy_float::money_to_f64(&typed.total_cost)?,
    })
}

/// Compute FIFO costs for a full daily report's meals.
///
/// Meals are processed in operational order (Breakfast → Lunch → Dinner)
/// regardless of payload order.  `consume` is called once per (product_id,
/// quantity) pair and must return the FIFO layer portions consumed.
///
/// Wire facade (ADR-0048): delegates to the exact typed core; the result DTOs
/// are converted exactly once at this boundary.
pub fn compute_meal_fifo_costs<F>(
    meals: &[MealSectionInput],
    mut consume: F,
) -> Result<MealCostComputation, AppError>
where
    F: FnMut(&str, f64) -> Result<Vec<ConsumedLayerPortion>, AppError>,
{
    let mut consume_typed =
        |pid: &str, qty: Quantity| -> Result<Vec<TypedConsumedPortion>, AppError> {
            let qty_f64 = legacy_float::quantity_to_f64(&qty)?;
            let portions = consume(pid, qty_f64)?;
            portions
                .into_iter()
                .map(|p| {
                    Ok(TypedConsumedPortion {
                        layer_id: p.layer_id,
                        quantity: legacy_float::quantity_from_f64(p.quantity)?,
                        unit_cost: legacy_float::money_from_f64(p.unit_cost)?,
                        total_cost: legacy_float::money_from_f64(p.total_cost)?,
                    })
                })
                .collect()
        };

    let typed = compute_meal_fifo_costs_typed(meals, &mut consume_typed)?;
    typed_to_wire(&typed)
}
