//! System Health Service
//!
//! Aggregates operational health metrics across the node:
//! database, backups, audit integrity, sync queue, and storage.

use crate::db::Database;
use crate::errors::AppError;
use crate::repositories::RepositoryProvider;
use serde::{Deserialize, Serialize};

// ─── DTOs ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemHealthReport {
    pub database_status: ComponentHealth,
    pub backup_status: BackupHealth,
    pub audit_status: ComponentHealth,
    pub sync_status: SyncHealth,
    pub storage_usage_bytes: u64,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentHealth {
    pub status: HealthStatus,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Critical,
    Unknown,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "HEALTHY"),
            Self::Degraded => write!(f, "DEGRADED"),
            Self::Critical => write!(f, "CRITICAL"),
            Self::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupHealth {
    pub status: HealthStatus,
    pub last_backup: Option<String>,
    pub backup_count: u32,
    pub backup_age_days: Option<i64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncHealth {
    pub status: HealthStatus,
    pub failed_imports_count: i64,
    pub failed_decryptions_count: i64,
    pub duplicate_package_attempts: i64,
    pub unresolved_conflicts: i64,
    pub last_successful_sync: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncNodeHealth {
    pub node_id: String,
    pub node_name: String,
    pub last_sync_timestamp: Option<String>,
    pub packages_received: i64,
    pub packages_rejected: i64,
    pub replay_attempts: i64,
    pub status: HealthStatus,
}

// ─── Service ─────────────────────────────────────────────────────────────────

pub struct SystemHealthService;

impl SystemHealthService {
    /// Build a full system health report from the current database state.
    pub fn get_health_report(db: &Database) -> Result<SystemHealthReport, AppError> {
        let executor = db.executor();
        let now = chrono::Utc::now().to_rfc3339();

        // Database health — if we can reach this point, the DB is accessible
        let database_status = ComponentHealth {
            status: HealthStatus::Healthy,
            message: "قاعدة البيانات تعمل بشكل طبيعي".to_string(),
            details: None,
        };

        // Backup health
        let backup_status = Self::compute_backup_health(db)?;

        // Audit chain health
        let audit_status = Self::compute_audit_health(db)?;

        // Sync health
        let sync_status = Self::compute_sync_health(executor, db)?;

        // Storage
        let storage_usage_bytes = db
            .get_connection_path()
            .ok()
            .and_then(|p| std::fs::metadata(&p).ok())
            .map(|m| m.len())
            .unwrap_or(0); // [arch:allow-unwrap-or] see ADR-0007 — Reason: 0 bytes safe default when path/metadata unavailable — not an error condition; Date: 2026-08-09; Owner: Architecture

        Ok(SystemHealthReport {
            database_status,
            backup_status,
            audit_status,
            sync_status,
            storage_usage_bytes,
            generated_at: now,
        })
    }

    fn compute_backup_health(db: &Database) -> Result<BackupHealth, AppError> {
        let db_path = db
            .get_connection_path()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        let backup_dir =
            crate::infrastructure::backup::SqliteBackupAdapter::compute_backup_dir(&db_path);

        let (backup_count, last_backup, backup_age_days) = if backup_dir.exists() {
            let mut count = 0u32;
            let mut latest: Option<std::time::SystemTime> = None;

            if let Ok(entries) = std::fs::read_dir(&backup_dir) {
                for entry in entries.flatten() {
                    if let Ok(meta) = entry.metadata() {
                        count += 1;
                        if let Ok(modified) = meta.modified() {
                            match latest {
                                Some(t) if modified > t => latest = Some(modified),
                                None => latest = Some(modified),
                                _ => {}
                            }
                        }
                    }
                }
            }

            let last_ts = latest.and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH).ok().and_then(|d| {
                    chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0)
                        .map(|dt| dt.to_rfc3339())
                })
            });

            let age = last_ts.as_deref().and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|dt| (chrono::Utc::now() - dt.with_timezone(&chrono::Utc)).num_days())
            });

            (count, last_ts, age)
        } else {
            (0, None, None)
        };

        let (status, message) = match backup_age_days {
            None => (
                HealthStatus::Degraded,
                "لا توجد نسخ احتياطية مسجلة".to_string(),
            ),
            Some(days) if days > 14 => (
                HealthStatus::Critical,
                format!(
                    "آخر نسخة احتياطية منذ {} يوم — يجب إنشاء نسخة جديدة فوراً",
                    days
                ),
            ),
            Some(days) if days > 7 => (
                HealthStatus::Degraded,
                format!(
                    "آخر نسخة احتياطية منذ {} أيام — يُنصح بإنشاء نسخة جديدة",
                    days
                ),
            ),
            Some(_) => (HealthStatus::Healthy, "النسخ الاحتياطية محدثة".to_string()),
        };

        // Suppress unused variable warning — db param kept for future extension
        let _ = db;

        Ok(BackupHealth {
            status,
            last_backup,
            backup_count,
            backup_age_days,
            message,
        })
    }

    fn compute_audit_health(db: &Database) -> Result<ComponentHealth, AppError> {
        let executor = db.executor();
        let repo = executor.audit();

        let start_time = std::time::Instant::now();
        let summary = repo.verify_audit_chain_streaming()?;
        let duration = start_time.elapsed().as_millis() as i64;

        let outcome = if summary.is_valid {
            crate::application::services::TelemetryOutcome::Success
        } else {
            crate::application::services::TelemetryOutcome::Failure
        };

        let _ = crate::application::services::TelemetryService::new(executor).record_event(
            crate::application::services::TelemetryEventType::IntegrityCheck,
            outcome,
            Some(duration),
            Some(serde_json::json!({ "entries": summary.verified_entries })),
            None,
        );

        if summary.is_valid {
            Ok(ComponentHealth {
                status: HealthStatus::Healthy,
                message: "سلسلة التدقيق سليمة".to_string(),
                details: None,
            })
        } else {
            Ok(ComponentHealth {
                status: HealthStatus::Critical,
                message: "تم اكتشاف خرق في سلسلة التدقيق".to_string(),
                details: summary.break_description,
            })
        }
    }

    fn compute_sync_health(
        executor: crate::repositories::DbExecutor<'_>,
        _db: &Database,
    ) -> Result<SyncHealth, AppError> {
        let conflicts_repo = executor.sync_conflicts();
        let unresolved_conflicts = conflicts_repo.count_unresolved()?;

        // Count failed imports and duplicate attempts from conflicts table
        let duplicate_package_attempts =
            conflicts_repo.count_by_conflict_type("DUPLICATE_PACKAGE")?;
        let _replay_attempts = conflicts_repo.count_by_conflict_type("REPLAY_ATTEMPT")?;
        let failed_imports_count = conflicts_repo.count_by_conflict_type("STALE_IMPORT")?;

        let last_successful_sync = executor
            .audit()
            .find_latest_action_timestamp("ImportDailyReportPackage")?;

        let (status, message) = if unresolved_conflicts == 0 {
            (
                HealthStatus::Healthy,
                "لا توجد تعارضات في المزامنة".to_string(),
            )
        } else if unresolved_conflicts < 5 {
            (
                HealthStatus::Degraded,
                format!("{} تعارض غير محلول يتطلب مراجعة", unresolved_conflicts),
            )
        } else {
            (
                HealthStatus::Critical,
                format!(
                    "{} تعارض غير محلول — يتطلب تدخلاً فورياً",
                    unresolved_conflicts
                ),
            )
        };

        Ok(SyncHealth {
            status,
            failed_imports_count,
            failed_decryptions_count: 0, // tracked via conflict type in future
            duplicate_package_attempts,
            unresolved_conflicts,
            last_successful_sync,
            message,
        })
    }

    /// Get per-node sync health metrics.
    pub fn get_sync_node_health(db: &Database) -> Result<Vec<SyncNodeHealth>, AppError> {
        let executor = db.executor();
        let conflicts_repo = executor.sync_conflicts();
        let node_metrics = conflicts_repo.get_per_node_metrics()?;

        Ok(node_metrics
            .into_iter()
            .map(|m| {
                let status = if m.packages_rejected == 0 && m.replay_attempts == 0 {
                    HealthStatus::Healthy
                } else if m.packages_rejected < 3 {
                    HealthStatus::Degraded
                } else {
                    HealthStatus::Critical
                };

                SyncNodeHealth {
                    node_id: m.node_id.clone(),
                    node_name: m.node_id, // use node_id as display name for now
                    last_sync_timestamp: m.last_sync_timestamp,
                    packages_received: m.packages_received,
                    packages_rejected: m.packages_rejected,
                    replay_attempts: m.replay_attempts,
                    status,
                }
            })
            .collect())
    }
}
