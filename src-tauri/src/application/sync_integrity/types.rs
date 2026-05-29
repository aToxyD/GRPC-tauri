use serde::{Deserialize, Serialize};
use std::fmt;

/// Fiscal scope of a sync operation or conflict.
///
/// Attached to every conflict to ensure auditability and
/// traceability to a specific fiscal period.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FiscalScope {
    pub from_year: Option<i32>,
    pub to_year: Option<i32>,
}

impl FiscalScope {
    pub fn unknown() -> Self {
        Self {
            from_year: None,
            to_year: None,
        }
    }

    pub fn single_year(year: i32) -> Self {
        Self {
            from_year: Some(year),
            to_year: Some(year),
        }
    }

    pub fn range(from: i32, to: i32) -> Self {
        Self {
            from_year: Some(from),
            to_year: Some(to),
        }
    }
}

impl fmt::Display for FiscalScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.from_year, self.to_year) {
            (Some(fy), Some(ty)) if fy == ty => write!(f, "FY{}", fy),
            (Some(fy), Some(ty)) => write!(f, "FY{}-FY{}", fy, ty),
            _ => write!(f, "unknown"),
        }
    }
}

/// Deterministic conflict identifier.
///
/// Produced from a SHA-256 hash of (package_id, conflict_type, metadata_json)
/// ensuring reproducibility: same input always produces the same ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConflictId(pub String);

/// Human-readable explanation of a conflict for audit and operator review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictExplanation {
    /// Short machine-readable label
    pub label: String,
    /// Longer human-readable description in English
    pub description: String,
}

/// Reproducible metadata attached to every conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictMetadata {
    /// Deterministic conflict identifier
    pub conflict_id: ConflictId,
    /// The package that triggered the conflict
    pub package_id: String,
    /// The source node that sent the package
    pub source_node_id: String,
    /// The target node evaluating the import
    pub target_node_id: String,
    /// Human-readable explanation
    pub explanation: ConflictExplanation,
    /// Affected fiscal scope if known
    pub fiscal_scope: FiscalScope,
    /// Wall-clock independent evidence: set of identifiers proving
    /// the conflict (e.g. applied transition IDs, sequence ranges)
    pub evidence: Vec<String>,
}

/// All possible sync conflict types in the system.
///
/// Each variant carries deterministic, reproducible data enabling
/// exact reconstruction of the conflict decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum SyncConflict {
    /// The stock state in the incoming package diverges from
    /// the current node state. Detected by comparing Merkle-like
    /// stock root hashes.
    DivergentStockState(ConflictMetadata),

    /// Two overlapping inventory mutations from different sources
    /// target the same product in the same window.
    ConflictingInventoryMutation(ConflictMetadata),

    /// The incoming package is older than the last-applied state
    /// for the same source.
    StaleImport(ConflictMetadata),

    /// The same package_id has already been imported (UNIQUE violation).
    DuplicatePackage(ConflictMetadata),

    /// An already-applied fiscal transition is being replayed.
    ReplayAttempt(ConflictMetadata),

    /// An import cannot proceed because prior sequences are missing.
    SequenceGap(ConflictMetadata),
}

impl SyncConflict {
    pub fn conflict_id(&self) -> &ConflictId {
        match self {
            Self::DivergentStockState(m)
            | Self::ConflictingInventoryMutation(m)
            | Self::StaleImport(m)
            | Self::DuplicatePackage(m)
            | Self::ReplayAttempt(m)
            | Self::SequenceGap(m) => &m.conflict_id,
        }
    }

    pub fn conflict_type_str(&self) -> &'static str {
        match self {
            Self::DivergentStockState(_) => "DivergentStockState",
            Self::ConflictingInventoryMutation(_) => "ConflictingInventoryMutation",
            Self::StaleImport(_) => "StaleImport",
            Self::DuplicatePackage(_) => "DuplicatePackage",
            Self::ReplayAttempt(_) => "ReplayAttempt",
            Self::SequenceGap(_) => "SequenceGap",
        }
    }

    pub fn metadata(&self) -> &ConflictMetadata {
        match self {
            Self::DivergentStockState(m)
            | Self::ConflictingInventoryMutation(m)
            | Self::StaleImport(m)
            | Self::DuplicatePackage(m)
            | Self::ReplayAttempt(m)
            | Self::SequenceGap(m) => m,
        }
    }

    pub fn fiscal_scope(&self) -> &FiscalScope {
        &self.metadata().fiscal_scope
    }
}

