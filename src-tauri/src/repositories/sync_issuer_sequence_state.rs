//! Producer-side per-issuer sequence ledger (SQL only).
//!
//! RFC 2026-08-04-node-identity-trust §3.4.1 / ADR-0038 / B6-B (Commit ④).
//!
//! Mirror of the consumer `sync_issuer_sequence` ledger: tracks the LAST ISSUED
//! sequence per LOCAL node identity so V2 export paths allocate monotonic,
//! per-issuer package sequences. Allocation and persistence are separated:
//!
//! - `begin_export` only READS the ledger and returns a `PendingIssuedSequence`
//!   token (no write);
//! - the ledger advances ONLY when `commit()` is invoked — the export stamps
//!   the pending value into the package metadata, builds/signs the file, and
//!   commits on success (advance-on-success). A failed export drops the token
//!   and the retry reuses the same number (no burned gaps).
//!
//! Keyed on `issuer_identity_id` (the stable node identity), NEVER on
//! `credential_id` — Transport ordering is independent of credential generation
//! (RFC §3.4.4), so continuity survives rotation/re-issue.

use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};

use super::executor::DbExecutor;

pub struct SyncIssuerSequenceStateRepository<'a> {
    executor: DbExecutor<'a>,
}

/// A reserved producer sequence for one export.
///
/// Ownership-based advance-on-success contract:
/// - `value()` returns the next sequence to stamp into package metadata;
/// - `commit(self)` persists `value` into the ledger — it CONSUMES the token,
///   so committing twice is a compile error, not a runtime possibility;
/// - dropping the token performs NO writes (no `Drop` impl touches the ledger).
pub struct PendingIssuedSequence<'a> {
    issuer_identity_id: String,
    value: u64,
    executor: DbExecutor<'a>,
}

impl<'a> PendingIssuedSequence<'a> {
    /// The sequence to write into the package being exported.
    pub fn value(&self) -> u64 {
        self.value
    }

    /// Persist the issued sequence (advance-on-success). Consumes the token.
    ///
    /// Fail-closed: refuses to write a sequence below the current ledger value
    /// (a regression). Under the single-writer export model this cannot occur;
    /// the check is defense-in-depth so the ledger never moves backwards.
    pub fn commit(self) -> AppResult<()> {
        let repo = SyncIssuerSequenceStateRepository::new(self.executor);
        if let Some(last) = repo.next_issued_sequence(&self.issuer_identity_id)? {
            if self.value < last {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::OperationNotPermitted {
                        message: format!(
                            "تراجع تسلسل الحزم للمُصدِر {}: المتوقّع >= {} ووصل {}",
                            self.issuer_identity_id, last, self.value
                        ),
                    },
                ));
            }
        }
        repo.record_issued_sequence(&self.issuer_identity_id, self.value)
    }
}

impl<'a> SyncIssuerSequenceStateRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Read the last issued sequence for `issuer_identity_id`.
    /// Returns `None` when no package has been issued by this node identity yet
    /// (the expected first sequence is 1).
    pub fn next_issued_sequence(&self, issuer_identity_id: &str) -> AppResult<Option<u64>> {
        validate_issuer_id(issuer_identity_id)?;
        let seq: Option<i64> = self.executor.query_row_optional(
            "SELECT last_issued_sequence FROM sync_issuer_sequence_state \
             WHERE issuer_identity_id = ?1",
            rusqlite::params![issuer_identity_id],
            |row| row.get(0),
        )?;
        Ok(seq.and_then(|s| u64::try_from(s).ok()))
    }

    /// Reserve the next producer sequence for `issuer_identity_id` WITHOUT writing.
    /// The ledger is advanced only by [`PendingIssuedSequence::commit`].
    pub fn begin_export(&self, issuer_identity_id: &str) -> AppResult<PendingIssuedSequence<'a>> {
        validate_issuer_id(issuer_identity_id)?;
        let value = match self.next_issued_sequence(issuer_identity_id)? {
            None => 1,
            Some(last) => last.checked_add(1).ok_or_else(|| {
                AppError::Validation(ValidationError::InvalidFormat {
                    field: "package_sequence".into(),
                    message: "تجاوز حدّ تسلسل الحزم لهذه الهوية".into(),
                })
            })?,
        };
        Ok(PendingIssuedSequence {
            issuer_identity_id: issuer_identity_id.to_string(),
            value,
            executor: self.executor,
        })
    }

    /// Persist an issued sequence (UPSERT). Invoked by
    /// [`PendingIssuedSequence::commit`] after a successful export — never from
    /// an allocation path.
    pub fn record_issued_sequence(
        &self,
        issuer_identity_id: &str,
        sequence: u64,
    ) -> AppResult<()> {
        validate_issuer_id(issuer_identity_id)?;
        self.executor
            .execute(
                r#"INSERT INTO sync_issuer_sequence_state (issuer_identity_id, last_issued_sequence, updated_at)
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

fn validate_issuer_id(issuer_identity_id: &str) -> AppResult<()> {
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
        let repo = SyncIssuerSequenceStateRepository::new(make_executor(&db));

        assert_eq!(repo.next_issued_sequence("issuer-a").unwrap(), None);

        let pending = repo.begin_export("issuer-a").unwrap();
        assert_eq!(pending.value(), 1);

        // begin_export alone performs NO write — the ledger is still absent.
        assert_eq!(repo.next_issued_sequence("issuer-a").unwrap(), None);
    }

    #[test]
    fn commit_advances_and_is_monotonic_per_issuer() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = SyncIssuerSequenceStateRepository::new(make_executor(&db));

        repo.begin_export("issuer-a").unwrap().commit().unwrap();
        repo.begin_export("issuer-a").unwrap().commit().unwrap();
        assert_eq!(repo.next_issued_sequence("issuer-a").unwrap(), Some(2));

        // A fresh issuer still starts at 1 regardless of other issuers' progress.
        let pending = repo.begin_export("issuer-b").unwrap();
        assert_eq!(pending.value(), 1);
        assert_eq!(repo.next_issued_sequence("issuer-b").unwrap(), None);
    }

    #[test]
    fn dropped_pending_sequence_never_advances() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = SyncIssuerSequenceStateRepository::new(make_executor(&db));

        let pending = repo.begin_export("issuer-a").unwrap();
        assert_eq!(pending.value(), 1);
        drop(pending);

        // No commit → ledger untouched → the retry reuses the same number.
        let retry = repo.begin_export("issuer-a").unwrap();
        assert_eq!(retry.value(), 1);
        assert_eq!(repo.next_issued_sequence("issuer-a").unwrap(), None);
    }

    #[test]
    fn commit_consumes_token_and_ledger_reflects_value() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = SyncIssuerSequenceStateRepository::new(make_executor(&db));

        repo.begin_export("issuer-a").unwrap().commit().unwrap();
        assert_eq!(repo.next_issued_sequence("issuer-a").unwrap(), Some(1));
    }

    #[test]
    fn commit_refuses_regression_fail_closed() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = SyncIssuerSequenceStateRepository::new(make_executor(&db));

        repo.record_issued_sequence("issuer-a", 5).unwrap();

        // A stale token carrying an older value must not move the ledger back.
        let stale = PendingIssuedSequence {
            issuer_identity_id: "issuer-a".to_string(),
            value: 3,
            executor: make_executor(&db),
        };
        let err = stale.commit().unwrap_err();
        assert!(err.to_string().contains("تراجع"), "got {err:?}");
        assert_eq!(repo.next_issued_sequence("issuer-a").unwrap(), Some(5));
    }

    #[test]
    fn record_issued_sequence_roundtrips() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = SyncIssuerSequenceStateRepository::new(make_executor(&db));

        repo.record_issued_sequence("issuer-a", 5).unwrap();
        repo.record_issued_sequence("issuer-a", 7).unwrap();
        assert_eq!(repo.next_issued_sequence("issuer-a").unwrap(), Some(7));
    }

    #[test]
    fn empty_issuer_id_is_rejected_fail_closed() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = SyncIssuerSequenceStateRepository::new(make_executor(&db));

        assert!(repo.next_issued_sequence("").is_err());
        assert!(repo.begin_export("").is_err());
        assert!(repo.record_issued_sequence("", 1).is_err());
        assert!(repo.next_issued_sequence("   ").is_err());
    }
}
