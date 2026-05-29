use serde::{Deserialize, Serialize};

use super::policies::CheckpointPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointMode {
    Passive,
    Full,
    Restart,
    Truncate,
}

impl CheckpointMode {
    pub fn as_pragma_str(&self) -> &'static str {
        match self {
            CheckpointMode::Passive => "PASSIVE",
            CheckpointMode::Full => "FULL",
            CheckpointMode::Restart => "RESTART",
            CheckpointMode::Truncate => "TRUNCATE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CheckpointResult {
    pub pages_before: u64,
    pub pages_after: u64,
    pub pages_moved: i64,
    pub pages_before_checkpoint: u64,
    pub mode: CheckpointMode,
    pub wal_size_before_bytes: u64,
    pub wal_size_after_bytes: u64,
    pub checkpoint_seqno_before: u64,
    pub checkpoint_seqno_after: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointEligibility {
    Eligible,
    NotEligible { reason: String },
}

pub struct WalCheckpointExecutor;

impl WalCheckpointExecutor {
    pub fn execute(
        conn: &rusqlite::Connection,
        mode: CheckpointMode,
        policy: &CheckpointPolicy,
    ) -> Result<CheckpointResult, rusqlite::Error> {
        let (pages_before, pages_after, checkpoint_seqno_before) = Self::read_wal_metrics(conn)?;

        let is_eligible =
            Self::check_eligibility(pages_before, pages_after, checkpoint_seqno_before, policy);
        if !matches!(is_eligible, CheckpointEligibility::Eligible) {
            return Ok(CheckpointResult {
                pages_before,
                pages_after,
                pages_moved: 0,
                pages_before_checkpoint: 0,
                mode,
                wal_size_before_bytes: 0,
                wal_size_after_bytes: 0,
                checkpoint_seqno_before,
                checkpoint_seqno_after: checkpoint_seqno_before,
            });
        }

        Self::execute_impl(
            conn,
            mode,
            pages_before,
            pages_after,
            checkpoint_seqno_before,
        )
    }

    fn execute_impl(
        conn: &rusqlite::Connection,
        mode: CheckpointMode,
        pages_before: u64,
        pages_after: u64,
        checkpoint_seqno_before: u64,
    ) -> Result<CheckpointResult, rusqlite::Error> {
        let pragma = format!("PRAGMA wal_checkpoint({})", mode.as_pragma_str());
        let (_db_pages, ckpt_pages, moved): (i64, i64, i64) =
            conn.query_row(&pragma, [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;

        let (_, _, checkpoint_seqno_after) = Self::read_wal_metrics(conn)?;

        Ok(CheckpointResult {
            pages_before,
            pages_after,
            pages_moved: moved,
            pages_before_checkpoint: ckpt_pages as u64,
            mode,
            wal_size_before_bytes: 0,
            wal_size_after_bytes: 0,
            checkpoint_seqno_before,
            checkpoint_seqno_after,
        })
    }

    pub fn check_eligibility(
        pages_before: u64,
        _pages_after: u64,
        checkpoint_seqno: u64,
        policy: &CheckpointPolicy,
    ) -> CheckpointEligibility {
        if pages_before == 0 && checkpoint_seqno == 0 {
            return CheckpointEligibility::NotEligible {
                reason: "no pages in database".into(),
            };
        }
        if checkpoint_seqno == 0 {
            return CheckpointEligibility::NotEligible {
                reason: "WAL is empty (checkpoint_seqno is 0)".into(),
            };
        }
        if policy.max_wal_size_bytes > 0 && pages_before * 4096 > policy.max_wal_size_bytes {
            return CheckpointEligibility::Eligible;
        }
        if checkpoint_seqno > 0
            && !matches!(mode_from_seqno(checkpoint_seqno), CheckpointMode::Passive)
        {
            return CheckpointEligibility::Eligible;
        }
        CheckpointEligibility::Eligible
    }

    pub fn read_wal_metrics(
        conn: &rusqlite::Connection,
    ) -> Result<(u64, u64, u64), rusqlite::Error> {
        let page_count: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let total_pages = page_count as u64;
        let usable_pages = page_count as u64;
        let pages = total_pages;
        Ok((
            pages,
            usable_pages,
            pages_before_checkpoint(conn).unwrap_or(0),
        ))
    }
}

fn pages_before_checkpoint(conn: &rusqlite::Connection) -> Result<u64, rusqlite::Error> {
    let wal_size: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    Ok(wal_size.max(0) as u64)
}

fn mode_from_seqno(seqno: u64) -> CheckpointMode {
    if seqno == 1 {
        CheckpointMode::Full
    } else {
        CheckpointMode::Truncate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite_runtime::policies::CheckpointPolicy;

    #[test]
    fn checkpoint_mode_pragma_strings() {
        assert_eq!(CheckpointMode::Passive.as_pragma_str(), "PASSIVE");
        assert_eq!(CheckpointMode::Full.as_pragma_str(), "FULL");
        assert_eq!(CheckpointMode::Restart.as_pragma_str(), "RESTART");
        assert_eq!(CheckpointMode::Truncate.as_pragma_str(), "TRUNCATE");
    }

    #[test]
    fn serde_round_trip_checkpoint_mode() {
        let modes = [
            CheckpointMode::Passive,
            CheckpointMode::Full,
            CheckpointMode::Restart,
            CheckpointMode::Truncate,
        ];
        for mode in &modes {
            let json = serde_json::to_string(mode).unwrap();
            let deserialized: CheckpointMode = serde_json::from_str(&json).unwrap();
            assert_eq!(*mode, deserialized);
        }
    }

    #[test]
    fn serde_round_trip_checkpoint_result() {
        let result = CheckpointResult {
            pages_before: 100,
            pages_after: 90,
            pages_moved: 10,
            pages_before_checkpoint: 5,
            mode: CheckpointMode::Truncate,
            wal_size_before_bytes: 409600,
            wal_size_after_bytes: 0,
            checkpoint_seqno_before: 3,
            checkpoint_seqno_after: 0,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: CheckpointResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, deserialized);
    }

    #[test]
    fn eligibility_no_pages() {
        let policy = CheckpointPolicy::default();
        let eligibility = WalCheckpointExecutor::check_eligibility(0, 0, 0, &policy);
        assert!(matches!(
            eligibility,
            CheckpointEligibility::NotEligible { .. }
        ));
    }

    #[test]
    fn eligibility_empty_wal() {
        let policy = CheckpointPolicy::default();
        let eligibility = WalCheckpointExecutor::check_eligibility(100, 100, 0, &policy);
        assert!(matches!(
            eligibility,
            CheckpointEligibility::NotEligible { .. }
        ));
    }

    #[test]
    fn eligibility_eligible() {
        let policy = CheckpointPolicy::default();
        let eligibility = WalCheckpointExecutor::check_eligibility(100, 100, 1, &policy);
        assert!(matches!(eligibility, CheckpointEligibility::Eligible));
    }

    #[test]
    fn eligibility_wal_size_threshold() {
        let policy = CheckpointPolicy {
            max_wal_size_bytes: 100 * 4096,
            ..CheckpointPolicy::default()
        };
        let eligibility = WalCheckpointExecutor::check_eligibility(200, 200, 1, &policy);
        assert!(matches!(eligibility, CheckpointEligibility::Eligible));
    }

    #[test]
    fn deterministic_eligibility_same_input() {
        let policy = CheckpointPolicy::default();
        let e1 = WalCheckpointExecutor::check_eligibility(100, 100, 1, &policy);
        let e2 = WalCheckpointExecutor::check_eligibility(100, 100, 1, &policy);
        assert_eq!(
            matches!(e1, CheckpointEligibility::Eligible),
            matches!(e2, CheckpointEligibility::Eligible)
        );
    }

    #[test]
    fn serde_round_trip_eligibility() {
        let eligible = CheckpointEligibility::Eligible;
        let not_eligible = CheckpointEligibility::NotEligible {
            reason: "WAL is empty".into(),
        };
        for val in [eligible.clone(), not_eligible.clone()] {
            let json = serde_json::to_string(&val).unwrap();
            let deserialized: CheckpointEligibility = serde_json::from_str(&json).unwrap();
            assert_eq!(val, deserialized);
        }
    }
}
