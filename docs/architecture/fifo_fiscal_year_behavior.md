# FIFO Layer Fiscal-Year Behaviour

## Decision

Remaining inventory is not copied into new FIFO layers during fiscal-year close.

Instead, the existing layer is retained and reclassified:

```
ORDER -> OPENING
```

## Rationale

- Avoid duplicate FIFO layers across multiple year closes.
- Preserve FIFO ordering (same `received_at`, same `id`).
- Preserve original `received_at` date for chronological accuracy.
- Preserve consumption history and cost basis.
- Simplify the inventory model — one physical layer, one truth.
- No display hacks needed: the database itself stores the correct accounting state.

## What changes at year close

```sql
UPDATE fifo_stock_layers
SET source_type = 'OPENING'
WHERE source_type = 'ORDER' AND qty_remaining > 0;
```

## What stays the same

| Field | Behaviour |
|---|---|
| `id` | unchanged |
| `unit_id` | unchanged |
| `product_id` | unchanged |
| `unit_cost` | unchanged |
| `qty_remaining` | unchanged (only decreased by FIFO consumption) |
| `received_at` | unchanged |
| `origin_fiscal_year` | unchanged — preserves the original entry year |

## Consequence

`source_type` represents the **current accounting classification** of the layer, not necessarily its historical origin.

The historical origin is preserved in `origin_fiscal_year`.

## Audit trail

A complete chain is maintained:

- `origin_fiscal_year` → tells you when the stock was originally booked
- `source_type = 'OPENING'` → tells you it is now opening balance
- `opening_balance_snapshots` → aggregated accounting summaries per product per year
- `inventory_layer_consumptions` → per-movement consumption records

## Related files

- `src-tauri/src/models/fifo.rs` — `FifoStockLayer.source_type` documentation
- `src-tauri/src/repositories/fifo_layers.rs` — `reclassify_active_order_to_opening()`
- `src-tauri/src/application/services/fiscal_year_service.rs` — `close_year()` reclassify step
- `src-tauri/src/db/migrations/002_origin_fiscal_year.sql` — schema migration
