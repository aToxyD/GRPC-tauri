//! Transport Guard — per-issuer package ordering (RFC 2026-08-04 §3.4.1).
//!
//! Pure, deterministic ordering check keyed on `(issuer_identity_id, package_sequence)`.
//! The guard is invoked ONLY inside `run_import_pipeline`; the pipeline resolves the
//! issuer's last applied sequence from the `sync_issuer_sequence` ledger before calling.
//!
//! Independence constraint (RFC §3.4.4): the Transport Guard MUST NOT depend on credential
//! generation. The guard signature therefore accepts no generation input.

use super::sequencing::{SequenceContinuity, SequenceValidator};

/// Verdict of the Transport Guard for a single incoming package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportVerdict {
    /// `incoming == last_applied + 1` — apply.
    Accept {
        issuer_identity_id: String,
        sequence: u64,
    },
    /// Gap or missing package — `incoming != expected` with `incoming > expected`.
    /// Deferrable: earlier packages may still be in transit.
    OutOfOrder {
        issuer_identity_id: String,
        expected: u64,
        got: u64,
    },
    /// Duplicate or replay — `incoming <= last_applied`. Reject.
    Replay {
        issuer_identity_id: String,
        last_applied: u64,
        incoming: u64,
    },
}

/// Evaluates transport ordering per issuer.
pub struct TransportGuard;

impl TransportGuard {
    /// Evaluate an incoming `package_sequence` from `issuer_identity_id` against the
    /// issuer's `last_applied_sequence` (`None` when the issuer has no applied package).
    ///
    /// The continuity arithmetic is owned by `SequenceValidator::check_sequence_continuity`
    /// (single source of truth, P2); this guard binds it to the per-issuer scope.
    pub fn check(
        issuer_identity_id: &str,
        incoming_sequence: u64,
        last_applied_sequence: Option<u64>,
    ) -> TransportVerdict {
        match SequenceValidator::check_sequence_continuity(incoming_sequence, last_applied_sequence)
        {
            SequenceContinuity::Contiguous => TransportVerdict::Accept {
                issuer_identity_id: issuer_identity_id.to_string(),
                sequence: incoming_sequence,
            },
            SequenceContinuity::Gap { expected, got } => TransportVerdict::OutOfOrder {
                issuer_identity_id: issuer_identity_id.to_string(),
                expected,
                got,
            },
            SequenceContinuity::Duplicate {
                last_applied,
                incoming,
            } => TransportVerdict::Replay {
                issuer_identity_id: issuer_identity_id.to_string(),
                last_applied,
                incoming,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_package_from_issuer_must_start_at_one() {
        assert_eq!(
            TransportGuard::check("issuer-a", 1, None),
            TransportVerdict::Accept {
                issuer_identity_id: "issuer-a".to_string(),
                sequence: 1,
            }
        );
        assert_eq!(
            TransportGuard::check("issuer-a", 2, None),
            TransportVerdict::OutOfOrder {
                issuer_identity_id: "issuer-a".to_string(),
                expected: 1,
                got: 2,
            }
        );
    }

    #[test]
    fn exact_next_sequence_is_accepted() {
        assert_eq!(
            TransportGuard::check("issuer-a", 6, Some(5)),
            TransportVerdict::Accept {
                issuer_identity_id: "issuer-a".to_string(),
                sequence: 6,
            }
        );
    }

    #[test]
    fn gap_is_out_of_order_and_deferrable() {
        assert_eq!(
            TransportGuard::check("issuer-a", 7, Some(5)),
            TransportVerdict::OutOfOrder {
                issuer_identity_id: "issuer-a".to_string(),
                expected: 6,
                got: 7,
            }
        );
    }

    #[test]
    fn replay_of_applied_sequence_is_rejected() {
        assert_eq!(
            TransportGuard::check("issuer-a", 5, Some(5)),
            TransportVerdict::Replay {
                issuer_identity_id: "issuer-a".to_string(),
                last_applied: 5,
                incoming: 5,
            }
        );
        assert_eq!(
            TransportGuard::check("issuer-a", 3, Some(5)),
            TransportVerdict::Replay {
                issuer_identity_id: "issuer-a".to_string(),
                last_applied: 5,
                incoming: 3,
            }
        );
    }

    #[test]
    fn sequences_are_scoped_per_issuer() {
        assert_eq!(
            TransportGuard::check("issuer-a", 6, Some(5)),
            TransportVerdict::Accept {
                issuer_identity_id: "issuer-a".to_string(),
                sequence: 6,
            }
        );
        // A fresh issuer still must start at 1 regardless of other issuers' progress.
        assert_eq!(
            TransportGuard::check("issuer-b", 1, None),
            TransportVerdict::Accept {
                issuer_identity_id: "issuer-b".to_string(),
                sequence: 1,
            }
        );
    }

    #[test]
    fn rfc_343_sequence_is_consistent_across_kinds() {
        // RFC §3.4.3: packages 100..103 arrive in consistent transport order even though
        // their credentials (A gen 7, B gen 3) are independent. Transport ordering is
        // per-issuer and must accept a contiguous run across mixed package kinds.
        let mut last: Option<u64> = None;
        for s in 1..=4u64 {
            let verdict = TransportGuard::check("issuer-a", s, last);
            assert!(
                matches!(verdict, TransportVerdict::Accept { .. }),
                "package sequence {s} should be contiguous"
            );
            last = Some(s);
        }
    }
}
