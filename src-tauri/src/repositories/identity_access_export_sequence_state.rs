//! Producer-side per-(issuer, target) sequence ledger for `identity_access`
//! exports (SQL only).
//!
//! F-1 Option A (owner decision 2026-08-15 — ADR-0045 §26.9, RFC §3.4.1
//! amendment): `identity_access` packages allocate their transport sequence
//! from a stream keyed by `(issuer_identity_id, target_unit_code)` — NOT the
//! global per-issuer `sync_issuer_sequence_state` ledger. Each fresh target
//! UNIT receives its own `1, 2, 3, ...` stream from the same WILAYA issuer,
//! which satisfies the ratified consumer Transport Guard rule "first import
//! from an issuer on an empty ledger MUST be sequence 1" (A45-06 / B8 control
//! 11) for EVERY unit in the fleet.
//!
//! Allocation and persistence are separated, mirroring
//! [`SyncIssuerSequenceStateRepository`](super::sync_issuer_sequence_state):
//!
//! - `begin_export` only READS the ledger and returns a pending token (no
//!   write);
//! - the ledger advances ONLY when `commit()` is invoked — the export stamps
//!   the pending value into the package metadata, builds/signs the file, and
//!   commits on success (advance-on-success). A failed export drops the token
//!   and the retry reuses the same number (no burned gaps).
//!
//! Concurrency: allocation is race-safe because the single-writer SQLite mutex
//! (`Mutex<Option<Connection>>`) is held by the exporting command across the
//! whole begin→build→commit span — the same guarantee the global producer
//! ledger relies on. Two concurrent exports for the same
//! `(issuer, target)` therefore serialize on the mutex and cannot receive the
//! same sequence.
//!
//! Keyed on `issuer_identity_id` (the stable node identity) and
//! `target_unit_code` (the authoritative UNIT code from the exported payload,
//! itself built from the local `units` row by `UserAccountSyncService::export`
//! — never an arbitrary renderer value). Transport ordering is independent of
//! credential generation (RFC §3.4.4), so continuity survives
//! rotation/re-issue.

use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};

use super::executor::DbExecutor;

pub struct IdentityAccessExportSequenceStateRepository<'a> {
    executor: DbExecutor<'a>,
}

/// A reserved `identity_access` producer sequence for one export.
///
/// Ownership-based advance-on-success contract (same shape as
/// [`PendingIssuedSequence`](super::sync_issuer_sequence_state::PendingIssuedSequence)):
/// - `value()` returns the next sequence to stamp into package metadata;
/// - `commit(self)` persists `value` into the ledger — it CONSUMES the token,
///   so committing twice is a compile error, not a runtime possibility;
/// - dropping the token performs NO writes (no `Drop` impl touches the ledger).
pub struct PendingIdentityAccessSequence<'a> {
    issuer_identity_id: String,
    target_unit_code: String,
    value: u64,
    executor: DbExecutor<'a>,
}

impl<'a> PendingIdentityAccessSequence<'a> {
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
        let repo = IdentityAccessExportSequenceStateRepository::new(self.executor);
        if let Some(last) = repo.next_issued_sequence(
            &self.issuer_identity_id,
            &self.target_unit_code,
        )? {
            if self.value < last {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::OperationNotPermitted {
                        message: format!(
                            "تراجع تسلسل الحزم للمُصدِر {} والوحدة {}: المتوقّع >= {} ووصل {}",
                            self.issuer_identity_id, self.target_unit_code, last, self.value
                        ),
                    },
                ));
            }
        }
        repo.record_issued_sequence(
            &self.issuer_identity_id,
            &self.target_unit_code,
            self.value,
        )
    }
}

impl<'a> IdentityAccessExportSequenceStateRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Read the last issued sequence for `(issuer_identity_id,
    /// target_unit_code)`. Returns `None` when no `identity_access` package
    /// has been issued for this stream yet (the expected first sequence is 1).
    pub fn next_issued_sequence(
        &self,
        issuer_identity_id: &str,
        target_unit_code: &str,
    ) -> AppResult<Option<u64>> {
        validate_stream_key(issuer_identity_id, target_unit_code)?;
        let seq: Option<i64> = self.executor.query_row_optional(
            "SELECT last_issued_sequence FROM identity_access_export_sequence \
             WHERE issuer_identity_id = ?1 AND target_unit_code = ?2",
            rusqlite::params![issuer_identity_id, target_unit_code],
            |row| row.get(0),
        )?;
        Ok(seq.and_then(|s| u64::try_from(s).ok()))
    }

    /// Reserve the next producer sequence for `(issuer_identity_id,
    /// target_unit_code)` WITHOUT writing. The ledger is advanced only by
    /// [`PendingIdentityAccessSequence::commit`].
    pub fn begin_export(
        &self,
        issuer_identity_id: &str,
        target_unit_code: &str,
    ) -> AppResult<PendingIdentityAccessSequence<'a>> {
        validate_stream_key(issuer_identity_id, target_unit_code)?;
        let value = match self.next_issued_sequence(issuer_identity_id, target_unit_code)? {
            None => 1,
            Some(last) => last.checked_add(1).ok_or_else(|| {
                AppError::Validation(ValidationError::InvalidFormat {
                    field: "package_sequence".into(),
                    message: "تجاوز حدّ تسلسل الحزم لهذه الوحدة".into(),
                })
            })?,
        };
        Ok(PendingIdentityAccessSequence {
            issuer_identity_id: issuer_identity_id.to_string(),
            target_unit_code: target_unit_code.to_string(),
            value,
            executor: self.executor,
        })
    }

    /// Persist an issued sequence (UPSERT). Invoked by
    /// [`PendingIdentityAccessSequence::commit`] after a successful export —
    /// never from an allocation path.
    pub fn record_issued_sequence(
        &self,
        issuer_identity_id: &str,
        target_unit_code: &str,
        sequence: u64,
    ) -> AppResult<()> {
        validate_stream_key(issuer_identity_id, target_unit_code)?;
        self.executor
            .execute(
                r#"INSERT INTO identity_access_export_sequence
                   (issuer_identity_id, target_unit_code, last_issued_sequence, updated_at)
                   VALUES (?1, ?2, ?3, datetime('now'))
                   ON CONFLICT(issuer_identity_id, target_unit_code) DO UPDATE SET
                     last_issued_sequence = excluded.last_issued_sequence,
                     updated_at = datetime('now')"#,
                rusqlite::params![issuer_identity_id, target_unit_code, sequence as i64],
            )
            .map_err(AppError::from)?;
        Ok(())
    }
}

