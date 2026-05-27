# FIFO & Fiscal Year Behavior

## Overview

FIFO stock layers and fiscal years are tightly coupled. This document explains how fiscal-year boundaries affect inventory accounting, report filtering, and data retention.

## Core Principle: Reclassification, Not Deletion

When a fiscal year is closed, **no historical records are deleted or modified**. The single mutation is a **reclassification** of the `source_type` column on `fifo_stock_layers`:

| Before close | After close | Meaning |
|---|---|---|
| `ORDER` (quantity remaining > 0) | `OPENING` | Carried forward as opening balance |
| `ORDER` (quantity remaining = 0) | Unchanged | Fully consumed — nothing to carry |

This means:
- The same FIFO layer record persists across year boundaries.
- `origin_fiscal_year` preserves the year in which the layer was originally created.
- `received_at` is never altered.
- No data is moved, copied, or deleted.

This is implemented in `FiscalYearService::close_year()`.

## Fiscal Year Scoping for Reports

### Stock Summary (`get_stock_summary`)

For UNIT nodes, the open fiscal year is automatically resolved from `FiscalYearStatusRepository::get_open_year()`. The `WHERE` clause on `stock_movements.fiscal_year` scopes only the **movement aggregates** (`total_in`, `total_out`, `movement_count`). The `current_quantity` field always reflects the **real inventory stock** (unfiltered).

### Daily Reports (`list_daily_reports`)

The `fiscal_year` and optional `month` filters take priority over `start_date`/`end_date` when supplied. Month filtering uses `CAST(strftime('%m', date) AS INTEGER)` since there is no dedicated `month` column on `daily_reports`. Invalid month values (outside 1–12) are rejected before SQL execution.

### Monthly Summary (`get_monthly_summary`)

When `month` is `None` (or 0 in the frontend), the summary covers the full calendar year (Jan 1 – Dec 31) via `fiscal_year_window()`.

## Data Model

- `stock_movements.fiscal_year` — populated at insert time from `input.date.year()` (always set)
- `daily_reports.fiscal_year` — populated at insert time from `input.date.year()` (always set)
- `fifo_stock_layers.origin_fiscal_year` — the year in which the layer was created (immutable)
- `fifo_stock_layers.source_type` — current accounting classification (`ORDER` or `OPENING`)
- `settings.current_year` — the open fiscal year, updated atomically during `close_year()`

## Year List

`ReportRepository::list_available_fiscal_years()` returns distinct years from both `daily_reports.fiscal_year` and `fiscal_year_status.year`, sorted descending. This ensures all years with data or status entries are available for filter dropdowns.

## Key Invariants

1. `current_quantity` is never filtered by fiscal year — it always represents real stock.
2. Wilaya nodes always see unfiltered data (no fiscal-year scoping).
3. Unit nodes default to the open fiscal year for stock summary; no filter = no date filter for reports.
4. Fiscal-year and month filters override `start_date`/`end_date` when both are supplied.
5. Month must be in 1..=12 or the request is rejected with `ValidationError::OutOfRange`.
6. Fiscal-year close does not delete or duplicate any records — only reclassifies `source_type`.
