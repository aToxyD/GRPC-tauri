//! Producer-side canonical per-(issuer, target) transport sequence ledger
//! (SQL only).
//!
//! ADR-0053 — Unified Per-Target Transport Sequence (Accepted 2026-08-24):
//! ONE contiguous stream per `(issuer_identity_id, target_node_id)` across
//! ALL pipeline-imported TransportGuard kinds (products, daily_report,
//! monthly_summary, stock_movements, admin_access, trust). This replaces the
//! fragmented allocation scopes of migrations 006/009/010 so that the
//! producer scope is structurally identical to the frozen consumer scope: each
//! receiving node's `sync_issuer_sequence` ledger implicitly represents
//! `target_node_id = self`, and a kind-blind continuity check runs against it.
//!
//! Canonical target resolution (ADR-0053 §3.3) happens at the service/command
//! layer BEFORE allocation; this repository only enforces non-empty keys:
//!
//! - UNIT recipient   → `units.code` (authoritative row-validated);
//! - WILAYA recipient → `settings.wilaya_code`.
//!
//! Allocation and persistence are separated:
//!
//! - `begin_export` only READS the ledger and returns a pending token (no
//!   write);
//! - the ledger advances ONLY when `commit()` is invoked — the export stamps
//!   the pending value into the package metadata, builds/signs/writes the
//!   file, and commits on success (advance-on-success). A failed export drops
//!   the token and the retry reuses the same number (no burned gaps).
//!
//! Concurrency: allocation is race-safe because the single-writer SQLite mutex
//! (`Mutex<Option<Connection>>`) is held by the exporting command across the
//! whole begin→build→commit span. Two concurrent exports for the same
//! `(issuer, target)` serialize on the mutex and cannot receive the same
//! sequence.
//!
//! Keyed on `issuer_identity_id` (the stable node identity — never
//! `credential_id`), so continuity survives rotation/re-issue (RFC §3.4.4).
//! `.unit` bootstrap packages remain fixed-sequence-1 artifacts outside every
//! ledger (ADR-0044 A44-08).

use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};

use super::executor::DbExecutor;

pub struct TransportExportSequenceRepository<'a> {
    executor: DbExecutor<'a>,
}

/// A reserved producer transport sequence for one export to one target.
///
/// Ownership-based advance-on-success contract:
/// - `value()` returns the next sequence to stamp into package metadata;
/// - `commit(self)` persists `value` into the ledger — it CONSUMES the token,
///   so committing twice is a compile error, not a runtime possibility;
/// - dropping the token performs NO writes (no `Drop` impl touches the ledger).
pub struct PendingTransportSequence<'a> {
    issuer_identity_id: String,
    target_node_id: String,
    value: u64,
    executor: DbExecutor<'a>,
}

impl<'a> PendingTransportSequence<'a> {
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
        let repo = TransportExportSequenceRepository::new(self.executor);
        if let Some(last) =
            repo.next_issued_sequence(&self.issuer_identity_id, &self.target_node_id)?
        {
            if self.value < last {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::OperationNotPermitted {
                        message: format!(
                            "تراجع تسلسل الحزم للمُصدِر {} والهدف {}: المتوقّع >= {} ووصل {}",
                            self.issuer_identity_id, self.target_node_id, last, self.value
                        ),
                    },
                ));
            }
        }
        repo.record_issued_sequence(&self.issuer_identity_id, &self.target_node_id, self.value)
    }
}

impl<'a> TransportExportSequenceRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Read the last issued sequence for `(issuer_identity_id,
    /// target_node_id)`. Returns `None` when no package has been issued on
    /// this stream yet (the expected first sequence is 1).
    pub fn next_issued_sequence(
        &self,
        issuer_identity_id: &str,
        target_node_id: &str,
    ) -> AppResult<Option<u64>> {
        validate_stream_key(issuer_identity_id, target_node_id)?;
        let seq: Option<i64> = self.executor.query_row_optional(
            "SELECT last_issued_sequence FROM transport_export_sequence \
             WHERE issuer_identity_id = ?1 AND target_node_id = ?2",
            rusqlite::params![issuer_identity_id, target_node_id],
            |row| row.get(0),
        )?;
        Ok(seq.and_then(|s| u64::try_from(s).ok()))
    }

    /// Reserve the next producer sequence for `(issuer_identity_id,
    /// target_node_id)` WITHOUT writing. The ledger is advanced only by
    /// [`PendingTransportSequence::commit`].
    pub fn begin_export(
        &self,
        issuer_identity_id: &str,
        target_node_id: &str,
    ) -> AppResult<PendingTransportSequence<'a>> {
        validate_stream_key(issuer_identity_id, target_node_id)?;
        let value = match self.next_issued_sequence(issuer_identity_id, target_node_id)? {
            None => 1,
            Some(last) => last.checked_add(1).ok_or_else(|| {
                AppError::Validation(ValidationError::InvalidFormat {
                    field: "package_sequence".into(),
                    message: "تجاوز حدّ تسلسل الحزم لهذا التدفّق".into(),
                })
            })?,
        };
        Ok(PendingTransportSequence {
            issuer_identity_id: issuer_identity_id.to_string(),
            target_node_id: target_node_id.to_string(),
            value,
            executor: self.executor,
        })
    }

    /// Persist an issued sequence (UPSERT). Invoked by
    /// [`PendingTransportSequence::commit`] after a successful export — never
    /// from an allocation path.
    pub fn record_issued_sequence(
        &self,
        issuer_identity_id: &str,
        target_node_id: &str,
        sequence: u64,
    ) -> AppResult<()> {
        validate_stream_key(issuer_identity_id, target_node_id)?;
        self.executor
            .execute(
                r#"INSERT INTO transport_export_sequence
                   (issuer_identity_id, target_node_id, last_issued_sequence, updated_at)
                   VALUES (?1, ?2, ?3, datetime('now'))
                   ON CONFLICT(issuer_identity_id, target_node_id) DO UPDATE SET
                     last_issued_sequence = excluded.last_issued_sequence,
                     updated_at = datetime('now')"#,
                rusqlite::params![issuer_identity_id, target_node_id, sequence as i64],
            )
            .map_err(AppError::from)?;
        Ok(())
    }
}

