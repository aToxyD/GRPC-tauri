//! Products Repository Module
//!
//! Handles all product-related database operations.

use crate::errors::AppError;
use crate::models::{CreateProductRequest, Product, UpdateProductRequest};
use crate::repositories::executor::DbExecutor;
use rusqlite::params;

/// Type alias for the product sync metadata tuple returned by `get_sync_info`.
type SyncInfo = (Option<String>, Option<String>, Option<i32>);

/// Repository for product-related database operations
pub struct ProductRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> ProductRepository<'a> {
    /// Create a new ProductRepository with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Insert a new product row (SQL only).
    pub fn insert_product(
        &self,
        id: &str,
        req: &CreateProductRequest,
        year: i32,
        created_at: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO products (id, name, base_price, tva, supplier_name, year, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, &req.name, &req.base_price, &req.tva, &req.supplier_name, &year, created_at],
        )?;
        Ok(())
    }

    pub fn product_exists(&self, product_id: &str) -> Result<bool, AppError> {
        let existing: Option<String> = self.executor.query_row_optional(
            "SELECT id FROM products WHERE id = ?1",
            [product_id],
            |row| row.get(0),
        )?;
        Ok(existing.is_some())
    }

    pub fn insert_raw_product(&self, product: &Product, now: &str) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO products (id, name, base_price, tva, supplier_name, year, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![&product.id, &product.name, &product.base_price, &product.tva, &product.supplier_name, &product.year, now],
        )?;
        Ok(())
    }

    /// Get a product's updated_at timestamp for sync
    pub fn get_product_updated_at(&self, product_id: &str) -> Result<Option<String>, AppError> {
        self.executor
            .query_row_optional(
                "SELECT updated_at FROM products WHERE id = ?1",
                [product_id],
                |row| row.get(0),
            )
            .map_err(AppError::from)
    }

    /// Insert or replace a product from a sync record
    pub fn upsert_product_sync(
        &self,
        record: &crate::models::ProductSyncRecord,
    ) -> Result<(), AppError> {
        // Use INSERT OR REPLACE to handle both new and existing products
        // This will delete the old row if it exists, which may fail with FK constraints
        // If that fails, try UPDATE instead
        let result = self.executor.execute(
            "INSERT OR REPLACE INTO products (id, name, base_price, tva, supplier_name, year, created_at, updated_at, node_id, deleted) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                &record.id,
                &record.name,
                &record.base_price,
                &record.tva,
                &record.supplier_name,
                &record.year,
                &record.created_at,
                &record.updated_at,
                &record.node_id,
                &record.deleted
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                // FK constraint failed, try UPDATE instead
                self.executor.execute(
                    "UPDATE products SET name = ?1, base_price = ?2, tva = ?3, supplier_name = ?4, year = ?5, updated_at = ?6, node_id = ?7, deleted = ?8 WHERE id = ?9",
                    rusqlite::params![
                        &record.name,
                        &record.base_price,
                        &record.tva,
                        &record.supplier_name,
                        &record.year,
                        &record.updated_at,
                        &record.node_id,
                        &record.deleted,
                        &record.id
                    ],
                )?;
                Ok(())
            }
            Err(e) => Err(AppError::Sqlite(e)),
        }
    }

    /// Update an existing product
    pub fn update_product(&self, req: &UpdateProductRequest) -> Result<(), AppError> {
        self.executor.execute(
            "UPDATE products SET name = ?1, base_price = ?2, tva = ?3, supplier_name = ?4 WHERE id = ?5",
            params![&req.name, &req.base_price, &req.tva, &req.supplier_name, &req.id],
        )?;
        Ok(())
    }

    /// Delete a product
    pub fn delete_product(&self, product_id: &str) -> Result<(), AppError> {
        self.executor
            .execute("DELETE FROM products WHERE id = ?1", [product_id])?;
        Ok(())
    }

    /// Get product by ID
    pub fn get_product(&self, product_id: &str) -> Result<Option<Product>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT id, name, base_price, tva, supplier_name, year, created_at FROM products WHERE id = ?1",
                [product_id],
                |row| {
                    let created_at_str: String = row.get(6)?;
                    let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
                    Ok(Product {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        base_price: row.get(2)?,
                        tva: row.get(3)?,
                        supplier_name: row.get(4)?,
                        year: row.get(5)?,
                        created_at,
                    })
                },
            )?;
        Ok(result)
    }

    /// List all products (across all years)
    pub fn list_products(&self) -> Result<Vec<Product>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, name, base_price, tva, supplier_name, year, created_at FROM products ORDER BY year DESC, name",
            [],
            |row| {
                let created_at_str: String = row.get(6)?;
                let created_at = crate::errors::parse_datetime_rfc3339(&created_at_str)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(Product {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    base_price: row.get(2)?,
                    tva: row.get(3)?,
                    supplier_name: row.get(4)?,
                    year: row.get(5)?,
                    created_at,
                })
            },
        )?)
    }

    pub fn count_active_products(&self) -> Result<i64, AppError> {
        let count = self.executor.query_row(
            "SELECT COUNT(*) FROM products WHERE deleted = 0",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn get_sync_info(&self, id: &str) -> Result<SyncInfo, AppError> {
        Ok(self.executor.query_row(
            "SELECT updated_at, node_id, deleted FROM products WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?)
    }
}
