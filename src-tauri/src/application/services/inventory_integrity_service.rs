use crate::domain::numeric::legacy_float;
use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use crate::repositories::RepositoryProvider;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InventoryIntegrityIssue {
    pub product_id: String,
    pub fiscal_year: i32,
    pub expected_quantity: f64,
    pub actual_quantity: f64,
    pub delta: f64,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InventoryIntegrityReport {
    pub fiscal_year: i32,
    pub checked_products: usize,
    pub mismatch_count: usize,
    pub issues: Vec<InventoryIntegrityIssue>,
}

pub struct InventoryIntegrityService<'a> {
    executor: DbExecutor<'a>,
}
impl<'a> InventoryIntegrityService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }
    pub fn verify_inventory_consistency(
        &self,
        fiscal_year: i32,
    ) -> Result<InventoryIntegrityReport, AppError> {
        log::info!(target:"grpc::inventory","[INVENTORY_VERIFY_START] fiscal_year={}",fiscal_year);
        let rows = self
            .executor
            .inventory()
            .verify_consistency_for_year(fiscal_year)?;
        let mut issues = Vec::new();
        for (product_id, opening, inbound, outbound, actual) in rows.iter() {
            // ADR-0048 (Target C): expected = opening + IN − OUT is Quantity
            // reconciliation. The delta is genuinely signed — the ledger floors
            // balance at zero, so OUT may exceed opening + IN — and is carried
            // in scale-3 scaled units, mirroring the snapshot service.
            let opening_scaled = legacy_float::quantity_from_f64(*opening)?.to_scaled_i64()?;
            let inbound_scaled = legacy_float::quantity_from_f64(*inbound)?.to_scaled_i64()?;
            let outbound_scaled = legacy_float::quantity_from_f64(*outbound)?.to_scaled_i64()?;
            let expected_scaled = opening_scaled
                .checked_add(inbound_scaled)
                .and_then(|v| v.checked_sub(outbound_scaled))
                .ok_or_else(|| {
                    AppError::Internal(format!(
                        "Inventory reconciliation overflow for product {}",
                        product_id
                    ))
                })?;
            let actual_scaled = legacy_float::quantity_from_f64(*actual)?.to_scaled_i64()?;
            let delta_scaled = expected_scaled.checked_sub(actual_scaled).ok_or_else(|| {
                AppError::Internal(format!(
                    "Inventory reconciliation overflow for product {}",
                    product_id
                ))
            })?;
            // Exact comparison: a drift of at least one scale-3 unit (0.001)
            // is a genuine mismatch. The legacy 0.0001 tolerance (roughly
            // one-tenth of a unit) could only mask sub-unit float residue;
            // exact decimals remove it deterministically.
            if delta_scaled.abs() > 0 {
                // Signed wire boundary: Quantity forbids negatives, so the
                // report fields are materialized as scaled units at exact
                // scale-3 precision (same formula as quantity_to_f64).
                let expected = expected_scaled as f64 / 1000.0;
                let delta = delta_scaled as f64 / 1000.0;
                log::warn!(target:"grpc::inventory","[INVENTORY_VERIFY_PRODUCT_MISMATCH] product_id={} fiscal_year={} expected={} actual={} delta={}",product_id,fiscal_year,expected,actual,delta);
                issues.push(InventoryIntegrityIssue {
                    product_id: product_id.clone(),
                    fiscal_year,
                    expected_quantity: expected,
                    actual_quantity: *actual,
                    delta,
                });
            }
        }
        log::info!(target:"grpc::inventory","[INVENTORY_VERIFY_SUCCESS] fiscal_year={} checked_products={} mismatches={}",fiscal_year,rows.len(),issues.len());
        Ok(InventoryIntegrityReport {
            fiscal_year,
            checked_products: rows.len(),
            mismatch_count: issues.len(),
            issues,
        })
    }
    pub fn verify_inventory_consistency_all_years(
        &self,
    ) -> Result<Vec<InventoryIntegrityReport>, AppError> {
        let years = self.executor.fiscal_year_status().get_all_years()?;
        years
            .into_iter()
            .map(|y| self.verify_inventory_consistency(y))
            .collect()
    }
}
