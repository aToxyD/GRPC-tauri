use sha2::{Digest, Sha256};

use super::types::{
    ConflictDetectionOutcome, ConflictExplanation, ConflictId, ConflictMetadata, FiscalScope,
    StockStateFingerprint, SyncConflict,
};

/// Import staleness check input: the incoming package timestamp
/// as a sequence number (monotonic per source) and the last-applied
/// sequence number for the same source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StalenessCheckInput {
    pub incoming_sequence: u64,
    pub last_applied_sequence: u64,
}

impl StalenessCheckInput {
    pub fn new(incoming: u64, last_applied: u64) -> Self {
        Self {
            incoming_sequence: incoming,
            last_applied_sequence: last_applied,
        }
    }
}

/// Divergent stock state check input: the incoming package's
/// stock fingerprint vs. the current node's computed fingerprint.
#[derive(Debug, Clone, PartialEq)]
pub struct StockDivergenceInput {
    pub incoming_fingerprint: StockStateFingerprint,
    pub local_fingerprint: StockStateFingerprint,
}

/// Conflicting inventory mutation check input: overlapping
/// mutation windows for the same product from different sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryMutationWindow {
    pub product_id: String,
    pub source_node_id: String,
    pub last_export_sequence: u64,
    pub mutation_count: u64,
}

/// Pure conflict detection functions.
///
/// All functions are deterministic and produce the same output
/// for the same input. No mutation, no wall-clock time.
pub struct ConflictDetector;

