# Reporting Semantics

## Layer Contract

The `application/reporting/` module is a read-only projection layer, separate from `application/services/`.

| Property | Rule |
|----------|------|
| Mutations | Forbidden — no INSERT/UPDATE/DELETE |
| Transaction ownership | Forbidden — no `with_transaction`, `with_event_context`, `with_event_persistence` |
| Side effects | Forbidden — no calls to mutation methods on repositories |
| Orchestration | Forbidden — no service-to-service orchestration |
| Read access | Permitted — queries via `DbExecutor`, accessing repositories via `RepositoryProvider` |

## Report Trait

```rust
pub trait Report {
    type Input: Serialize;
    type Output: Serialize;
    type Error: std::error::Error;

    fn slug() -> &'static str;       // unique identifier for caching
    fn version() -> u32;              // incremented on semantic change
    fn compute(executor: DbExecutor<'_>, input: Self::Input)
        -> Result<ReportEnvelope<Self::Output>, Self::Error>;
    fn is_reproducible() -> bool;     // default: true
}
```

### Semantics

- `slug()` must be globally unique across all reports. Duplicate slugs will produce cache collisions.
- `version()` starts at 1 and is incremented when the report's computation or output schema changes. Breaking changes to a report's semantics MUST:
  1. Increment the version number
  2. Document the change in an ADR
  3. Note the version change in the changelog
- `compute()` is a pure function with respect to database state: same input + same DB state = deterministic output data (metadata.computed_at may differ).
- `is_reproducible()` returns `false` for any report that depends on wall-clock time, random values, or external state. Default is `true`. Non-reproducible reports must document why.

### ReportEnvelope

Every report returns a `ReportEnvelope<T>`:

```rust
pub struct ReportEnvelope<T: Serialize> {
    pub metadata: ReportMetadata,
    pub data: T,
}
```

The envelope separates **context metadata** (always present) from **report data** (the domain output).

## ReportMetadata

Every report output includes:

| Field | Type | Purpose | Reproducible? |
|-------|------|---------|---------------|
| `report_slug` | `String` | Unique report identifier | Yes |
| `report_version` | `u32` | Version at computation time | Yes |
| `computed_at` | `String` | Unix timestamp (seconds) | No — wall clock |
| `fiscal_scope` | `Option<i32>` | Fiscal year scope, if any | Yes |
| `database_state_hash` | `Option<String>` | Lightweight DB state fingerprint | Yes* |
| `snapshot_source` | `Option<String>` | Table/view name used | Yes |

`database_state_hash` is optional and currently unused; it is reserved for future use where a SHA-256 of relevant table row counts + last-modified timestamps provides a lightweight state fingerprint.

## Report Catalog

### FiscalYearSummaryReport

- **Slug**: `fiscal-year-summary`
- **Version**: 1
- **Reproducible**: Yes
- **Input**: `{ fiscal_year: i32 }`
- **Output**: aggregated fiscal year metrics (movements, consumption value, opening value, daily report count, beneficiaries, ending inventory value, layer count)
- **Snapshot source**: `fifo_stock_layers + stock_movements + daily_reports`

### InventoryValuationReport

- **Slug**: `inventory-valuation`
- **Version**: 1
- **Reproducible**: Yes
- **Input**: `{ fiscal_year: Option<i32>, unit_id: Option<String> }`
- **Output**: total inventory value, product-level breakdown with weighted-average unit cost per product
- **Snapshot source**: `fifo_stock_layers`
- **Ordering**: `ORDER BY p.name COLLATE NOCASE`

### StockMovementLedgerReport

- **Slug**: `stock-movement-ledger`
- **Version**: 1
- **Reproducible**: Yes
- **Input**: `{ fiscal_year, product_id, movement_type, unit_id, start_timestamp, end_timestamp, limit, offset }`
- **Output**: paginated movement rows with total count
- **Ordering**: `ORDER BY sm.timestamp ASC, sm.id ASC` (deterministic tiebreaker)

## Fiscal-Aware Reporting

- All reports accept an optional `fiscal_year` filter.
- When `fiscal_year` is provided, report data is scoped to that year.
- Inventory valuations are optionally fiscal-filtered via `origin_fiscal_year` on FIFO layers.
- The `fiscal_scope` field in metadata reflects the active fiscal filter.

## Cache Identity

Cache keys are deterministic and derived from:

```
SHA256(slug | version | canonical_input_json | fiscal_scope)
```

- Same slug + version + input + fiscal scope = same cache key (byte-identical).
- Cache entries for closed fiscal years are never invalidated (immutable data → immutable report).
- Cache entries for open fiscal years are invalidated via specific DomainEvents.
- No TTL-based invalidation — only semantic invalidation.

## Architectural Enforcement

Architecture rules (defined in `scripts/check_arch.ts`):

| Rule | Check | Severity |
|------|-------|----------|
| 49 | No INSERT/UPDATE/DELETE in reporting/ | error |
| 50 | No transaction ownership in reporting/ | error |
| 51 | No repository mutation calls in reporting/ | error |
| 52 | No command returning report projections directly | error |

## Zero-Warning Policy

All `check_arch.ts` violations must be resolved before merge. Exceptions require documented rationale linked to an ADR.
