use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use super::types::{
    ConflictDetectionOutcome, ConflictExplanation, ConflictId, ConflictMetadata, FiscalScope,
    SyncConflict,
};

/// Set of already-applied package IDs.
///
/// Used as input to replay detection. The caller supplies the
/// current state (from DB); the detector is a pure function.
pub struct AppliedPackages {
    ids: BTreeSet<String>,
}

impl AppliedPackages {
    pub fn new(ids: BTreeSet<String>) -> Self {
        Self { ids }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }
}

/// Set of already-applied fiscal transition IDs.
pub struct AppliedTransitions {
    ids: BTreeSet<String>,
}

impl AppliedTransitions {
    pub fn new(ids: BTreeSet<String>) -> Self {
        Self { ids }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }
}

/// The set of previously seen transaction IDs.
pub struct SeenTransactions {
    ids: BTreeSet<String>,
}

impl SeenTransactions {
    pub fn new(ids: BTreeSet<String>) -> Self {
        Self { ids }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }
}

/// Deterministic replay detection state.
///
/// All inputs are provided as immutable state slices. No mutation
/// occurs inside the detector.
pub struct ReplayDetector {
    applied_packages: AppliedPackages,
    applied_transitions: AppliedTransitions,
    seen_transactions: SeenTransactions,
    source_node_id: String,
    target_node_id: String,
}

impl ReplayDetector {
    pub fn new(
        applied_packages: AppliedPackages,
        applied_transitions: AppliedTransitions,
        seen_transactions: SeenTransactions,
        source_node_id: String,
        target_node_id: String,
    ) -> Self {
        Self {
            applied_packages,
            applied_transitions,
            seen_transactions,
            source_node_id,
            target_node_id,
        }
    }

