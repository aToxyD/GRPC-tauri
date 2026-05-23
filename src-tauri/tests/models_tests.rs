//! Models Integration Tests
//!
//! Tests for domain models and business logic

use chrono::Utc;
use grpc_lib::models::*;

/// Test user role conversions
#[test]
fn test_user_role_conversion() {
    let admin = UserRole::Admin;
    assert_eq!(admin.to_string(), "Admin");

    let from_str = UserRole::from("Admin".to_string());
    assert_eq!(from_str, UserRole::Admin);
}

/// Test order status lifecycle
#[test]
fn test_order_status_lifecycle() {
    assert!(OrderStatus::Draft.can_confirm());
    assert!(!OrderStatus::Confirmed.can_confirm());
    assert!(OrderStatus::Confirmed.can_receive());
}

/// Test product price calculations
#[test]
fn test_product_calculations() {
    let base_price = 100.0;
    let tva = 19.0;
    let total = base_price * (1.0 + tva / 100.0);

    assert_eq!(total, 119.0);
}

/// Test meal section calculations
#[test]
fn test_meal_consumption_calculations() {
    let total = DailyReportMeal::compute_total_beneficiaries(50, 30, 10, 5, 20);
    assert_eq!(total, 115);
    let avg = DailyReportMeal::compute_meal_average(5000.0, total);
    assert!((avg - 43.478).abs() < 0.01);
}

/// Test stock movement balance calculation
#[test]
fn test_stock_movement_balance() {
    let movement = StockMovement {
        id: "M001".to_string(),
        product_id: "P001".to_string(),
        product_name: Some("Product".to_string()),
        movement_type: StockMovementType::In,
        quantity: 50.0,
        balance_before: 100.0,
        balance_after: 150.0,
        reference_type: Some("Order".to_string()),
        reference_id: Some("O001".to_string()),
        notes: None,
        timestamp: Utc::now().to_rfc3339(),
        user_id: "U001".to_string(),
        username: "admin".to_string(),
        unit_id: None,
        fiscal_year: Some(2024),
        unit_cost: None,
    };

    assert_eq!(movement.stock_change(), 50.0);
    assert!(!movement.is_system_generated());
}

/// Test settings node type detection
#[test]
fn test_settings_node_type() {
    let wilaya = Settings {
        id: 1,
        node_type: grpc_lib::models::NodeType::Wilaya,
        unit_name: None,
        unit_code: None,
        current_year: 2024,
        wilaya_code: Some("16".to_string()),
        wilaya_name: Some("Algiers".to_string()),
        configured: true,
    };

    assert!(wilaya.is_wilaya());
    assert!(!wilaya.is_unit());
    assert!(wilaya.display_name().contains("Wilaya"));
}
