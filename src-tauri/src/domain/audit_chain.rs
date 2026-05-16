//! Lightweight hash chain over `audit_log` rows for tamper-evidence (not a blockchain).
//!
//! Each new row stores `previous_hash` (digest of the prior chained row) and `entry_hash`
//! (SHA-256 over a stable, pipe-delimited projection of the entry). Legacy rows may have
//! both columns NULL; verification skips them but continues the chain after the last
//! chained row.

use crate::domain::audit::NewAuditEntry;
use crate::errors::{AppError, AppResult};
use sha2::{Digest, Sha256};

/// Deterministic digest for one audit row given the previous row's `entry_hash` (if any).
pub fn compute_entry_hash(previous_entry_hash: Option<&str>, entry: &NewAuditEntry) -> String {
    let prev = previous_entry_hash.unwrap_or("");
    let payload = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        prev,
        entry.id,
        entry.user_id,
        entry.username,
        entry.action,
        entry.entity_type,
        entry.entity_id.as_deref().unwrap_or(""),
        entry.entity_name.as_deref().unwrap_or(""),
        entry.timestamp,
        entry.status,
        entry.error_message.as_deref().unwrap_or(""),
        entry.session_id.as_deref().unwrap_or(""),
        entry.old_value.as_deref().unwrap_or(""),
        entry.new_value.as_deref().unwrap_or(""),
        entry.metadata.as_deref().unwrap_or(""),
    );
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Row shape for [`verify_chain`].
#[derive(Debug, Clone)]
pub struct AuditChainVerifyRow {
    pub previous_hash: Option<String>,
    pub entry_hash: Option<String>,
    pub entry: NewAuditEntry,
}

/// Walk rows in storage order (`rowid`); verify recomputed digest matches `entry_hash`.
///
/// Also enforces **strict consecutive linkage**: for each chained row, `previous_hash` must
/// equal the prior chained row's `entry_hash` (after normalizing `None` ↔ empty string),
/// catching row reorder / splice attempts that leave per-row digests self-consistent.
pub fn verify_chain(rows: &[AuditChainVerifyRow]) -> AppResult<()> {
    let mut last_entry_hash: Option<String> = None;
    for (i, row) in rows.iter().enumerate() {
        let Some(stored) = row.entry_hash.as_deref() else {
            continue;
        };
        let expected_prev_key = last_entry_hash.as_deref().unwrap_or("");
        let actual_prev = row.previous_hash.as_deref().unwrap_or("");
        if actual_prev != expected_prev_key {
            return Err(AppError::Internal(format!(
                "audit chain linkage break at index {}: id {}",
                i, row.entry.id
            )));
        }
        let expected = compute_entry_hash(row.previous_hash.as_deref(), &row.entry);
        if expected != stored {
            return Err(AppError::Internal(format!(
                "audit chain break at index {}: recomputed hash != stored entry_hash",
                i
            )));
        }
        last_entry_hash = Some(stored.to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationSummary {
    pub is_valid: bool,
    pub verified_entries: i64,
    pub first_broken_entry_id: Option<String>,
    pub break_description: Option<String>,
    pub latest_entry_timestamp: Option<String>,
}

/// Verify the chain in a streaming fashion (constant memory O(1))
pub fn verify_chain_stream<I>(rows: I) -> AppResult<VerificationSummary>
where
    I: Iterator<Item = AppResult<AuditChainVerifyRow>>,
{
    let mut verified_entries = 0;
    let mut latest_entry_timestamp = None;
    let mut last_entry_hash: Option<String> = None;

    for (i, row_res) in rows.enumerate() {
        let row = row_res?;
        latest_entry_timestamp = Some(row.entry.timestamp.clone());

        let Some(stored) = row.entry_hash.as_deref() else {
            continue;
        };

        let expected_prev_key = last_entry_hash.as_deref().unwrap_or("");
        let actual_prev = row.previous_hash.as_deref().unwrap_or("");
        if actual_prev != expected_prev_key {
            let msg = format!(
                "audit chain linkage break at index {}: expected previous_hash {:?} got {:?} for id {}",
                i,
                if expected_prev_key.is_empty() {
                    None::<&str>
                } else {
                    Some(expected_prev_key)
                },
                row.previous_hash,
                row.entry.id
            );
            return Ok(VerificationSummary {
                is_valid: false,
                verified_entries,
                first_broken_entry_id: Some(row.entry.id.clone()),
                break_description: Some(msg),
                latest_entry_timestamp,
            });
        }

        let expected = compute_entry_hash(row.previous_hash.as_deref(), &row.entry);
        if expected != stored {
            let msg = format!(
                "audit chain break at index {}: recomputed hash != stored entry_hash for id {}",
                i, row.entry.id
            );
            return Ok(VerificationSummary {
                is_valid: false,
                verified_entries,
                first_broken_entry_id: Some(row.entry.id.clone()),
                break_description: Some(msg),
                latest_entry_timestamp,
            });
        }
        verified_entries += 1;
        last_entry_hash = Some(stored.to_string());
    }

    Ok(VerificationSummary {
        is_valid: true,
        verified_entries,
        first_broken_entry_id: None,
        break_description: None,
        latest_entry_timestamp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::audit::NewAuditEntry;

    fn stub(id: &str) -> NewAuditEntry {
        NewAuditEntry {
            id: id.to_string(),
            user_id: "u1".into(),
            username: "user".into(),
            action: "Login".into(),
            entity_type: "User".into(),
            entity_id: None,
            entity_name: None,
            old_value: None,
            new_value: None,
            session_id: None,
            timestamp: "2026-05-01T12:00:00Z".into(),
            status: "Success".into(),
            error_message: None,
            metadata: None,
            previous_hash: None,
            entry_hash: None,
        }
    }

    #[test]
    fn chain_verifies_two_linked_rows() {
        let e1 = stub("a");
        let h1 = compute_entry_hash(None, &e1);
        let e2 = stub("b");
        let h2 = compute_entry_hash(Some(&h1), &e2);
        let rows = vec![
            AuditChainVerifyRow {
                previous_hash: None,
                entry_hash: Some(h1.clone()),
                entry: e1,
            },
            AuditChainVerifyRow {
                previous_hash: Some(h1),
                entry_hash: Some(h2),
                entry: e2,
            },
        ];
        verify_chain(&rows).unwrap();
    }

    #[test]
    fn chain_rejects_wrong_linkage_even_if_row_digest_self_consistent() {
        let e1 = stub("a");
        let h1 = compute_entry_hash(None, &e1);
        let e2 = stub("b");
        let h2 = compute_entry_hash(Some(&h1), &e2);
        // Second row stored first: self-consistent hash for e2 but previous_hash does not match "".
        let rows = vec![
            AuditChainVerifyRow {
                previous_hash: Some(h1.clone()),
                entry_hash: Some(h2),
                entry: e2,
            },
            AuditChainVerifyRow {
                previous_hash: None,
                entry_hash: Some(h1),
                entry: e1,
            },
        ];
        assert!(verify_chain(&rows).is_err());
    }

    #[test]
    fn chain_rejects_tampered_entry_hash() {
        let e1 = stub("a");
        let h1 = compute_entry_hash(None, &e1);
        let e2 = stub("b");
        let rows = vec![
            AuditChainVerifyRow {
                previous_hash: None,
                entry_hash: Some(h1),
                entry: e1,
            },
            AuditChainVerifyRow {
                previous_hash: None,
                entry_hash: Some("deadbeef".into()),
                entry: e2,
            },
        ];
        assert!(verify_chain(&rows).is_err());
    }
}
