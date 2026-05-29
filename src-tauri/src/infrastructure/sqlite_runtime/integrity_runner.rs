use serde::{Deserialize, Serialize};

use crate::infrastructure::sqlite_observability::integrity::{
    IntegrityCheckResult, IntegritySeverity, QuickCheckResult,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntegrityExecutionPolicy {
    CheckOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntegrityExecutionResult {
    pub check_result: IntegrityCheckResult,
    pub quick_check_result: QuickCheckResult,
    pub severity: IntegritySeverity,
    pub execution_order: u64,
}

pub struct IntegrityRunner;

impl IntegrityRunner {
    pub fn execute(
        conn: &rusqlite::Connection,
        _policy: &IntegrityExecutionPolicy,
        execution_order: u64,
    ) -> Result<IntegrityExecutionResult, rusqlite::Error> {
        let check_result = IntegrityCheckResult::from_connection(conn)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let quick_check_result = QuickCheckResult::from_connection(conn)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let severity = match (check_result.passed, quick_check_result.passed) {
            (true, true) => IntegritySeverity::Ok,
            (false, true) => IntegritySeverity::Warning,
            (true, false) => IntegritySeverity::Error,
            (false, false) => {
                let error_count = check_result.issues.len();
                if error_count > 10 {
                    IntegritySeverity::Fatal
                } else if error_count > 5 {
                    IntegritySeverity::Critical
                } else {
                    IntegritySeverity::Error
                }
            }
        };
        Ok(IntegrityExecutionResult {
            check_result,
            quick_check_result,
            severity,
            execution_order,
        })
    }

    pub fn execute_quick_check_only(
        conn: &rusqlite::Connection,
        execution_order: u64,
    ) -> Result<IntegrityExecutionResult, rusqlite::Error> {
        let quick_check_result = QuickCheckResult::from_connection(conn)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let check_result = IntegrityCheckResult {
            passed: quick_check_result.passed,
            issues: vec![],
            raw_output: quick_check_result.raw_output.clone(),
        };
        let severity = if quick_check_result.passed {
            IntegritySeverity::Ok
        } else {
            IntegritySeverity::Error
        };
        Ok(IntegrityExecutionResult {
            check_result,
            quick_check_result,
            severity,
            execution_order,
        })
    }
}

pub struct QuickCheckRunner;

impl QuickCheckRunner {
    pub fn execute(
        conn: &rusqlite::Connection,
        _execution_order: u64,
    ) -> Result<QuickCheckResult, rusqlite::Error> {
        QuickCheckResult::from_connection(conn)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(unused)]
    fn make_mock_ok_check() -> IntegrityCheckResult {
        IntegrityCheckResult::parse("ok")
    }

    #[allow(unused)]
    fn make_mock_ok_quick() -> QuickCheckResult {
        QuickCheckResult::parse("ok")
    }

    #[allow(unused)]
    fn make_mock_failing_check() -> IntegrityCheckResult {
        IntegrityCheckResult::parse(
            "row 5 missing from index idx_products\nwrong page 42 in table products",
        )
    }

    #[allow(unused)]
    fn make_mock_failing_quick() -> QuickCheckResult {
        QuickCheckResult::parse("wrong page 42 in table products")
    }

    #[allow(unused)]
    fn make_result(
        check: IntegrityCheckResult,
        quick: QuickCheckResult,
        order: u64,
    ) -> IntegrityExecutionResult {
        let severity = match (check.passed, quick.passed) {
            (true, true) => IntegritySeverity::Ok,
            (false, true) => IntegritySeverity::Warning,
            (true, false) => IntegritySeverity::Error,
            (false, false) => {
                let error_count = check.issues.len();
                if error_count > 10 {
                    IntegritySeverity::Fatal
                } else if error_count > 5 {
                    IntegritySeverity::Critical
                } else {
                    IntegritySeverity::Error
                }
            }
        };
        IntegrityExecutionResult {
            check_result: check,
            quick_check_result: quick,
            severity,
            execution_order: order,
        }
    }

    #[test]
    fn execution_result_ok() {
        let result = make_result(make_mock_ok_check(), make_mock_ok_quick(), 1);
        assert_eq!(result.severity, IntegritySeverity::Ok);
        assert!(result.check_result.passed);
        assert!(result.quick_check_result.passed);
    }

    #[test]
    fn execution_result_warning() {
        let result = make_result(make_mock_failing_check(), make_mock_ok_quick(), 1);
        assert_eq!(result.severity, IntegritySeverity::Warning);
        assert!(!result.check_result.passed);
    }

    #[test]
    fn execution_result_error() {
        let result = make_result(make_mock_failing_check(), make_mock_failing_quick(), 1);
        assert!(!result.check_result.passed);
        assert!(!result.quick_check_result.passed);
    }

    #[test]
    fn deterministic_issue_ordering() {
        let raw =
            "ok\nrow 5 missing from index idx_a\nrow 3 missing from index idx_b\nwrong page 1";
        let result1 = make_result(
            IntegrityCheckResult::parse(raw),
            QuickCheckResult::parse("ok"),
            1,
        );
        let result2 = make_result(
            IntegrityCheckResult::parse(raw),
            QuickCheckResult::parse("ok"),
            1,
        );
        assert_eq!(result1, result2);
    }

    #[test]
    fn serde_round_trip_execution_result() {
        let result = make_result(make_mock_failing_check(), make_mock_failing_quick(), 42);
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: IntegrityExecutionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, deserialized);
    }

    #[test]
    fn execution_order_preserved() {
        let result = make_result(make_mock_ok_check(), make_mock_ok_quick(), 7);
        assert_eq!(result.execution_order, 7);
    }

    #[test]
    fn same_input_same_execution_result() {
        let raw = "row 5 missing from index idx_products";
        let r1 = make_result(
            IntegrityCheckResult::parse(raw),
            QuickCheckResult::parse("wrong page 42"),
            1,
        );
        let r2 = make_result(
            IntegrityCheckResult::parse(raw),
            QuickCheckResult::parse("wrong page 42"),
            1,
        );
        assert_eq!(r1, r2);
    }
}
