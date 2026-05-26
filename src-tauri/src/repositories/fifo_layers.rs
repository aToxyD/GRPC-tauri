use crate::domain::fifo_engine::{simulate_fifo_consumption, FifoLayerRow};
use crate::errors::{AppError, AppResult};
use crate::models::{ConsumedLayerPortion, FifoStockLayer, InventoryLayerConsumption};
use crate::repositories::executor::DbExecutor;
use rusqlite::params;
use uuid::Uuid;

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
    ) -> AppResult<String> {
        let id = Uuid::new_v4().to_string();

        self.executor
            .execute(
                r#"
            INSERT INTO fifo_stock_layers (
                id, unit_id, product_id, source_type, source_id,
                unit_cost, qty_original, qty_remaining, received_at, created_by
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
                params![
                    id,
                    unit_id,
                    product_id,
                    source_type,
                    source_id,
                    unit_cost,
                    qty_original,
                    qty_original, // qty_remaining starts equal to qty_original
                    received_at,
                    created_by
                ],
            )
            .map_err(AppError::from)?;

        Ok(id)
    }

    /// Consume `quantity` units from the oldest available FIFO layers for a given
    /// unit+product pair. Returns the list of layer portions consumed.
    pub fn consume_fifo(
        &self,
        unit_id: &str,
        product_id: &str,
        quantity: f64,
    ) -> AppResult<Vec<ConsumedLayerPortion>> {
        // Fetch all active layers ordered by received_at ASC, id ASC (stable FIFO)
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
                        row.get::<_, f64>(1)?,
                        row.get::<_, f64>(2)?,
                    ))
                },
            )
            .map_err(AppError::from)?;

        let available_layers: Vec<FifoLayerRow> = layer_iter;

        let consumed_portions = simulate_fifo_consumption(product_id, &available_layers, quantity)?;

        for portion in &consumed_portions {
            self.executor
                .execute(
                    r#"
                UPDATE fifo_stock_layers
                SET qty_remaining = qty_remaining - ?1
                WHERE id = ?2
                "#,
                    params![portion.quantity, portion.layer_id],
                )
                .map_err(AppError::from)?;
        }

        Ok(consumed_portions)
    }

    pub fn create_consumption_record(
        &self,
        unit_id: &str,
        movement_id: &str,
        layer_id: &str,
        quantity: f64,
        unit_cost: f64,
        consumed_at: &str,
    ) -> AppResult<()> {
        let id = Uuid::new_v4().to_string();
        let total_cost = quantity * unit_cost;

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
                    quantity,
                    unit_cost,
                    total_cost,
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
                   unit_cost, qty_original, qty_remaining, received_at, created_by
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
                        unit_cost: row.get(5)?,
                        qty_original: row.get(6)?,
                        qty_remaining: row.get(7)?,
                        received_at: row.get(8)?,
                        created_by: row.get(9)?,
                    })
                },
            )
            .map_err(AppError::from)
    }

    /// Returns the total remaining quantity for a unit+product pair.
    pub fn get_total_available(&self, unit_id: &str, product_id: &str) -> AppResult<f64> {
        let total: f64 = self
            .executor
            .query_row(
                r#"
            SELECT COALESCE(SUM(qty_remaining), 0.0)
            FROM fifo_stock_layers
            WHERE unit_id = ?1 AND product_id = ?2 AND qty_remaining > 0
            "#,
                params![unit_id, product_id],
                |row| row.get(0),
            )
            .map_err(AppError::from)?;
        Ok(total)
    }

    /// Returns the global total remaining quantity and total value for a specific product across all units.
    pub fn get_global_quantity_and_value_for_product(
        &self,
        product_id: &str,
    ) -> AppResult<(f64, f64)> {
        let result: (f64, f64) = self
            .executor
            .query_row(
                r#"
            SELECT COALESCE(SUM(qty_remaining), 0.0), COALESCE(SUM(qty_remaining * unit_cost), 0.0)
            FROM fifo_stock_layers
            WHERE product_id = ?1 AND qty_remaining > 0
            "#,
                params![product_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(AppError::from)?;
        Ok(result)
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
                        row.get::<_, f64>(1)?,
                        row.get::<_, f64>(2)?,
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
        let total: f64 = self
            .executor
            .query_row(
                r#"
            SELECT COALESCE(SUM(qty_remaining * unit_cost), 0.0)
            FROM fifo_stock_layers
            WHERE unit_id = ?1 AND qty_remaining > 0
            "#,
                params![unit_id],
                |row| row.get(0),
            )
            .map_err(AppError::from)?;
        Ok(total)
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
                   unit_cost, qty_original, qty_remaining, received_at, created_by
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
                        unit_cost: row.get(5)?,
                        qty_original: row.get(6)?,
                        qty_remaining: row.get(7)?,
                        received_at: row.get(8)?,
                        created_by: row.get(9)?,
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
                        quantity: row.get(4)?,
                        unit_cost: row.get(5)?,
                        total_cost: row.get(6)?,
                        consumed_at: row.get(7)?,
                    })
                },
            )
            .map_err(AppError::from)
    }
}
