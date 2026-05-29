use std::collections::BTreeMap;

use super::types::{SequenceWindow, StockStateFingerprint};

/// Result of comparing two sequence windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceWindowComparison {
    pub window_a: SequenceWindow,
    pub window_b: SequenceWindow,
    pub is_identical: bool,
    pub differences: Vec<String>,
}

/// Result of comparing two fiscal transition lineages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiscalTransitionComparison {
    pub lineage_a: Vec<String>,
    pub lineage_b: Vec<String>,
    pub is_identical: bool,
    pub divergence_point: Option<usize>,
    pub differences: Vec<String>,
}

/// Result of comparing two stock state fingerprints.
#[derive(Debug, Clone, PartialEq)]
pub struct StockStateComparison {
    pub fingerprints_a: Vec<StockStateFingerprint>,
    pub fingerprints_b: Vec<StockStateFingerprint>,
    pub is_identical: bool,
    pub diverging_products: Vec<String>,
}

/// Result of comparing event ordering consistency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventOrderingComparison {
    /// Per-transaction comparison results.
    pub transaction_comparisons: Vec<TransactionOrderComparison>,
    pub is_consistent: bool,
    pub inconsistencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionOrderComparison {
    pub transaction_id: String,
    pub sequences_a: Vec<u64>,
    pub sequences_b: Vec<u64>,
    pub is_identical: bool,
}

/// Deterministic reconciliation comparer.
///
/// Pure comparison functions. No mutation, no wall-clock time.
/// All ordering is explicit and deterministic.
pub struct ReconciliationComparer;

impl ReconciliationComparer {
    /// Compare two sequence windows for equality.
    ///
    /// Two windows are equal iff they have the same transaction_id,
    /// same start and end, and same contiguity flag.
    pub fn compare_sequence_windows(window_a: &SequenceWindow, window_b: &SequenceWindow) -> SequenceWindowComparison {
        let mut differences = Vec::new();

        if window_a.transaction_id != window_b.transaction_id {
            differences.push(format!(
                "transaction_id: {} != {}",
                window_a.transaction_id, window_b.transaction_id
            ));
        }
        if window_a.sequence_start != window_b.sequence_start {
            differences.push(format!(
                "sequence_start: {} != {}",
                window_a.sequence_start, window_b.sequence_start
            ));
        }
        if window_a.sequence_end != window_b.sequence_end {
            differences.push(format!(
                "sequence_end: {} != {}",
                window_a.sequence_end, window_b.sequence_end
            ));
        }
        if window_a.is_contiguous != window_b.is_contiguous {
            differences.push(format!(
                "is_contiguous: {} != {}",
                window_a.is_contiguous, window_b.is_contiguous
            ));
        }

        SequenceWindowComparison {
            window_a: window_a.clone(),
            window_b: window_b.clone(),
            is_identical: differences.is_empty(),
            differences,
        }
    }

    /// Compare two fiscal transition lineages.
    ///
    /// Lineages are ordered lists of transition IDs. The comparison
    /// finds the first point where they diverge.
    pub fn compare_fiscal_transition_lineage(
        lineage_a: &[String],
        lineage_b: &[String],
    ) -> FiscalTransitionComparison {
        let mut differences = Vec::new();
        let mut divergence_point = None;

        for (i, (a, b)) in lineage_a.iter().zip(lineage_b.iter()).enumerate() {
            if a != b {
                differences.push(format!("position {}: {} != {}", i, a, b));
                if divergence_point.is_none() {
                    divergence_point = Some(i);
                }
            }
        }

        if lineage_a.len() != lineage_b.len() {
            differences.push(format!(
                "length: {} != {}",
                lineage_a.len(),
                lineage_b.len()
            ));
        }

        FiscalTransitionComparison {
            lineage_a: lineage_a.to_vec(),
            lineage_b: lineage_b.to_vec(),
            is_identical: differences.is_empty(),
            divergence_point,
            differences,
        }
    }

    /// Compare two sets of stock state fingerprints.
    ///
    /// Iterates over deterministic product order (sorted by
    /// product_id) and reports any products with differing
    /// fingerprints.
    pub fn compare_stock_state(
        fingerprints_a: &[StockStateFingerprint],
        fingerprints_b: &[StockStateFingerprint],
    ) -> StockStateComparison {
        // Ensure deterministic ordering by product_id
        let mut map_a: BTreeMap<String, &StockStateFingerprint> = BTreeMap::new();
        for fp in fingerprints_a {
            map_a.insert(fp.product_id.clone(), fp);
        }

        let mut map_b: BTreeMap<String, &StockStateFingerprint> = BTreeMap::new();
        for fp in fingerprints_b {
            map_b.insert(fp.product_id.clone(), fp);
        }

        let mut diverging_products = Vec::new();
        let all_products: BTreeSet<String> = map_a.keys().chain(map_b.keys()).cloned().collect();

        for pid in &all_products {
            match (map_a.get(pid), map_b.get(pid)) {
                (Some(a), Some(b)) if a.fingerprint_hash != b.fingerprint_hash => {
                    diverging_products.push(pid.clone());
                }
                (Some(_), None) | (None, Some(_)) => {
                    diverging_products.push(pid.clone());
                }
                _ => {}
            }
        }

        // Sort diverging products deterministically
        diverging_products.sort();

        StockStateComparison {
            fingerprints_a: fingerprints_a.to_vec(),
            fingerprints_b: fingerprints_b.to_vec(),
            is_identical: diverging_products.is_empty(),
            diverging_products,
        }
    }

