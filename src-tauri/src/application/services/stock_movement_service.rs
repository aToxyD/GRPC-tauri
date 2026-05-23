//! Stock Movement Service Module
//!
//! Handles inserting and retrieving stock movements.

use crate::errors::AppError;
use crate::models::{
    NewStockMovement, StockMovement, StockMovementFilters, StockMovementResponse, StockMovementType,
};
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::Datelike;

/// Service for handling stock movements
pub struct StockMovementService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> StockMovementService<'a> {
    /// Create a new StockMovementService
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// تسجيل حركة مخزون جديدة
    pub fn record_stock_movement(&self, movement: &NewStockMovement) -> Result<String, AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        let fiscal_year = chrono::Utc::now().year();

        // 1. Fiscal Guard
        crate::application::services::FiscalYearService::new(self.executor)
            .assert_fiscal_year_open(fiscal_year)?;

        let inventory_repo = self.executor.inventory();
        let balance_before = match inventory_repo.get_stock(&movement.product_id)? {
            Some(s) => s.quantity,
            None => 0.0,
        };
        let balance_after = match movement.movement_type {
            StockMovementType::In | StockMovementType::Opening => {
                balance_before + movement.quantity
            }
            StockMovementType::Out => (balance_before - movement.quantity).max(0.0),
        };

        let id = uuid::Uuid::new_v4().to_string();
        self.executor.stock_movements().insert_stock_movement(
            &id,
            movement,
            balance_before,
            balance_after,
            &now,
            fiscal_year,
        )?;
        inventory_repo.update_stock(&movement.product_id, balance_after)?;
        Ok(id)
    }

    pub fn import_stock_movement(&self, movement: &StockMovement) -> Result<(), AppError> {
        // 1. Fiscal Guard (using movement's timestamp to determine fiscal year)
        let dt = crate::errors::parse_datetime_rfc3339(&movement.timestamp)?;
        // FALLBACK: movement lacks explicit fiscal_year (legacy sync path) — infer from timestamp.
        let fiscal_year = dt.year();
        log::warn!(
            target: "grpc::fiscal",
            "FISCAL FALLBACK: raw stock movement fiscal_year inferred from timestamp year={}",
            fiscal_year
        );
        crate::application::services::FiscalYearService::new(self.executor)
            .assert_fiscal_year_open(fiscal_year)?;

        let product_repo = self.executor.products();
        if !product_repo.product_exists(&movement.product_id)? {
            return Ok(()); // Skip movement if product doesn't exist
        }
        self.executor
            .stock_movements()
            .insert_raw_stock_movement(movement)
    }

    pub fn get_stock_movements(
        &self,
        filters: &StockMovementFilters,
        page: usize,
        page_size: usize,
    ) -> Result<StockMovementResponse, AppError> {
        let limit = page_size as i64;
        let offset = (page * page_size) as i64;

        let q = crate::models::StockMovementQuery {
            product_id: filters.product_id.clone(),
            movement_type: filters.movement_type.clone(),
            start_timestamp: filters
                .start_date
                .as_ref()
                .map(|d| format!("{d}T00:00:00Z")),
            end_timestamp: filters.end_date.as_ref().map(|d| format!("{d}T23:59:59Z")),
            reference_type: filters.reference_type.clone(),
            reference_id: filters.reference_id.clone(),
            unit_id: filters.unit_id.clone(),
            limit,
            offset,
        };

        let repo = self.executor.stock_movements();
        let total_count = repo.count_stock_movements(&q)?;
        let rows = repo.fetch_stock_movements(&q)?;
        let movements = rows
            .into_iter()
            .map(db_row_to_stock_movement)
            .collect::<Result<Vec<_>, _>>()?;

        let has_more = (offset + movements.len() as i64) < total_count;
        Ok(StockMovementResponse {
            movements,
            total_count,
            page,
            page_size,
            has_more,
        })
    }

    pub fn get_stock_movements_by_unit(
        &self,
        unit_id: &str,
        page_size: usize,
        page: usize,
    ) -> Result<StockMovementResponse, AppError> {
        let limit = page_size as i64;
        let offset = (page * page_size) as i64;
        let q = crate::models::StockMovementQuery {
            unit_id: Some(unit_id.to_string()),
            limit,
            offset,
            ..Default::default()
        };

        let repo = self.executor.stock_movements();
        let total_count = repo.count_stock_movements(&q)?;
        let rows = repo.fetch_stock_movements(&q)?;
        let movements = rows
            .into_iter()
            .map(db_row_to_stock_movement)
            .collect::<Result<Vec<_>, _>>()?;

        let has_more = (offset + movements.len() as i64) < total_count;
        Ok(StockMovementResponse {
            movements,
            total_count,
            page,
            page_size,
            has_more,
        })
    }

    pub fn get_stock_movements_in_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<crate::models::StockMovement>, AppError> {
        let rows = self
            .executor
            .stock_movements()
            .get_stock_movements_in_range(start_date, end_date)?;
        rows.into_iter()
            .map(db_row_to_stock_movement)
            .collect::<Result<Vec<_>, _>>()
    }
}

pub fn db_row_to_stock_movement(
    r: crate::models::StockMovementDbRow,
) -> Result<crate::models::StockMovement, AppError> {
    let movement_type =
        crate::models::StockMovementType::parse(&r.movement_type).ok_or_else(|| {
            AppError::Internal(format!("Unknown movement_type '{}'", r.movement_type))
        })?;

    Ok(crate::models::StockMovement {
        id: r.id,
        product_id: r.product_id,
        product_name: r.product_name,
        movement_type,
        quantity: r.quantity,
        balance_before: r.balance_before,
        balance_after: r.balance_after,
        reference_type: r.reference_type,
        reference_id: r.reference_id,
        notes: r.notes,
        timestamp: r.timestamp,
        user_id: r.user_id,
        username: r.username,
        unit_id: r.unit_id,
        fiscal_year: r.fiscal_year,
        unit_cost: r.unit_cost,
    })
}
