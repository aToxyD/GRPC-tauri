//! Suppliers Repository Module
//!
//! Handles supplier-related database operations (ADR-0055 / SEC-087-F).
//! ARCHITECTURE: SQL only — no loops, no calculations, no cross-repo calls.

use crate::errors::AppError;
use crate::models::{CreateSupplierRequest, Supplier, UpdateSupplierRequest};
use crate::repositories::executor::DbExecutor;
use rusqlite::{params, Row};

fn map_supplier_row(row: &Row<'_>) -> Result<Supplier, rusqlite::Error> {
    Ok(Supplier {
        id: row.get(0)?,
        name: row.get(1)?,
        contact_info: row.get(2)?,
        active: row.get(3)?,
        created_at: row.get(4)?,
    })
}

/// Repository for supplier-related database operations
pub struct SupplierRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> SupplierRepository<'a> {
    /// Create a new SupplierRepository with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn insert_supplier(
        &self,
        id: &str,
        req: &CreateSupplierRequest,
        created_at: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO suppliers (id, name, contact_info, active, created_at) VALUES (?1, ?2, ?3, 1, ?4)",
            params![id, &req.name, &req.contact_info, created_at],
        )?;
        Ok(())
    }

    pub fn update_supplier(&self, req: &UpdateSupplierRequest) -> Result<usize, AppError> {
        let n = self.executor.execute(
            "UPDATE suppliers SET name = ?1, contact_info = ?2 WHERE id = ?3",
            params![&req.name, &req.contact_info, &req.id],
        )?;
        Ok(n)
    }

    pub fn set_supplier_active(&self, id: &str, active: bool) -> Result<usize, AppError> {
        let n = self.executor.execute(
            "UPDATE suppliers SET active = ?1 WHERE id = ?2",
            params![active, id],
        )?;
        Ok(n)
    }

    pub fn get_supplier(&self, id: &str) -> Result<Option<Supplier>, AppError> {
        let result = self.executor.query_row_optional(
            "SELECT id, name, contact_info, active, created_at FROM suppliers WHERE id = ?1",
            [id],
            map_supplier_row,
        )?;
        Ok(result)
    }

    pub fn find_supplier_by_name(&self, name: &str) -> Result<Option<Supplier>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT id, name, contact_info, active, created_at FROM suppliers WHERE name = ?1 COLLATE NOCASE",
                [name],
                map_supplier_row,
            )?;
        Ok(result)
    }

    pub fn list_suppliers(&self) -> Result<Vec<Supplier>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, name, contact_info, active, created_at FROM suppliers ORDER BY name",
            [],
            map_supplier_row,
        )?)
    }

    pub fn list_active_suppliers(&self) -> Result<Vec<Supplier>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, name, contact_info, active, created_at FROM suppliers WHERE active = 1 ORDER BY name",
            [],
            map_supplier_row,
        )?)
    }

    pub fn count_supplier_orders(&self, supplier_id: &str) -> Result<u32, AppError> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM supplier_orders WHERE supplier_id = ?1",
            [supplier_id],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }

    pub fn count_supplier_contracts(&self, supplier_id: &str) -> Result<u32, AppError> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM contracts WHERE supplier_id = ?1",
            [supplier_id],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }

    pub fn associate_with_unit(&self, unit_id: &str, supplier_id: &str) -> Result<(), AppError> {
        // `unit_suppliers.created_at` is NOT NULL (no DEFAULT); omitting it made
        // `INSERT OR IGNORE` silently swallow the NOT NULL violation and insert
        // nothing. Deterministic single-write ordering, SQL confined to the repo.
        self.executor.execute(
            "INSERT OR IGNORE INTO unit_suppliers (unit_id, supplier_id, created_at)
             VALUES (?1, ?2, datetime('now'))",
            params![unit_id, supplier_id],
        )?;
        Ok(())
    }

    pub fn disassociate_from_unit(&self, unit_id: &str, supplier_id: &str) -> Result<(), AppError> {
        self.executor.execute(
            "DELETE FROM unit_suppliers WHERE unit_id = ?1 AND supplier_id = ?2",
            params![unit_id, supplier_id],
        )?;
        Ok(())
    }

    pub fn list_unit_supplier_ids(&self, unit_id: &str) -> Result<Vec<String>, AppError> {
        Ok(self.executor.query_all(
            "SELECT supplier_id FROM unit_suppliers WHERE unit_id = ?1",
            [unit_id],
            |row| row.get(0),
        )?)
    }

    pub fn supplier_associated_with_unit(
        &self,
        unit_id: &str,
        supplier_id: &str,
    ) -> Result<bool, AppError> {
        let existing: Option<String> = self.executor.query_row_optional(
            "SELECT supplier_id FROM unit_suppliers WHERE unit_id = ?1 AND supplier_id = ?2",
            params![unit_id, supplier_id],
            |row| row.get(0),
        )?;
        Ok(existing.is_some())
    }

    /// All UNIT↔supplier associations ordered deterministically (ContractCatalog
    /// sync dataset export).
    pub fn list_all_unit_supplier_links(&self) -> Result<Vec<(String, String)>, AppError> {
        Ok(self.executor.query_all(
            "SELECT unit_id, supplier_id FROM unit_suppliers ORDER BY unit_id, supplier_id",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    /// WILAYA-authoritative supplier upsert (ContractCatalog V2, WILAYA → UNIT
    /// read-only projection). Refreshes every supplier-owned column on conflict.
    pub fn upsert_sync_supplier(&self, supplier: &Supplier) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO suppliers (id, name, contact_info, active, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                contact_info = excluded.contact_info,
                active = excluded.active",
            params![
                supplier.id,
                supplier.name,
                supplier.contact_info,
                supplier.active as i64,
                supplier.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }
}
