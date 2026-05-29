use serde::{Deserialize, Serialize};

use super::types::SortDirection;

/// Result of validating a sequence of numbers.
///
/// Provides:
/// - contiguity check
/// - explicitly enumerated missing sequences
/// - explicitly enumerated duplicate sequences
/// - reconstruction-safety (all data needed to rebuild)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SequenceValidationResult {
    /// Whether the sequence is contiguous (no gaps, no duplicates).
    pub contiguous: bool,
    /// Missing sequence numbers found (sorted ascending).
    pub missing_sequences: Vec<u64>,
    /// Duplicate sequence numbers found (sorted ascending).
    pub duplicate_sequences: Vec<u64>,
    /// Total count of sequences analyzed.
    pub total_sequences: usize,
    /// Expected range (min..=max) of the sequence.
    pub expected_range: Option<(u64, u64)>,
}

impl SequenceValidationResult {
    pub fn is_valid(&self) -> bool {
        self.contiguous
    }
}

/// Validates sequence number arrays for contiguity and integrity.
///
/// Deterministic: same input produces same result.
/// No wall-clock time dependency.
/// No mutation.
pub struct SequenceValidator;

impl SequenceValidator {
    /// Validate a sequence of numbers.
    ///
    /// Checks:
    /// - Whether the sequence is contiguous (no gaps)
    /// - Missing sequences (sorted ascending)
    /// - Duplicate sequences (sorted ascending)
    pub fn validate(sequences: &[u64]) -> SequenceValidationResult {
        if sequences.is_empty() {
            return SequenceValidationResult {
                contiguous: true,
                missing_sequences: vec![],
                duplicate_sequences: vec![],
                total_sequences: 0,
                expected_range: None,
            };
        }

        // Sort deterministically
        let mut sorted: Vec<u64> = sequences.to_vec();
        sorted.sort();

        let mut missing = Vec::new();
        let mut duplicates = Vec::new();
        let mut seen = std::collections::BTreeSet::new();

        // Detect duplicates and find min/max
        let min = sorted[0];
        let max = sorted[sorted.len() - 1];

        for &s in &sorted {
            if !seen.insert(s) {
                duplicates.push(s);
            }
        }

        // Detect gaps in the range [min, max]
        // Only check if there's a clear sequence (at least 2 unique values)
        if seen.len() >= 2 {
            for v in min..=max {
                if !seen.contains(&v) {
                    missing.push(v);
                }
            }
        }

        // Deduplicate and sort duplicates
        duplicates.sort();
        duplicates.dedup();

        let contiguous = missing.is_empty() && duplicates.is_empty();

        SequenceValidationResult {
            contiguous,
            missing_sequences: missing,
            duplicate_sequences: duplicates,
            total_sequences: sequences.len(),
            expected_range: Some((min, max)),
        }
    }

    /// Validate a sequence of numbers that MUST start at 1.
    ///
    /// This is the stricter version used for transaction-level
    /// event sequences (where sequence always starts at 1).
    pub fn validate_from_one(sequences: &[u64]) -> SequenceValidationResult {
        let result = Self::validate(sequences);

        if result.expected_range.is_none_or(|(min, _)| min == 1) {
            return result;
        }

        // Add missing 1 if the sequence doesn't start at 1
        let mut missing = result.missing_sequences;
        if let Some((min, _)) = result.expected_range {
            if min > 1 {
                for v in 1..min {
                    missing.push(v);
                }
            }
        }
        missing.sort();

        SequenceValidationResult {
            contiguous: missing.is_empty() && result.duplicate_sequences.is_empty(),
            missing_sequences: missing,
            duplicate_sequences: result.duplicate_sequences,
            total_sequences: result.total_sequences,
            expected_range: result.expected_range,
        }
    }

    /// Check if an incoming sequence can be applied given the
    /// currently applied max sequence.
    ///
    /// Operational-order imports are sequentially dependent:
    /// an incoming sequence must be exactly (last_applied + 1)
    /// for the import to proceed without gaps.
    pub fn check_sequence_continuity(
        incoming_sequence: u64,
        last_applied_sequence: Option<u64>,
    ) -> SequenceContinuity {
        match last_applied_sequence {
            None => {
                // First import: must start at 1
                if incoming_sequence == 1 {
                    SequenceContinuity::Contiguous
                } else {
                    SequenceContinuity::Gap {
                        expected: 1,
                        got: incoming_sequence,
                    }
                }
            }
            Some(last) => {
                let expected = last + 1;
                if incoming_sequence == expected {
                    SequenceContinuity::Contiguous
                } else if incoming_sequence > expected {
                    SequenceContinuity::Gap {
                        expected,
                        got: incoming_sequence,
                    }
                } else {
                    SequenceContinuity::Duplicate {
                        last_applied: last,
                        incoming: incoming_sequence,
                    }
                }
            }
        }
    }

    /// Order sequences deterministically.
    ///
    /// Explicit ORDER BY for sequence arrays.
    pub fn order_sequences(sequences: &mut [u64], direction: SortDirection) {
        match direction {
            SortDirection::Ascending => sequences.sort(),
            SortDirection::Descending => sequences.sort_by(|a, b| b.cmp(a)),
        }
    }
}

