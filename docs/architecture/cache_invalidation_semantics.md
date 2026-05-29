# Cache Invalidation Semantics

## Phase 5.A — DomainEvent-Based Semantic Invalidation

### Principle

Cache invalidation depends ONLY on `DomainEvent` variants. There is no
TTL-based, heuristic, or "clear all" invalidation. Every invalidation decision
is driven by a semantic rule that maps a domain event to the reports whose data
could have changed as a result of that event.

### Invalidation Mapping

| DomainEvent | Report Slugs Invalidated |
|------------|--------------------------|
| `StockMovementRecorded` | `inventory-valuation`, `stock-movement-ledger`, `fifo-layer-report`, `inventory-snapshot` |
| `InventoryCorrected` | `inventory-valuation`, `fifo-layer-report`, `inventory-snapshot` |
| `FiscalYearClosed` | `fiscal-year-summary`, `fiscal-transition-report`, `fiscal-closure-report` |
| `SyncPackageImported` | All entries scoped to open fiscal years |
| `FifoLayerConsumed` | None (FIFO consumption does not change reportable aggregates) |
| `FiscalYearArchived` | None (archiving does not change report data) |
| `FiscalTransitionApplied` | None (transitions are metadata-only) |
| `SyncConflictDetected` | None (conflicts are operational, not data) |
| `SyncConflictResolved` | None (resolution is operational, not data) |
| `AuditEventWritten` | None (audit events do not affect reports) |
| `AuditIntegrityBreach` | None (integrity breaches do not affect reports) |

### Closed Fiscal Year Protection

Cache entries scoped to a closed fiscal year are NEVER invalidated. The
`invalidate()` method accepts an `is_year_closed` predicate that is evaluated
for each cached entry. If the entry's `fiscal_scope` year is closed, the entry
is preserved unconditionally.

This rule is based on the immutability of closed fiscal year data:

- Once a fiscal year is closed, its data never changes
- A report computed against a closed year will always produce the same output
  for the same input
- Invalidating a closed-year cache entry would force a recomputation that
  produces an identical result — wasted work with no semantic benefit

### SyncPackageImported Behavior

When a sync package is imported, the local data for the currently open fiscal
year may change. The `SyncPackageImported` event invalidates:

1. All cache entries whose `fiscal_scope` is `None` (unscoped reports that
   operate on current data)
2. All cache entries whose `fiscal_scope` is an open fiscal year

Entries scoped to closed fiscal years are preserved.

### InvalidationKind

```rust
pub enum InvalidationKind {
    /// Invalidate specific report slugs
    Slugs(Vec<&'static str>),
    /// Invalidate all entries whose fiscal scope is an open year
    AllOpenFiscalYear,
}
```

- `Slugs` — used for most events. Only entries whose `report_slug` matches
  the list are candidates for invalidation.
- `AllOpenFiscalYear` — used for `SyncPackageImported`. All entries operating
  on current or open-year data are invalidated.

### Determinism

Given the same `DomainEvent` and the same `is_year_closed` state, the set of
invalidated cache keys is deterministic. This is verified by tests that create
identical cache states and identical events, and assert identical invalidation
results.

### Unknown Event Safety

Events that are not explicitly mapped (e.g., `AuditEventWritten`,
`FifoLayerConsumed`) produce `InvalidationKind::Slugs(vec![])` — an empty list
of slugs. This means:

- No entries are invalidated
- No panic occurs
- The cache is unaffected

Adding a new `DomainEvent` variant does NOT require updating the invalidation
logic unless the event changes reportable data.

### Why TTL Is Forbidden

TTL-based invalidation would introduce non-determinism:

- Cache entries would expire based on wall-clock time
- Report consumers would see different hit/miss behavior depending on when they
  accessed the cache
- Reproducibility guarantees would be violated: running the same sequence of
  operations on different days could produce different cache behavior

Semantic invalidation avoids all these problems by tying cache lifetime to
domain events, not to time.

### Why No "Clear All"

A "clear all" operation would:

- Invalidate closed fiscal year entries, violating immutability guarantees
- Remove entries that are still valid, causing unnecessary recomputation
- Mask bugs where specific invalidation rules are missing (a clear-all would
  hide an incomplete invalidation mapping)

Every invalidation must be traceable to a specific domain event.
