//! Order Service Module
//!
//! Business logic for supplier orders and order confirmation.
//! SQL is delegated exclusively to OrderRepository (inventory movements via StockMovementService).

use crate::domain::validation::{
    check_order_is_editable, validate_create_order_request, validate_update_order_request,
};
use crate::errors::{AppError, BusinessLogicError};
use crate::models::{CreateOrderRequest, NewStockMovement, StockMovementType, UpdateOrderRequest};
use crate::repositories::{DbExecutor, RepositoryProvider};

/// Service for supplier order business logic
pub struct OrderService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> OrderService<'a> {
    /// Create a new OrderService with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn create_supplier_order(
        &self,
        req: &CreateOrderRequest,
    ) -> Result<(String, f64), AppError> {
        validate_create_order_request(req)?;
        let total_amount: f64 = req.items.iter().map(|i| i.quantity * i.unit_price).sum();
        let repo = self.executor.orders();

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let order_date = chrono::Utc::now().date_naive().to_string();

        repo.create_supplier_order_header(&id, req, total_amount, &order_date, &now)?;

        for item in &req.items {
            let item_cost = item.quantity * item.unit_price;
            let item_id = uuid::Uuid::new_v4().to_string();
            repo.insert_order_item(&item_id, &id, item, item_cost)?;
        }

        Ok((id, total_amount))
    }

    pub fn update_supplier_order(&self, req: &UpdateOrderRequest) -> Result<f64, AppError> {
        validate_update_order_request(req)?;

        let repo = self.executor.orders();
        let order = repo.get_supplier_order(&req.id)?.ok_or_else(|| {
            AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
                resource: "طلبية".to_string(),
                id: req.id.clone(),
            })
        })?;
        check_order_is_editable(&order)?;

        let total_amount: f64 = req.items.iter().map(|i| i.quantity * i.unit_price).sum();

        let updated = repo.update_supplier_order_header(
            &req.id,
            &req.supplier_name,
            &req.reference_number,
            total_amount,
        )?;
        if updated == 0 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OrderAlreadyConfirmed {
                    order_id: req.id.clone(),
                },
            ));
        }
        repo.delete_order_items(&req.id)?;

        for item in &req.items {
            let item_cost = item.quantity * item.unit_price;
            let item_id = uuid::Uuid::new_v4().to_string();
            repo.insert_order_item(&item_id, &req.id, item, item_cost)?;
        }

        Ok(total_amount)
    }

    pub fn delete_supplier_order(&self, order_id: &str) -> Result<(), AppError> {
        let repo = self.executor.orders();
        let order = repo.get_supplier_order(order_id)?.ok_or_else(|| {
            AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
                resource: "طلبية".to_string(),
                id: order_id.to_string(),
            })
        })?;
        check_order_is_editable(&order)?;
        repo.delete_supplier_order(order_id)
    }

    /// Confirm an order atomically:
    /// - validate order is not already confirmed
    /// - record an IN stock movement for every item
    /// - update order status to Confirmed
    ///
    /// The caller is responsible for wrapping this in a transaction via
    /// `db.with_transaction(|tx| OrderService::new(tx).confirm_order_atomic(...))`
    pub fn confirm_order_atomic(
        &self,
        order_id: &str,
        user_id: &str,
        username: &str,
        unit_id: Option<&str>,
    ) -> Result<(), AppError> {
        let repo = self.executor.orders();
        let stock_repo = crate::application::services::StockMovementService::new(self.executor);
        let fifo_repo = self.executor.fifo_layers();
        let status_repo = self.executor.fiscal_year_status();

        let unit_id_str = unit_id.ok_or_else(|| {
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "unit_id is required to confirm an order".to_string(),
            })
        })?;

        // 1. Guard: check order status
        let order = repo
            .get_supplier_order(order_id)?
            .ok_or_else(|| AppError::Internal(format!("Order not found: {}", order_id)))?;

        if order.status == crate::models::OrderStatus::Confirmed {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OrderAlreadyConfirmed {
                    order_id: order_id.to_string(),
                },
            ));
        }

        // 2. Fetch items (product_id, quantity, product_name, unit_price)
        let items = repo.get_order_items_for_confirmation(order_id)?;

        // 3. Resolve active fiscal year and guard it is still open
        let now = chrono::Utc::now().to_rfc3339();

        let active_fy = match order.fiscal_year {
            Some(fy) => fy,
            None => status_repo.get_open_year()?.ok_or_else(|| {
                AppError::Internal("No open fiscal year found for order confirmation".to_string())
            })?,
        };

        crate::application::services::FiscalYearService::new(self.executor)
            .assert_fiscal_year_open(active_fy)?;

        for (product_id, quantity, product_name, unit_price) in &items {
            let movement = NewStockMovement {
                product_id: product_id.clone(),
                movement_type: StockMovementType::In,
                quantity: *quantity,
                reference_type: Some("Order".to_string()),
                reference_id: Some(order_id.to_string()),
                notes: Some(format!(
                    "طلبية من: {} - {}",
                    order.supplier_name, product_name
                )),
                user_id: user_id.to_string(),
                username: username.to_string(),
                unit_id: unit_id.map(|u| u.to_string()),
                unit_cost: Some(*unit_price),
            };
            stock_repo.record_stock_movement(&movement)?;

            fifo_repo.create_layer(
                unit_id_str,
                product_id,
                "ORDER",
                Some(order_id),
                *unit_price,
                *quantity,
                &now,
                user_id,
                active_fy,
            )?;
        }

        // 4. Mark order confirmed
        repo.set_order_confirmed(order_id)?;

        Ok(())
    }

    pub fn get_supplier_order(
        &self,
        order_id: &str,
    ) -> Result<Option<crate::models::SupplierOrder>, AppError> {
        self.executor.orders().get_supplier_order(order_id)
    }

    pub fn get_supplier_order_items(
        &self,
        order_id: &str,
    ) -> Result<Vec<crate::models::SupplierOrderItem>, AppError> {
        self.executor.orders().get_supplier_order_items(order_id)
    }

    pub fn list_supplier_orders(
        &self,
        fiscal_year: Option<i32>,
    ) -> Result<Vec<crate::models::SupplierOrder>, AppError> {
        self.executor.orders().list_supplier_orders(fiscal_year)
    }
}
