use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntegritySeverity {
    Ok,
    Warning,
    Error,
    Critical,
    Fatal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntegrityIssue {
    pub line_number: usize,
    pub severity: IntegritySeverity,
    pub description: String,
    pub page_number: Option<u64>,
    pub table_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntegrityCheckResult {
    pub passed: bool,
    pub issues: Vec<IntegrityIssue>,
    pub raw_output: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickCheckResult {
    pub passed: bool,
    pub issue: Option<IntegrityIssue>,
    pub raw_output: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntegritySnapshot {
    pub check_result: IntegrityCheckResult,
    pub quick_check_result: QuickCheckResult,
    pub severity: IntegritySeverity,
}

impl IntegrityCheckResult {
    pub fn parse(raw: &str) -> Self {
        let lines: Vec<&str> = raw.lines().collect();
        let passed = lines.iter().any(|l| l.trim() == "ok");
        let issues: Vec<IntegrityIssue> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.trim() != "ok" && !l.trim().is_empty())
            .map(|(i, l)| {
                let severity = classify_integrity_severity(l);
                let page_number = extract_page_number(l);
                let table_name = extract_table_name_integrity(l);
                IntegrityIssue {
                    line_number: i + 1,
                    severity,
                    description: l.to_string(),
                    page_number,
                    table_name,
                }
            })
            .collect();
        Self {
            passed,
            issues,
            raw_output: raw.to_string(),
        }
    }

    pub fn from_connection(conn: &rusqlite::Connection) -> crate::errors::AppResult<Self> {
        use crate::errors::DatabaseError;
        let raw: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| {
                crate::errors::AppError::Database(DatabaseError::QueryExecution {
                    message: format!("integrity_check failed: {}", e),
                })
            })?;
        Ok(Self::parse(&raw))
    }
}

impl QuickCheckResult {
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        let passed = trimmed == "ok";
        let issue = if !passed && !trimmed.is_empty() {
            Some(IntegrityIssue {
                line_number: 1,
                severity: classify_integrity_severity(trimmed),
                description: trimmed.to_string(),
                page_number: extract_page_number(trimmed),
                table_name: extract_table_name_integrity(trimmed),
            })
        } else {
            None
        };
        Self {
            passed,
            issue,
            raw_output: raw.to_string(),
        }
    }

    pub fn from_connection(conn: &rusqlite::Connection) -> crate::errors::AppResult<Self> {
        use crate::errors::DatabaseError;
        let raw: String = conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(|e| {
                crate::errors::AppError::Database(DatabaseError::QueryExecution {
                    message: format!("quick_check failed: {}", e),
                })
            })?;
        Ok(Self::parse(&raw))
    }
}

impl IntegritySnapshot {
    pub fn new(check: IntegrityCheckResult, quick: QuickCheckResult) -> Self {
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
        Self {
            check_result: check,
            quick_check_result: quick,
            severity,
        }
    }
}

fn classify_integrity_severity(line: &str) -> IntegritySeverity {
    let lower = line.to_lowercase();
    if lower.contains("row ") && lower.contains(" missing from index ") {
        IntegritySeverity::Error
    } else if lower.contains("wrong") || lower.contains("invalid") || lower.contains("corrupt") {
        IntegritySeverity::Critical
    } else if lower.contains("missing") || lower.contains("null") {
        IntegritySeverity::Warning
    } else {
        IntegritySeverity::Error
    }
}

fn extract_page_number(line: &str) -> Option<u64> {
    let re = regex::Regex::new(r"\bpage\s+(\d+)\b").ok()?;
    re.captures(line)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

fn extract_table_name_integrity(line: &str) -> Option<String> {
    let re = regex::Regex::new(r#""?(\w+)"?\s*(?:table|index)"#).ok()?;
    re.captures(line)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_integrity_check_ok() {
        let result = IntegrityCheckResult::parse("ok");
        assert!(result.passed);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn parses_integrity_check_with_errors() {
        let raw = "row 5 missing from index idx_products\nwrong page 42 in table products";
        let result = IntegrityCheckResult::parse(raw);
        assert!(!result.passed);
        assert_eq!(result.issues.len(), 2);
    }

    #[test]
    fn parses_quick_check_ok() {
        let result = QuickCheckResult::parse("ok");
        assert!(result.passed);
        assert!(result.issue.is_none());
    }

    #[test]
    fn parses_quick_check_failure() {
        let result = QuickCheckResult::parse("wrong page 42 in table products");
        assert!(!result.passed);
        assert!(result.issue.is_some());
        assert_eq!(result.issue.unwrap().severity, IntegritySeverity::Critical);
    }

    #[test]
    fn integrity_severity_classification_ok() {
        let snapshot = IntegritySnapshot::new(
            IntegrityCheckResult::parse("ok"),
            QuickCheckResult::parse("ok"),
        );
        assert_eq!(snapshot.severity, IntegritySeverity::Ok);
    }

    #[test]
    fn integrity_severity_escalates() {
        let raw = "row 5 missing from index idx_products\nrow 6 missing from index idx_stock\nwrong page 42\ncorrupt index idx_main\nmissing table t1\nnull value in col";
        let check = IntegrityCheckResult::parse(raw);
        let quick = QuickCheckResult::parse("wrong page 42 in table products");
        let snapshot = IntegritySnapshot::new(check, quick);
        assert_eq!(snapshot.severity, IntegritySeverity::Critical);
    }

    #[test]
    fn deterministic_issue_ordering() {
        let raw =
            "ok\nrow 5 missing from index idx_a\nrow 3 missing from index idx_b\nwrong page 1";
        let result1 = IntegrityCheckResult::parse(raw);
        let result2 = IntegrityCheckResult::parse(raw);
        assert_eq!(result1, result2);
    }

    #[test]
    fn serde_round_trip_integrity_check() {
        let result = IntegrityCheckResult::parse("ok\nrow 5 missing from index idx_products");
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: IntegrityCheckResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, deserialized);
    }

    #[test]
    fn serde_round_trip_snapshot() {
        let check = IntegrityCheckResult::parse("ok");
        let quick = QuickCheckResult::parse("ok");
        let snapshot = IntegritySnapshot::new(check, quick);
        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: IntegritySnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot, deserialized);
    }
}