/// Result of a conflict detection check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictDetectionOutcome {
    NoConflict,
    ConflictDetected(SyncConflict),
}

/// Pre-import validation result.
///
/// All checks happen BEFORE any mutation. If rejected, the
/// rejection reason is fully auditable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreImportValidationResult {
    pub package_id: String,
    pub passed: bool,
    pub replay_check: Option<ConflictDetectionOutcome>,
    pub duplicate_check: Option<ConflictDetectionOutcome>,
    pub stale_check: Option<ConflictDetectionOutcome>,
    pub fiscal_year: Option<i32>,
}

impl PreImportValidationResult {
    pub fn all_checks_passed(&self) -> bool {
        self.passed
            && self
                .replay_check
                .as_ref()
                .is_none_or(|c| matches!(c, ConflictDetectionOutcome::NoConflict))
            && self
                .duplicate_check
                .as_ref()
                .is_none_or(|c| matches!(c, ConflictDetectionOutcome::NoConflict))
            && self
                .stale_check
                .as_ref()
                .is_none_or(|c| matches!(c, ConflictDetectionOutcome::NoConflict))
    }
}

/// Determines ordering for reconciliation comparisons.
///
/// All ordering MUST use:
/// - explicit ORDER BY
/// - deterministic tiebreakers
/// - stable serialization
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// A single ordering criterion for deterministic ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderCriterion {
    pub field: &'static str,
    pub direction: SortDirection,
}

impl OrderCriterion {
    pub fn asc(field: &'static str) -> Self {
        Self {
            field,
            direction: SortDirection::Ascending,
        }
    }

    pub fn desc(field: &'static str) -> Self {
        Self {
            field,
            direction: SortDirection::Descending,
        }
    }
}

/// Default ordering for sync reconciliation operations.
///
/// Never relies on:
/// - HashMap iteration order
/// - timestamps alone
/// - insertion order without explicit sequence
pub fn default_reconciliation_ordering() -> Vec<OrderCriterion> {
    vec![
        OrderCriterion::asc("transaction_id"),
        OrderCriterion::asc("sequence_number"),
    ]
}

/// Stock state fingerprint for reconciliation comparison.
///
/// A deterministic hash of the stock state at a point in time.
/// Two nodes with identical import histories MUST produce
/// identical fingerprints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockStateFingerprint {
    pub product_id: String,
    pub current_quantity: f64,
    pub last_mutation_sequence: u64,
    pub fingerprint_hash: String,
}

/// A window of sequence numbers for ordering comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SequenceWindow {
    pub transaction_id: String,
    pub sequence_start: u64,
    pub sequence_end: u64,
    pub is_contiguous: bool,
}

impl SequenceWindow {
    pub fn new(transaction_id: String, start: u64, end: u64) -> Self {
        Self {
            transaction_id,
            sequence_start: start,
            sequence_end: end,
            is_contiguous: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fiscal_scope_display_single_year() {
        let scope = FiscalScope::single_year(2025);
        assert_eq!(scope.to_string(), "FY2025");
    }

    #[test]
    fn fiscal_scope_display_range() {
        let scope = FiscalScope::range(2024, 2025);
        assert_eq!(scope.to_string(), "FY2024-FY2025");
    }

    #[test]
    fn fiscal_scope_display_unknown() {
        let scope = FiscalScope::unknown();
        assert_eq!(scope.to_string(), "unknown");
    }

    #[test]
    fn conflict_id_hash_determinism() {
        let id1 = ConflictId("abc123".into());
        let id2 = ConflictId("abc123".into());
        assert_eq!(id1, id2);
        let id3 = ConflictId("def456".into());
        assert_ne!(id1, id3);
    }

    #[test]
    fn conflict_metadata_round_trip() {
        let meta = ConflictMetadata {
            conflict_id: ConflictId("test-id".into()),
            package_id: "pkg-1".into(),
            source_node_id: "node-a".into(),
            target_node_id: "node-b".into(),
            explanation: ConflictExplanation {
                label: "REPLAY".into(),
                description: "Transition already applied".into(),
            },
            fiscal_scope: FiscalScope::single_year(2025),
            evidence: vec!["txn-123".into()],
        };
        let json = serde_json::to_string(&meta).unwrap();
        let restored: ConflictMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.conflict_id, meta.conflict_id);
        assert_eq!(restored.explanation.label, "REPLAY");
    }

    #[test]
    fn sync_conflict_serde_round_trip_all_variants() {
        let variants = vec![
            SyncConflict::DivergentStockState(conflict_meta("divergent")),
            SyncConflict::ConflictingInventoryMutation(conflict_meta("conflicting")),
            SyncConflict::StaleImport(conflict_meta("stale")),
            SyncConflict::DuplicatePackage(conflict_meta("duplicate")),
            SyncConflict::ReplayAttempt(conflict_meta("replay")),
            SyncConflict::SequenceGap(conflict_meta("gap")),
        ];

        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let restored: SyncConflict = serde_json::from_str(&json).unwrap();
            assert_eq!(v.conflict_type_str(), restored.conflict_type_str());
            assert_eq!(v.conflict_id(), restored.conflict_id());
        }
    }