fn validate_stream_key(issuer_identity_id: &str, target_unit_code: &str) -> AppResult<()> {
    if issuer_identity_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "issuer_identity_id".into(),
            message: "معرّف هوية المُصدِر فارغ".into(),
        }));
    }
    if target_unit_code.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "target_unit_code".into(),
            message: "رمز الوحدة المستهدفة فارغ".into(),
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
        let repo = IdentityAccessExportSequenceStateRepository::new(make_executor(&db));

        assert_eq!(repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(), None);

        let pending = repo.begin_export("issuer-a", "UNIT-A").unwrap();
        assert_eq!(pending.value(), 1);

        // begin_export alone performs NO write — the stream is still absent.
        assert_eq!(repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(), None);
    }

    #[test]
    fn commit_advances_monotonically_per_stream() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = IdentityAccessExportSequenceStateRepository::new(make_executor(&db));

        repo.begin_export("issuer-a", "UNIT-A").unwrap().commit().unwrap();
        repo.begin_export("issuer-a", "UNIT-A").unwrap().commit().unwrap();
        assert_eq!(repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(), Some(2));

        // A fresh target stream still starts at 1 regardless of UNIT-A's
        // progress (this is the F-1 resolution: UNIT-B receives its own 1).
        let pending_b = repo.begin_export("issuer-a", "UNIT-B").unwrap();
        assert_eq!(pending_b.value(), 1);
        assert_eq!(repo.next_issued_sequence("issuer-a", "UNIT-B").unwrap(), None);

        // A fresh issuer stream is independent of both targets.
        let pending_c = repo.begin_export("issuer-b", "UNIT-A").unwrap();
        assert_eq!(pending_c.value(), 1);
        assert_eq!(repo.next_issued_sequence("issuer-b", "UNIT-A").unwrap(), None);
    }

    #[test]
    fn dropped_pending_sequence_never_advances() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = IdentityAccessExportSequenceStateRepository::new(make_executor(&db));

        let pending = repo.begin_export("issuer-a", "UNIT-A").unwrap();
        assert_eq!(pending.value(), 1);
        drop(pending);

        // No commit → stream untouched → the retry reuses the same number.
        let retry = repo.begin_export("issuer-a", "UNIT-A").unwrap();
        assert_eq!(retry.value(), 1);
        assert_eq!(repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(), None);
    }

    #[test]
    fn commit_consumes_token_and_ledger_reflects_value() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = IdentityAccessExportSequenceStateRepository::new(make_executor(&db));

        repo.begin_export("issuer-a", "UNIT-A").unwrap().commit().unwrap();
        assert_eq!(repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(), Some(1));
    }

    #[test]
    fn commit_refuses_regression_fail_closed() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = IdentityAccessExportSequenceStateRepository::new(make_executor(&db));

        repo.record_issued_sequence("issuer-a", "UNIT-A", 5).unwrap();

        // A stale token carrying an older value must not move the stream back.
        let stale = PendingIdentityAccessSequence {
            issuer_identity_id: "issuer-a".to_string(),
            target_unit_code: "UNIT-A".to_string(),
            value: 3,
            executor: make_executor(&db),
        };
        let err = stale.commit().unwrap_err();
        assert!(err.to_string().contains("تراجع"), "got {err:?}");
        assert_eq!(repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(), Some(5));
    }

    #[test]
    fn record_issued_sequence_roundtrips() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = IdentityAccessExportSequenceStateRepository::new(make_executor(&db));

        repo.record_issued_sequence("issuer-a", "UNIT-A", 5).unwrap();
        repo.record_issued_sequence("issuer-a", "UNIT-A", 7).unwrap();
        assert_eq!(repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(), Some(7));
    }

    #[test]
    fn empty_stream_keys_are_rejected_fail_closed() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = IdentityAccessExportSequenceStateRepository::new(make_executor(&db));

        assert!(repo.next_issued_sequence("", "UNIT-A").is_err());
        assert!(repo.next_issued_sequence("issuer-a", "").is_err());
        assert!(repo.begin_export("", "UNIT-A").is_err());
        assert!(repo.begin_export("issuer-a", "  ").is_err());
        assert!(repo.record_issued_sequence("", "UNIT-A", 1).is_err());
        assert!(repo.record_issued_sequence("issuer-a", "", 1).is_err());
    }
}
