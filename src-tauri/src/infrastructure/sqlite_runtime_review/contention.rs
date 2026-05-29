use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WalPressureLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadContentionLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentionAssessment {
    pub wal_pressure: WalPressureLevel,
    pub read_contention: ReadContentionLevel,
    pub reporting_frequency: u64,
    pub write_frequency: u64,
    pub contention_score: f64,
    pub assessment_order: u64,
}

pub struct ContentionClassifier;

impl ContentionClassifier {
    pub fn classify(
        wal_size_bytes: u64,
        wal_checkpoint_seqno: u64,
        reporting_frequency: u64,
        write_frequency: u64,
        assessment_order: u64,
    ) -> ContentionAssessment {
        let wal_pressure = if wal_size_bytes > 100 * 1024 * 1024 {
            WalPressureLevel::High
        } else if wal_size_bytes > 10 * 1024 * 1024 || wal_checkpoint_seqno > 10 {
            WalPressureLevel::Medium
        } else {
            WalPressureLevel::Low
        };

        let read_contention = if write_frequency > 100 && reporting_frequency > 50 {
            ReadContentionLevel::High
        } else if write_frequency > 50 || reporting_frequency > 30 {
            ReadContentionLevel::Medium
        } else {
            ReadContentionLevel::Low
        };

        let contention_score = {
            let wf = write_frequency.min(1000) as f64 / 1000.0;
            let rf = reporting_frequency.min(1000) as f64 / 1000.0;
            let wp = match wal_pressure {
                WalPressureLevel::Low => 0.0,
                WalPressureLevel::Medium => 0.3,
                WalPressureLevel::High => 0.6,
            };
            (wf * 0.5 + rf * 0.3 + wp * 0.2).min(1.0)
        };

        ContentionAssessment {
            wal_pressure,
            read_contention,
            reporting_frequency,
            write_frequency,
            contention_score,
            assessment_order,
        }
    }

    pub fn classify_wal_pressure(wal_size_bytes: u64, checkpoint_seqno: u64) -> WalPressureLevel {
        if wal_size_bytes > 100 * 1024 * 1024 {
            WalPressureLevel::High
        } else if wal_size_bytes > 10 * 1024 * 1024 || checkpoint_seqno > 10 {
            WalPressureLevel::Medium
        } else {
            WalPressureLevel::Low
        }
    }

    pub fn classify_read_contention(
        write_frequency: u64,
        reporting_frequency: u64,
    ) -> ReadContentionLevel {
        if write_frequency > 100 && reporting_frequency > 50 {
            ReadContentionLevel::High
        } else if write_frequency > 50 || reporting_frequency > 30 {
            ReadContentionLevel::Medium
        } else {
            ReadContentionLevel::Low
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_wal_pressure_small_wal() {
        let pressure = ContentionClassifier::classify_wal_pressure(1024, 0);
        assert_eq!(pressure, WalPressureLevel::Low);
    }

    #[test]
    fn medium_wal_pressure_moderate_wal() {
        let pressure = ContentionClassifier::classify_wal_pressure(20 * 1024 * 1024, 5);
        assert_eq!(pressure, WalPressureLevel::Medium);
    }

    #[test]
    fn high_wal_pressure_large_wal() {
        let pressure = ContentionClassifier::classify_wal_pressure(200 * 1024 * 1024, 50);
        assert_eq!(pressure, WalPressureLevel::High);
    }

    #[test]
    fn medium_pressure_from_checkpoint_seqno() {
        let pressure = ContentionClassifier::classify_wal_pressure(1024, 15);
        assert_eq!(pressure, WalPressureLevel::Medium);
    }

    #[test]
    fn low_read_contention() {
        let contention = ContentionClassifier::classify_read_contention(10, 5);
        assert_eq!(contention, ReadContentionLevel::Low);
    }

    #[test]
    fn medium_read_contention() {
        let contention = ContentionClassifier::classify_read_contention(60, 10);
        assert_eq!(contention, ReadContentionLevel::Medium);
    }

    #[test]
    fn high_read_contention() {
        let contention = ContentionClassifier::classify_read_contention(200, 100);
        assert_eq!(contention, ReadContentionLevel::High);
    }

    #[test]
    fn full_classification_determinism() {
        let a1 = ContentionClassifier::classify(50 * 1024 * 1024, 20, 40, 60, 1);
        let a2 = ContentionClassifier::classify(50 * 1024 * 1024, 20, 40, 60, 1);
        assert_eq!(a1, a2);
    }

    #[test]
    fn contention_score_bounded() {
        let a = ContentionClassifier::classify(200 * 1024 * 1024, 100, 500, 500, 1);
        assert!(a.contention_score <= 1.0);
        assert!(a.contention_score >= 0.0);
    }

    #[test]
    fn serde_round_trip_wal_pressure() {
        let values = [
            WalPressureLevel::Low,
            WalPressureLevel::Medium,
            WalPressureLevel::High,
        ];
        for v in &values {
            let json = serde_json::to_string(v).unwrap();
            let deserialized: WalPressureLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, deserialized);
        }
    }

    #[test]
    fn serde_round_trip_read_contention() {
        let values = [
            ReadContentionLevel::Low,
            ReadContentionLevel::Medium,
            ReadContentionLevel::High,
        ];
        for v in &values {
            let json = serde_json::to_string(v).unwrap();
            let deserialized: ReadContentionLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, deserialized);
        }
    }

    #[test]
    fn serde_round_trip_assessment() {
        let a = ContentionClassifier::classify(50 * 1024 * 1024, 20, 40, 60, 42);
        let json = serde_json::to_string(&a).unwrap();
        let deserialized: ContentionAssessment = serde_json::from_str(&json).unwrap();
        assert_eq!(a, deserialized);
    }

    #[test]
    fn assessment_order_preserved() {
        let a = ContentionClassifier::classify(0, 0, 0, 0, 7);
        assert_eq!(a.assessment_order, 7);
    }
}
