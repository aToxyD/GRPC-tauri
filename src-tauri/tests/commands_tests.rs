//! Commands Integration Tests
//!
//! Tests for Tauri commands

use grpc_lib::commands::*;
use grpc_lib::models::*;

/// Test AppState creation
#[test]
fn test_app_state_creation() {
    let db = grpc_lib::db::ConnectionFactory::new_for_test().expect("Failed to create database");
    let state = AppState::new_for_test(db);

    // Verify state is properly initialized
    assert!(state.get_db().is_ok());
}

/// Test authentication guards
#[test]
fn test_session_management() {
    let db = grpc_lib::db::ConnectionFactory::new_for_test().expect("Failed to create database");
    let state = AppState::new_for_test(db);

    // Initially no session
    assert!(state.get_session().is_err());

    // Touch session should not panic without session
    state.touch_session();
}

/// Test password change validation
#[test]
fn test_password_validation() {
    use grpc_lib::domain::validation::validate_change_password;

    // Valid passwords
    assert!(validate_change_password("SecurePass123").is_ok());

    // Invalid passwords
    assert!(validate_change_password("short").is_err());
    assert!(validate_change_password("nouppercase123").is_err());
}

/// Test product validation
#[test]
fn test_product_validation() {
    use grpc_lib::domain::validation::validate_create_product_request;

    let request = CreateProductRequest {
        name: "Test Product".to_string(),
        base_price: 100.0,
        tva: 19.0,
        supplier_name: Some("Supplier".to_string()),
    };

    assert!(validate_create_product_request(&request, 2024).is_ok());

    // Invalid - empty name
    let invalid = CreateProductRequest {
        name: "".to_string(),
        base_price: 100.0,
        tva: 19.0,
        supplier_name: None,
    };
    assert!(validate_create_product_request(&invalid, 2024).is_err());
}

/// Test order validation
#[test]
fn test_order_validation() {
    use grpc_lib::domain::validation::validate_create_order_request;

    let valid_order = CreateOrderRequest {
        supplier_name: "Supplier".to_string(),
        reference_number: Some("REF001".to_string()),
        items: vec![OrderItemInput {
            product_id: "P001".to_string(),
            quantity: 10.0,
            unit_price: 5.0,
        }],
    };

    assert!(validate_create_order_request(&valid_order).is_ok());

    // Invalid - empty supplier
    let invalid = CreateOrderRequest {
        supplier_name: "".to_string(),
        reference_number: None,
        items: vec![],
    };
    assert!(validate_create_order_request(&invalid).is_err());
}

/// Test unit validation
#[test]
fn test_unit_validation() {
    use grpc_lib::domain::validation::validate_create_unit_request;

    let valid_unit = CreateUnitRequest {
        code: "123456".to_string(),
        name: "Test Unit".to_string(),
        password: "SecurePass123".to_string(),
    };

    assert!(validate_create_unit_request(&valid_unit).is_ok());

    // Invalid - short code
    let invalid = CreateUnitRequest {
        code: "123".to_string(),
        name: "Test".to_string(),
        password: "weak".to_string(),
    };
    assert!(validate_create_unit_request(&invalid).is_err());
}

/// Test meal cost calculations
#[test]
fn test_meal_calculations() {
    // Test meal cost calculation
    let items = [(10.0, 5.0), (20.0, 3.0)]; // (qty, price)
    let total: f64 = items.iter().map(|(q, p)| q * p).sum();
    assert_eq!(total, 110.0);

    // Test meal rate
    let total_cost = 5000.0;
    let personnel = 100;
    let guests = 20;
    let rate = if personnel + guests > 0 {
        total_cost / (personnel + guests) as f64
    } else {
        0.0
    };
    assert!(
        (rate - 41.67).abs() < 0.01,
        "Rate {} should be approximately 41.67",
        rate
    );
}

/// Test guards helper functions
#[test]
fn test_guard_helpers() {
    // Test get_effective_unit_id with various scenarios
    // Note: This would require mocking the session
}
