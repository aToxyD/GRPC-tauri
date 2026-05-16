use crate::errors::{AppError, ValidationError};
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

        repo.update_product(req)?;

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

    pub fn list_products(&self, year: i32) -> Result<Vec<crate::models::Product>, AppError> {
        self.executor.products().list_products(year)
    }

    pub fn list_all_products(&self) -> Result<Vec<crate::models::Product>, AppError> {
        self.executor.products().list_all_products()
    }
}
