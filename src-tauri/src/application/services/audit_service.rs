use crate::domain::audit::{
    AuditAction, AuditEntry, AuditEntryDbRow, AuditFilters, AuditLogResponse, AuditQuery,
    AuditStats, AuditStatus, EntityType, NewAuditEntry,
};
use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use crate::repositories::RepositoryProvider;
use chrono::Utc;
use uuid::Uuid;

pub struct AuditService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> AuditService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn log_success(
        &self,
        user_id: &str,
        username: &str,
        action: AuditAction,
        entity_type: EntityType,
        entity_id: Option<&str>,
        entity_name: Option<&str>,
        old_value: Option<serde_json::Value>,
        new_value: Option<serde_json::Value>,
        session_id: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<(), AppError> {
        let entry = NewAuditEntry {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            username: username.to_string(),
            action: action.as_str().to_string(),
            entity_type: entity_type.as_str().to_string(),
            entity_id: entity_id.map(|s| s.to_string()),
            entity_name: entity_name.map(|s| s.to_string()),
            old_value: old_value.map(|v| v.to_string()),
            new_value: new_value.map(|v| v.to_string()),
            session_id: session_id.map(|s| s.to_string()),
            timestamp: Utc::now().to_rfc3339(),
            status: AuditStatus::Success.as_str().to_string(),
            error_message: None,
            metadata: metadata.map(|v| v.to_string()),
            previous_hash: None,
            entry_hash: None,
        };

        self.insert_audit_log(&entry)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn log_failure(
        &self,
        user_id: &str,
        username: &str,
        action: AuditAction,
        entity_type: EntityType,
        entity_id: Option<&str>,
        error_message: &str,
        session_id: Option<&str>,
    ) -> Result<(), AppError> {
        let entry = NewAuditEntry {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            username: username.to_string(),
            action: action.as_str().to_string(),
            entity_type: entity_type.as_str().to_string(),
            entity_id: entity_id.map(|s| s.to_string()),
            entity_name: None,
            old_value: None,
            new_value: None,
            session_id: session_id.map(|s| s.to_string()),
            timestamp: Utc::now().to_rfc3339(),
            status: AuditStatus::Failed.as_str().to_string(),
            error_message: Some(error_message.to_string()),
            metadata: None,
            previous_hash: None,
            entry_hash: None,
        };

        self.insert_audit_log(&entry)
    }

    pub fn insert_audit_log(&self, entry: &NewAuditEntry) -> Result<(), AppError> {
        let repo = self.executor.audit();
        let prev_tip = repo.fetch_latest_entry_hash()?;
        let digest = crate::domain::audit_chain::compute_entry_hash(prev_tip.as_deref(), entry);
        let chained = NewAuditEntry {
            previous_hash: prev_tip,
            entry_hash: Some(digest),
            ..entry.clone()
        };
        repo.insert_audit_log(&chained)
    }

    /// Recompute chain digests for all rows that have `entry_hash` set; fails on first mismatch.
    pub fn verify_audit_hash_chain(&self) -> Result<(), AppError> {
        let summary = self.executor.audit().verify_audit_chain_streaming()?;
        if summary.is_valid {
            Ok(())
        } else {
            Err(AppError::Internal(
                summary
                    .break_description
                    .unwrap_or_else(|| "Audit chain broken".to_string()),
            ))
        }
    }

    pub fn get_audit_entries(
        &self,
        filters: &AuditFilters,
        page: usize,
        page_size: usize,
    ) -> Result<AuditLogResponse, AppError> {
        let limit = page_size as i64;
        let offset = (page * page_size) as i64;

        let q = AuditQuery {
            user_id: filters.user_id.clone(),
            action: filters.action.clone(),
            entity_type: filters.entity_type.clone(),
            start_timestamp: filters
                .start_date
                .as_ref()
                .map(|d| format!("{d}T00:00:00Z")),
            end_timestamp: filters.end_date.as_ref().map(|d| format!("{d}T23:59:59Z")),
            status: filters.status.clone(),
            search_like: filters.search.as_ref().map(|s| format!("%{s}%")),
            limit,
            offset,
        };

        let repo = self.executor.audit();
        let total_count = repo.count_entries(&q)?;
        let rows = repo.fetch_entries(&q)?;
        let entries = rows
            .into_iter()
            .map(db_row_to_audit_entry)
            .collect::<Result<Vec<_>, _>>()?;

        let has_more = (offset + entries.len() as i64) < total_count;
        Ok(AuditLogResponse {
            entries,
            total_count,
            page,
            page_size,
            has_more,
        })
    }

    pub fn get_audit_statistics(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<AuditStats, AppError> {
        let start_ts = format!("{start_date}T00:00:00Z");
        let end_ts = format!("{end_date}T23:59:59Z");

        let repo = self.executor.audit();
        let total_ops = repo.count_total_ops(&start_ts, &end_ts)?;
        let failed_ops = repo.count_failed_ops(&start_ts, &end_ts)?;

        let most_active_users = repo
            .top_active_users(&start_ts, &end_ts)?
            .into_iter()
            .map(|r| crate::models::UserActivitySummary {
                user_id: r.user_id,
                username: r.username,
                total_operations: r.total_operations,
                failed_operations: r.failed_operations,
                last_activity: r.last_activity,
            })
            .collect();

        let operations_by_type = repo
            .ops_by_type(&start_ts, &end_ts)?
            .into_iter()
            .map(|r| {
                let action = AuditAction::parse(&r.action).ok_or_else(|| {
                    AppError::Internal(format!("Unknown audit action '{}'", r.action))
                })?;
                Ok(crate::models::OperationCount {
                    action: r.action,
                    action_display: action.display_arabic().to_string(),
                    count: r.count,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;

        let operations_by_day = repo
            .ops_by_day(&start_ts, &end_ts)?
            .into_iter()
            .map(|r| crate::models::DailyOperationCount {
                date: r.date,
                total: r.total,
                failed: r.failed,
            })
            .collect();

        let success_rate = if total_ops > 0 {
            ((total_ops - failed_ops) as f64 / total_ops as f64) * 100.0
        } else {
            100.0
        };

        Ok(crate::models::AuditStats {
            total_operations: total_ops,
            failed_operations: failed_ops,
            success_rate,
            most_active_users,
            operations_by_type,
            operations_by_day,
            period_start: start_date.to_string(),
            period_end: end_date.to_string(),
        })
    }

    pub fn update_daily_summary(&self, date: &str) -> Result<(), AppError> {
        let start_ts = format!("{date}T00:00:00Z");
        let end_ts = format!("{date}T23:59:59Z");

        let repo = self.executor.audit();
        let total = repo.count_total_ops(&start_ts, &end_ts)?;
        let failed = repo.count_failed_ops(&start_ts, &end_ts)?;
        let users_active = repo.count_distinct_users(&start_ts, &end_ts)?;
        let now = Utc::now().to_rfc3339();
        repo.upsert_daily_summary(date, total, failed, users_active, &now)
    }

    pub fn get_user_audit_activity(
        &self,
        user_id: &str,
        days: i32,
    ) -> Result<Vec<AuditEntry>, AppError> {
        let since = chrono::Utc::now() - chrono::Duration::days(days as i64);
        let since_str = since.to_rfc3339();

        self.executor
            .audit()
            .fetch_user_activity_since(user_id, &since_str)?
            .into_iter()
            .map(db_row_to_audit_entry)
            .collect()
    }

    pub fn export_audit_to_excel(
        &self,
        filters: &AuditFilters,
        file_path: &str,
    ) -> Result<usize, AppError> {
        let mut all_entries = Vec::new();
        let chunk_size = 1000usize;
        let mut current_page = 0usize;
        let max_entries = 50_000usize;

        let mut actual_filters = filters.clone();
        if actual_filters.start_date.is_none() && actual_filters.end_date.is_none() {
            let thirty_days_ago = chrono::Utc::now() - chrono::Duration::days(30);
            actual_filters.start_date = Some(thirty_days_ago.format("%Y-%m-%d").to_string());
            actual_filters.end_date = Some(chrono::Utc::now().format("%Y-%m-%d").to_string());
        }

        loop {
            let response = self.get_audit_entries(&actual_filters, current_page, chunk_size)?;
            if response.entries.is_empty() {
                break;
            }
            all_entries.extend(response.entries);

            if all_entries.len() >= max_entries {
                all_entries.truncate(max_entries);
                break;
            }

            if !response.has_more {
                break;
            }
            current_page += 1;
        }

        use crate::domain::ports::export::ExcelPort;
        use crate::infrastructure::export::XlsxAdapter;
        use std::fs;

        let adapter = XlsxAdapter::new();
        let buffer = adapter
            .export_audit_log(&all_entries)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        fs::write(file_path, buffer).map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(all_entries.len())
    }

    pub fn cleanup_old_audit_logs(&self, before_date: Option<String>) -> Result<u64, AppError> {
        let cutoff = before_date.unwrap_or_else(|| {
            let one_year_ago = chrono::Utc::now() - chrono::Duration::days(365);
            one_year_ago.to_rfc3339()
        });

        self.executor.audit().delete_older_than(&cutoff)
    }
}

fn parse_json_opt(s: Option<String>) -> Result<Option<serde_json::Value>, AppError> {
    match s {
        None => Ok(None),
        Some(raw) => {
            let v: serde_json::Value = serde_json::from_str(&raw)
                .map_err(|e| AppError::Internal(format!("Invalid JSON stored in DB: {e}")))?;
            Ok(Some(v))
        }
    }
}

fn db_row_to_audit_entry(r: AuditEntryDbRow) -> Result<AuditEntry, AppError> {
    let action = AuditAction::parse(&r.action)
        .ok_or_else(|| AppError::Internal(format!("Unknown audit action '{}'", r.action)))?;
    let entity_type = EntityType::parse(&r.entity_type)
        .ok_or_else(|| AppError::Internal(format!("Unknown entity_type '{}'", r.entity_type)))?;
    let status = AuditStatus::parse(&r.status)
        .ok_or_else(|| AppError::Internal(format!("Unknown audit status '{}'", r.status)))?;

    Ok(AuditEntry {
        id: r.id,
        user_id: r.user_id,
        username: r.username,
        action: action.clone(),
        action_display: action.display_arabic().to_string(),
        entity_type: entity_type.clone(),
        entity_type_display: entity_type.display_arabic().to_string(),
        entity_id: r.entity_id,
        entity_name: r.entity_name,
        old_value: parse_json_opt(r.old_value_json)?,
        new_value: parse_json_opt(r.new_value_json)?,
        session_id: r.session_id,
        timestamp: r.timestamp,
        status,
        error_message: r.error_message,
        metadata: parse_json_opt(r.metadata_json)?,
        previous_hash: r.previous_hash,
        entry_hash: r.entry_hash,
    })
}
