use serde::{Deserialize, Serialize};

use super::contention::WalPressureLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionTopology {
    SingleWriter,
    SingleWriterWithReadConnection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionTopologyEvaluation {
    pub current_topology: ConnectionTopology,
    pub is_single_writer: bool,
    pub has_read_connection: bool,
    pub reporting_read_connection_beneficial: bool,
    pub reason: String,
    pub evaluation_order: u64,
}

pub struct ConnectionModelEvaluator;

impl ConnectionModelEvaluator {
    pub fn evaluate(
        has_read_connection: bool,
        reporting_query_count: u64,
        wal_pressure: WalPressureLevel,
        concurrent_readers: u64,
        evaluation_order: u64,
    ) -> ConnectionTopologyEvaluation {
        let current_topology = if has_read_connection {
            ConnectionTopology::SingleWriterWithReadConnection
        } else {
            ConnectionTopology::SingleWriter
        };

        let reporting_read_connection_beneficial = match wal_pressure {
            WalPressureLevel::Low => reporting_query_count > 100 && concurrent_readers > 2,
            WalPressureLevel::Medium => reporting_query_count > 50 && concurrent_readers > 1,
            WalPressureLevel::High => reporting_query_count > 10 || concurrent_readers > 0,
        };

        let reason = if has_read_connection {
            "read connection already present".into()
        } else if reporting_read_connection_beneficial {
            format!(
                "reporting workload ({reporting_query_count} queries) and WAL pressure ({wal_pressure:?}) suggest read connection benefit"
            )
        } else {
            format!(
                "single writer adequate for current load ({reporting_query_count} queries, {concurrent_readers} concurrent readers)"
            )
        };

        ConnectionTopologyEvaluation {
            current_topology,
            is_single_writer: !has_read_connection,
            has_read_connection,
            reporting_read_connection_beneficial,
            reason,
            evaluation_order,
        }
    }

    pub fn is_read_connection_beneficial(
        reporting_query_count: u64,
        wal_pressure: WalPressureLevel,
        concurrent_readers: u64,
    ) -> bool {
        Self::evaluate(
            false,
            reporting_query_count,
            wal_pressure,
            concurrent_readers,
            0,
        )
        .reporting_read_connection_beneficial
    }

    pub fn evaluate_topology_only(has_read_connection: bool) -> ConnectionTopologyEvaluation {
        let current_topology = if has_read_connection {
            ConnectionTopology::SingleWriterWithReadConnection
        } else {
            ConnectionTopology::SingleWriter
        };
        ConnectionTopologyEvaluation {
            current_topology,
            is_single_writer: !has_read_connection,
            has_read_connection,
            reporting_read_connection_beneficial: false,
            reason: "topology-only evaluation".into(),
            evaluation_order: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_writer_topology_when_no_read_connection() {
        let eval = ConnectionModelEvaluator::evaluate(false, 0, WalPressureLevel::Low, 0, 1);
        assert_eq!(eval.current_topology, ConnectionTopology::SingleWriter);
        assert!(eval.is_single_writer);
        assert!(!eval.has_read_connection);
    }

    #[test]
    fn read_connection_topology_detected() {
        let eval = ConnectionModelEvaluator::evaluate(true, 0, WalPressureLevel::Low, 0, 1);
        assert_eq!(
            eval.current_topology,
            ConnectionTopology::SingleWriterWithReadConnection
        );
        assert!(!eval.is_single_writer);
        assert!(eval.has_read_connection);
    }

    #[test]
    fn low_pressure_no_benefit_with_few_queries() {
        let beneficial =
            ConnectionModelEvaluator::is_read_connection_beneficial(50, WalPressureLevel::Low, 1);
        assert!(!beneficial);
    }

    #[test]
    fn low_pressure_benefit_with_many_queries_and_readers() {
        let beneficial =
            ConnectionModelEvaluator::is_read_connection_beneficial(150, WalPressureLevel::Low, 3);
        assert!(beneficial);
    }

    #[test]
    fn medium_pressure_benefit_with_moderate_load() {
        let beneficial = ConnectionModelEvaluator::is_read_connection_beneficial(
            60,
            WalPressureLevel::Medium,
            2,
        );
        assert!(beneficial);
    }

    #[test]
    fn high_pressure_benefit_with_minimal_load() {
        let beneficial =
            ConnectionModelEvaluator::is_read_connection_beneficial(15, WalPressureLevel::High, 0);
        assert!(beneficial);
    }

    #[test]
    fn determinism_same_input() {
        let e1 = ConnectionModelEvaluator::evaluate(false, 100, WalPressureLevel::Medium, 2, 1);
        let e2 = ConnectionModelEvaluator::evaluate(false, 100, WalPressureLevel::Medium, 2, 1);
        assert_eq!(e1, e2);
    }

    #[test]
    fn serde_round_trip_topology() {
        let topologies = [
            ConnectionTopology::SingleWriter,
            ConnectionTopology::SingleWriterWithReadConnection,
        ];
        for t in &topologies {
            let json = serde_json::to_string(t).unwrap();
            let deserialized: ConnectionTopology = serde_json::from_str(&json).unwrap();
            assert_eq!(*t, deserialized);
        }
    }

    #[test]
    fn serde_round_trip_evaluation() {
        let eval = ConnectionModelEvaluator::evaluate(true, 50, WalPressureLevel::Low, 1, 42);
        let json = serde_json::to_string(&eval).unwrap();
        let deserialized: ConnectionTopologyEvaluation = serde_json::from_str(&json).unwrap();
        assert_eq!(eval, deserialized);
    }

    #[test]
    fn topology_only_evaluation() {
        let eval = ConnectionModelEvaluator::evaluate_topology_only(false);
        assert_eq!(eval.current_topology, ConnectionTopology::SingleWriter);
        assert!(!eval.reporting_read_connection_beneficial);
    }

    #[test]
    fn evaluation_order_preserved() {
        let eval = ConnectionModelEvaluator::evaluate(false, 0, WalPressureLevel::Low, 0, 7);
        assert_eq!(eval.evaluation_order, 7);
    }
}
