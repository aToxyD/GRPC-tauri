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
        // Validation
        if req.name.trim().is_empty() {
            return Err(AppError::Validation(ValidationError::Required {
                field: "name".to_string(),
            }));
        }
        if req.base_price <= 0.0 {
            return Err(AppError::Validation(ValidationError::InvalidPrice {
                value: req.base_price,
            }));
        }

        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        let repo = self.executor.products();
        repo.insert_product(&id, req, year, &now)?;

        // Initialize stock record (orchestration belongs to service).
        let stock_id = Uuid::new_v4().to_string();
        self.executor
            .inventory()
            .create_initial_stock_for_product(&stock_id, &id, &now)?;

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

        // Check if price is being modified
        if (current_product.base_price - req.base_price).abs() > f64::EPSILON {
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
}