    #[test]
    fn conflict_type_str_is_correct() {
        assert_eq!(
            SyncConflict::DivergentStockState(conflict_meta("x")).conflict_type_str(),
            "DivergentStockState"
        );
        assert_eq!(
            SyncConflict::SequenceGap(conflict_meta("x")).conflict_type_str(),
            "SequenceGap"
        );
    }

    #[test]
    fn pre_import_validation_passes_when_all_clear() {
        let result = PreImportValidationResult {
            package_id: "pkg-1".into(),
            passed: true,
            replay_check: Some(ConflictDetectionOutcome::NoConflict),
            duplicate_check: Some(ConflictDetectionOutcome::NoConflict),
            stale_check: Some(ConflictDetectionOutcome::NoConflict),
            fiscal_year: Some(2025),
        };
        assert!(result.all_checks_passed());
    }

    #[test]
    fn pre_import_validation_fails_on_conflict() {
        let conflict = SyncConflict::ReplayAttempt(conflict_meta("replay"));
        let result = PreImportValidationResult {
            package_id: "pkg-1".into(),
            passed: false,
            replay_check: Some(ConflictDetectionOutcome::ConflictDetected(conflict)),
            duplicate_check: None,
            stale_check: None,
            fiscal_year: Some(2025),
        };
        assert!(!result.all_checks_passed());
    }

    #[test]
    fn order_criterion_asc_creates_correct_sort() {
        let c = OrderCriterion::asc("sequence_number");
        assert_eq!(c.field, "sequence_number");
        assert_eq!(c.direction, SortDirection::Ascending);
    }

    #[test]
    fn default_reconciliation_ordering_is_stable() {
        let order1 = default_reconciliation_ordering();
        let order2 = default_reconciliation_ordering();
        assert_eq!(order1, order2);
        assert_eq!(order1.len(), 2);
        assert_eq!(order1[0].field, "transaction_id");
        assert_eq!(order1[1].field, "sequence_number");
    }

    #[test]
    fn stock_fingerprint_equality() {
        let fp1 = StockStateFingerprint {
            product_id: "prod-1".into(),
            current_quantity: 100.0,
            last_mutation_sequence: 42,
            fingerprint_hash: "abc".into(),
        };
        let fp2 = StockStateFingerprint {
            fingerprint_hash: "abc".into(),
            ..fp1.clone()
        };
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn sequence_window_construction() {
        let w = SequenceWindow::new("txn-1".into(), 1, 5);
        assert_eq!(w.transaction_id, "txn-1");
        assert_eq!(w.sequence_start, 1);
        assert_eq!(w.sequence_end, 5);
        assert!(w.is_contiguous);
    }

    fn conflict_meta(label: &str) -> ConflictMetadata {
        ConflictMetadata {
            conflict_id: ConflictId(format!("id-{}", label)),
            package_id: format!("pkg-{}", label),
            source_node_id: "node-a".into(),
            target_node_id: "node-b".into(),
            explanation: ConflictExplanation {
                label: label.into(),
                description: format!("Conflict: {}", label),
            },
            fiscal_scope: FiscalScope::unknown(),
            evidence: vec![],
        }
    }
}
