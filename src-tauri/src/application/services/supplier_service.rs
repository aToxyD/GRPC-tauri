//! Supplier Service Module
//!
//! Supplier CRUD + UNIT association (ADR-0055 / SEC-087-F). WILAYA-only
//! operations: a supplier is a first-class procurement entity managed at the
//! Wilaya node; UNITs consume suppliers through contracts.
//! SQL is delegated exclusively to `SupplierRepository`.

use crate::domain::validation::{
    validate_create_supplier_request, validate_update_supplier_request,
};
use crate::errors::{AppError, BusinessLogicError};
use crate::models::{CreateSupplierRequest, Supplier, UpdateSupplierRequest};
use crate::repositories::{DbExecutor, RepositoryProvider};

/// Service for supplier business logic
pub struct SupplierService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> SupplierService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Create a supplier (WILAYA only — enforced at the command/authz layer).
    pub fn create_supplier(&self, req: &CreateSupplierRequest) -> Result<Supplier, AppError> {
        validate_create_supplier_request(req)?;
        if self
            .executor
            .suppliers()
            .find_supplier_by_name(&req.name)?
            .is_some()
        {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::ValidationError {
                    field: "name".to_string(),
                    message: "مورد بنفس الاسم موجود مسبقاً".to_string(),
                },
            ));
        }
        let now = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        self.executor.suppliers().insert_supplier(&id, req, &now)?;
        Ok(self
            .executor
            .suppliers()
            .get_supplier(&id)?
            .expect("inserted supplier must exist"))
    }

    /// Update supplier profile. Renaming does NOT rewrite historical
    /// `supplier_orders.supplier_name` snapshots (kept by design).
    pub fn update_supplier(&self, req: &UpdateSupplierRequest) -> Result<Supplier, AppError> {
        validate_update_supplier_request(req)?;
        let n = self.executor.suppliers().update_supplier(req)?;
        if n == 0 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::ResourceNotFound {
                    resource: "مورد".to_string(),
                    id: req.id.clone(),
                },
            ));
        }
        Ok(self
            .executor
            .suppliers()
            .get_supplier(&req.id)?
            .expect("supplier must exist after update"))
    }

    /// Disable or re-enable a supplier (WILAYA only).
    pub fn set_supplier_active(
        &self,
        supplier_id: &str,
        active: bool,
    ) -> Result<Supplier, AppError> {
        let n = self
            .executor
            .suppliers()
            .set_supplier_active(supplier_id, active)?;
        if n == 0 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::ResourceNotFound {
                    resource: "مورد".to_string(),
                    id: supplier_id.to_string(),
                },
            ));
        }
        Ok(self
            .executor
            .suppliers()
            .get_supplier(supplier_id)?
            .expect("supplier must exist after toggle"))
    }

    /// Associate a supplier with a UNIT (WILAYA only). Idempotent.
    pub fn associate_with_unit(&self, unit_id: &str, supplier_id: &str) -> Result<(), AppError> {
        self.executor
            .suppliers()
            .associate_with_unit(unit_id, supplier_id)
    }

    /// Remove a UNIT association (WILAYA only).
    pub fn disassociate_from_unit(&self, unit_id: &str, supplier_id: &str) -> Result<(), AppError> {
        self.executor
            .suppliers()
            .disassociate_from_unit(unit_id, supplier_id)
    }

    pub fn get_supplier(&self, id: &str) -> Result<Option<Supplier>, AppError> {
        self.executor.suppliers().get_supplier(id)
    }

    pub fn list_suppliers(&self) -> Result<Vec<Supplier>, AppError> {
        self.executor.suppliers().list_suppliers()
    }

    pub fn list_active_suppliers(&self) -> Result<Vec<Supplier>, AppError> {
        self.executor.suppliers().list_active_suppliers()
    }

    pub fn list_unit_suppliers(&self, unit_id: &str) -> Result<Vec<Supplier>, AppError> {
        let ids = self.executor.suppliers().list_unit_supplier_ids(unit_id)?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(s) = self.executor.suppliers().get_supplier(&id)? {
                out.push(s);
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }
}

impl crate::architecture::Service for SupplierService<'_> {}
