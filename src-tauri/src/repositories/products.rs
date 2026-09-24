//! Products Repository Module
//!
//! Handles all product-related database operations.

use crate::errors::AppError;
use crate::models::{CreateProductRequest, Product, UpdateProductRequest};
use crate::repositories::executor::DbExecutor;
use crate::repositories::numeric_row;
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

    /// Insert a new product row (SQL only). The SEC-087 unit/TVA configuration
    /// arrives already validated by the application layer.
    pub fn insert_product(
        &self,
        id: &str,
        req: &CreateProductRequest,
        year: i32,
        config: &crate::domain::units::ProductUnitConfig,
        created_at: &str,
    ) -> Result<(), AppError> {
        let base_price_scaled = numeric_row::money_scaled(req.base_price)?;
        self.executor.execute(
            "INSERT INTO products (id, name, base_price, year, purchase_unit, consumption_unit, conversion_factor, tva_classification, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                &req.name,
                base_price_scaled,
                &year,
                config.purchase_unit.code(),
                config.consumption_unit.code(),
                config.conversion_factor,
                config.tva_classification.code(),
                created_at,
            ],
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

    /// Read the product's SEC-087 unit/TVA configuration wire codes for the
    /// sync export surface (SQL only). Unlike `get_product_config_codes`, this
    /// does NOT filter on `deleted` so the WILAYA exporter can emit a
    /// conforming V3 payload for soft-deleted rows too (a deleted Product
    /// payload must still carry the configuration, SEC-087 Phase 6B).
    pub fn get_product_sync_config(
        &self,
        product_id: &str,
    ) -> Result<Option<crate::models::ProductUnitConfigCodes>, AppError> {
        Ok(self.executor.query_row_optional(
            "SELECT purchase_unit, consumption_unit, conversion_factor, tva_classification
             FROM products WHERE id = ?1",
            [product_id],
            |r| {
                Ok(crate::models::ProductUnitConfigCodes {
                    purchase_unit: r.get::<_, i32>(0)?,
                    consumption_unit: r.get::<_, i32>(1)?,
                    conversion_factor: r.get::<_, i32>(2)?,
                    tva_classification: r.get::<_, i32>(3)?,
                })
            },
        )?)
    }

    /// Read the product's SEC-087 unit/TVA configuration codes (SQL only).
    ///
    /// The application layer validates these codes through
    /// `domain::validation::validate_product_units` (fail closed) before they
    /// can seed a contract price snapshot. Repository performs no logic.
    pub fn get_product_config_codes(
        &self,
        product_id: &str,
    ) -> Result<Option<crate::models::ProductUnitConfigCodes>, AppError> {
        let row = self.executor.query_row_optional(
            "SELECT purchase_unit, consumption_unit, conversion_factor, tva_classification
             FROM products WHERE id = ?1 AND deleted = 0",
            [product_id],
            |r| {
                Ok(crate::models::ProductUnitConfigCodes {
                    purchase_unit: r.get::<_, i32>(0)?,
                    consumption_unit: r.get::<_, i32>(1)?,
                    conversion_factor: r.get::<_, i32>(2)?,
                    tva_classification: r.get::<_, i32>(3)?,
                })
            },
        )?;
        Ok(row)
    }

    pub fn insert_raw_product(
        &self,
        product: &Product,
        config: &crate::models::ProductUnitConfigCodes,
        now: &str,
    ) -> Result<(), AppError> {
        let base_price_scaled = numeric_row::money_scaled(product.base_price)?;
        self.executor.execute(
            "INSERT INTO products (id, name, base_price, year, purchase_unit, consumption_unit, conversion_factor, tva_classification, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                &product.id,
                &product.name,
                base_price_scaled,
                &product.year,
                config.purchase_unit,
                config.consumption_unit,
                config.conversion_factor,
                config.tva_classification,
                now
            ],
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

    /// Insert or update a product from a sync record (SEC-087 Phase 6B).
    ///
    /// Explicit `ON CONFLICT(id) DO UPDATE` — NEVER `INSERT OR REPLACE`.
    /// REPLACE is DELETE-then-INSERT, which would cascade-delete
    /// `inventory_stocks` rows (`ON DELETE CASCADE`) and be blocked by the
    /// `product_tax_classifications` RESTRICT FK, silently destroying or
    /// resetting inventory identity. `created_at` is local insert history
    /// (WILAYA-authoritative id) and is preserved on conflict. The update list
    /// is explicit and covers every WILAYA-authoritative Product field;
    /// no local-only column is touched.
    pub fn upsert_product_sync(
        &self,
        record: &crate::models::ProductSyncRecord,
    ) -> Result<(), AppError> {
        let base_price_scaled = numeric_row::money_scaled(record.base_price)?;
        self.executor.execute(
            "INSERT INTO products (id, name, base_price, year, purchase_unit, consumption_unit, conversion_factor, tva_classification, created_at, updated_at, node_id, deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                base_price = excluded.base_price,
                year = excluded.year,
                purchase_unit = excluded.purchase_unit,
                consumption_unit = excluded.consumption_unit,
                conversion_factor = excluded.conversion_factor,
                tva_classification = excluded.tva_classification,
                updated_at = excluded.updated_at,
                node_id = excluded.node_id,
                deleted = excluded.deleted",
            rusqlite::params![
                &record.id,
                &record.name,
                base_price_scaled,
                &record.year,
                &record.purchase_unit,
                &record.consumption_unit,
                &record.conversion_factor,
                &record.tva_classification,
                &record.created_at,
                &record.updated_at,
                &record.node_id,
                &record.deleted
            ],
        )?;
        Ok(())
    }

    /// Update an existing product
    pub fn update_product(&self, req: &UpdateProductRequest) -> Result<(), AppError> {
        let base_price_scaled = numeric_row::money_scaled(req.base_price)?;
        self.executor.execute(
            "UPDATE products SET name = ?1, base_price = ?2 WHERE id = ?3",
            params![&req.name, base_price_scaled, &req.id],
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
        let result = self.executor.query_row_optional(
            "SELECT id, name, base_price, year, created_at FROM products WHERE id = ?1",
            [product_id],
            |row| {
                let created_at_str: String = row.get(4)?;
                let created_at =
                    crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(Product {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    base_price: numeric_row::money_col(2, row.get::<_, i64>(2)?)?,
                    year: row.get(3)?,
                    created_at,
                })
            },
        )?;
        Ok(result)
    }

    /// List all products (across all years)
    pub fn list_products(&self) -> Result<Vec<Product>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, name, base_price, year, created_at FROM products ORDER BY year DESC, name",
            [],
            |row| {
                let created_at_str: String = row.get(4)?;
                let created_at =
                    crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(Product {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    base_price: numeric_row::money_col(2, row.get::<_, i64>(2)?)?,
                    year: row.get(3)?,
                    created_at,
                })
            },
        )?)
    }

    /// Read a single product via the Phase 6D local read projection
    /// [`crate::models::ProductRead`] (SQL only; the SEC-087 config columns are
    /// NOT NULL in the canonical schema).
    ///
    /// LOCAL-ONLY: feeds the `get_product` IPC command. It is NOT used by the
    /// V3 sync exporter, which keeps consuming the config-free
    /// [`crate::models::Product`] via `list_products()`.
    pub fn get_product_with_config(
        &self,
        product_id: &str,
    ) -> Result<Option<crate::models::ProductRead>, AppError> {
        Ok(self.executor.query_row_optional(
            "SELECT id, name, base_price, year, created_at, purchase_unit, consumption_unit, conversion_factor, tva_classification
             FROM products WHERE id = ?1",
            [product_id],
            |row| {
                let created_at_str: String = row.get(4)?;
                let created_at =
                    crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(crate::models::ProductRead {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    base_price: numeric_row::money_col(2, row.get::<_, i64>(2)?)?,
                    year: row.get(3)?,
                    created_at,
                    purchase_unit: row.get(5)?,
                    consumption_unit: row.get(6)?,
                    conversion_factor: row.get(7)?,
                    tva_classification: row.get(8)?,
                    tva_rate: numeric_row::tva_rate_col(8, row.get::<_, i32>(8)?)?,
                })
            },
        )?)
    }

    /// List all products via the Phase 6D local read projection
    /// [`crate::models::ProductRead`] (SQL only). Same ordering contract as
    /// `list_products()`: `year DESC, name`.
    ///
    /// LOCAL-ONLY: feeds the `list_products` IPC command. It is NOT used by the
    /// V3 sync exporter or any persistence/domain/FIFO/order path.
    pub fn list_products_with_config(&self) -> Result<Vec<crate::models::ProductRead>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, name, base_price, year, created_at, purchase_unit, consumption_unit, conversion_factor, tva_classification
             FROM products ORDER BY year DESC, name",
            [],
            |row| {
                let created_at_str: String = row.get(4)?;
                let created_at =
                    crate::errors::parse_datetime_rfc3339(&created_at_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(crate::models::ProductRead {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    base_price: numeric_row::money_col(2, row.get::<_, i64>(2)?)?,
                    year: row.get(3)?,
                    created_at,
                    purchase_unit: row.get(5)?,
                    consumption_unit: row.get(6)?,
                    conversion_factor: row.get(7)?,
                    tva_classification: row.get(8)?,
                    tva_rate: numeric_row::tva_rate_col(8, row.get::<_, i32>(8)?)?,
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