    /// Compare event ordering consistency between two sets of
    /// per-transaction sequences.
    ///
    /// Input is a deterministic ordering of (transaction_id, sequences).
    /// The comparison verifies that both sides have the same events
    /// in the same order.
    pub fn compare_event_ordering(
        events_a: &[(String, Vec<u64>)],
        events_b: &[(String, Vec<u64>)],
    ) -> EventOrderingComparison {
        let mut transaction_comparisons = Vec::new();
        let mut inconsistencies = Vec::new();

        // Sort both inputs by transaction_id deterministically
        let mut sorted_a: Vec<(String, Vec<u64>)> = events_a.to_vec();
        sorted_a.sort_by(|a, b| a.0.cmp(&b.0));

        let mut sorted_b: Vec<(String, Vec<u64>)> = events_b.to_vec();
        sorted_b.sort_by(|a, b| a.0.cmp(&b.0));

        for (txn_id, seqs_a) in sorted_a.iter() {
            let seqs_b = sorted_b
                .iter()
                .find(|(id, _)| id == txn_id)
                .map(|(_, seqs)| seqs);

            match seqs_b {
                Some(seqs_b) => {
                    let is_identical = seqs_a == seqs_b;
                    if !is_identical {
                        inconsistencies.push(format!(
                            "transaction {}: sequences differ ({:?} vs {:?})",
                            txn_id, seqs_a, seqs_b
                        ));
                    }
                    transaction_comparisons.push(TransactionOrderComparison {
                        transaction_id: txn_id.clone(),
                        sequences_a: seqs_a.clone(),
                        sequences_b: seqs_b.clone(),
                        is_identical,
                    });
                }
                None => {
                    inconsistencies.push(format!(
                        "transaction {}: missing in side B",
                        txn_id
                    ));
                }
            }
        }

        // Check for transactions in B that are missing from A
        for (txn_id, _) in &sorted_b {
            if !sorted_a.iter().any(|(id, _)| id == txn_id) {
                inconsistencies.push(format!(
                    "transaction {}: missing in side A",
                    txn_id
                ));
            }
        }

        EventOrderingComparison {
            transaction_comparisons,
            is_consistent: inconsistencies.is_empty(),
            inconsistencies,
        }
    }
}

