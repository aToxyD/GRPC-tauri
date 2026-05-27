//! Meal Cost Engine Tests
//!
//! The shared engine guarantees that preview and save always allocate
//! layers identically.  These tests exercise the engine in isolation
//! with a simulated consume callback (no database needed).

use grpc_lib::domain::meal_cost_engine::compute_meal_fifo_costs;
use grpc_lib::domain::meal_cost_engine::{
    ComputedMealCost, ComputedProductCost, MealCostComputation,
};
use grpc_lib::models::{ConsumedLayerPortion, ConsumptionItemInput, MealSectionInput, MealType};

// ── test helpers ──────────────────────────────────────────────────────────────

/// Simulated FIFO layer stack: `(layer_id, unit_cost, qty_remaining)`.
/// `consume_from_layers` mimics real FIFO — oldest first — and mutates
/// `layers` in place, just like `simulate_fifo_consumption`.
fn consume_from_layers(
    layers: &mut Vec<(String, f64, f64)>,
    product_id: &str,
    quantity: f64,
) -> Result<Vec<ConsumedLayerPortion>, grpc_lib::errors::AppError> {
    if quantity <= 0.0 {
        return Ok(Vec::new());
    }

    let total: f64 = layers.iter().map(|(_, _, q)| q).sum();
    if total < quantity {
        return Err(grpc_lib::errors::AppError::BusinessLogic(
            grpc_lib::errors::BusinessLogicError::InsufficientStock(format!(
                "Insufficient stock for {}. Requested {}, available {}",
                product_id, quantity, total
            )),
        ));
    }

    let mut remaining = quantity;
    let mut portions = Vec::new();

    for (lid, cost, qty) in layers.iter_mut() {
        if remaining <= 0.0 {
            break;
        }
        let take = (*qty).min(remaining);
        portions.push(ConsumedLayerPortion {
            layer_id: lid.clone(),
            quantity: take,
            unit_cost: *cost,
            total_cost: take * *cost,
        });
        *qty -= take;
        remaining -= take;
    }

    layers.retain(|(_, _, q)| *q > 0.0);
    Ok(portions)
}

fn layer(id: &str, cost: f64, qty: f64) -> (String, f64, f64) {
    (id.to_string(), cost, qty)
}

fn make_meal(meal_type: MealType, items: Vec<(&str, f64)>) -> MealSectionInput {
    MealSectionInput {
        meal_type,
        staff_24h_count: 0,
        staff_8h_count: 0,
        reservation_count: 0,
        mission_count: 0,
        guest_count: 0,
        items: items
            .into_iter()
            .map(|(pid, qty)| ConsumptionItemInput {
                product_id: pid.to_string(),
                quantity: qty,
            })
            .collect(),
    }
}

fn assert_cost(computation: &MealCostComputation, expected_total: f64) {
    assert!(
        (computation.total_cost - expected_total).abs() < 0.001,
        "total_cost expected {}, got {}",
        expected_total,
        computation.total_cost
    );
}

fn assert_meal_cost(meal: &ComputedMealCost, expected_cost: f64) {
    assert!(
        (meal.total_cost - expected_cost).abs() < 0.001,
        "meal {} cost expected {}, got {}",
        meal.meal_type.as_str(),
        expected_cost,
        meal.total_cost
    );
}

fn assert_product_cost(product: &ComputedProductCost, expected_cost: f64) {
    assert!(
        (product.total_cost - expected_cost).abs() < 0.001,
        "product {} cost expected {}, got {}",
        product.product_id,
        expected_cost,
        product.total_cost
    );
}