impl ConflictDetector {
    /// Detect if an incoming import is stale.
    ///
    /// If incoming_sequence <= last_applied_sequence, the import
    /// is stale and MUST be rejected.
    pub fn detect_stale_import(
        input: StalenessCheckInput,
        package_id: &str,
        source_node_id: &str,
        target_node_id: &str,
    ) -> ConflictDetectionOutcome {
        if input.incoming_sequence <= input.last_applied_sequence {
            let evidence = vec![
                format!("incoming_sequence:{}", input.incoming_sequence),
                format!("last_applied_sequence:{}", input.last_applied_sequence),
            ];
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::StaleImport(
                Self::build_metadata(package_id, source_node_id, target_node_id, "StaleImport", evidence, FiscalScope::unknown()),
            ))
        } else {
            ConflictDetectionOutcome::NoConflict
        }
    }

    /// Detect divergent stock state between the incoming package
    /// and the current node.
    ///
    /// Compares Merkle-like fingerprints: if the fingerprints
    /// differ, the stock states have diverged.
    pub fn detect_divergent_stock(
        input: StockDivergenceInput,
        package_id: &str,
        source_node_id: &str,
        target_node_id: &str,
    ) -> ConflictDetectionOutcome {
        if input.incoming_fingerprint != input.local_fingerprint {
            let evidence = vec![
                format!("incoming_hash:{}", input.incoming_fingerprint.fingerprint_hash),
                format!("local_hash:{}", input.local_fingerprint.fingerprint_hash),
                format!("product_id:{}", input.incoming_fingerprint.product_id),
            ];
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::DivergentStockState(
                Self::build_metadata(package_id, source_node_id, target_node_id, "DivergentStockState", evidence, FiscalScope::unknown()),
            ))
        } else {
            ConflictDetectionOutcome::NoConflict
        }
    }

    /// Detect conflicting inventory mutations.
    ///
    /// Compares mutation windows for the same product from
    /// two different sources. If the last_export_sequence from
    /// source A is >= the last_applied from source B for the
    /// same product, there is an overlapping window.
    pub fn detect_conflicting_inventory_mutation(
        source_a: &InventoryMutationWindow,
        source_b: &InventoryMutationWindow,
        package_id: &str,
        target_node_id: &str,
    ) -> ConflictDetectionOutcome {
        let same_product = source_a.product_id == source_b.product_id;
        let different_source = source_a.source_node_id != source_b.source_node_id;
        let overlapping = source_a.last_export_sequence >= source_b.mutation_count
            || source_b.last_export_sequence >= source_a.mutation_count;

        if same_product && different_source && overlapping {
            let evidence = vec![
                format!("product_id:{}", source_a.product_id),
                format!(
                    "source_a:{} last_export:{}",
                    source_a.source_node_id, source_a.last_export_sequence
                ),
                format!(
                    "source_b:{} last_export:{}",
                    source_b.source_node_id, source_b.last_export_sequence
                ),
            ];
            ConflictDetectionOutcome::ConflictDetected(
                SyncConflict::ConflictingInventoryMutation(Self::build_metadata(
                    package_id,
                    &source_a.source_node_id,
                    target_node_id,
                    "ConflictingInventoryMutation",
                    evidence,
                    FiscalScope::unknown(),
                )),
            )
        } else {
            ConflictDetectionOutcome::NoConflict
        }
    }

    fn build_metadata(
        package_id: &str,
        source_node_id: &str,
        target_node_id: &str,
        label: &str,
        evidence: Vec<String>,
        fiscal_scope: FiscalScope,
    ) -> ConflictMetadata {
        let id_input = format!("{}:{}:{}:{}", package_id, label, source_node_id, evidence.join(","));
        let hash = hex::encode(Sha256::digest(id_input.as_bytes()));

        ConflictMetadata {
            conflict_id: ConflictId(hash),
            package_id: package_id.to_string(),
            source_node_id: source_node_id.to_string(),
            target_node_id: target_node_id.to_string(),
            explanation: ConflictExplanation {
                label: label.to_string(),
                description: format!(
                    "{}: package={} source={} evidence={}",
                    label,
                    package_id,
                    source_node_id,
                    evidence.join("; ")
                ),
            },
            fiscal_scope,
            evidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fingerprint(product_id: &str, hash: &str) -> StockStateFingerprint {
        StockStateFingerprint {
            product_id: product_id.to_string(),
            current_quantity: 100.0,
            last_mutation_sequence: 1,
            fingerprint_hash: hash.to_string(),
        }
    }

    #[test]
    fn stale_import_newer_sequence_passes() {
        let input = StalenessCheckInput::new(10, 5);
        let result = ConflictDetector::detect_stale_import(input, "pkg-1", "node-a", "node-b");
        assert!(matches!(result, ConflictDetectionOutcome::NoConflict));
    }

    #[test]
    fn stale_import_older_sequence_rejected() {
        let input = StalenessCheckInput::new(3, 5);
        let result = ConflictDetector::detect_stale_import(input, "pkg-1", "node-a", "node-b");
        match result {
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::StaleImport(meta)) => {
                assert_eq!(meta.explanation.label, "StaleImport");
                assert!(meta.evidence[0].contains("incoming_sequence:3"));
                assert!(meta.evidence[1].contains("last_applied_sequence:5"));
            }
            _ => panic!("expected StaleImport conflict"),
        }
    }

    #[test]
    fn stale_import_equal_sequence_rejected() {
        let input = StalenessCheckInput::new(5, 5);
        let result = ConflictDetector::detect_stale_import(input, "pkg-1", "node-a", "node-b");
        assert!(matches!(
            result,
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::StaleImport(_))
        ));
    }

    #[test]
    fn divergent_stock_detected_when_fingerprints_differ() {
        let input = StockDivergenceInput {
            incoming_fingerprint: make_fingerprint("prod-1", "abc"),
            local_fingerprint: make_fingerprint("prod-1", "def"),
        };
        let result = ConflictDetector::detect_divergent_stock(input, "pkg-1", "node-a", "node-b");
        match result {
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::DivergentStockState(meta)) => {
                assert_eq!(meta.explanation.label, "DivergentStockState");
            }
            _ => panic!("expected DivergentStockState conflict"),
        }
    }

    #[test]
    fn matching_stock_fingerprints_pass() {
        let input = StockDivergenceInput {
            incoming_fingerprint: make_fingerprint("prod-1", "abc"),
            local_fingerprint: make_fingerprint("prod-1", "abc"),
        };
        let result = ConflictDetector::detect_divergent_stock(input, "pkg-1", "node-a", "node-b");
        assert!(matches!(result, ConflictDetectionOutcome::NoConflict));
    }

    #[test]
    fn same_source_no_conflict_for_inventory_mutation() {
        let a = InventoryMutationWindow {
            product_id: "prod-1".into(),
            source_node_id: "node-a".into(),
            last_export_sequence: 10,
            mutation_count: 5,
        };
        let b = InventoryMutationWindow {
            product_id: "prod-1".into(),
            source_node_id: "node-a".into(),
            last_export_sequence: 5,
            mutation_count: 3,
        };
        let result = ConflictDetector::detect_conflicting_inventory_mutation(&a, &b, "pkg-1", "node-b");
        assert!(matches!(result, ConflictDetectionOutcome::NoConflict));
    }

    #[test]
    fn different_product_no_conflict() {
        let a = InventoryMutationWindow {
            product_id: "prod-1".into(),
            source_node_id: "node-a".into(),
            last_export_sequence: 10,
            mutation_count: 5,
        };
        let b = InventoryMutationWindow {
            product_id: "prod-2".into(),
            source_node_id: "node-b".into(),
            last_export_sequence: 5,
            mutation_count: 3,
        };
        let result = ConflictDetector::detect_conflicting_inventory_mutation(&a, &b, "pkg-1", "node-b");
        assert!(matches!(result, ConflictDetectionOutcome::NoConflict));
    }

    #[test]
    fn conflicting_inventory_mutation_detected() {
        let a = InventoryMutationWindow {
            product_id: "prod-1".into(),
            source_node_id: "node-a".into(),
            last_export_sequence: 10,
            mutation_count: 5,
        };
        let b = InventoryMutationWindow {
            product_id: "prod-1".into(),
            source_node_id: "node-b".into(),
            last_export_sequence: 8,
            mutation_count: 3,
        };
        let result = ConflictDetector::detect_conflicting_inventory_mutation(&a, &b, "pkg-1", "node-c");
        match result {
            ConflictDetectionOutcome::ConflictDetected(SyncConflict::ConflictingInventoryMutation(meta)) => {
                assert_eq!(meta.explanation.label, "ConflictingInventoryMutation");
            }
            _ => panic!("expected ConflictingInventoryMutation conflict"),
        }
    }

    #[test]
    fn deterministic_id_for_stale_import() {
        let input = StalenessCheckInput::new(3, 5);
        let r1 = ConflictDetector::detect_stale_import(input, "pkg-x", "node-a", "node-b");
        let r2 = ConflictDetector::detect_stale_import(input, "pkg-x", "node-a", "node-b");
        match (r1, r2) {
            (
                ConflictDetectionOutcome::ConflictDetected(c1),
                ConflictDetectionOutcome::ConflictDetected(c2),
            ) => {
                assert_eq!(c1.conflict_id(), c2.conflict_id());
            }
            _ => panic!("expected conflict"),
        }
    }

    #[test]
    fn different_input_different_conflict_id() {
        let input_a = StalenessCheckInput::new(1, 5);
        let input_b = StalenessCheckInput::new(2, 5);
        let r1 = ConflictDetector::detect_stale_import(input_a, "pkg-x", "node-a", "node-b");
        let r2 = ConflictDetector::detect_stale_import(input_b, "pkg-x", "node-a", "node-b");
        match (r1, r2) {
            (
                ConflictDetectionOutcome::ConflictDetected(c1),
                ConflictDetectionOutcome::ConflictDetected(c2),
            ) => {
                assert_ne!(c1.conflict_id(), c2.conflict_id());
            }
            _ => panic!("expected conflict"),
        }
    }
}
