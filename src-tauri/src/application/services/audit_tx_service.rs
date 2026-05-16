//! Atomic Audit Transaction Service
//!
//! ## Security Guarantee
//!
//! Every state-mutating operation that requires an audit trail MUST use this service.
//! This guarantees that the audit log and the mutation are **one atomic unit** — either
//! both commit, or neither commits.

use crate::db::Database;
use crate::domain::audit::AuditAction;
use crate::errors::AppError;
use crate::repositories::DbExecutor;
// Note: We'll need to fix this import once AuditService is moved
use super::{AuditService, UserContext};
use crate::infrastructure::db::transaction::TxContext;

/// Executes a mutation closure and an audit-log entry inside the **same** SQLite transaction.
pub struct AuditTxService;

impl AuditTxService {
    /// Execute a business logic closure within a transaction and record an audit log.
    ///
    /// If the closure fails, the transaction rolls back and NO audit log is written.
    /// If the audit log fails, the transaction rolls back and the mutation is reverted.
    pub fn execute_with_audit<F, T>(
        db: &mut Database,
        action: AuditAction,
        user_ctx: &UserContext,
        f: F,
    ) -> Result<T, AppError>
    where
        F: FnOnce(&TxContext) -> Result<T, AppError>,
    {
        let conn = &mut db.conn;
        let tx = conn.transaction()?;

        let tx_ctx = TxContext {
            executor: DbExecutor::Tx(&tx),
        };

        // 1. Run business logic
        let result = match f(&tx_ctx) {
            Ok(v) => v,
            Err(e) => {
                log::error!(
                    "Transactional operation failed for action {:?}, rolling back: {:?}",
                    action,
                    e
                );
                // tx is dropped here → automatic rollback
                return Err(e);
            }
        };

        // 2. Write audit log inside the SAME transaction
        let audit_svc = AuditService::new(tx_ctx.executor);
        audit_svc.log_success(
            &user_ctx.user_id,
            &user_ctx.username,
            action.clone(),
            action.default_entity_type(),
            None,
            None,
            None,
            None,
            user_ctx.session_id.as_deref(),
            None,
        )?;

        // 3. Both succeeded → commit
        tx.commit()?;

        log::info!(
            "Transactional operation succeeded for action {:?}, committed with audit.",
            action
        );

        Ok(result)
    }
}
