//! Audit Observability Service
//!
//! Provides operational metrics for audit chain health monitoring.
//! All queries are read-only and do not mutate audit state.

use crate::errors::AppError;
use crate::repositories::{DbExecutor, RepositoryProvider};
use serde::{Deserialize, Serialize};

// ─── DTOs ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditChainStatus {
    pub is_valid: bool,
    pub total_entries: i64,
    pub verified_entries: i64,
    pub latest_hash: Option<String>,
    pub latest_entry_timestamp: Option<String>,
    pub first_broken_entry_id: Option<String>,
    pub break_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditHealthReport {
    pub chain_status: AuditChainStatus,
    pub last_import_timestamp: Option<String>,
    pub last_export_timestamp: Option<String>,
    pub last_backup_timestamp: Option<String>,
    pub last_restore_timestamp: Option<String>,
    pub anomalies: Vec<AuditAnomaly>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditAnomaly {
    pub anomaly_type: String,
    pub description: String,
    pub detected_at: String,
    pub severity: String,
}

// ─── Service ─────────────────────────────────────────────────────────────────

pub struct AuditObservabilityService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> AuditObservabilityService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Verify the full audit hash chain and return a detailed status report.
    pub fn get_chain_status(&self) -> Result<AuditChainStatus, AppError> {
        let repo = self.executor.audit();
        let total_entries = repo.count_entries(&crate::domain::audit::AuditQuery {
            user_id: None,
            action: None,
            entity_type: None,
            start_timestamp: None,
            end_timestamp: None,
            status: None,
            search_like: None,
            limit: 1,
            offset: 0,
        })?;

        let latest_hash = repo.fetch_latest_entry_hash()?;

        // Run memory-aware chain verification (bounded peak RAM)
        let summary = repo.verify_audit_chain_streaming()?;

        Ok(AuditChainStatus {
            is_valid: summary.is_valid,
            total_entries,
            verified_entries: summary.verified_entries,
            latest_hash,
            latest_entry_timestamp: summary.latest_entry_timestamp,
            first_broken_entry_id: summary.first_broken_entry_id,
            break_description: summary.break_description,
        })
    }

    /// Build a full health report including last sync/backup timestamps and anomalies.
    pub fn get_health_report(&self) -> Result<AuditHealthReport, AppError> {
        let chain_status = self.get_chain_status()?;
        let repo = self.executor.audit();

        // Last import timestamp
        let last_import_timestamp = repo
            .find_latest_action_timestamp("ImportDailyReportPackage")?
            .or_else(|| {
                repo.find_latest_action_timestamp("ImportProductsPackage")
                    .ok()
                    .flatten()
            })
            .or_else(|| {
                repo.find_latest_action_timestamp("ImportMonthlySummaryPackage")
                    .ok()
                    .flatten()
            });

        // Last export timestamp
        let last_export_timestamp = repo
            .find_latest_action_timestamp("ExportDailyReport")?
            .or_else(|| {
                repo.find_latest_action_timestamp("ExportProducts")
                    .ok()
                    .flatten()
            });

        // Last backup/restore timestamps
        let last_backup_timestamp = repo.find_latest_action_timestamp("BackupCreated")?;
        let last_restore_timestamp = repo.find_latest_action_timestamp("BackupRestored")?;

        // Detect anomalies
        let mut anomalies = Vec::new();
        if !chain_status.is_valid {
            anomalies.push(AuditAnomaly {
                anomaly_type: "CHAIN_INTEGRITY_VIOLATION".to_string(),
                description: chain_status
                    .break_description
                    .clone()
                    .unwrap_or_else(|| "Hash chain integrity compromised".to_string()),
                detected_at: chrono::Utc::now().to_rfc3339(),
                severity: "CRITICAL".to_string(),
            });
        }

        // Warn if no backup in last 7 days
        if let Some(ref last_bk) = last_backup_timestamp {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(last_bk) {
                let age = chrono::Utc::now() - dt.with_timezone(&chrono::Utc);
                if age.num_days() > 7 {
                    anomalies.push(AuditAnomaly {
                        anomaly_type: "STALE_BACKUP".to_string(),
                        description: format!(
                            "Last backup was {} days ago. Recommend creating a new backup.",
                            age.num_days()
                        ),
                        detected_at: chrono::Utc::now().to_rfc3339(),
                        severity: "WARNING".to_string(),
                    });
                }
            }
        } else {
            anomalies.push(AuditAnomaly {
                anomaly_type: "NO_BACKUP_RECORDED".to_string(),
                description: "No backup has been recorded in the audit log.".to_string(),
                detected_at: chrono::Utc::now().to_rfc3339(),
                severity: "WARNING".to_string(),
            });
        }

        Ok(AuditHealthReport {
            chain_status,
            last_import_timestamp,
            last_export_timestamp,
            last_backup_timestamp,
            last_restore_timestamp,
            anomalies,
        })
    }
}
