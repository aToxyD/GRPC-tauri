use crate::domain::fifo_engine::{
    simulate_fifo_consumption, simulate_fifo_consumption_typed, FifoLayerRow, TypedConsumedPortion,
    TypedFifoLayer,
};
use crate::domain::numeric::{legacy_float, Money, Quantity};
use crate::errors::{AppError, AppResult};
use crate::models::{ConsumedLayerPortion, FifoStockLayer, InventoryLayerConsumption};
use crate::repositories::executor::DbExecutor;
use crate::repositories::numeric_row;
use rusqlite::params;
use uuid::Uuid;

/// Internal row returned by get_inventory_fifo_view query.
pub(crate) struct InventoryFifoLayerRow {
    pub layer_id: String,
    pub product_id: String,
    pub product_name: String,
    pub source_type: String,
    pub received_at: String,
    pub qty_remaining: f64,
    pub unit_cost: f64,
}

pub struct FifoLayerRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FifoLayerRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_layer(
        &self,
        unit_id: &str,
        product_id: &str,
        source_type: &str,
        source_id: Option<&str>,
        unit_cost: f64,
        qty_original: f64,
        received_at: &str,
        created_by: &str,
        origin_fiscal_year: i32,
    ) -> AppResult<String> {
        let id = Uuid::new_v4().to_string();

        let unit_cost_scaled = numeric_row::money_scaled(unit_cost)?;
        let qty_original_scaled = numeric_row::qty_scaled(qty_original)?;

        self.executor
            .execute(
                r#"
            INSERT INTO fifo_stock_layers (
                id, unit_id, product_id, source_type, source_id,
                unit_cost, qty_original, qty_remaining, received_at, created_by,
                origin_fiscal_year
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
                params![
                    id,
                    unit_id,
                    product_id,
                    source_type,
                    source_id,
                    unit_cost_scaled,
                    qty_original_scaled,
                    qty_original_scaled, // qty_remaining starts equal to qty_original
                    received_at,
                    created_by,
                    origin_fiscal_year
                ],
            )
            .map_err(AppError::from)?;

        Ok(id)
    }

    /// Reclassify all remaining ORDER stock as OPENING (called at year close).
    /// Returns the number of layers reclassified.
    pub fn reclassify_active_order_to_opening(&self) -> AppResult<usize> {
        let affected = self
            .executor
            .execute(
                "UPDATE fifo_stock_layers
                 SET source_type = 'OPENING'
                 WHERE source_type = 'ORDER' AND qty_remaining > 0",
                [],
            )
            .map_err(AppError::from)?;
        Ok(affected)
    }

    /// Consume `quantity` units from the oldest available FIFO layers for a given
    /// unit+product pair. Returns the list of layer portions consumed.
    ///
    /// Wire facade (ADR-0048): converts at the boundary and delegates to the
    /// exact typed path.
    pub fn consume_fifo(
        &self,
        unit_id: &str,
        product_id: &str,
        quantity: f64,
    ) -> AppResult<Vec<ConsumedLayerPortion>> {
        let requested = legacy_float::quantity_from_f64(quantity)?;
        let consumed = self.consume_fifo_typed(unit_id, product_id, requested)?;
        consumed
            .into_iter()
            .map(|p| {
                Ok(ConsumedLayerPortion {
                    layer_id: p.layer_id,
                    quantity: legacy_float::quantity_to_f64(&p.quantity)?,
                    unit_cost: legacy_float::money_to_f64(&p.unit_cost)?,
                    total_cost: legacy_float::money_to_f64(&p.total_cost)?,
                })
            })
            .collect()
    }

    /// Exact-typed consume path (ADR-0048): the row mapping converts REAL `f64`
    /// columns to exact `Money`/`Quantity`, the engine computes exact portions,
    /// and quantity is written back via the single boundary conversion. No
    /// float arithmetic exists in the decision/consumption path.
    pub fn consume_fifo_typed(
        &self,
        unit_id: &str,
        product_id: &str,
        quantity: Quantity,
    ) -> AppResult<Vec<TypedConsumedPortion>> {
        let rows = self
            .executor
            .query_all(
                r#"
            SELECT id, unit_cost, qty_remaining
            FROM fifo_stock_layers
            WHERE unit_id = ?1 AND product_id = ?2 AND qty_remaining > 0
            ORDER BY received_at ASC, id ASC
            "#,
                params![unit_id, product_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_err(AppError::from)?;

        let layers = rows
            .into_iter()
            .map(|(id, cost_scaled, qty_scaled)| {
                Ok(TypedFifoLayer {
                    layer_id: id,
                    unit_cost: Money::from_centimes(cost_scaled)?,
                    qty_remaining: Quantity::from_scaled_i64(qty_scaled)?,
                })
            })
            .collect::<AppResult<Vec<TypedFifoLayer>>>()?;

        let consumed_portions = simulate_fifo_consumption_typed(product_id, &layers, quantity)?;

        for portion in &consumed_portions {
            self.executor
                .execute(
                    r#"
                UPDATE fifo_stock_layers
                SET qty_remaining = qty_remaining - ?1
                WHERE id = ?2
                "#,
                    params![portion.quantity.to_scaled_i64()?, portion.layer_id],
                )
                .map_err(AppError::from)?;
        }

        Ok(consumed_portions)
    }

    /// Insert a persisted layer-consumption ledger row.
    ///
    /// ADR-0048: `total_cost` is a caller-computed boundary value so that no
    /// money arithmetic ever lives in the repository (SQL is row mapping only).
    #[allow(clippy::too_many_arguments)]
    pub fn create_consumption_record(
        &self,
        unit_id: &str,
        movement_id: &str,
        layer_id: &str,
        quantity: f64,
        unit_cost: f64,
        total_cost: f64,
        consumed_at: &str,
    ) -> AppResult<()> {
        let id = Uuid::new_v4().to_string();

        let quantity_scaled = numeric_row::qty_scaled(quantity)?;
        let unit_cost_scaled = numeric_row::money_scaled(unit_cost)?;
        let total_cost_scaled = numeric_row::money_scaled(total_cost)?;

        log::info!(
            target: "grpc::fifo",
            "Creating consumption record: unit_id={}, movement_id={}, layer_id={}",
            unit_id, movement_id, layer_id
        );

        self.executor
            .execute(
                r#"
            INSERT INTO inventory_layer_consumptions (
                id, unit_id, movement_id, layer_id, quantity, unit_cost, total_cost, consumed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
                params![
                    id,
                    unit_id,
                    movement_id,
                    layer_id,
                    quantity_scaled,
                    unit_cost_scaled,
                    total_cost_scaled,
                    consumed_at
                ],
            )
            .map_err(AppError::from)?;

        Ok(())
    }

    /// Returns all layers (active + exhausted) for a unit+product pair.
    pub fn get_layers_for_product(
        &self,
        unit_id: &str,
        product_id: &str,
    ) -> AppResult<Vec<FifoStockLayer>> {
        self.executor
            .query_all(
                r#"
            SELECT id, unit_id, product_id, source_type, source_id,
                   unit_cost, qty_original, qty_remaining, received_at, created_by,
                   origin_fiscal_year
            FROM fifo_stock_layers
            WHERE unit_id = ?1 AND product_id = ?2
            ORDER BY received_at ASC, id ASC
            "#,
                params![unit_id, product_id],
                |row| {
                    Ok(FifoStockLayer {
                        id: row.get(0)?,
                        unit_id: row.get(1)?,
                        product_id: row.get(2)?,
                        source_type: row.get(3)?,
                        source_id: row.get(4)?,
                        unit_cost: numeric_row::money_col(5, row.get::<_, i64>(5)?)?,
                        qty_original: numeric_row::qty_col(6, row.get::<_, i64>(6)?)?,
                        qty_remaining: numeric_row::qty_col(7, row.get::<_, i64>(7)?)?,
                        received_at: row.get(8)?,
                        created_by: row.get(9)?,
                        origin_fiscal_year: row.get(10)?,
                    })
                },
            )
            .map_err(AppError::from)
    }

    /// Returns the total remaining quantity for a unit+product pair.
    pub fn get_total_available(&self, unit_id: &str, product_id: &str) -> AppResult<f64> {
        let total: i64 = self
            .executor
            .query_row(
                r#"
            SELECT COALESCE(SUM(qty_remaining), 0)
            FROM fifo_stock_layers
            WHERE unit_id = ?1 AND product_id = ?2 AND qty_remaining > 0
            "#,
                params![unit_id, product_id],
                |row| row.get(0),
            )
            .map_err(AppError::from)?;
        Ok(numeric_row::qty_scaled_i64_to_f64(total)?)
    }

    /// Returns the global total remaining quantity and total value for a specific product across all units.
    pub fn get_global_quantity_and_value_for_product(
        &self,
        product_id: &str,
    ) -> AppResult<(f64, f64)> {
        let (qty_scaled, value_times_1000): (i64, i64) = self
            .executor
            .query_row(
                r#"
            SELECT COALESCE(SUM(qty_remaining), 0), COALESCE(SUM(qty_remaining * unit_cost), 0)
            FROM fifo_stock_layers
            WHERE product_id = ?1 AND qty_remaining > 0
            "#,
                params![product_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(AppError::from)?;
        Ok((
            numeric_row::qty_scaled_i64_to_f64(qty_scaled)?,
            numeric_row::money_sum_col(value_times_1000)?,
        ))
    }

    /// Fetch active layers for simulation (read-only, ordered FIFO).
    pub fn fetch_active_layers(
        &self,
        unit_id: &str,
        product_id: &str,
    ) -> AppResult<Vec<FifoLayerRow>> {
        let layer_iter = self
            .executor
            .query_all(
                r#"
            SELECT id, unit_cost, qty_remaining
            FROM fifo_stock_layers
            WHERE unit_id = ?1 AND product_id = ?2 AND qty_remaining > 0
            ORDER BY received_at ASC, id ASC
            "#,
                params![unit_id, product_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        numeric_row::money_col(1, row.get::<_, i64>(1)?)?,
                        numeric_row::qty_col(2, row.get::<_, i64>(2)?)?,
                    ))
                },
            )
            .map_err(AppError::from)?;
        Ok(layer_iter)
    }

    /// Simulate FIFO consumption without mutating layers (preview / dry-run).
    pub fn preview_consume_fifo(
        &self,
        unit_id: &str,
        product_id: &str,
        quantity: f64,
    ) -> AppResult<Vec<ConsumedLayerPortion>> {
        let layers = self.fetch_active_layers(unit_id, product_id)?;
        simulate_fifo_consumption(product_id, &layers, quantity)
    }

    /// Returns the FIFO inventory value for a unit (remaining qty * unit cost).
    pub fn get_inventory_value_fifo(&self, unit_id: &str) -> AppResult<f64> {
        let value_times_1000: i64 = self
            .executor
            .query_row(
                r#"
            SELECT COALESCE(SUM(qty_remaining * unit_cost), 0)
            FROM fifo_stock_layers
            WHERE unit_id = ?1 AND qty_remaining > 0
            "#,
                params![unit_id],
                |row| row.get(0),
            )
            .map_err(AppError::from)?;
        Ok(numeric_row::money_sum_col(value_times_1000)?)
    }

    /// Returns only the active (non-exhausted) layers for a unit+product pair.
    pub fn get_remaining_layers(
        &self,
        unit_id: &str,
        product_id: &str,
    ) -> AppResult<Vec<FifoStockLayer>> {
        self.executor
            .query_all(
                r#"
            SELECT id, unit_id, product_id, source_type, source_id,
                   unit_cost, qty_original, qty_remaining, received_at, created_by,
                   origin_fiscal_year
            FROM fifo_stock_layers
            WHERE unit_id = ?1 AND product_id = ?2 AND qty_remaining > 0
            ORDER BY received_at ASC, id ASC
            "#,
                params![unit_id, product_id],
                |row| {
                    Ok(FifoStockLayer {
                        id: row.get(0)?,
                        unit_id: row.get(1)?,
                        product_id: row.get(2)?,
                        source_type: row.get(3)?,
                        source_id: row.get(4)?,
                        unit_cost: numeric_row::money_col(5, row.get::<_, i64>(5)?)?,
                        qty_original: numeric_row::qty_col(6, row.get::<_, i64>(6)?)?,
                        qty_remaining: numeric_row::qty_col(7, row.get::<_, i64>(7)?)?,
                        received_at: row.get(8)?,
                        created_by: row.get(9)?,
                        origin_fiscal_year: row.get(10)?,
                    })
                },
            )
            .map_err(AppError::from)
    }

    /// Returns all active FIFO layers grouped by product for the inventory page view.
    pub(crate) fn get_inventory_fifo_view(
        &self,
        unit_id: &str,
    ) -> AppResult<Vec<InventoryFifoLayerRow>> {
        self.executor
            .query_all(
                r#"
            SELECT f.id, f.product_id, p.name, f.source_type,
                   f.received_at, f.qty_remaining, f.unit_cost
            FROM fifo_stock_layers f
            JOIN products p ON f.product_id = p.id
            WHERE f.unit_id = ?1 AND f.qty_remaining > 0
            ORDER BY p.name COLLATE NOCASE, f.received_at ASC, f.id ASC
            "#,
                params![unit_id],
                |row| {
                    Ok(InventoryFifoLayerRow {
                        layer_id: row.get(0)?,
                        product_id: row.get(1)?,
                        product_name: row.get(2)?,
                        source_type: row.get(3)?,
                        received_at: row.get(4)?,
                        qty_remaining: numeric_row::qty_col(5, row.get::<_, i64>(5)?)?,
                        unit_cost: numeric_row::money_col(6, row.get::<_, i64>(6)?)?,
                    })
                },
            )
            .map_err(AppError::from)
    }

    pub fn get_consumption_history(
        &self,
        movement_id: &str,
    ) -> AppResult<Vec<InventoryLayerConsumption>> {
        self.executor
            .query_all(
                r#"
            SELECT id, unit_id, movement_id, layer_id, quantity, unit_cost, total_cost, consumed_at
            FROM inventory_layer_consumptions
            WHERE movement_id = ?1
            ORDER BY consumed_at ASC, id ASC
            "#,
                params![movement_id],
                |row| {
                    Ok(InventoryLayerConsumption {
                        id: row.get(0)?,
                        unit_id: row.get(1)?,
                        movement_id: row.get(2)?,
                        layer_id: row.get(3)?,
                        quantity: numeric_row::qty_col(4, row.get::<_, i64>(4)?)?,
                        unit_cost: numeric_row::money_col(5, row.get::<_, i64>(5)?)?,
                        total_cost: numeric_row::money_col(6, row.get::<_, i64>(6)?)?,
                        consumed_at: row.get(7)?,
                    })
                },
            )
            .map_err(AppError::from)
    }
}