    /// Check if a package_id has already been imported.
    ///
    /// Replay rejection is deterministic and auditable.
    /// Detection occurs BEFORE any mutation.
    pub fn check_package_id(&self, package_id: &str) -> ConflictDetectionOutcome {
        if self.applied_packages.contains(package_id) {
            let evidence = vec![format!("already_applied:{}", package_id)];
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::DuplicatePackage(
                self.build_metadata(package_id, "DuplicatePackage", evidence),
            ))
        } else {
            ConflictDetectionOutcome::NoConflict
        }
    }

    /// Check if a fiscal transition has already been applied.
    ///
    /// Transition uniqueness is enforced at the application layer,
    /// not at the DB constraint level.
    pub fn check_transition(&self, transition_id: &str, package_id: &str) -> ConflictDetectionOutcome {
        if self.applied_transitions.contains(transition_id) {
            let evidence = vec![format!("transition_already_applied:{}", transition_id)];
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::ReplayAttempt(
                self.build_metadata(package_id, "ReplayAttempt", evidence),
            ))
        } else {
            ConflictDetectionOutcome::NoConflict
        }
    }

    /// Check if a transaction ID has already been seen (replay detection
    /// at the transaction level).
    pub fn check_transaction(&self, transaction_id: &str, package_id: &str) -> ConflictDetectionOutcome {
        if self.seen_transactions.contains(transaction_id) {
            let evidence = vec![format!("transaction_already_seen:{}", transaction_id)];
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::ReplayAttempt(
                self.build_metadata(package_id, "ReplayAttempt", evidence),
            ))
        } else {
            ConflictDetectionOutcome::NoConflict
        }
    }

    /// Check if an incoming sequence window has already been applied.
    ///
    /// This is a stronger check than package_id alone: it verifies
    /// that the *content* (transaction + sequence range) has not
    /// been replayed.
    pub fn check_applied_sequence(
        &self,
        transaction_id: &str,
        sequence_number: u64,
        package_id: &str,
    ) -> ConflictDetectionOutcome {
        let key = format!("{}:{}", transaction_id, sequence_number);
        if self.seen_transactions.contains(&key) {
            let evidence = vec![
                format!("transaction:{}", transaction_id),
                format!("sequence:{}", sequence_number),
            ];
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::ReplayAttempt(
                self.build_metadata(package_id, "ReplayAttempt", evidence),
            ))
        } else {
            ConflictDetectionOutcome::NoConflict
        }
    }

    /// Run all replay checks for a given package.
    ///
    /// Returns the first conflict detected, or NoConflict.
    pub fn check_all(
        &self,
        package_id: &str,
        transition_ids: &[String],
        transaction_ids: &[String],
    ) -> ConflictDetectionOutcome {
        let pkg_check = self.check_package_id(package_id);
        if !matches!(pkg_check, ConflictDetectionOutcome::NoConflict) {
            return pkg_check;
        }

        for tid in transition_ids {
            let tx_check = self.check_transition(tid, package_id);
            if !matches!(tx_check, ConflictDetectionOutcome::NoConflict) {
                return tx_check;
            }
        }

        for txn_id in transaction_ids {
            let txn_check = self.check_transaction(txn_id, package_id);
            if !matches!(txn_check, ConflictDetectionOutcome::NoConflict) {
                return txn_check;
            }
        }

        ConflictDetectionOutcome::NoConflict
    }

    fn build_metadata(
        &self,
        package_id: &str,
        label: &str,
        evidence: Vec<String>,
    ) -> ConflictMetadata {
        let id_input = format!("{}:{}:{}:{}", package_id, label, self.source_node_id, evidence.join(","));
        let hash = hex::encode(Sha256::digest(id_input.as_bytes()));

        ConflictMetadata {
            conflict_id: ConflictId(hash),
            package_id: package_id.to_string(),
            source_node_id: self.source_node_id.clone(),
            target_node_id: self.target_node_id.clone(),
            explanation: ConflictExplanation {
                label: label.to_string(),
                description: format!("{}: package={} source={}", label, package_id, self.source_node_id),
            },
            fiscal_scope: FiscalScope::unknown(),
            evidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_detector() -> ReplayDetector {
        ReplayDetector::new(
            AppliedPackages::new(BTreeSet::new()),
            AppliedTransitions::new(BTreeSet::new()),
            SeenTransactions::new(BTreeSet::new()),
            "node-a".into(),
            "node-b".into(),
        )
    }

    fn detector_with_package(package_id: &str) -> ReplayDetector {
        let mut pkgs = BTreeSet::new();
        pkgs.insert(package_id.to_string());
        ReplayDetector::new(
            AppliedPackages::new(pkgs),
            AppliedTransitions::new(BTreeSet::new()),
            SeenTransactions::new(BTreeSet::new()),
            "node-a".into(),
            "node-b".into(),
        )
    }

    fn detector_with_transition(transition_id: &str) -> ReplayDetector {
        let mut txs = BTreeSet::new();
        txs.insert(transition_id.to_string());
        ReplayDetector::new(
            AppliedPackages::new(BTreeSet::new()),
            AppliedTransitions::new(txs),
            SeenTransactions::new(BTreeSet::new()),
            "node-a".into(),
            "node-b".into(),
        )
    }

    #[test]
    fn fresh_package_passes_replay_check() {
        let d = empty_detector();
        let result = d.check_package_id("pkg-new");
        assert!(matches!(result, ConflictDetectionOutcome::NoConflict));
    }

    #[test]
    fn duplicate_package_detected_as_replay() {
        let d = detector_with_package("pkg-dup");
        let result = d.check_package_id("pkg-dup");
        match result {
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::DuplicatePackage(meta)) => {
                assert_eq!(meta.package_id, "pkg-dup");
                assert_eq!(meta.explanation.label, "DuplicatePackage");
                assert!(meta.evidence[0].contains("already_applied"));
            }
            _ => panic!("expected DuplicatePackage conflict"),
        }
    }

    #[test]
    fn applied_transition_detected_as_replay() {
        let d = detector_with_transition("txn-100");
        let result = d.check_transition("txn-100", "pkg-1");
        match result {
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::ReplayAttempt(meta)) => {
                assert_eq!(meta.explanation.label, "ReplayAttempt");
                assert!(meta.evidence[0].contains("transition_already_applied"));
            }
            _ => panic!("expected ReplayAttempt conflict"),
        }
    }

    #[test]
    fn seen_transaction_detected_as_replay() {
        let mut seen = BTreeSet::new();
        seen.insert("txn-abc".into());
        let d = ReplayDetector::new(
            AppliedPackages::new(BTreeSet::new()),
            AppliedTransitions::new(BTreeSet::new()),
            SeenTransactions::new(seen),
            "node-a".into(),
            "node-b".into(),
        );
        let result = d.check_transaction("txn-abc", "pkg-1");
        assert!(matches!(
            result,
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::ReplayAttempt(_))
        ));
    }

    #[test]
    fn check_all_returns_first_conflict() {
        let mut pkgs = BTreeSet::new();
        pkgs.insert("pkg-dup".into());
        let d = ReplayDetector::new(
            AppliedPackages::new(pkgs),
            AppliedTransitions::new(BTreeSet::new()),
            SeenTransactions::new(BTreeSet::new()),
            "node-a".into(),
            "node-b".into(),
        );

        let result = d.check_all("pkg-dup", &["txn-1".into()], &["txn-abc".into()]);
        assert!(matches!(
            result,
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::DuplicatePackage(_))
        ));
    }

    #[test]
    fn check_all_no_conflict() {
        let d = empty_detector();
        let result = d.check_all("pkg-fresh", &[], &[]);
        assert!(matches!(result, ConflictDetectionOutcome::NoConflict));
    }

    #[test]
    fn deterministic_conflict_id_for_same_input() {
        let d = detector_with_package("pkg-x");
        let r1 = d.check_package_id("pkg-x");
        let r2 = d.check_package_id("pkg-x");
        match (r1, r2) {
            (
                ConflictDetectionOutcome::ConflictDetected(c1),
                ConflictDetectionOutcome::ConflictDetected(c2),
            ) => {
                assert_eq!(c1.conflict_id(), c2.conflict_id());
            }
            _ => panic!("expected conflict for duplicated package"),
        }
    }

    #[test]
    fn applied_sequence_check_detects_replay() {
        let mut seen = BTreeSet::new();
        seen.insert("txn-123:5".into());
        let d = ReplayDetector::new(
            AppliedPackages::new(BTreeSet::new()),
            AppliedTransitions::new(BTreeSet::new()),
            SeenTransactions::new(seen),
            "node-a".into(),
            "node-b".into(),
        );
        let result = d.check_applied_sequence("txn-123", 5, "pkg-1");
        assert!(matches!(
            result,
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::ReplayAttempt(_))
        ));
    }

    #[test]
    fn fresh_sequence_passes_check() {
        let d = empty_detector();
        let result = d.check_applied_sequence("txn-new", 1, "pkg-1");
        assert!(matches!(result, ConflictDetectionOutcome::NoConflict));
    }

    #[test]
    fn applied_packages_empty_is_ok() {
        let ap = AppliedPackages::new(BTreeSet::new());
        assert!(ap.is_empty());
        assert_eq!(ap.len(), 0);
    }

    #[test]
    fn applied_transitions_empty_is_ok() {
        let at = AppliedTransitions::new(BTreeSet::new());
        assert!(at.is_empty());
        assert_eq!(at.len(), 0);
    }
}