fn validate_stream_key(issuer_identity_id: &str, target_node_id: &str) -> AppResult<()> {
    if issuer_identity_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "issuer_identity_id".into(),
            message: "معرّف هوية المُصدِر فارغ".into(),
        }));
    }
    if target_node_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "target_node_id".into(),
            message: "معرّف العقدة المستهدفة فارغ".into(),
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
        let repo = TransportExportSequenceRepository::new(make_executor(&db));

        assert_eq!(
            repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(),
            None
        );

        let pending = repo.begin_export("issuer-a", "UNIT-A").unwrap();
        assert_eq!(pending.value(), 1);

        // begin_export alone performs NO write — the stream is still absent.
        assert_eq!(
            repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(),
            None
        );
    }

    #[test]
    fn commit_advances_monotonically_per_stream() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = TransportExportSequenceRepository::new(make_executor(&db));

        repo.begin_export("issuer-a", "UNIT-A")
            .unwrap()
            .commit()
            .unwrap();
        repo.begin_export("issuer-a", "UNIT-A")
            .unwrap()
            .commit()
            .unwrap();
        assert_eq!(
            repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(),
            Some(2)
        );

        // A fresh target stream still starts at 1 regardless of UNIT-A progress.
        let pending_b = repo.begin_export("issuer-a", "UNIT-B").unwrap();
        assert_eq!(pending_b.value(), 1);
        assert_eq!(
            repo.next_issued_sequence("issuer-a", "UNIT-B").unwrap(),
            None
        );

        // A fresh issuer stream is independent of both targets.
        let pending_c = repo.begin_export("issuer-b", "UNIT-A").unwrap();
        assert_eq!(pending_c.value(), 1);
        assert_eq!(
            repo.next_issued_sequence("issuer-b", "UNIT-A").unwrap(),
            None
        );
    }

    #[test]
    fn per_target_streams_are_isolated_across_kinds() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = TransportExportSequenceRepository::new(make_executor(&db));

        // One contiguous stream per (issuer, target): interleaved kinds share
        // the numbering; different targets do not.
        for expected in 1..=4u64 {
            let p = repo.begin_export("issuer-a", "UNIT-A").unwrap();
            assert_eq!(p.value(), expected);
            p.commit().unwrap();
        }
        assert_eq!(
            repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(),
            Some(4)
        );
        assert_eq!(
            repo.next_issued_sequence("issuer-a", "WILAYA-16").unwrap(),
            None
        );
    }

    #[test]
    fn dropped_pending_sequence_never_advances() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = TransportExportSequenceRepository::new(make_executor(&db));

        let pending = repo.begin_export("issuer-a", "UNIT-A").unwrap();
        assert_eq!(pending.value(), 1);
        drop(pending);

        // No commit → stream untouched → the retry reuses the same number.
        let retry = repo.begin_export("issuer-a", "UNIT-A").unwrap();
        assert_eq!(retry.value(), 1);
        assert_eq!(
            repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(),
            None
        );
    }

    #[test]
    fn commit_consumes_token_and_ledger_reflects_value() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = TransportExportSequenceRepository::new(make_executor(&db));

        repo.begin_export("issuer-a", "UNIT-A")
            .unwrap()
            .commit()
            .unwrap();
        assert_eq!(
            repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(),
            Some(1)
        );
    }

    #[test]
    fn commit_refuses_regression_fail_closed() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = TransportExportSequenceRepository::new(make_executor(&db));

        repo.record_issued_sequence("issuer-a", "UNIT-A", 5)
            .unwrap();

        // A stale token carrying an older value must not move the stream back.
        let stale = PendingTransportSequence {
            issuer_identity_id: "issuer-a".to_string(),
            target_node_id: "UNIT-A".to_string(),
            value: 3,
            executor: make_executor(&db),
        };
        let err = stale.commit().unwrap_err();
        assert!(err.to_string().contains("تراجع"), "got {err:?}");
        assert_eq!(
            repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(),
            Some(5)
        );
    }

    #[test]
    fn record_issued_sequence_roundtrips() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = TransportExportSequenceRepository::new(make_executor(&db));

        repo.record_issued_sequence("issuer-a", "UNIT-A", 5)
            .unwrap();
        repo.record_issued_sequence("issuer-a", "UNIT-A", 7)
            .unwrap();
        assert_eq!(
            repo.next_issued_sequence("issuer-a", "UNIT-A").unwrap(),
            Some(7)
        );
    }

    #[test]
    fn empty_stream_keys_are_rejected_fail_closed() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = TransportExportSequenceRepository::new(make_executor(&db));

        assert!(repo.next_issued_sequence("", "UNIT-A").is_err());
        assert!(repo.next_issued_sequence("issuer-a", "").is_err());
        assert!(repo.begin_export("", "UNIT-A").is_err());
        assert!(repo.begin_export("issuer-a", "  ").is_err());
        assert!(repo.begin_export("   ", "").is_err());
        assert!(repo.record_issued_sequence("", "UNIT-A", 1).is_err());
        assert!(repo.record_issued_sequence("issuer-a", "", 1).is_err());
    }
}