use std::collections::BTreeSet;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_window(txn_id: &str, start: u64, end: u64, contiguous: bool) -> SequenceWindow {
        SequenceWindow {
            transaction_id: txn_id.to_string(),
            sequence_start: start,
            sequence_end: end,
            is_contiguous: contiguous,
        }
    }

    #[test]
    fn identical_sequence_windows_match() {
        let a = make_window("txn-1", 1, 5, true);
        let b = make_window("txn-1", 1, 5, true);
        let result = ReconciliationComparer::compare_sequence_windows(&a, &b);
        assert!(result.is_identical);
        assert!(result.differences.is_empty());
    }

    #[test]
    fn different_sequence_windows_detected() {
        let a = make_window("txn-1", 1, 5, true);
        let b = make_window("txn-1", 1, 6, true);
        let result = ReconciliationComparer::compare_sequence_windows(&a, &b);
        assert!(!result.is_identical);
        assert!(!result.differences.is_empty());
    }

    #[test]
    fn identical_fiscal_lineages_match() {
        let a = vec!["t1".into(), "t2".into(), "t3".into()];
        let b = vec!["t1".into(), "t2".into(), "t3".into()];
        let result = ReconciliationComparer::compare_fiscal_transition_lineage(&a, &b);
        assert!(result.is_identical);
        assert!(result.divergence_point.is_none());
    }

    #[test]
    fn diverging_fiscal_lineages_detected() {
        let a = vec!["t1".into(), "t2".into(), "t3".into()];
        let b = vec!["t1".into(), "t2-x".into(), "t3".into()];
        let result = ReconciliationComparer::compare_fiscal_transition_lineage(&a, &b);
        assert!(!result.is_identical);
        assert_eq!(result.divergence_point, Some(1));
    }

    #[test]
    fn fiscal_lineage_different_lengths_detected() {
        let a = vec!["t1".into(), "t2".into()];
        let b = vec!["t1".into(), "t2".into(), "t3".into()];
        let result = ReconciliationComparer::compare_fiscal_transition_lineage(&a, &b);
        assert!(!result.is_identical);
        assert!(result.differences.iter().any(|d| d.contains("length")));
    }

    #[test]
    fn identical_stock_fingerprints_match() {
        let fp_a = StockStateFingerprint {
            product_id: "prod-1".into(),
            current_quantity: 100.0,
            last_mutation_sequence: 1,
            fingerprint_hash: "abc".into(),
        };
        let fp_b = StockStateFingerprint {
            fingerprint_hash: "abc".into(),
            ..fp_a.clone()
        };
        let result = ReconciliationComparer::compare_stock_state(&[fp_a], &[fp_b]);
        assert!(result.is_identical);
        assert!(result.diverging_products.is_empty());
    }

    #[test]
    fn diverging_stock_fingerprints_detected() {
        let fp_a = StockStateFingerprint {
            product_id: "prod-1".into(),
            current_quantity: 100.0,
            last_mutation_sequence: 1,
            fingerprint_hash: "abc".into(),
        };
        let fp_b = StockStateFingerprint {
            product_id: "prod-1".into(),
            current_quantity: 90.0,
            last_mutation_sequence: 2,
            fingerprint_hash: "def".into(),
        };
        let result = ReconciliationComparer::compare_stock_state(&[fp_a], &[fp_b]);
        assert!(!result.is_identical);
        assert_eq!(result.diverging_products, vec!["prod-1".to_string()]);
    }

    #[test]
    fn product_missing_from_one_side_detected() {
        let fp_a = StockStateFingerprint {
            product_id: "prod-1".into(),
            current_quantity: 100.0,
            last_mutation_sequence: 1,
            fingerprint_hash: "abc".into(),
        };
        let fp_b = StockStateFingerprint {
            product_id: "prod-2".into(),
            current_quantity: 50.0,
            last_mutation_sequence: 1,
            fingerprint_hash: "xyz".into(),
        };
        let result = ReconciliationComparer::compare_stock_state(&[fp_a], &[fp_b]);
        assert!(!result.is_identical);
        assert!(result.diverging_products.contains(&"prod-1".into()));
        assert!(result.diverging_products.contains(&"prod-2".into()));
    }

    #[test]
    fn identical_event_ordering_is_consistent() {
        let a = vec![("txn-1".into(), vec![1, 2, 3])];
        let b = vec![("txn-1".into(), vec![1, 2, 3])];
        let result = ReconciliationComparer::compare_event_ordering(&a, &b);
        assert!(result.is_consistent);
        assert!(result.inconsistencies.is_empty());
    }

    #[test]
    fn differing_event_order_detected() {
        let a = vec![("txn-1".into(), vec![1, 2, 3])];
        let b = vec![("txn-1".into(), vec![1, 2, 4])];
        let result = ReconciliationComparer::compare_event_ordering(&a, &b);
        assert!(!result.is_consistent);
        assert!(!result.inconsistencies.is_empty());
    }

    #[test]
    fn missing_transaction_detected() {
        let a = vec![
            ("txn-1".into(), vec![1, 2]),
            ("txn-2".into(), vec![1]),
        ];
        let b = vec![("txn-1".into(), vec![1, 2])];
        let result = ReconciliationComparer::compare_event_ordering(&a, &b);
        assert!(!result.is_consistent);
    }

    #[test]
    fn ordering_is_deterministic_by_transaction_id() {
        let a = vec![
            ("txn-b".into(), vec![1]),
            ("txn-a".into(), vec![1]),
        ];
        let b = vec![
            ("txn-a".into(), vec![1]),
            ("txn-b".into(), vec![1]),
        ];
        let result = ReconciliationComparer::compare_event_ordering(&a, &b);
        assert!(result.is_consistent);
    }

    #[test]
    fn stock_comparison_is_deterministic_by_product_id() {
        let fp_a1 = StockStateFingerprint {
            product_id: "prod-z".into(),
            current_quantity: 100.0,
            last_mutation_sequence: 1,
            fingerprint_hash: "abc".into(),
        };
        let fp_a2 = StockStateFingerprint {
            product_id: "prod-a".into(),
            current_quantity: 50.0,
            last_mutation_sequence: 1,
            fingerprint_hash: "xyz".into(),
        };
        let fp_b1 = StockStateFingerprint {
            product_id: "prod-a".into(),
            current_quantity: 50.0,
            last_mutation_sequence: 1,
            fingerprint_hash: "xyz".into(),
        };
        let fp_b2 = StockStateFingerprint {
            product_id: "prod-z".into(),
            current_quantity: 100.0,
            last_mutation_sequence: 1,
            fingerprint_hash: "abc".into(),
        };

        let result_a = ReconciliationComparer::compare_stock_state(&[fp_a1, fp_a2], &[fp_b1, fp_b2]);
        assert!(result_a.is_identical);
    }
}
