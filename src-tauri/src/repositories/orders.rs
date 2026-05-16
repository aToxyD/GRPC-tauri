//! Orders Repository Module
//!
//! Handles supplier order-related database operations.
//! ARCHITECTURE: SQL only — no loops, no calculations, no cross-repo calls.

use crate::errors::AppError;
use crate::models::{CreateOrderRequest, OrderStatus, SupplierOrder, SupplierOrderItem};
use crate::repositories::executor::DbExecutor;
use chrono::Datelike;
use rusqlite::{params, Row};

fn map_supplier_order_row(row: &Row<'_>) -> Result<SupplierOrder, rusqlite::Error> {
    let order_date_str: String = row.get(1)?;
    let created_at_str: String = row.get(6)?;
    let fiscal_year: Option<i32> = row.get(7)?;
    let order_date = crate::errors::parse_naive_date(&order_date_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(SupplierOrder {
        id: row.get(0)?,
        order_date,
        supplier_name: row.get(2)?,
        reference_number: row.get(3)?,
        total_amount: row.get(4)?,
        status: OrderStatus::from(row.get::<_, String>(5)?),
        created_at,
        fiscal_year,
    })
}
/// Repository for order-related database operations
pub struct OrderRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> OrderRepository<'a> {
    /// Create a new OrderRepository with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn create_supplier_order_header(
        &self,
        id: &str,
        req: &CreateOrderRequest,
        total_amount: f64,
        order_date: &str,
        created_at: &str,
    ) -> Result<(), AppError> {
        let fy = chrono::NaiveDate::parse_from_str(order_date, "%Y-%m-%d")
            .map(|d| d.year())
            .unwrap_or_else(|_| chrono::Utc::now().date_naive().year());
        self.executor.execute(
            "INSERT INTO supplier_orders (id, order_date, supplier_name, reference_number, total_amount, status, created_at, fiscal_year) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, order_date, &req.supplier_name, &req.reference_number, &total_amount, "Draft", created_at, fy],
        )?;
        Ok(())
    }

    pub fn insert_order_item(
        &self,
        item_id: &str,
        order_id: &str,
        item: &crate::models::OrderItemInput,
        item_cost: f64,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO supplier_order_items (id, order_id, product_id, quantity, unit_price, total_cost) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![item_id, order_id, &item.product_id, &item.quantity, &item.unit_price, &item_cost],
        )?;
        Ok(())
    }

    /// Fetch order items for confirmation (product_id, quantity, product_name).
    /// Pure SQL — used by OrderService to orchestrate stock movements.
    pub fn get_order_items_for_confirmation(
        &self,
        order_id: &str,
    ) -> Result<Vec<(String, f64, String)>, AppError> {
        Ok(self.executor.query_all(
            "SELECT soi.product_id, soi.quantity, p.name
             FROM supplier_order_items soi
             JOIN products p ON soi.product_id = p.id
             WHERE soi.order_id = ?1",
            [order_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )?)
    }

    /// Mark an order as Confirmed. Pure SQL — called by OrderService after movements are recorded.
    pub fn set_order_confirmed(&self, order_id: &str) -> Result<(), AppError> {
        self.executor.execute(
            "UPDATE supplier_orders SET status = 'Confirmed' WHERE id = ?1",
            [order_id],
        )?;
        Ok(())
    }

    /// Get supplier order by ID
    pub fn get_supplier_order(&self, order_id: &str) -> Result<Option<SupplierOrder>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT id, order_date, supplier_name, reference_number, total_amount, status, created_at, fiscal_year FROM supplier_orders WHERE id = ?1",
                [order_id],
                map_supplier_order_row,
            )?;
        Ok(result)
    }

    /// Get supplier order items
    pub fn get_supplier_order_items(
        &self,
        order_id: &str,
    ) -> Result<Vec<SupplierOrderItem>, AppError> {
        Ok(self.executor.query_all(
            r#"SELECT oi.id, oi.order_id, oi.product_id, p.name, oi.quantity, oi.unit_price, oi.total_cost
               FROM supplier_order_items oi
               JOIN products p ON oi.product_id = p.id
               WHERE oi.order_id = ?1"#,
            [order_id],
            |row| {
                Ok(SupplierOrderItem {
                    id: row.get(0)?,
                    order_id: row.get(1)?,
                    product_id: row.get(2)?,
                    product_name: row.get(3)?,
                    quantity: row.get(4)?,
                    unit_price: row.get(5)?,
                    total_cost: row.get(6)?,
                })
            },
        )?)
    }

    /// List supplier orders, optionally restricted to a fiscal year (calendar year of `order_date` / `fiscal_year` column).
    pub fn list_supplier_orders(
        &self,
        fiscal_year: Option<i32>,
    ) -> Result<Vec<SupplierOrder>, AppError> {
        let rows = match fiscal_year {
            None => self.executor.query_all(
                "SELECT id, order_date, supplier_name, reference_number, total_amount, status, created_at, fiscal_year FROM supplier_orders ORDER BY order_date DESC",
                [],
                map_supplier_order_row,
            )?,
            Some(y) => self.executor.query_all(
                "SELECT id, order_date, supplier_name, reference_number, total_amount, status, created_at, fiscal_year FROM supplier_orders WHERE fiscal_year = ?1 ORDER BY order_date DESC",
                params![y],
                map_supplier_order_row,
            )?,
        };
        Ok(rows)
    }

    pub fn count_today_orders(&self, today_date: &str) -> Result<u32, AppError> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM supplier_orders WHERE date(created_at) = ?1",
            [today_date],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }
}
