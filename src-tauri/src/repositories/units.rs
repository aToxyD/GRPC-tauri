//! Units Repository Module
//!
//! Handles unit-related database operations.

use crate::errors::AppError;
use crate::models::{CreateUnitRequest, Unit};
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

/// A `supplier_orders` ↔ `supplier_order_items` ownership conflict surfaced by the
/// ADR-0064 §4.7 (D4/Q6) preflight. Carries the conflicting item together with its
/// parent order and **both** independently-nullable ownership values so the
/// fail-closed rejection is diagnosable. `None` denotes a SQL `NULL` ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierOrderOwnershipConflict {
    pub item_id: String,
    pub order_id: String,
    pub item_unit_id: Option<String>,
    pub order_unit_id: Option<String>,
}

/// Repository for unit-related database operations
pub struct UnitRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> UnitRepository<'a> {
    /// Create a new UnitRepository with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Insert a new unit row (SQL-only).
    /// Note: For atomic unit creation with user, use with_transaction at Database level.
    pub fn insert_unit(
        &self,
        id: &str,
        req: &CreateUnitRequest,
        wilaya_code: &str,
        user_id: &str,
        created_at: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO units (id, code, name, wilaya_code, user_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, &req.code, &req.name, wilaya_code, user_id, created_at],
        )?;
        Ok(())
    }

    /// Insert or replace raw unit data from imported package
    pub fn upsert_raw_unit(
        &self,
        id: &str,
        code: &str,
        name: &str,
        wilaya_code: &str,
        now: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT OR REPLACE INTO units (id, code, name, wilaya_code, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, code, name, wilaya_code, now],
        )?;
        Ok(())
    }

    /// Update a unit's user_id
    pub fn update_unit_user(&self, unit_id: &str, user_id: &str) -> Result<(), AppError> {
        self.executor.execute(
            "UPDATE units SET user_id = ?1 WHERE id = ?2",
            rusqlite::params![user_id, unit_id],
        )?;
        Ok(())
    }

    /// Get unit by ID
    pub fn get_unit(&self, unit_id: &str) -> Result<Option<Unit>, AppError> {
        let result = self.executor.query_row_optional(
            "SELECT id, code, name, wilaya_code, user_id, created_at FROM units WHERE id = ?1",
            [unit_id],
            |row| {
                let created_at_str: String = row.get(5)?;
                let created_at =
                    crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(Unit {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    name: row.get(2)?,
                    wilaya_code: row.get(3)?,
                    user_id: row.get(4)?,
                    created_at,
                })
            },
        )?;
        Ok(result)
    }

    /// Get unit by code
    pub fn get_unit_by_code(&self, code: &str) -> Result<Option<Unit>, AppError> {
        let result = self.executor.query_row_optional(
            "SELECT id, code, name, wilaya_code, user_id, created_at FROM units WHERE code = ?1",
            [code],
            |row| {
                let created_at_str: String = row.get(5)?;
                let created_at =
                    crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(Unit {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    name: row.get(2)?,
                    wilaya_code: row.get(3)?,
                    user_id: row.get(4)?,
                    created_at,
                })
            },
        )?;
        Ok(result)
    }

    /// Resolve canonical `units.id` when the local DB has a row for this display name + wilaya (UNIT nodes).
    pub fn find_unit_id_by_name_and_wilaya(
        &self,
        name: &str,
        wilaya_code: &str,
    ) -> Result<Option<String>, AppError> {
        self.executor
            .query_row_optional(
                "SELECT id FROM units WHERE name = ?1 AND wilaya_code = ?2 LIMIT 1",
                [name, wilaya_code],
                |row| row.get(0),
            )
            .map_err(AppError::from)
    }

    /// Get unit by associated user ID (for IDOR protection in password changes)
    pub fn get_unit_by_user_id(&self, user_id: &str) -> Result<Option<Unit>, AppError> {
        let result = self.executor.query_row_optional(
            "SELECT id, code, name, wilaya_code, user_id, created_at FROM units WHERE user_id = ?1",
            [user_id],
            |row| {
                let created_at_str: String = row.get(5)?;
                let created_at =
                    crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(Unit {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    name: row.get(2)?,
                    wilaya_code: row.get(3)?,
                    user_id: row.get(4)?,
                    created_at,
                })
            },
        )?;
        Ok(result)
    }

    /// List all units for a wilaya
    pub fn list_units(&self, wilaya_code: &str) -> Result<Vec<Unit>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, code, name, wilaya_code, user_id, created_at FROM units WHERE wilaya_code = ?1 ORDER BY code",
            [wilaya_code],
            |row| {
                let created_at_str: String = row.get(5)?;
                let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(Unit {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    name: row.get(2)?,
                    wilaya_code: row.get(3)?,
                    user_id: row.get(4)?,
                    created_at,
                })
            },
        )?)
    }

    /// List all units across all wilayas (for cross-unit benchmarks).
    pub fn list_all_units(&self) -> Result<Vec<Unit>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, code, name, wilaya_code, user_id, created_at FROM units ORDER BY code",
            [],
            |row| {
                let created_at_str: String = row.get(5)?;
                let created_at =
                    crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(Unit {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    name: row.get(2)?,
                    wilaya_code: row.get(3)?,
                    user_id: row.get(4)?,
                    created_at,
                })
            },
        )?)
    }

    /// Preflight: D14 gate - count audit_log entries for a user
    pub fn count_audit_log_for_user(&self, user_id: &str) -> Result<i64, AppError> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM audit_log WHERE user_id = ?1",
            [user_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Preflight: D4/Q6 - target-owned items hanging off a NULL/foreign parent order.
    /// Each row carries the conflicting item id, its `order_id`, and both ownership
    /// values (ADR-0064 §4.7 diagnostic requirement).
    pub fn find_supplier_order_items_with_conflicting_parents(
        &self,
        unit_id: &str,
    ) -> Result<Vec<SupplierOrderOwnershipConflict>, AppError> {
        let rows = self.executor.query_all(
            r#"SELECT soi.id, soi.order_id, soi.unit_id, so.unit_id
               FROM supplier_order_items soi
               JOIN supplier_orders so ON soi.order_id = so.id
               WHERE soi.unit_id = ?1
                 AND (so.unit_id IS NULL OR so.unit_id != soi.unit_id)"#,
            [unit_id],
            |row| {
                Ok(SupplierOrderOwnershipConflict {
                    item_id: row.get(0)?,
                    order_id: row.get(1)?,
                    item_unit_id: row.get(2)?,
                    order_unit_id: row.get(3)?,
                })
            },
        )?;
        Ok(rows)
    }

    /// Preflight: D4 - target orders owning a NULL/foreign child item. The reject set is
    /// identical to the previous `DISTINCT so.id` form; the projection now reports the
    /// conflicting child item and both ownership values (ADR-0064 §4.7).
    pub fn find_supplier_orders_with_conflicting_children(
        &self,
        unit_id: &str,
    ) -> Result<Vec<SupplierOrderOwnershipConflict>, AppError> {
        let rows = self.executor.query_all(
            r#"SELECT soi.id, soi.order_id, soi.unit_id, so.unit_id
               FROM supplier_orders so
               JOIN supplier_order_items soi ON soi.order_id = so.id
               WHERE so.unit_id = ?1
                 AND (soi.unit_id IS NULL OR soi.unit_id != so.unit_id)"#,
            [unit_id],
            |row| {
                Ok(SupplierOrderOwnershipConflict {
                    item_id: row.get(0)?,
                    order_id: row.get(1)?,
                    item_unit_id: row.get(2)?,
                    order_unit_id: row.get(3)?,
                })
            },
        )?;
        Ok(rows)
    }

    /// Preflight: D5 (§4.7) - for every allocation whose own unit_id = target OR whose parent
    /// contract's unit_id = target, the allocation's unit_id MUST equal the parent contract's.
    pub fn find_contract_allocation_unit_mismatches(&self, unit_id: &str) -> Result<Vec<String>, AppError> {
        let rows: Vec<String> = self.executor.query_all(
            r#"SELECT ca.id
               FROM contract_allocations ca
               JOIN contracts c ON ca.contract_id = c.id
               WHERE (ca.unit_id = ?1 OR c.unit_id = ?1)
                 AND ca.unit_id != c.unit_id"#,
            [unit_id],
            |row| row.get(0),
        )?;
        Ok(rows)
    }

    /// Preflight: D6 / A1 (§4.7) - a supplier_order_item_allocations row is safe to delete only
    /// when BOTH parents resolve to the target UNIT. Reject when either parent is not target-owned
    /// while the other is (two-sided ownership cannot be proven).
    pub fn find_soa_two_sided_mismatches(&self, unit_id: &str) -> Result<Vec<String>, AppError> {
        let rows: Vec<String> = self.executor.query_all(
            r#"SELECT soia.id
               FROM supplier_order_item_allocations soia
               JOIN supplier_order_items soi ON soia.item_id = soi.id
               JOIN contract_allocations ca ON soia.allocation_id = ca.id
               WHERE (
                     (soi.unit_id = ?1 AND ca.unit_id != ?1)
                  OR (ca.unit_id = ?1 AND (soi.unit_id IS NULL OR soi.unit_id != ?1))
               )"#,
            [unit_id],
            |row| row.get(0),
        )?;
        Ok(rows)
    }

    /// Preflight: §4.9 - surviving report meal items referencing target FIFO layers
    pub fn find_surviving_report_fifo_refs(&self, unit_id: &str) -> Result<Vec<String>, AppError> {
        let rows: Vec<String> = self.executor.query_all(
            r#"SELECT drmi.id
               FROM daily_report_meal_items drmi
               JOIN daily_report_meals drm ON drmi.meal_id = drm.id
               JOIN daily_reports dr ON drm.daily_report_id = dr.id
               WHERE (dr.unit_id IS NULL OR dr.unit_id != ?1)
                 AND drmi.fifo_layer_id IS NOT NULL
                 AND drmi.fifo_layer_id IN (SELECT id FROM fifo_stock_layers WHERE unit_id = ?1)"#,
            [unit_id],
            |row| row.get(0),
        )?;
        Ok(rows)
    }

    /// Preflight: §4.10 - non-target inventory_layer_consumptions referencing target FIFO layers
    pub fn find_crossunit_ilm_to_fifo(&self, unit_id: &str) -> Result<Vec<String>, AppError> {
        let rows: Vec<String> = self.executor.query_all(
            r#"SELECT id
               FROM inventory_layer_consumptions
               WHERE unit_id != ?1
                 AND layer_id IN (SELECT id FROM fifo_stock_layers WHERE unit_id = ?1)"#,
            [unit_id],
            |row| row.get(0),
        )?;
        Ok(rows)
    }

    /// Preflight: §4.10 - non-target inventory_layer_consumptions referencing target stock movements
    pub fn find_crossunit_ilm_to_movement(&self, unit_id: &str) -> Result<Vec<String>, AppError> {
        let rows: Vec<String> = self.executor.query_all(
            r#"SELECT id
               FROM inventory_layer_consumptions
               WHERE unit_id != ?1
                 AND movement_id IN (SELECT id FROM stock_movements WHERE unit_id = ?1)"#,
            [unit_id],
            |row| row.get(0),
        )?;
        Ok(rows)
    }

    /// Preflight: §4.10 - contract_allocations references into target contract_products.
    /// Scoped to `contract_products` lines owned by a target contract (deleted in A5). Reject
    /// when (a) a non-target allocation references such a line, or (b) the referencing
    /// allocation's `contract_id` differs from the referenced contract_product's `contract_id`
    /// (structural inconsistency the schema permits; detected here rather than by FK rollback).
    pub fn find_alloc_to_contract_product_conflicts(&self, unit_id: &str) -> Result<Vec<String>, AppError> {
        let rows: Vec<String> = self.executor.query_all(
            r#"SELECT ca.id
               FROM contract_allocations ca
               JOIN contract_products cp ON ca.contract_product_id = cp.id
               WHERE cp.contract_id IN (SELECT id FROM contracts WHERE unit_id = ?1)
                 AND (ca.unit_id != ?1 OR ca.contract_id != cp.contract_id)"#,
            [unit_id],
            |row| row.get(0),
        )?;
        Ok(rows)
    }

    /// Execute Phase 1 purge: delete Class-A entities in ADR-0064 §4.4 order
    /// All deletes scoped to unit_id as specified. Assumes preflight passed.
    pub fn purge_unit_owned_data(&self, unit_id: &str, resolved_user_id: &str) -> Result<(), AppError> {
        // A1: supplier_order_item_allocations
        self.executor.execute(
            r#"DELETE FROM supplier_order_item_allocations
               WHERE item_id IN (
                 SELECT soi.id FROM supplier_order_items soi WHERE soi.unit_id = ?1
               ) AND allocation_id IN (
                 SELECT ca.id FROM contract_allocations ca WHERE ca.unit_id = ?1
               )"#,
            [unit_id],
        )?;

        // A2: contract_allocation_exceptions
        self.executor.execute(
            r#"DELETE FROM contract_allocation_exceptions
               WHERE allocation_id IN (SELECT id FROM contract_allocations WHERE unit_id = ?1)"#,
            [unit_id],
        )?;

        // A3: inventory_layer_consumptions
        self.executor.execute(
            "DELETE FROM inventory_layer_consumptions WHERE unit_id = ?1",
            [unit_id],
        )?;

        // A4: contract_allocations
        self.executor.execute(
            "DELETE FROM contract_allocations WHERE unit_id = ?1",
            [unit_id],
        )?;

        // A5: contract_products
        self.executor.execute(
            r#"DELETE FROM contract_products
               WHERE contract_id IN (SELECT id FROM contracts WHERE unit_id = ?1)"#,
            [unit_id],
        )?;

        // A6: supplier_order_items
        self.executor.execute(
            "DELETE FROM supplier_order_items WHERE unit_id = ?1",
            [unit_id],
        )?;

        // A7: contracts
        self.executor.execute(
            "DELETE FROM contracts WHERE unit_id = ?1",
            [unit_id],
        )?;

        // A8: supplier_orders (NULL preserved as per §4.8)
        self.executor.execute(
            "DELETE FROM supplier_orders WHERE unit_id = ?1",
            [unit_id],
        )?;

        // A9: unit_suppliers
        self.executor.execute(
            "DELETE FROM unit_suppliers WHERE unit_id = ?1",
            [unit_id],
        )?;

        // A10: fifo_stock_layers
        self.executor.execute(
            "DELETE FROM fifo_stock_layers WHERE unit_id = ?1",
            [unit_id],
        )?;

        // A11: stock_movements (NULL preserved as per D20)
        self.executor.execute(
            "DELETE FROM stock_movements WHERE unit_id = ?1",
            [unit_id],
        )?;

        // A12: daily_report_meal_items
        self.executor.execute(
            r#"DELETE FROM daily_report_meal_items
               WHERE meal_id IN (
                 SELECT drm.id FROM daily_report_meals drm
                 JOIN daily_reports dr ON drm.daily_report_id = dr.id
                 WHERE dr.unit_id = ?1
               )"#,
            [unit_id],
        )?;

        // A13: daily_report_meals
        self.executor.execute(
            r#"DELETE FROM daily_report_meals
               WHERE daily_report_id IN (SELECT id FROM daily_reports WHERE unit_id = ?1)"#,
            [unit_id],
        )?;

        // A14: daily_reports (NULL preserved as per §4.8)
        self.executor.execute(
            "DELETE FROM daily_reports WHERE unit_id = ?1",
            [unit_id],
        )?;

        // A15: monthly_reports
        self.executor.execute(
            "DELETE FROM monthly_reports WHERE unit_id = ?1",
            [unit_id],
        )?;

        // A16: unit_monthly_snapshots
        self.executor.execute(
            "DELETE FROM unit_monthly_snapshots WHERE unit_id = ?1",
            [unit_id],
        )?;

        // A17: identity_store rows for this UNIT
        self.executor.execute(
            "DELETE FROM identity_store WHERE subject_type = 'UNIT' AND subject_id = ?1",
            [unit_id],
        )?;

        // A18: users (canonical operator)
        self.executor.execute(
            "DELETE FROM users WHERE id = ?1",
            [resolved_user_id],
        )?;

        // A19: units
        self.executor.execute(
            "DELETE FROM units WHERE id = ?1",
            [unit_id],
        )?;

        Ok(())
    }

    /// Update a unit's mutable information (SQL-only).
    ///
    /// ADR-0063 D8: `code` is immutable after creation — the canonical UNIT
    /// operator credential is node-bound to the code, so the column is never
    /// written by an update path. Only `name` is editable.
    pub fn update_unit_name(&self, unit_id: &str, name: &str, now: &str) -> Result<(), AppError> {
        self.executor.execute(
            "UPDATE units SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, now, unit_id],
        )?;

        Ok(())
    }

    /// Get user ID associated with a unit
    pub fn get_unit_user_id(&self, unit_id: &str) -> Result<Option<String>, AppError> {
        let user_id: Option<String> = self.executor.query_row_optional(
            "SELECT user_id FROM units WHERE id = ?1",
            [unit_id],
            |row| row.get(0),
        )?;
        Ok(user_id)
    }
}
