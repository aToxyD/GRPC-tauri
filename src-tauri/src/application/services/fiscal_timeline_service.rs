//! Fiscal Timeline Service
//!
//! Read-only reconstruction of a unified chronological timeline of all
//! operationally-significant events the system has already recorded.
//!
//! Hard constraints:
//!   - READ-ONLY (no INSERT/UPDATE/DELETE)
//!   - APPEND-ONLY semantics (existing events never rewritten)
//!   - NOT an event-sourcing engine
//!   - NOT a replay engine (we do not derive state from these events)
//!
//! Sources unified into a single timeline:
//!   - Fiscal closes / opens             (audit_log: FiscalYearClosed/Opened)
//!   - Archival events                   (audit_log: FiscalYearArchived if present, fiscal_year_status.archived)
//!   - Imports                           (audit_log: Import* + import_audit_events)
//!   - Integrity verification failures   (integrity_verification_attempts)
//!   - Backup validations                (audit_log: CreateBackup / RestoreBackup)
//!   - Corruption detections             (operational_findings_log severity=CRITICAL)
//!   - Export generations                (fiscal_export_snapshots)

use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TimelineEventKind {
    FiscalClose,
    FiscalOpen,
    Archival,
    Import,
    IntegrityFailure,
    BackupCreated,
    BackupRestored,
    CorruptionDetected,
    ExportGenerated,
}

impl TimelineEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimelineEventKind::FiscalClose => "FISCAL_CLOSE",
            TimelineEventKind::FiscalOpen => "FISCAL_OPEN",
            TimelineEventKind::Archival => "ARCHIVAL",
            TimelineEventKind::Import => "IMPORT",
            TimelineEventKind::IntegrityFailure => "INTEGRITY_FAILURE",
            TimelineEventKind::BackupCreated => "BACKUP_CREATED",
            TimelineEventKind::BackupRestored => "BACKUP_RESTORED",
            TimelineEventKind::CorruptionDetected => "CORRUPTION_DETECTED",
            TimelineEventKind::ExportGenerated => "EXPORT_GENERATED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FiscalTimelineEvent {
    pub timestamp: String,
    pub kind: TimelineEventKind,
    pub fiscal_year: Option<i32>,
    pub actor: Option<String>,
    pub summary: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FiscalTimelineQuery {
    pub fiscal_year: Option<i32>,
    pub from_timestamp: Option<String>,
    pub to_timestamp: Option<String>,
    pub limit: Option<i64>,
}

