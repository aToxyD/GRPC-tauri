//! Supplier Order Item Allocations Repository Module
//!
//! Persists the authoritative price leg between an order item and the
//! contract allocation it fulfills/reserves (ADR-0055 / SEC-087-F).
//! unit_price is immutable after write (historical integrity).
//! ARCHITECTURE: SQL only — no loops, no calculations, no cross-repo calls.

use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use crate::repositories::numeric_row;
use rusqlite::{params, Row};

/// Recorded allocation leg of an order item.
#[derive(Debug, Clone)]
pub struct SupplierOrderItemAllocation {
    pub id: String,
    pub item_id: String,
    pub allocation_id: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub total_cost: f64,
}

fn map_row(row: &Row<'_>) -> Result<SupplierOrderItemAllocation, rusqlite::Error> {
    Ok(SupplierOrderItemAllocation {
        id: row.get(0)?,
        item_id: row.get(1)?,
        allocation_id: row.get(2)?,
        quantity: numeric_row::qty_col(3, row.get::<_, i64>(3)?)?,
        unit_price: numeric_row::money_col(4, row.get::<_, i64>(4)?)?,
        total_cost: numeric_row::money_col(5, row.get::<_, i64>(5)?)?,
    })
}

/// Repository for supplier_order_item_allocations
pub struct OrderAllocationRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> OrderAllocationRepository<'a> {
    /// Create a new OrderAllocationRepository with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert(
        &self,
        id: &str,
        item_id: &str,
        allocation_id: &str,
        quantity: f64,
        unit_price: f64,
        total_cost: f64,
        created_at: &str,
    ) -> Result<(), AppError> {
        let quantity_scaled = numeric_row::qty_scaled(quantity)?;
        let unit_price_scaled = numeric_row::money_scaled(unit_price)?;
        let total_cost_scaled = numeric_row::money_scaled(total_cost)?;
        self.executor.execute(
            "INSERT INTO supplier_order_item_allocations (id, item_id, allocation_id, quantity, unit_price, total_cost, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, item_id, allocation_id, quantity_scaled, unit_price_scaled, total_cost_scaled, created_at],
        )?;
        Ok(())
    }

    pub fn list_for_item(
        &self,
        item_id: &str,
    ) -> Result<Vec<SupplierOrderItemAllocation>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, item_id, allocation_id, quantity, unit_price, total_cost FROM supplier_order_item_allocations WHERE item_id = ?1 AND deleted = 0 ORDER BY created_at",
            [item_id],
            map_row,
        )?)
    }

    pub fn list_for_order(
        &self,
        order_id: &str,
    ) -> Result<Vec<SupplierOrderItemAllocation>, AppError> {
        Ok(self.executor.query_all(
            "SELECT soia.id, soia.item_id, soia.allocation_id, soia.quantity, soia.unit_price, soia.total_cost
             FROM supplier_order_item_allocations soia
             JOIN supplier_order_items soi ON soi.id = soia.item_id
             WHERE soi.order_id = ?1 AND soia.deleted = 0
             ORDER BY soia.created_at",
            [order_id],
            map_row,
        )?)
    }

    pub fn delete_for_item(&self, item_id: &str) -> Result<(), AppError> {
        self.executor.execute(
            "DELETE FROM supplier_order_item_allocations WHERE item_id = ?1",
            [item_id],
        )?;
        Ok(())
    }

    pub fn delete_for_order(&self, order_id: &str) -> Result<(), AppError> {
        self.executor.execute(
            "DELETE FROM supplier_order_item_allocations WHERE item_id IN (SELECT id FROM supplier_order_items WHERE order_id = ?1)",
            [order_id],
        )?;
        Ok(())
    }
}
