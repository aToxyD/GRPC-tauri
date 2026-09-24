use crate::errors::{AppError, BusinessLogicError, ValidationError};
use crate::models::{CreateProductRequest, UpdateProductRequest};
use crate::repositories::executor::DbExecutor;
use crate::repositories::RepositoryProvider;
use chrono::Utc;
use uuid::Uuid;

pub struct ProductService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> ProductService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn create_product(
        &self,
        req: &CreateProductRequest,
        year: i32,
    ) -> Result<String, AppError> {
        // Validation: name, price, year, and the SEC-087 unit/TVA/factor set
        // (single authoritative validator in `domain::validation`).
        crate::domain::validation::validate_create_product_request(req, year)?;
        let config = crate::domain::validation::validate_product_units(
            req.purchase_unit,
            req.consumption_unit,
            req.conversion_factor,
            req.tva_classification,
        )?;

        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        let repo = self.executor.products();
        repo.insert_product(&id, req, year, &config, &now)?;

        // Initialize stock record keyed by the product's authoritative
        // consumption-unit identity (SEC-087 Phase 6C). `config` is the validated
        // ProductUnitConfig, so every product gets its keyed row.
        let stock_id = Uuid::new_v4().to_string();
        self.executor.inventory().create_initial_stock_for_product(
            &stock_id,
            &id,
            config.consumption_unit.code(),
            &now,
        )?;

        Ok(id)
    }

    pub fn update_product(&self, req: &UpdateProductRequest) -> Result<(), AppError> {
        let repo = self.executor.products();

        // Get current product to check if price is being changed
        let current_product = repo.get_product(&req.id)?.ok_or_else(|| {
            AppError::Validation(ValidationError::Required {
                field: "product_id".to_string(),
            })
        })?;

        // Check if price is being modified. `base_price` is Money-dimension
        // (`ReferencePrice`); ADR-0048: exact equality on the boundary-scaled
        // values, never an epsilon comparison on floats.
        let current_base_price =
            crate::domain::numeric::legacy_float::money_from_f64(current_product.base_price)?;
        let requested_base_price =
            crate::domain::numeric::legacy_float::money_from_f64(req.base_price)?;
        if current_base_price != requested_base_price {
            // Price change detected - validate against fiscal year locking
            self.validate_price_modification_allowed(&current_product.year)?;
        }

        repo.update_product(req)?;

        Ok(())
    }

    /// Validate that price modification is allowed for the given fiscal year.
    /// Price changes are NOT allowed during an active synchronized fiscal year.
    fn validate_price_modification_allowed(&self, product_year: &i32) -> Result<(), AppError> {
        let fiscal_status = self
            .executor
            .fiscal_year_status()
            .get_by_year(*product_year)?;

        if let Some(status) = fiscal_status {
            // If the fiscal year is open (active), price changes are forbidden
            if status.status == "open" {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::PriceLockedForActiveFiscalYear {
                        fiscal_year: *product_year,
                    },
                ));
            }
        }

        Ok(())
    }

    pub fn delete_product(&self, product_id: &str) -> Result<(), AppError> {
        let repo = self.executor.products();

        repo.delete_product(product_id)?;

        Ok(())
    }

    pub fn get_product(&self, id: &str) -> Result<Option<crate::models::Product>, AppError> {
        self.executor.products().get_product(id)
    }

    pub fn list_products(&self) -> Result<Vec<crate::models::Product>, AppError> {
        self.executor.products().list_products()
    }

    /// Phase 6D local read projection incl. the SEC-087 unit/TVA config
    /// (feeds the `get_product` IPC command). LOCAL-ONLY: never used by sync,
    /// persistence, create/update/delete, FIFO, order, or domain paths.
    pub fn get_product_read(
        &self,
        id: &str,
    ) -> Result<Option<crate::models::ProductRead>, AppError> {
        self.executor.products().get_product_with_config(id)
    }

    /// Phase 6D local read projection incl. the SEC-087 unit/TVA config
    /// (feeds the `list_products` IPC command). LOCAL-ONLY: never used by sync,
    /// persistence, create/update/delete, FIFO, order, or domain paths.
    pub fn list_products_read(&self) -> Result<Vec<crate::models::ProductRead>, AppError> {
        self.executor.products().list_products_with_config()
    }
}
