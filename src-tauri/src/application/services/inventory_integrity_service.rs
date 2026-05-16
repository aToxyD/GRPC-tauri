use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
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
        let rows = self.executor.query_all(
            r#"SELECT s.product_id,
      COALESCE(obs.opening_quantity,0),
      COALESCE(SUM(CASE WHEN sm.movement_type IN ('IN','OPENING') THEN sm.quantity ELSE 0 END),0),
      COALESCE(SUM(CASE WHEN sm.movement_type='OUT' THEN sm.quantity ELSE 0 END),0),
      COALESCE(s.quantity,0)
      FROM inventory_stocks s
      LEFT JOIN opening_balance_snapshots obs ON obs.product_id=s.product_id AND obs.fiscal_year=?1
      LEFT JOIN stock_movements sm ON sm.product_id=s.product_id AND sm.fiscal_year=?1
      GROUP BY s.product_id,s.quantity,obs.opening_quantity"#,
            [fiscal_year],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, f64>(2)?,
                    r.get::<_, f64>(3)?,
                    r.get::<_, f64>(4)?,
                ))
            },
        )?;
        let mut issues = Vec::new();
        for (product_id, opening, inbound, outbound, actual) in rows.iter() {
            let expected = opening + inbound - outbound;
            let delta = expected - actual;
            if delta.abs() > 0.0001 {
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
        let years = self.executor.query_all(
            "SELECT year FROM fiscal_year_status ORDER BY year",
            [],
            |r| r.get::<_, i32>(0),
        )?;
        years
            .into_iter()
            .map(|y| self.verify_inventory_consistency(y))
            .collect()
    }
}
