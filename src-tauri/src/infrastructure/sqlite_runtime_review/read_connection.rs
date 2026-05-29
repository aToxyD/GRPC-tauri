use serde::{Deserialize, Serialize};

use super::contention::{ReadContentionLevel, WalPressureLevel};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadConnectionSuitability {
    NotNeeded,
    Beneficial,
    Recommended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadConnectionAssessment {
    pub suitability: ReadConnectionSuitability,
    pub risk_level: ReadConnectionRisk,
    pub reason: String,
    pub assessment_order: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadConnectionRisk {
    None,
    Low,
    Medium,
    High,
}

pub struct ReadConnectionSuitabilityEvaluator;

impl ReadConnectionSuitabilityEvaluator {
    pub fn evaluate(
        reporting_query_count: u64,
        wal_pressure: WalPressureLevel,
        contention: ReadContentionLevel,
        concurrent_readers: u64,
        assessment_order: u64,
    ) -> ReadConnectionAssessment {
        let risk_level = match contention {
            ReadContentionLevel::Low => ReadConnectionRisk::Low,
            ReadContentionLevel::Medium => ReadConnectionRisk::Medium,
            ReadContentionLevel::High => ReadConnectionRisk::High,
        };

        let suitability = match wal_pressure {
            WalPressureLevel::High => {
                if reporting_query_count > 5 || concurrent_readers > 0 {
                    ReadConnectionSuitability::Recommended
                } else {
                    ReadConnectionSuitability::Beneficial
                }
            }
            WalPressureLevel::Medium => {
                if reporting_query_count > 30 && concurrent_readers > 1 {
                    ReadConnectionSuitability::Recommended
                } else if reporting_query_count > 10 || concurrent_readers > 0 {
                    ReadConnectionSuitability::Beneficial
                } else {
                    ReadConnectionSuitability::NotNeeded
                }
            }
            WalPressureLevel::Low => {
                if reporting_query_count > 100 && concurrent_readers > 2 {
                    ReadConnectionSuitability::Beneficial
                } else {
                    ReadConnectionSuitability::NotNeeded
                }
            }
        };

        let reason = format!(
            "suitability={suitability:?} risk={risk_level:?} queries={reporting_query_count} contention={contention:?}"
        );

        ReadConnectionAssessment {
            suitability,
            risk_level,
            reason,
            assessment_order,
        }
    }

    pub fn is_multi_connection_recommended(
        reporting_query_count: u64,
        wal_pressure: WalPressureLevel,
        contention: ReadContentionLevel,
        concurrent_readers: u64,
    ) -> bool {
        matches!(
            Self::evaluate(
                reporting_query_count,
                wal_pressure,
                contention,
                concurrent_readers,
                0,
            )
            .suitability,
            ReadConnectionSuitability::Recommended
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_pressure_few_queries_not_needed() {
        let a = ReadConnectionSuitabilityEvaluator::evaluate(
            10,
            WalPressureLevel::Low,
            ReadContentionLevel::Low,
            0,
            1,
        );
        assert_eq!(a.suitability, ReadConnectionSuitability::NotNeeded);
    }

    #[test]
    fn low_pressure_many_queries_beneficial() {
        let a = ReadConnectionSuitabilityEvaluator::evaluate(
            150,
            WalPressureLevel::Low,
            ReadContentionLevel::Low,
            3,
            1,
        );
        assert_eq!(a.suitability, ReadConnectionSuitability::Beneficial);
    }

    #[test]
    fn medium_pressure_recommended() {
        let a = ReadConnectionSuitabilityEvaluator::evaluate(
            50,
            WalPressureLevel::Medium,
            ReadContentionLevel::Medium,
            2,
            1,
        );
        assert_eq!(a.suitability, ReadConnectionSuitability::Recommended);
    }

    #[test]
    fn high_pressure_recommended_even_minimal_load() {
        let a = ReadConnectionSuitabilityEvaluator::evaluate(
            3,
            WalPressureLevel::High,
            ReadContentionLevel::High,
            0,
            1,
        );
        assert_eq!(a.suitability, ReadConnectionSuitability::Beneficial);
    }

    #[test]
    fn high_pressure_recommended_with_queries() {
        let a = ReadConnectionSuitabilityEvaluator::evaluate(
            10,
            WalPressureLevel::High,
            ReadContentionLevel::High,
            1,
            1,
        );
        assert_eq!(a.suitability, ReadConnectionSuitability::Recommended);
    }

    #[test]
    fn determinism_same_input() {
        let a1 = ReadConnectionSuitabilityEvaluator::evaluate(
            50,
            WalPressureLevel::Medium,
            ReadContentionLevel::Medium,
            2,
            1,
        );
        let a2 = ReadConnectionSuitabilityEvaluator::evaluate(
            50,
            WalPressureLevel::Medium,
            ReadContentionLevel::Medium,
            2,
            1,
        );
        assert_eq!(a1, a2);
    }

    #[test]
    fn serde_round_trip_suitability() {
        let values = [
            ReadConnectionSuitability::NotNeeded,
            ReadConnectionSuitability::Beneficial,
            ReadConnectionSuitability::Recommended,
        ];
        for v in &values {
            let json = serde_json::to_string(v).unwrap();
            let deserialized: ReadConnectionSuitability = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, deserialized);
        }
    }

    #[test]
    fn serde_round_trip_risk() {
        let values = [
            ReadConnectionRisk::None,
            ReadConnectionRisk::Low,
            ReadConnectionRisk::Medium,
            ReadConnectionRisk::High,
        ];
        for v in &values {
            let json = serde_json::to_string(v).unwrap();
            let deserialized: ReadConnectionRisk = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, deserialized);
        }
    }

    #[test]
    fn serde_round_trip_assessment() {
        let a = ReadConnectionSuitabilityEvaluator::evaluate(
            50,
            WalPressureLevel::Medium,
            ReadContentionLevel::Medium,
            2,
            42,
        );
        let json = serde_json::to_string(&a).unwrap();
        let deserialized: ReadConnectionAssessment = serde_json::from_str(&json).unwrap();
        assert_eq!(a, deserialized);
    }

    #[test]
    fn assessment_order_preserved() {
        let a = ReadConnectionSuitabilityEvaluator::evaluate(
            10,
            WalPressureLevel::Low,
            ReadContentionLevel::Low,
            0,
            7,
        );
        assert_eq!(a.assessment_order, 7);
    }

    #[test]
    fn risk_level_matches_contention() {
        let a = ReadConnectionSuitabilityEvaluator::evaluate(
            10,
            WalPressureLevel::Low,
            ReadContentionLevel::High,
            0,
            1,
        );
        assert_eq!(a.risk_level, ReadConnectionRisk::High);
    }

    #[test]
    fn is_multi_connection_recommended_true() {
        let r = ReadConnectionSuitabilityEvaluator::is_multi_connection_recommended(
            50,
            WalPressureLevel::Medium,
            ReadContentionLevel::Medium,
            2,
        );
        assert!(r);
    }

    #[test]
    fn is_multi_connection_recommended_false() {
        let r = ReadConnectionSuitabilityEvaluator::is_multi_connection_recommended(
            5,
            WalPressureLevel::Low,
            ReadContentionLevel::Low,
            0,
        );
        assert!(!r);
    }
}
