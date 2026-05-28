# ADR: Transaction Model

## الحالة
مقبول

## السياق
Fiscal operations require transactional guarantees: a closure either completes fully (update year status + snapshot + FIFO reclassification + audit log) or rolls back entirely. Multiple services previously managed their own SQL transactions ad-hoc, creating risk of partial updates and inconsistent state.

## القرار
Establish a two-tier transaction model:

1. **Repository-level**: Individual `&self` methods are auto-commit. No repository method manages transactions internally.
2. **Service-level**: Services that need multi-step atomicity receive a `DbExecutor` (which wraps a single connection) and call `executor.transactional(|tx| ...)`. The `tx` parameter provides a `DbExecutor<'_>` scoped to the transaction.
3. **Command-level**: Commands that coordinate multiple services use `db.with_transaction(|tx| ...)` from the `Database` connection factory.

Key constraints:
- Repositories must NOT call `transactional()` — they operate on whatever executor is given.
- Only `FiscalClosingService` and `SyncImportExecutionService` use explicit transactions.
- Read-only queries never open transactions.
- All transaction boundaries are explicit — no implicit nesting.

## النتائج المترتبة
- Atomic fiscal operations (close, archive, restore)
- No implicit transaction nesting
- Repositories remain pure persistence — no transaction orchestration
- Test code uses `with_transaction` for setup/teardown isolation
