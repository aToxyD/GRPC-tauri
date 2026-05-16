use crate::domain::audit::AuditQuery;
use crate::errors::AppError;
use crate::repositories::{DbExecutor, RepositoryProvider};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiscalTransitionHistoryEntry {
    pub transition_id: String,
    pub timestamp: String,
    pub action: String,
    pub fiscal_year: i32,
    pub next_year: i32,
    pub actor: String,
    pub execution_node: Option<String>,
    pub signing_key_id: Option<String>,
    pub package_fingerprint: Option<String>,
    pub source: String,
    pub status: String,
}

pub struct FiscalTransitionHistoryService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalTransitionHistoryService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Returns deterministic newest-first timeline entries combining:
    /// - applied_fiscal_transitions
    /// - audit_log entries:
    ///   - FiscalClosurePackageExported
    ///   - FiscalClosurePackageApplied
    ///   - FiscalYearClosed
    ///   - FiscalYearArchived
    pub fn build_transition_history(&self) -> Result<Vec<FiscalTransitionHistoryEntry>, AppError> {
        let mut history = Vec::new();

        // 1. Fetch relevant audit logs
        let actions = vec![
            "FiscalYearClosed",
            "FiscalYearArchived",
            "FiscalClosurePackageExported",
            "FiscalClosurePackageApplied",
        ];

        for action in actions {
            let query = AuditQuery {
                user_id: None,
                action: Some(action.to_string()),
                entity_type: None,
                start_timestamp: None,
                end_timestamp: None,
                status: Some("Success".to_string()),
                search_like: None,
                limit: 200,
                offset: 0,
            };

            let entries = self.executor.audit().fetch_entries(&query)?;
            for entry in entries {
                let metadata: Option<serde_json::Value> = entry
                    .metadata_json
                    .as_ref()
                    .and_then(|m| serde_json::from_str(m).ok());

                let transition_id = metadata
                    .as_ref()
                    .and_then(|m| m.get("fiscal_transition_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "N/A".to_string());

                let closed_year = metadata
                    .as_ref()
                    .and_then(|m| m.get("closed_year"))
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32)
                    .or_else(|| {
                        entry
                            .entity_id
                            .as_ref()
                            .and_then(|id| id.parse::<i32>().ok())
                    });

                let closed_year = match closed_year {
                    Some(y) => y,
                    None => continue, // Skip entries without a valid fiscal year
                };

                let opened_year = match metadata
                    .as_ref()
                    .and_then(|m| m.get("opened_year"))
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32)
                {
                    Some(y) => y,
                    None => closed_year + 1,
                };

                let status = match entry.action.as_str() {
                    "FiscalYearClosed" => "FISCAL_YEAR_CLOSED",
                    "FiscalYearArchived" => "FISCAL_YEAR_ARCHIVED",
                    "FiscalClosurePackageExported" => "PACKAGE_EXPORTED",
                    "FiscalClosurePackageApplied" => "PACKAGE_APPLIED",
                    _ => continue,
                };

                let source = if entry.action == "FiscalClosurePackageExported"
                    || entry.action == "FiscalYearClosed"
                {
                    "AUTHORITY"
                } else {
                    "EXECUTION"
                };

                history.push(FiscalTransitionHistoryEntry {
                    transition_id,
                    timestamp: entry.timestamp,
                    action: entry.action,
                    fiscal_year: closed_year,
                    next_year: opened_year,
                    actor: entry.username,
                    execution_node: metadata
                        .as_ref()
                        .and_then(|m| m.get("authority_node_id"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    signing_key_id: metadata
                        .as_ref()
                        .and_then(|m| m.get("signing_key_id"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    package_fingerprint: metadata
                        .as_ref()
                        .and_then(|m| m.get("package_fingerprint"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    source: source.to_string(),
                    status: status.to_string(),
                });
            }
        }

        // 2. Fetch rejections from audit logs
        let rejection_actions = vec!["CarryForwardRejected"];
        for action in rejection_actions {
            let query = AuditQuery {
                user_id: None,
                action: Some(action.to_string()),
                entity_type: None,
                start_timestamp: None,
                end_timestamp: None,
                status: None,
                search_like: None,
                limit: 100,
                offset: 0,
            };
            let entries = self.executor.audit().fetch_entries(&query)?;
            for entry in entries {
                let metadata: Option<serde_json::Value> = entry
                    .metadata_json
                    .as_ref()
                    .and_then(|m| serde_json::from_str(m).ok());

                let reason = entry
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string());
                let status = if reason.contains("expired") {
                    "EXECUTION_WINDOW_EXPIRED"
                } else if reason.contains("replay") || reason.contains("already been applied") {
                    "REPLAY_REJECTED"
                } else {
                    "EXECUTION_REJECTED"
                };

                let fiscal_year = metadata
                    .as_ref()
                    .and_then(|m| m.get("closed_year"))
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32);

                let fiscal_year = match fiscal_year {
                    Some(y) => y,
                    None => continue, // Skip
                };

                let opened_year = match metadata
                    .as_ref()
                    .and_then(|m| m.get("opened_year"))
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32)
                {
                    Some(y) => y,
                    None => fiscal_year + 1,
                };

                history.push(FiscalTransitionHistoryEntry {
                    transition_id: metadata
                        .as_ref()
                        .and_then(|m| m.get("fiscal_transition_id"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
                    timestamp: entry.timestamp,
                    action: entry.action,
                    fiscal_year,
                    next_year: opened_year,
                    actor: entry.username,
                    execution_node: None,
                    signing_key_id: None,
                    package_fingerprint: None,
                    source: "EXECUTION".to_string(),
                    status: status.to_string(),
                });
            }
        }

        // 3. Sort newest-first (deterministic)
        history.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(history)
    }
}

impl crate::architecture::Service for FiscalTransitionHistoryService<'_> {}