fn assert_portion(
    portion: &ConsumedLayerPortion,
    expected_layer: &str,
    expected_qty: f64,
    expected_cost: f64,
) {
    assert_eq!(portion.layer_id, expected_layer);
    assert!((portion.quantity - expected_qty).abs() < 0.001);
    assert!((portion.unit_cost * portion.quantity - portion.total_cost).abs() < 0.001);
    assert!((portion.total_cost - expected_cost).abs() < 0.001);
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn single_product_single_layer() {
    let mut stack = vec![layer("L1", 500.0, 100.0)];

    let meals = vec![make_meal(MealType::Lunch, vec![("P1", 30.0)])];
    let comp =
        compute_meal_fifo_costs(&meals, |pid, qty| consume_from_layers(&mut stack, pid, qty))
            .unwrap();

    assert_eq!(comp.meals.len(), 1);
    assert_meal_cost(&comp.meals[0], 15000.0);
    assert_eq!(comp.meals[0].products.len(), 1);
    assert_product_cost(&comp.meals[0].products[0], 15000.0);
    assert_eq!(comp.meals[0].products[0].portions.len(), 1);
    assert_portion(&comp.meals[0].products[0].portions[0], "L1", 30.0, 15000.0);
    assert_cost(&comp, 15000.0);
}

#[test]
fn single_product_multi_layer() {
    let mut stack = vec![layer("L1", 500.0, 20.0), layer("L2", 800.0, 50.0)];

    let meals = vec![make_meal(MealType::Lunch, vec![("P1", 30.0)])];
    let comp =
        compute_meal_fifo_costs(&meals, |pid, qty| consume_from_layers(&mut stack, pid, qty))
            .unwrap();

    // 20 from L1 (20*500=10000) + 10 from L2 (10*800=8000) = 18000
    assert_eq!(comp.meals.len(), 1);
    assert_meal_cost(&comp.meals[0], 18000.0);
    assert_eq!(comp.meals[0].products.len(), 1);
    assert_product_cost(&comp.meals[0].products[0], 18000.0);
    assert_eq!(comp.meals[0].products[0].portions.len(), 2);
    assert_portion(&comp.meals[0].products[0].portions[0], "L1", 20.0, 10000.0);
    assert_portion(&comp.meals[0].products[0].portions[1], "L2", 10.0, 8000.0);
    assert_cost(&comp, 18000.0);

    // In-memory state: L1 exhausted, L2 has 40 remaining
    assert_eq!(stack.len(), 1);
    assert!((stack[0].2 - 40.0).abs() < 0.001);
}

#[test]
fn multi_meal_split() {
    let mut stack = vec![layer("L1", 500.0, 10.0), layer("L2", 800.0, 20.0)];

    let meals = vec![
        make_meal(MealType::Breakfast, vec![("P1", 10.0)]),
        make_meal(MealType::Lunch, vec![("P1", 10.0)]),
    ];
    let comp =
        compute_meal_fifo_costs(&meals, |pid, qty| consume_from_layers(&mut stack, pid, qty))
            .unwrap();

    assert_eq!(comp.meals.len(), 2);

    // Breakfast: 10 from L1 (10*500 = 5000)
    assert_meal_cost(&comp.meals[0], 5000.0);
    assert_eq!(comp.meals[0].meal_type, MealType::Breakfast);
    assert_eq!(comp.meals[0].products[0].portions.len(), 1);
    assert_portion(&comp.meals[0].products[0].portions[0], "L1", 10.0, 5000.0);

    // Lunch: 10 from L2 (10*800 = 8000)
    assert_meal_cost(&comp.meals[1], 8000.0);
    assert_eq!(comp.meals[1].meal_type, MealType::Lunch);
    assert_eq!(comp.meals[1].products[0].portions.len(), 1);
    assert_portion(&comp.meals[1].products[0].portions[0], "L2", 10.0, 8000.0);

    assert_cost(&comp, 13000.0);
}

#[test]
fn multi_product() {
    let mut stack_p1 = vec![layer("L1", 100.0, 50.0)];
    let mut stack_p2 = vec![layer("L2", 200.0, 30.0)];

    let meals = vec![make_meal(
        MealType::Dinner,
        vec![("P1", 10.0), ("P2", 10.0)],
    )];
    let comp = compute_meal_fifo_costs(&meals, |pid, qty| match pid {
        "P1" => consume_from_layers(&mut stack_p1, pid, qty),
        "P2" => consume_from_layers(&mut stack_p2, pid, qty),
        _ => panic!("unexpected product"),
    })
    .unwrap();

    assert_eq!(comp.meals.len(), 1);
    assert_eq!(comp.meals[0].products.len(), 2);
    assert_product_cost(&comp.meals[0].products[0], 1000.0); // P1: 10*100
    assert_product_cost(&comp.meals[0].products[1], 2000.0); // P2: 10*200
    assert_meal_cost(&comp.meals[0], 3000.0);
    assert_cost(&comp, 3000.0);

    // Remaining: P1=40, P2=20
    assert!((stack_p1[0].2 - 40.0).abs() < 0.001);
    assert!((stack_p2[0].2 - 20.0).abs() < 0.001);
}

#[test]
fn order_independence() {
    let meal_bf = make_meal(MealType::Breakfast, vec![("P1", 10.0)]);
    let meal_d = make_meal(MealType::Dinner, vec![("P1", 5.0)]);
    let meal_l = make_meal(MealType::Lunch, vec![("P1", 8.0)]);

    // Payload order: Dinner, Breakfast, Lunch (reversed)
    let payload_order = vec![meal_d, meal_bf, meal_l];

    let mut stack = vec![layer("L1", 100.0, 100.0)];
    let comp = compute_meal_fifo_costs(&payload_order, |pid, qty| {
        consume_from_layers(&mut stack, pid, qty)
    })
    .unwrap();

    // Engine must reorder to Breakfast → Lunch → Dinner
    assert_eq!(comp.meals.len(), 3);
    assert_eq!(comp.meals[0].meal_type, MealType::Breakfast);
    assert_eq!(comp.meals[1].meal_type, MealType::Lunch);
    assert_eq!(comp.meals[2].meal_type, MealType::Dinner);

    // Breakfast first: 10*100 = 1000
    assert_meal_cost(&comp.meals[0], 1000.0);
    // Lunch second: 8*100 = 800
    assert_meal_cost(&comp.meals[1], 800.0);
    // Dinner third: 5*100 = 500
    assert_meal_cost(&comp.meals[2], 500.0);
    assert_cost(&comp, 2300.0);
}

#[test]
fn insufficient_stock() {
    let mut stack = vec![layer("L1", 100.0, 5.0)];
    let meals = vec![make_meal(MealType::Lunch, vec![("P1", 10.0)])];

    let result =
        compute_meal_fifo_costs(&meals, |pid, qty| consume_from_layers(&mut stack, pid, qty));

    match result {
        Err(grpc_lib::errors::AppError::BusinessLogic(
            grpc_lib::errors::BusinessLogicError::InsufficientStock(_),
        )) => {} // expected
        other => panic!("Expected InsufficientStock, got {:?}", other),
    }
}

#[test]
fn preview_and_save_allocate_layers_identically() {
    // This is the regression test for the real bug:
    //   Layer A:  10 @ 500
    //   Layer B:  20 @ 800
    //   Breakfast: 10 → consumes Layer A entirely
    //   Lunch:     10 → consumes 10 from Layer B
    //
    // The original bug: preview redistributed by ratio, giving each meal 6500.
    // Correct allocation: Breakfast 5000, Lunch 8000, Total 13000.

    let mut stack = vec![layer("A", 500.0, 10.0), layer("B", 800.0, 20.0)];

    let meals = vec![
        make_meal(MealType::Breakfast, vec![("P1", 10.0)]),
        make_meal(MealType::Lunch, vec![("P1", 10.0)]),
    ];
    let comp =
        compute_meal_fifo_costs(&meals, |pid, qty| consume_from_layers(&mut stack, pid, qty))
            .unwrap();

    assert_eq!(comp.meals.len(), 2);

    // Breakfast: 10 units from Layer A @ 500 = 5000
    assert_meal_cost(&comp.meals[0], 5000.0);
    assert_eq!(comp.meals[0].meal_type, MealType::Breakfast);
    assert_eq!(comp.meals[0].products.len(), 1);
    assert_eq!(comp.meals[0].products[0].portions.len(), 1);
    assert_portion(&comp.meals[0].products[0].portions[0], "A", 10.0, 5000.0);

    // Lunch: 10 units from Layer B @ 800 = 8000
    assert_meal_cost(&comp.meals[1], 8000.0);
    assert_eq!(comp.meals[1].meal_type, MealType::Lunch);
    assert_eq!(comp.meals[1].products.len(), 1);
    assert_eq!(comp.meals[1].products[0].portions.len(), 1);
    assert_portion(&comp.meals[1].products[0].portions[0], "B", 10.0, 8000.0);

    // Total: 5000 + 8000 = 13000 (NOT 6500 + 6500)
    assert_cost(&comp, 13000.0);

    // In-memory state: Layer A exhausted, Layer B has 10 remaining
    assert_eq!(stack.len(), 1);
    assert_eq!(stack[0].0, "B");
    assert!((stack[0].2 - 10.0).abs() < 0.001);
}
