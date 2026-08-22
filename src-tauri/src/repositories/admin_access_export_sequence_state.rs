//! Producer-side per-issuer sequence ledger for `admin_access` exports
//! (SQL only).
//!
//! ADR-0051 §7 (Admin-Only B8 Account Synchronization — Accepted
//! 2026-08-22): `admin_access` packages allocate their transport sequence
//! from a dedicated stream keyed by `issuer_identity_id` ALONE — NOT the
//! global per-issuer producer ledger (`sync_issuer_sequence_state`,
//! migration 006) and NOT the per-target `identity_access` stream (migration
//! 009). The package is fleet-wide WILAYA → all UNIT nodes with NO target
// binding, so a single per-issuer stream serves every UNIT, and the same
//! signed artifact is independently importable by every authorized UNIT.
//!
//! Allocation and persistence are separated, mirroring
//! [`IdentityAccessExportSequenceStateRepository`](super::identity_access_export_sequence_state):
//!
//! - `begin_export` only READS the ledger and returns a pending token (no
//!   write);
//! - the ledger advances ONLY when `commit()` is invoked — advance-on-success;
//!   a failed export drops the token and the retry reuses the same number.
//!
//! Concurrency: allocation is race-safe because the single-writer SQLite mutex
//! (`Mutex<Option<Connection>>`) is held by the exporting command across the
//! whole begin→build→commit span.

use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};

use super::executor::DbExecutor;

pub struct AdminAccessExportSequenceStateRepository<'a> {
    executor: DbExecutor<'a>,
}

/// A reserved `admin_access` producer sequence for one export.
///
/// Ownership-based advance-on-success contract:
/// - `value()` returns the next sequence to stamp into package metadata;
/// - `commit(self)` persists `value` into the ledger — it CONSUMES the token,
///   so committing twice is a compile error;
/// - dropping the token performs NO writes.
pub struct PendingAdminAccessSequence<'a> {
    issuer_identity_id: String,
    value: u64,
    executor: DbExecutor<'a>,
}

impl<'a> PendingAdminAccessSequence<'a> {
    /// The sequence to write into the package being exported.
    pub fn value(&self) -> u64 {
        self.value
    }

    /// Persist the issued sequence (advance-on-success). Consumes the token.
    ///
    /// Fail-closed: refuses to write a sequence below the current ledger value
    /// (defense-in-depth so the ledger never moves backwards).
    pub fn commit(self) -> AppResult<()> {
        let repo = AdminAccessExportSequenceStateRepository::new(self.executor);
        if let Some(last) = repo.next_issued_sequence(&self.issuer_identity_id)? {
            if self.value < last {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::OperationNotPermitted {
                        message: format!(
                            "تراجع تسلسل حزم حساب المدير للمُصدِر {}: المتوقّع >= {} ووصل {}",
                            self.issuer_identity_id, last, self.value
                        ),
                    },
                ));
            }
        }
        repo.record_issued_sequence(&self.issuer_identity_id, self.value)
    }
}

impl<'a> AdminAccessExportSequenceStateRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Read the last issued sequence for `issuer_identity_id`. Returns `None`
    /// when no `admin_access` package has been issued by this issuer yet
    /// (the expected first sequence is 1).
    pub fn next_issued_sequence(&self, issuer_identity_id: &str) -> AppResult<Option<u64>> {
        validate_stream_key(issuer_identity_id)?;
        let seq: Option<i64> = self.executor.query_row_optional(
            "SELECT last_issued_sequence FROM admin_access_export_sequence \
             WHERE issuer_identity_id = ?1",
            rusqlite::params![issuer_identity_id],
            |row| row.get(0),
        )?;
        Ok(seq.and_then(|s| u64::try_from(s).ok()))
    }

    /// Reserve the next producer sequence for `issuer_identity_id` WITHOUT
    /// writing. The ledger advances only via
    /// [`PendingAdminAccessSequence::commit`].
    pub fn begin_export(
        &self,
        issuer_identity_id: &str,
    ) -> AppResult<PendingAdminAccessSequence<'a>> {
        validate_stream_key(issuer_identity_id)?;
        let value = match self.next_issued_sequence(issuer_identity_id)? {
            None => 1,
            Some(last) => last.checked_add(1).ok_or_else(|| {
                AppError::Validation(ValidationError::InvalidFormat {
                    field: "package_sequence".into(),
                    message: "تجاوز حدّ تسلسل حزم حساب المدير".into(),
                })
            })?,
        };
        Ok(PendingAdminAccessSequence {
            issuer_identity_id: issuer_identity_id.to_string(),
            value,
            executor: self.executor,
        })
    }

    /// Persist an issued sequence (UPSERT). Invoked by
    /// [`PendingAdminAccessSequence::commit`] after a successful export —
    /// never from an allocation path.
    pub fn record_issued_sequence(&self, issuer_identity_id: &str, sequence: u64) -> AppResult<()> {
        validate_stream_key(issuer_identity_id)?;
        self.executor
            .execute(
                r#"INSERT INTO admin_access_export_sequence
                   (issuer_identity_id, last_issued_sequence, updated_at)
                   VALUES (?1, ?2, datetime('now'))
                   ON CONFLICT(issuer_identity_id) DO UPDATE SET
                     last_issued_sequence = excluded.last_issued_sequence,
                     updated_at = datetime('now')"#,
                rusqlite::params![issuer_identity_id, sequence as i64],
            )
            .map_err(AppError::from)?;
        Ok(())
    }
}

