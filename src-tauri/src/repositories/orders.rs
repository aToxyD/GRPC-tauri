//! Orders Repository Module
//!
//! Handles supplier order-related database operations.
//! ARCHITECTURE: SQL only — no loops, no calculations, no cross-repo calls.

use crate::errors::AppError;
use crate::models::{OrderStatus, SupplierOrder, SupplierOrderItem};
use crate::repositories::executor::DbExecutor;
use crate::repositories::numeric_row;
use rusqlite::{params, Row};

fn map_supplier_order_row(row: &Row<'_>) -> Result<SupplierOrder, rusqlite::Error> {
    let order_date_str: String = row.get(1)?;
    let created_at_str: String = row.get(7)?;
    let fiscal_year: Option<i32> = row.get(8)?;
    let order_date = crate::errors::parse_naive_date(&order_date_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(SupplierOrder {
        id: row.get(0)?,
        order_date,
        supplier_id: row.get(2)?,
        supplier_name: row.get(3)?,
        reference_number: row.get(4)?,
        total_amount: match row.get::<_, Option<i64>>(5)? {
            Some(v) => Some(numeric_row::money_col(5, v)?),
            None => None,
        },
        status: OrderStatus::from(row.get::<_, String>(6)?),
        created_at,
        unit_id: row.get(9)?,
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

    #[allow(clippy::too_many_arguments)]
    pub fn create_supplier_order_header(
        &self,
        id: &str,
        reference_number: &Option<String>,
        supplier_id: &str,
        supplier_name: &str,
        unit_id: &str,
        fiscal_year: i32,
        total_amount: f64,
        order_date: &str,
        created_at: &str,
    ) -> Result<(), AppError> {
        let total_amount_scaled = numeric_row::money_scaled(total_amount)?;
        self.executor.execute(
            "INSERT INTO supplier_orders (id, order_date, supplier_id, supplier_name, reference_number, total_amount, status, created_at, fiscal_year, unit_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, order_date, supplier_id, supplier_name, reference_number, total_amount_scaled, "Draft", created_at, fiscal_year, unit_id],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_order_item(
        &self,
        item_id: &str,
        order_id: &str,
        item: &crate::models::OrderItemInput,
        unit_price: f64,
        item_cost: f64,
        unit_id: &str,
        fiscal_year: i32,
    ) -> Result<(), AppError> {
        let quantity_scaled = numeric_row::qty_scaled(item.quantity)?;
        let unit_price_scaled = numeric_row::money_scaled(unit_price)?;
        let item_cost_scaled = numeric_row::money_scaled(item_cost)?;
        self.executor.execute(
            "INSERT INTO supplier_order_items (id, order_id, product_id, quantity, unit_price, total_cost, unit_id, fiscal_year) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![item_id, order_id, &item.product_id, quantity_scaled, unit_price_scaled, item_cost_scaled, unit_id, &fiscal_year],
        )?;
        Ok(())
    }

    /// Fetch order items for confirmation: (product_id, quantity, product_name,
    /// unit_price, allocation_id). The allocation_id is the recorded reservation
    /// leg used to verify unchanged entitlement at confirmation time.
    ///
    /// Deterministic ordering contract (ADR-0056): rows are ordered by the
    /// resolver's allocation priority key (fiscal_year ASC, created_at ASC,
    /// id ASC) with `supplier_order_items.id` as the final tie-break. The order
    /// MUST NOT depend on SQLite rowid/insertion/index/join/plan order.
    /// Confirmation correctness itself is order-independent under the
    /// greedy-drain allocation invariant and Phase 4A own-reservation netting;
    /// this ORDER BY is the explicit determinism/execution-order contract.
    #[allow(clippy::type_complexity)]
    pub fn get_order_items_for_confirmation(
        &self,
        order_id: &str,
    ) -> Result<Vec<(String, f64, String, f64, String)>, AppError> {
        Ok(self.executor.query_all(
            "SELECT soi.product_id, soi.quantity, p.name, soi.unit_price, soia.allocation_id
             FROM supplier_order_items soi
             JOIN products p ON soi.product_id = p.id
             JOIN supplier_order_item_allocations soia ON soia.item_id = soi.id
             JOIN contract_allocations ca ON ca.id = soia.allocation_id
             WHERE soi.order_id = ?1
             ORDER BY ca.fiscal_year ASC, ca.created_at ASC, ca.id ASC, soi.id ASC",
            [order_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    numeric_row::qty_col(1, row.get::<_, i64>(1)?)?,
                    row.get::<_, String>(2)?,
                    numeric_row::money_col(3, row.get::<_, i64>(3)?)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )?)
    }

    /// Update draft order header. Supplier may be re-derived on draft update
    /// (still backend-resolved); it is immutable once confirmed.
    pub fn update_supplier_order_header(
        &self,
        id: &str,
        supplier_id: &str,
        supplier_name: &str,
        reference_number: &Option<String>,
        total_amount: f64,
    ) -> Result<usize, AppError> {
        let total_amount_scaled = numeric_row::money_scaled(total_amount)?;
        let n = self.executor.execute(
            "UPDATE supplier_orders SET supplier_id = ?1, supplier_name = ?2, reference_number = ?3, total_amount = ?4 WHERE id = ?5 AND status = 'Draft'",
            params![supplier_id, supplier_name, reference_number, total_amount_scaled, id],
        )?;
        Ok(n)
    }

    pub fn delete_order_items(&self, order_id: &str) -> Result<(), AppError> {
        self.executor.execute(
            "DELETE FROM supplier_order_items WHERE order_id = ?1",
            [order_id],
        )?;
        Ok(())
    }

    pub fn delete_supplier_order(&self, order_id: &str) -> Result<(), AppError> {
        let deleted = self.executor.execute(
            "DELETE FROM supplier_orders WHERE id = ?1 AND status = 'Draft'",
            [order_id],
        )?;
        if deleted == 0 {
            return Err(AppError::BusinessLogic(
                crate::errors::BusinessLogicError::ResourceNotFound {
                    resource: "طلبية".to_string(),
                    id: order_id.to_string(),
                },
            ));
        }
        Ok(())
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
                "SELECT id, order_date, supplier_id, supplier_name, reference_number, total_amount, status, created_at, fiscal_year, unit_id FROM supplier_orders WHERE id = ?1",
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
            r#"SELECT oi.id, oi.order_id, oi.product_id, p.name, oi.quantity, oi.unit_price, oi.total_cost, oi.unit_id, oi.fiscal_year
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
                    quantity: numeric_row::qty_col(4, row.get::<_, i64>(4)?)?,
                    unit_price: numeric_row::money_col(5, row.get::<_, i64>(5)?)?,
                    total_cost: numeric_row::money_col(6, row.get::<_, i64>(6)?)?,
                    unit_id: row.get(7)?,
                    fiscal_year: row.get(8)?,
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
                "SELECT id, order_date, supplier_id, supplier_name, reference_number, total_amount, status, created_at, fiscal_year, unit_id FROM supplier_orders ORDER BY order_date DESC",
                [],
                map_supplier_order_row,
            )?,
            Some(y) => self.executor.query_all(
                "SELECT id, order_date, supplier_id, supplier_name, reference_number, total_amount, status, created_at, fiscal_year, unit_id FROM supplier_orders WHERE fiscal_year = ?1 ORDER BY order_date DESC",
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
