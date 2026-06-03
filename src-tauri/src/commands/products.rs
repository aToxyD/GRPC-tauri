//! Product Management Commands
//!
//! CRUD operations for products with full audit trail
//! Strictly follows Clean Architecture: Commands -> Services -> Repositories -> DB

use crate::application::authz::Action;
use crate::application::services::{AuditTxService, ProductService, SettingsService};
use crate::commands::common::{
    db_mut_or_command_error, db_ref_or_command_error, user_ctx_from_session,
};
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::domain::audit::AuditAction;
use crate::errors::into_command_error;
use crate::models::{CreateProductRequest, Product, UpdateProductRequest};
use tauri::State;

/// Create a new product with full audit trail
#[tauri::command]
pub fn create_product(
    state: State<AppState>,
    request: CreateProductRequest,
) -> Result<String, String> {
    // 1. AUTHORIZATION GUARD
    let (session, _settings) =
        authorize_command(&state, Action::ManageProducts, None).map_err(into_command_error)?;

    state.touch_session();

    // 2. GET DATABASE
    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    // 3. GET YEAR (via SettingsService)
    let settings = SettingsService::new(db.executor())
        .get_settings()
        .map_err(into_command_error)?;
    let year = settings.current_year;

    // 4. 🔒 ATOMIC EXECUTION (Validation + Mutation + Audit)
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::CreateProduct, &user_ctx, |tx| {
        ProductService::new(tx.executor).create_product(&request, year)
    })
    .map_err(into_command_error)
}

/// Update product with full audit trail and atomicity guarantee
#[tauri::command]
pub fn update_product(state: State<AppState>, request: UpdateProductRequest) -> Result<(), String> {
    // 1. AUTHORIZATION GUARD
    let (session, _settings) =
        authorize_command(&state, Action::ManageProducts, None).map_err(into_command_error)?;

    state.touch_session();

    // 2. GET DATABASE
    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    // 3. 🔒 ATOMIC EXECUTION (via AuditTxService + ProductService)
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::UpdateProduct, &user_ctx, |tx| {
        ProductService::new(tx.executor).update_product(&request)
    })
    .map_err(into_command_error)?;

    Ok(())
}

/// Delete product with full audit trail
#[tauri::command]
pub fn delete_product(state: State<AppState>, product_id: String) -> Result<(), String> {
    // 1. AUTHORIZATION GUARD
    let (session, _settings) =
        authorize_command(&state, Action::ManageProducts, None).map_err(into_command_error)?;

    state.touch_session();

    // 2. GET DATABASE
    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    // 3. 🔒 ATOMIC EXECUTION
    let user_ctx = user_ctx_from_session(&session);

    AuditTxService::execute_with_audit(db, AuditAction::DeleteProduct, &user_ctx, |tx| {
        ProductService::new(tx.executor).delete_product(&product_id)
    })
    .map_err(into_command_error)?;

    Ok(())
}

/// Get single product by ID
#[tauri::command]
pub fn get_product(state: State<AppState>, product_id: String) -> Result<Option<Product>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadProducts, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    // Read operations call Repository directly (or Service if logic is needed)
    ProductService::new(db.executor())
        .get_product(&product_id)
        .map_err(into_command_error)
}

/// List all products (unfiltered)
#[tauri::command]
pub fn list_products(state: State<AppState>) -> Result<Vec<Product>, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadProducts, None).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    ProductService::new(db.executor())
        .list_products()
        .map_err(into_command_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::types::AppState;
    use crate::db::ConnectionFactory;
    use crate::domain::session::{CurrentSession, UserSnapshot};
    use crate::models::CreateProductRequest;
    use crate::models::UserRole;

    fn setup_admin_state() -> AppState {
        let db = ConnectionFactory::new_for_test().unwrap();
        let state = AppState::new_for_test(db);
        let snapshot = UserSnapshot {
            id: "admin1".to_string(),
            username: "admin1".to_string(),
            role: UserRole::Admin,
            created_at: chrono::Utc::now(),
        };
        let session = CurrentSession::new(
            "admin1".to_string(),
            "admin1".to_string(),
            UserRole::Admin,
            snapshot,
        );
        if let Ok(mut s) = state.current_session.lock() {
            *s = Some(session);
        }
        state
    }

    #[test]
    fn test_create_product_validation_empty_name() {
        let state = setup_admin_state();
        let guard = state.get_db().unwrap();
        let db = guard.as_ref().unwrap();
        let svc = ProductService::new(db.executor());

        let request = CreateProductRequest {
            name: "".to_string(),
            base_price: 100.0,
            tva: 19.0,
            supplier_name: None,
        };

        let result = svc.create_product(&request, 2024);
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::errors::AppError::Validation(crate::errors::ValidationError::Required {
                field,
            }) => assert_eq!(field, "name"),
            _ => panic!("Expected Required name validation error"),
        }
    }

    #[test]
    fn test_create_product_validation_invalid_price() {
        let state = setup_admin_state();
        let guard = state.get_db().unwrap();
        let db = guard.as_ref().unwrap();
        let svc = ProductService::new(db.executor());

        let request = CreateProductRequest {
            name: "Test".to_string(),
            base_price: -10.0,
            tva: 19.0,
            supplier_name: None,
        };

        let result = svc.create_product(&request, 2024);
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::errors::AppError::Validation(crate::errors::ValidationError::InvalidPrice {
                value,
            }) => assert_eq!(value, -10.0),
            _ => panic!("Expected InvalidPrice validation error"),
        }
    }
}
