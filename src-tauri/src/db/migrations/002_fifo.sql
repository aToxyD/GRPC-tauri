-- FIFO Stock Layers Table
CREATE TABLE fifo_stock_layers (
    id TEXT PRIMARY KEY,
    unit_id TEXT NOT NULL
        REFERENCES units(id) ON DELETE RESTRICT,
    product_id TEXT NOT NULL
        REFERENCES products(id) ON DELETE RESTRICT,
    source_type TEXT NOT NULL CHECK(source_type IN ('ORDER')),
    source_id TEXT,
    unit_cost REAL NOT NULL CHECK(unit_cost >= 0),
    qty_original REAL NOT NULL CHECK(qty_original > 0),
    qty_remaining REAL NOT NULL CHECK(qty_remaining >= 0),
    received_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    CHECK(qty_remaining <= qty_original)
);

CREATE INDEX idx_fifo_active
ON fifo_stock_layers(unit_id, product_id, received_at ASC, id ASC)
WHERE qty_remaining > 0;

CREATE INDEX idx_fifo_source
ON fifo_stock_layers(source_type, source_id);

-- Inventory Layer Consumptions Table
CREATE TABLE inventory_layer_consumptions (
    id TEXT PRIMARY KEY,
    unit_id TEXT NOT NULL
        REFERENCES units(id) ON DELETE RESTRICT,
    movement_id TEXT NOT NULL
        REFERENCES stock_movements(id),
    layer_id TEXT NOT NULL
        REFERENCES fifo_stock_layers(id),
    quantity REAL NOT NULL CHECK(quantity > 0),
    unit_cost REAL NOT NULL CHECK(unit_cost >= 0),
    total_cost REAL NOT NULL CHECK(total_cost >= 0),
    consumed_at TEXT NOT NULL
);

CREATE INDEX idx_layer_consumptions_layer
ON inventory_layer_consumptions(layer_id);

CREATE INDEX idx_layer_consumptions_movement
ON inventory_layer_consumptions(movement_id);

-- Extend Meal Items with Layer Link
ALTER TABLE daily_report_meal_items ADD COLUMN fifo_layer_id TEXT;

-- Reference Prices published by WILAYA (separate from FIFO unit_cost)
CREATE TABLE reference_price_snapshots (
    id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL
        REFERENCES products(id) ON DELETE RESTRICT,
    fiscal_year INTEGER NOT NULL,
    reference_price REAL NOT NULL CHECK(reference_price >= 0),
    approved_by TEXT NOT NULL,
    approved_at TEXT NOT NULL,
    UNIQUE(product_id, fiscal_year)
);

CREATE INDEX idx_ref_price_year
ON reference_price_snapshots(fiscal_year);