pub struct FiscalTimelineService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalTimelineService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Reconstruct the unified timeline. Returned newest-first.
    pub fn build_timeline(
        &self,
        query: &FiscalTimelineQuery,
    ) -> Result<Vec<FiscalTimelineEvent>, AppError> {
        let mut events = Vec::new();

        self.collect_fiscal_lifecycle(&mut events)?;
        self.collect_archival_events(&mut events)?;
        self.collect_imports(&mut events)?;
        self.collect_integrity_failures(&mut events)?;
        self.collect_backups(&mut events)?;
        self.collect_corruption_findings(&mut events)?;
        self.collect_exports(&mut events)?;

        // Filter (Rust side — read-only, no SQL mutation).
        let filtered: Vec<FiscalTimelineEvent> = events
            .into_iter()
            .filter(|e| match query.fiscal_year {
                Some(fy) => e.fiscal_year == Some(fy),
                None => true,
            })
            .filter(|e| match &query.from_timestamp {
                Some(from) => e.timestamp.as_str() >= from.as_str(),
                None => true,
            })
            .filter(|e| match &query.to_timestamp {
                Some(to) => e.timestamp.as_str() <= to.as_str(),
                None => true,
            })
            .collect();

        // Sort newest-first by timestamp string (RFC3339 is lexicographically sortable).
        let mut sorted = filtered;
        sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        let cap = match query.limit {
            Some(l) => l.clamp(1, 5000) as usize,
            None => 500,
        };
        if sorted.len() > cap {
            sorted.truncate(cap);
        }

        Ok(sorted)
    }

    fn collect_fiscal_lifecycle(&self, out: &mut Vec<FiscalTimelineEvent>) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let rows = self.executor.timeline().fetch_audit_lifecycle()?;

        for r in rows {
            let fy = r
                .entity_id
                .as_deref()
                .and_then(|s| s.parse::<i32>().ok())
                .or_else(|| {
                    r.new_value
                        .as_deref()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                        .and_then(|v| {
                            v.get("closed_year")
                                .or_else(|| v.get("opened_year"))
                                .cloned()
                        })
                        .and_then(|v| v.as_i64())
                        .map(|n| n as i32)
                });
            let kind = if r.action == "FiscalYearClosed" {
                TimelineEventKind::FiscalClose
            } else {
                TimelineEventKind::FiscalOpen
            };
            out.push(FiscalTimelineEvent {
                timestamp: r.timestamp,
                kind,
                fiscal_year: fy,
                actor: r.username,
                summary: match kind {
                    TimelineEventKind::FiscalClose => format!(
                        "تم إغلاق السنة المالية {}",
                        fy.map(|y| y.to_string()).unwrap_or_else(|| "?".into())
                    ),
                    _ => format!(
                        "تم فتح السنة المالية {}",
                        fy.map(|y| y.to_string()).unwrap_or_else(|| "?".into())
                    ),
                },
                source: "audit_log".into(),
            });
        }
        Ok(())
    }

    fn collect_archival_events(&self, out: &mut Vec<FiscalTimelineEvent>) -> Result<(), AppError> {
        // From fiscal_year_status
        let rows = self.executor.query_all(
            "SELECT year, closed_at, closed_by FROM fiscal_year_status WHERE archived = 1",
            [],
            |r| {
                Ok((
                    r.get::<_, i32>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )?;

        for (year, closed_at, closed_by) in rows {
            out.push(FiscalTimelineEvent {
                timestamp: closed_at.unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                kind: TimelineEventKind::Archival,
                fiscal_year: Some(year),
                actor: closed_by,
                summary: format!("تم أرشفة السنة المالية {}", year),
                source: "fiscal_year_status".into(),
            });
        }
        Ok(())
    }

    fn collect_imports(&self, out: &mut Vec<FiscalTimelineEvent>) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let repo = self.executor.timeline();

        let audit_rows = repo.fetch_imports_audit()?;
        for r in audit_rows {
            let fy = r
                .new_value
                .as_deref()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .and_then(|v| v.get("fiscal_year").cloned())
                .and_then(|v| v.as_i64())
                .map(|n| n as i32);
            out.push(FiscalTimelineEvent {
                timestamp: r.timestamp,
                kind: TimelineEventKind::Import,
                fiscal_year: fy,
                actor: r.username,
                summary: format!("عملية استيراد: {}", r.action),
                source: "audit_log".into(),
            });
        }

        let event_rows = repo.fetch_import_events(500)?;
        for r in event_rows {
            out.push(FiscalTimelineEvent {
                timestamp: r.occurred_at,
                kind: TimelineEventKind::Import,
                fiscal_year: None,
                actor: r.source_node_id,
                summary: format!("حدث استيراد [{}] النوع={}", r.package_kind, r.event_type),
                source: "import_audit_events".into(),
            });
        }
        Ok(())
    }

    #[allow(clippy::manual_unwrap_or)]
    fn collect_integrity_failures(
        &self,
        out: &mut Vec<FiscalTimelineEvent>,
    ) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let rows = self.executor.timeline().fetch_integrity_failures(500)?;
        for r in rows {
            out.push(FiscalTimelineEvent {
                timestamp: r.timestamp,
                kind: TimelineEventKind::IntegrityFailure,
                fiscal_year: None,
                actor: None,
                summary: format!(
                    "فشل التحقق من السلامة: {} — {}",
                    r.verification_type,
                    match r.details.as_deref() {
                        Some(d) => d,
                        None => "لا توجد تفاصيل",
                    }
                ),
                source: "integrity_verification_attempts".into(),
            });
        }
        Ok(())
    }

    fn collect_backups(&self, out: &mut Vec<FiscalTimelineEvent>) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let rows = self.executor.timeline().fetch_backups_audit(500)?;
        for r in rows {
            let kind = if r.action.contains("Restore") {
                TimelineEventKind::BackupRestored
            } else {
                TimelineEventKind::BackupCreated
            };
            out.push(FiscalTimelineEvent {
                timestamp: r.timestamp,
                kind,
                fiscal_year: None,
                actor: r.username,
                summary: format!("عملية نسخة احتياطية: {}", r.action),
                source: "audit_log".into(),
            });
        }
        Ok(())
    }

    fn collect_corruption_findings(
        &self,
        out: &mut Vec<FiscalTimelineEvent>,
    ) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let rows = self.executor.timeline().fetch_critical_findings(500)?;
        for r in rows {
            out.push(FiscalTimelineEvent {
                timestamp: r.emitted_at,
                kind: TimelineEventKind::CorruptionDetected,
                fiscal_year: None,
                actor: None,
                summary: format!("[{}/{}] {}", r.category, r.code, r.message),
                source: "operational_findings_log".into(),
            });
        }
        Ok(())
    }

    fn collect_exports(&self, out: &mut Vec<FiscalTimelineEvent>) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let rows = self.executor.timeline().fetch_exports(500)?;
        for r in rows {
            out.push(FiscalTimelineEvent {
                timestamp: r.generated_at,
                kind: TimelineEventKind::ExportGenerated,
                fiscal_year: Some(r.fiscal_year),
                actor: Some(r.generated_by),
                summary: format!(
                    "تم إنشاء تصدير مالي (الهاش={}…)",
                    &r.export_hash.chars().take(12).collect::<String>()
                ),
                source: "fiscal_export_snapshots".into(),
            });
        }
        Ok(())
    }
}

impl crate::architecture::Service for FiscalTimelineService<'_> {}