/// Result of a continuity check for sequential import ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceContinuity {
    /// The incoming sequence continues cleanly from the last applied.
    Contiguous,
    /// There is a gap: expected X but got Y.
    Gap { expected: u64, got: u64 },
    /// The incoming sequence has already been applied.
    Duplicate { last_applied: u64, incoming: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contiguous_sequence_passes_validation() {
        let result = SequenceValidator::validate(&[1, 2, 3, 4, 5]);
        assert!(result.contiguous);
        assert!(result.missing_sequences.is_empty());
        assert!(result.duplicate_sequences.is_empty());
        assert_eq!(result.total_sequences, 5);
        assert_eq!(result.expected_range, Some((1, 5)));
    }

    #[test]
    fn single_element_is_contiguous() {
        let result = SequenceValidator::validate(&[42]);
        assert!(result.contiguous);
    }

    #[test]
    fn empty_sequence_is_contiguous() {
        let result = SequenceValidator::validate(&[]);
        assert!(result.contiguous);
        assert_eq!(result.total_sequences, 0);
    }

    #[test]
    fn gap_detected_in_middle() {
        let result = SequenceValidator::validate(&[1, 2, 4, 5]);
        assert!(!result.contiguous);
        assert_eq!(result.missing_sequences, vec![3]);
    }

    #[test]
    fn multiple_gaps_detected() {
        let result = SequenceValidator::validate(&[1, 5]);
        assert!(!result.contiguous);
        assert_eq!(result.missing_sequences, vec![2, 3, 4]);
    }

    #[test]
    fn duplicates_detected() {
        let result = SequenceValidator::validate(&[1, 2, 2, 3]);
        assert!(!result.contiguous);
        assert_eq!(result.duplicate_sequences, vec![2]);
    }

    #[test]
    fn both_gaps_and_duplicates_detected() {
        let result = SequenceValidator::validate(&[1, 2, 2, 5]);
        assert!(!result.contiguous);
        assert_eq!(result.duplicate_sequences, vec![2]);
        assert_eq!(result.missing_sequences, vec![3, 4]);
    }

    #[test]
    fn unordered_input_is_handled_deterministically() {
        let result = SequenceValidator::validate(&[5, 1, 3, 2, 4]);
        assert!(result.contiguous);
        assert_eq!(result.expected_range, Some((1, 5)));
    }

    #[test]
    fn validate_from_one_rejects_sequence_not_starting_at_one() {
        let result = SequenceValidator::validate_from_one(&[2, 3, 4]);
        assert!(!result.contiguous);
        assert_eq!(result.missing_sequences, vec![1]);
    }

    #[test]
    fn validate_from_one_accepts_sequence_starting_at_one() {
        let result = SequenceValidator::validate_from_one(&[1, 2, 3]);
        assert!(result.contiguous);
    }

    #[test]
    fn continuity_check_first_import_must_start_at_one() {
        assert_eq!(
            SequenceValidator::check_sequence_continuity(1, None),
            SequenceContinuity::Contiguous
        );
        assert_eq!(
            SequenceValidator::check_sequence_continuity(2, None),
            SequenceContinuity::Gap {
                expected: 1,
                got: 2
            }
        );
    }

    #[test]
    fn continuity_check_exact_next_sequence() {
        assert_eq!(
            SequenceValidator::check_sequence_continuity(6, Some(5)),
            SequenceContinuity::Contiguous
        );
    }

    #[test]
    fn continuity_check_gap_detected() {
        assert_eq!(
            SequenceValidator::check_sequence_continuity(7, Some(5)),
            SequenceContinuity::Gap {
                expected: 6,
                got: 7
            }
        );
    }

    #[test]
    fn continuity_check_duplicate_detected() {
        assert_eq!(
            SequenceValidator::check_sequence_continuity(5, Some(5)),
            SequenceContinuity::Duplicate {
                last_applied: 5,
                incoming: 5
            }
        );
    }

    #[test]
    fn order_sequences_ascending() {
        let mut seqs = vec![3, 1, 2];
        SequenceValidator::order_sequences(&mut seqs, SortDirection::Ascending);
        assert_eq!(seqs, vec![1, 2, 3]);
    }

    #[test]
    fn order_sequences_descending() {
        let mut seqs = vec![1, 3, 2];
        SequenceValidator::order_sequences(&mut seqs, SortDirection::Descending);
        assert_eq!(seqs, vec![3, 2, 1]);
    }

    #[test]
    fn result_is_reproducible_for_same_input() {
        let r1 = SequenceValidator::validate(&[1, 3, 5]);
        let r2 = SequenceValidator::validate(&[1, 3, 5]);
        assert_eq!(
            serde_json::to_string(&r1).unwrap(),
            serde_json::to_string(&r2).unwrap()
        );
    }

    #[test]
    fn serde_round_trip() {
        let result = SequenceValidator::validate(&[1, 2, 4]);
        let json = serde_json::to_string(&result).unwrap();
        let restored: SequenceValidationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, restored);
    }
}
