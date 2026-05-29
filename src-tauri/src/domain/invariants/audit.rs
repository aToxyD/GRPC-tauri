use std::fmt;

use crate::domain::invariants::Invariant;

/// Violation raised when the audit hash chain is broken or inconsistent.
#[derive(Debug, Clone, PartialEq)]
pub enum AuditChainIntegrityViolation {
    /// An entry's previous_hash does not match the prior entry's entry_hash.
    LinkageBreak {
        entry_id: String,
        index: usize,
        expected_previous: Option<String>,
        actual_previous: Option<String>,
    },
    /// A non-legacy entry is missing its entry_hash.
    MissingEntryHash {
        entry_id: String,
        index: usize,
        has_previous_hash: bool,
    },
}

impl fmt::Display for AuditChainIntegrityViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditChainIntegrityViolation::LinkageBreak {
                entry_id,
                index,
                expected_previous,
                actual_previous,
            } => {
                write!(
                    f,
                    "audit chain linkage break at index {}: entry {} expected previous_hash {:?}, got {:?}",
                    index, entry_id, expected_previous, actual_previous
                )
            }
            AuditChainIntegrityViolation::MissingEntryHash {
                entry_id,
                index,
                has_previous_hash,
            } => {
                write!(
                    f,
                    "audit entry {} at index {} has previous_hash={} but missing entry_hash",
                    entry_id, index, has_previous_hash
                )
            }
        }
    }
}

/// A single entry in the audit chain for verification purposes.
#[derive(Debug, Clone)]
pub struct ChainEntry {
    pub id: String,
    pub previous_hash: Option<String>,
    pub entry_hash: Option<String>,
}

/// Input context for the AuditChainIntegrity invariant.
#[derive(Debug, Clone)]
pub struct AuditChainContext {
    pub entries: Vec<ChainEntry>,
}

/// Invariant: The audit hash chain must be intact and self-consistent.
///
/// Checks:
/// - Each chained entry's `previous_hash` matches the preceding entry's `entry_hash`.
/// - No entry references a previous hash without providing its own entry_hash.
pub struct AuditChainIntegrity;

impl Invariant for AuditChainIntegrity {
    type Context = AuditChainContext;
    type Violation = AuditChainIntegrityViolation;

    fn check(ctx: &AuditChainContext) -> Vec<AuditChainIntegrityViolation> {
        let mut violations = Vec::new();
        let mut last_entry_hash: Option<&str> = None;

        for (i, entry) in ctx.entries.iter().enumerate() {
            let has_previous = entry.previous_hash.is_some();
            let has_own = entry.entry_hash.is_some();

            if has_previous && !has_own {
                violations.push(AuditChainIntegrityViolation::MissingEntryHash {
                    entry_id: entry.id.clone(),
                    index: i,
                    has_previous_hash: true,
                });
                continue;
            }

            if has_own {
                if has_previous {
                    let expected_prev = last_entry_hash;
                    let actual_prev = entry.previous_hash.as_deref();

                    if actual_prev != expected_prev {
                        violations.push(AuditChainIntegrityViolation::LinkageBreak {
                            entry_id: entry.id.clone(),
                            index: i,
                            expected_previous: expected_prev.map(|s| s.to_string()),
                            actual_previous: actual_prev.map(|s| s.to_string()),
                        });
                    }
                }

                last_entry_hash = Some(entry.entry_hash.as_deref().unwrap());
            }
        }

        violations
    }
}

/// Convenience: check that an audit chain has integrity.
pub fn check_audit_chain_integrity(entries: Vec<ChainEntry>) -> Vec<AuditChainIntegrityViolation> {
    let ctx = AuditChainContext { entries };
    AuditChainIntegrity::check(&ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, previous: Option<&str>, current: Option<&str>) -> ChainEntry {
        ChainEntry {
            id: id.to_string(),
            previous_hash: previous.map(|s| s.to_string()),
            entry_hash: current.map(|s| s.to_string()),
        }
    }

    #[test]
    fn single_chained_entry_is_valid() {
        let entries = vec![entry("a", None, Some("hash-a"))];
        assert!(check_audit_chain_integrity(entries).is_empty());
    }

    #[test]
    fn two_linked_entries_is_valid() {
        let entries = vec![
            entry("a", None, Some("hash-a")),
            entry("b", Some("hash-a"), Some("hash-b")),
        ];
        assert!(check_audit_chain_integrity(entries).is_empty());
    }

    #[test]
    fn three_linked_entries_is_valid() {
        let entries = vec![
            entry("a", None, Some("hash-a")),
            entry("b", Some("hash-a"), Some("hash-b")),
            entry("c", Some("hash-b"), Some("hash-c")),
        ];
        assert!(check_audit_chain_integrity(entries).is_empty());
    }

    #[test]
    fn linkage_break_detected() {
        let entries = vec![
            entry("a", None, Some("hash-a")),
            entry("b", Some("wrong-prev"), Some("hash-b")),
        ];
        let v = check_audit_chain_integrity(entries);
        assert!(!v.is_empty());
        match &v[0] {
            AuditChainIntegrityViolation::LinkageBreak {
                entry_id, index, ..
            } => {
                assert_eq!(entry_id, "b");
                assert_eq!(*index, 1);
            }
            other => panic!("expected LinkageBreak, got {:?}", other),
        }
    }

    #[test]
    fn missing_entry_hash_with_previous_detected() {
        let entries = vec![
            entry("a", None, Some("hash-a")),
            entry("b", Some("hash-a"), None),
        ];
        let v = check_audit_chain_integrity(entries);
        assert!(!v.is_empty());
        match &v[0] {
            AuditChainIntegrityViolation::MissingEntryHash {
                entry_id,
                index,
                has_previous_hash,
            } => {
                assert_eq!(entry_id, "b");
                assert_eq!(*index, 1);
                assert!(*has_previous_hash);
            }
            other => panic!("expected MissingEntryHash, got {:?}", other),
        }
    }

    #[test]
    fn legacy_rows_are_allowed() {
        let entries = vec![entry("legacy-1", None, None), entry("legacy-2", None, None)];
        assert!(check_audit_chain_integrity(entries).is_empty());
    }

    #[test]
    fn chain_after_legacy_rows_is_valid() {
        let entries = vec![
            entry("legacy", None, None),
            entry("a", None, Some("hash-a")),
            entry("b", Some("hash-a"), Some("hash-b")),
        ];
        assert!(check_audit_chain_integrity(entries).is_empty());
    }

    #[test]
    fn empty_entries_returns_empty() {
        assert!(check_audit_chain_integrity(vec![]).is_empty());
    }

    #[test]
    fn disconnected_chained_entries_are_valid_as_separate_segments() {
        let entries = vec![
            entry("a", None, Some("hash-a")),
            entry("b", None, Some("hash-b")),
        ];
        assert!(check_audit_chain_integrity(entries).is_empty());
    }

    #[test]
    fn legacy_then_chained_then_legacy_then_chained_is_valid() {
        let entries = vec![
            entry("legacy-1", None, None),
            entry("a", None, Some("hash-a")),
            entry("b", Some("hash-a"), Some("hash-b")),
            entry("legacy-2", None, None),
            entry("c", None, Some("hash-c")),
        ];
        assert!(check_audit_chain_integrity(entries).is_empty());
    }
}
