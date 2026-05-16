//! Models Integration Tests
//!
//! Tests for domain models and business logic

use chrono::{Datelike, Utc};
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

/// Test daily report meal calculations
#[test]
fn test_daily_report_calculations() {
    let report = DailyReport {
        id: "R001".to_string(),
        date: Utc::now().date_naive(),
        personnel_count: 100,
        guest_count: 20,
        total_meals_cost: 5000.0,
        actual_meal_rate: 41.67,
        unit_id: Some("U001".to_string()),
        fiscal_year: Utc::now().year(),
        created_at: Utc::now(),
    };

    assert_eq!(report.total_meals(), 120);
    assert!(report.has_consumption());
    assert!(report.cost_per_meal().is_some());
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
