use serde::{Deserialize, Serialize};

use crate::infrastructure::sqlite_observability::integrity::IntegrityIssue;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupValidationResult {
    pub passed: bool,
    pub issues: Vec<IntegrityIssue>,
    pub integrity_raw_output: String,
    pub page_count: u64,
    pub page_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupValidationFailure {
    pub reason: String,
}

pub struct BackupValidationRunner;

impl BackupValidationRunner {
    pub fn validate(
        path: &std::path::Path,
    ) -> Result<BackupValidationResult, BackupValidationFailure> {
        let conn = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| BackupValidationFailure {
            reason: format!("cannot open backup file: {}", e),
        })?;

        Self::validate_connection(&conn)
    }

    pub fn validate_connection(
        conn: &rusqlite::Connection,
    ) -> Result<BackupValidationResult, BackupValidationFailure> {
        let raw: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| BackupValidationFailure {
                reason: format!("integrity_check query failed: {}", e),
            })?;

        let page_count: i64 = conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .map_err(|e| BackupValidationFailure {
                reason: format!("page_count query failed: {}", e),
            })?;

        let page_size: i64 = conn
            .query_row("PRAGMA page_size", [], |r| r.get(0))
            .map_err(|e| BackupValidationFailure {
                reason: format!("page_size query failed: {}", e),
            })?;

        let lines: Vec<&str> = raw.lines().collect();
        let passed = lines.iter().any(|l| l.trim() == "ok");
        let issues: Vec<IntegrityIssue> = if passed {
            vec![]
        } else {
            lines
                .iter()
                .enumerate()
                .filter(|(_, l)| l.trim() != "ok" && !l.trim().is_empty())
                .map(|(i, l)| {
                    use crate::infrastructure::sqlite_observability::integrity::IntegritySeverity;
                    IntegrityIssue {
                        line_number: i + 1,
                        severity: IntegritySeverity::Error,
                        description: l.to_string(),
                        page_number: None,
                        table_name: None,
                    }
                })
                .collect()
        };

        Ok(BackupValidationResult {
            passed,
            issues,
            integrity_raw_output: raw,
            page_count: page_count.max(0) as u64,
            page_size: page_size.max(0) as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip_validation_result() {
        let result = BackupValidationResult {
            passed: true,
            issues: vec![],
            integrity_raw_output: "ok".into(),
            page_count: 42,
            page_size: 4096,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: BackupValidationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, deserialized);
    }

    #[test]
    fn serde_round_trip_failure() {
        let failure = BackupValidationFailure {
            reason: "cannot open file".into(),
        };
        let json = serde_json::to_string(&failure).unwrap();
        let deserialized: BackupValidationFailure = serde_json::from_str(&json).unwrap();
        assert_eq!(failure, deserialized);
    }

    #[test]
    fn valid_backup_file_passes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute("CREATE TABLE t (id INTEGER)", []).unwrap();
            conn.execute("INSERT INTO t VALUES (1)", []).unwrap();
        }
        let result = BackupValidationRunner::validate(&path).unwrap();
        assert!(result.passed);
        assert!(result.issues.is_empty());
        assert_eq!(result.integrity_raw_output.trim(), "ok");
    }

    #[test]
    fn invalid_file_returns_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not_a_db.bak");
        std::fs::write(&path, b"not a sqlite database").unwrap();
        let result = BackupValidationRunner::validate(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.reason.contains("cannot open") || err.reason.contains("not a database"),
            "error should mention open failure: {}",
            err.reason
        );
    }

    #[test]
    fn non_existent_file_returns_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.db");
        let result = BackupValidationRunner::validate(&path);
        assert!(result.is_err());
    }
}
