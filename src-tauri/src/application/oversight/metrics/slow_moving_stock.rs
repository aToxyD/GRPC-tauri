use crate::application::oversight::context::ReportsContext;
use crate::application::oversight::types::{Dimension, MetricValue, OversightMetric, UnitType};
use crate::application::reporting::stock_movement_ledger::StockMovementLedgerInput;

/// SlowMovingStock — products with zero OUT movements in the last N days.
///
/// N is provided via the context's fiscal year (full year = 365 days).
/// Products are identified as those in inventory but with no OUT movements
/// in the specified period.
pub struct SlowMovingStock;

impl OversightMetric for SlowMovingStock {
    fn id(&self) -> &'static str {
        "slow-moving-stock"
    }
    fn name(&self) -> &'static str {
        "Slow Moving Stock"
    }
    fn formula(&self) -> &'static str {
        "products in inventory without OUT movements in last N days"
    }
    fn unit(&self) -> UnitType {
        UnitType::Count
    }
    fn dimension(&self) -> Dimension {
        Dimension::Product
    }

    fn compute(&self, ctx: &ReportsContext<'_>) -> Result<MetricValue, String> {
        let valuation = ctx
            .inventory_valuation(None, None)
            .map_err(|e| e.to_string())?;

        // Get all distinct product_ids from inventory
        let inventory_ids: std::collections::HashSet<String> = valuation
            .products
            .iter()
            .map(|p| p.product_id.clone())
            .collect();

        if inventory_ids.is_empty() {
            return Ok(MetricValue::Count(0));
        }

        // Get all OUT movements within the fiscal year
        let ledger = ctx
            .stock_movement_ledger(StockMovementLedgerInput {
                fiscal_year: Some(ctx.fiscal_year()),
                movement_type: Some("OUT".to_string()),
                product_id: None,
                unit_id: None,
                start_timestamp: None,
                end_timestamp: None,
                limit: 0,
                offset: 0,
            })
            .map_err(|e| e.to_string())?;

        // Collect product_ids that HAVE had OUT movements
        let active_ids: std::collections::HashSet<String> =
            ledger.rows.iter().map(|r| r.product_id.clone()).collect();

        // Slow-moving = in inventory but no OUT movements
        let slow_count = inventory_ids.difference(&active_ids).count();
        Ok(MetricValue::Count(slow_count as i64))
    }
}