fn validate_stream_key(issuer_identity_id: &str) -> AppResult<()> {
    if issuer_identity_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "issuer_identity_id".into(),
            message: "معرّف هوية المُصدِر فارغ".into(),
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ConnectionFactory, Database};

    fn make_executor(db: &Database) -> DbExecutor<'_> {
        db.executor()
    }

    #[test]
    fn begin_export_reads_only_and_first_is_one() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = AdminAccessExportSequenceStateRepository::new(make_executor(&db));

        assert_eq!(repo.next_issued_sequence("issuer-a").unwrap(), None);

        let pending = repo.begin_export("issuer-a").unwrap();
        assert_eq!(pending.value(), 1);

        // begin_export alone performs NO write — the stream is still absent.
        assert_eq!(repo.next_issued_sequence("issuer-a").unwrap(), None);
    }

    #[test]
    fn commit_advances_monotonically_per_issuer_without_target_dimension() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = AdminAccessExportSequenceStateRepository::new(make_executor(&db));

        // ADR-0051 §7: ONE stream per issuer — there is no target dimension,
        // so consecutive exports for DIFFERENT units still share the stream.
        repo.begin_export("issuer-a").unwrap().commit().unwrap();
        repo.begin_export("issuer-a").unwrap().commit().unwrap();
        assert_eq!(repo.next_issued_sequence("issuer-a").unwrap(), Some(2));

        let pending = repo.begin_export("issuer-a").unwrap();
        assert_eq!(pending.value(), 3);

        // A fresh issuer stream is independent.
        assert_eq!(repo.next_issued_sequence("issuer-b").unwrap(), None);
        assert_eq!(repo.begin_export("issuer-b").unwrap().value(), 1);
    }

    #[test]
    fn dropped_pending_sequence_never_advances() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = AdminAccessExportSequenceStateRepository::new(make_executor(&db));

        let pending = repo.begin_export("issuer-a").unwrap();
        assert_eq!(pending.value(), 1);
        drop(pending);

        // No commit → stream untouched → the retry reuses the same number.
        let retry = repo.begin_export("issuer-a").unwrap();
        assert_eq!(retry.value(), 1);
        assert_eq!(repo.next_issued_sequence("issuer-a").unwrap(), None);
    }

    #[test]
    fn commit_consumes_token_and_ledger_reflects_value() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = AdminAccessExportSequenceStateRepository::new(make_executor(&db));

        repo.begin_export("issuer-a").unwrap().commit().unwrap();
        assert_eq!(repo.next_issued_sequence("issuer-a").unwrap(), Some(1));
    }

    #[test]
    fn commit_refuses_regression_fail_closed() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = AdminAccessExportSequenceStateRepository::new(make_executor(&db));

        repo.record_issued_sequence("issuer-a", 5).unwrap();

        let stale = PendingAdminAccessSequence {
            issuer_identity_id: "issuer-a".to_string(),
            value: 3,
            executor: make_executor(&db),
        };
        let err = stale.commit().unwrap_err();
        assert!(err.to_string().contains("تراجع"), "got {err:?}");
        assert_eq!(repo.next_issued_sequence("issuer-a").unwrap(), Some(5));
    }

    #[test]
    fn empty_stream_keys_are_rejected_fail_closed() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = AdminAccessExportSequenceStateRepository::new(make_executor(&db));

        assert!(repo.next_issued_sequence("").is_err());
        assert!(repo.next_issued_sequence("  ").is_err());
        assert!(repo.begin_export("").is_err());
        assert!(repo.record_issued_sequence("", 1).is_err());
    }
}
