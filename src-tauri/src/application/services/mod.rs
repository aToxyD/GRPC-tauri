pub mod audit_observability_service;
pub mod audit_service;
pub mod audit_tx_service;
pub mod daily_report_service;
pub mod fifo_preview_service;
pub mod fiscal_scope;
pub mod fiscal_year_service;
pub mod import_sync_service;
pub mod inventory_snapshot_service;
pub mod monthly_report_service;
pub mod node_package_service;
pub mod order_service;
pub mod product_service;
pub mod report_calculation_service;
pub mod settings_service;
pub mod stock_level_service;
pub mod stock_movement_service;
pub mod sync_conflict_service;
pub mod system_diagnostics_service;
pub mod system_health_service;
pub mod system_stats_service;
pub mod unit_service;
pub mod user_service;

pub use audit_observability_service::AuditObservabilityService;
pub use audit_service::AuditService;
pub use audit_tx_service::AuditTxService;
pub use daily_report_service::DailyReportService;
pub use fifo_preview_service::FifoPreviewService;
pub use fiscal_year_service::FiscalYearService;
pub use import_sync_service::ImportSyncService;
pub use inventory_snapshot_service::InventorySnapshotService;
pub use monthly_report_service::MonthlyReportService;
pub use node_package_service::{NodePackageService, ParsedUnitPackageRaw};
pub use order_service::OrderService;
pub use product_service::ProductService;
pub use report_calculation_service::ReportCalculationService;
pub use settings_service::SettingsService;
pub use stock_level_service::StockLevelService;
pub use stock_movement_service::StockMovementService;
pub use sync_conflict_service::SyncConflictService;
pub use system_diagnostics_service::SystemDiagnosticsService;
pub use system_health_service::SystemHealthService;
pub use system_stats_service::SystemStatsService;
pub use unit_service::UnitService;
pub use user_service::UserService;

/// User identification context for auditing.
#[derive(Debug, Clone)]
pub struct UserContext {
    pub user_id: String,
    pub username: String,
    pub session_id: Option<String>,
}

impl UserContext {
    pub fn new(user_id: &str, username: &str, session_id: Option<&str>) -> Self {
        Self {
            user_id: user_id.to_string(),
            username: username.to_string(),
            session_id: session_id.map(|s| s.to_string()),
        }
    }
}

// Markers
impl crate::architecture::Service for ImportSyncService<'_> {}
impl crate::architecture::Service for NodePackageService<'_> {}
impl crate::architecture::Service for StockLevelService<'_> {}
impl crate::architecture::Service for StockMovementService<'_> {}
impl crate::architecture::Service for InventorySnapshotService<'_> {}
impl crate::architecture::Service for MonthlyReportService<'_> {}
impl crate::architecture::Service for OrderService<'_> {}
impl crate::architecture::Service for DailyReportService<'_> {}
impl crate::architecture::Service for UnitService<'_> {}
impl crate::architecture::Service for AuditService<'_> {}
impl crate::architecture::Service for ProductService<'_> {}
impl crate::architecture::Service for UserService<'_> {}
impl crate::architecture::Service for SettingsService<'_> {}
impl crate::architecture::Service for ReportCalculationService<'_> {}
impl crate::architecture::Service for SystemStatsService {}

pub mod backup_integrity_service;
pub use backup_integrity_service::BackupIntegrityService;
pub mod database_retention_service;
pub mod fiscal_export_snapshot_service;
pub use fiscal_export_snapshot_service::{FiscalExportSnapshot, FiscalExportSnapshotService};
pub mod fiscal_integrity_service;
pub mod inventory_integrity_service;
pub mod system_integrity_state_service;

// ─── Controlled Operational Intelligence Phase ───────────────────────────────
pub mod deployment_readiness_service;
pub mod export_reproducibility_helper;
pub mod fiscal_closure_package_service;
pub mod fiscal_historical_guard;
pub mod fiscal_operational_snapshot_service;
pub mod fiscal_timeline_service;
pub mod fiscal_transition_history_service;
pub mod import_reproducibility_service;
pub mod integrity_attempt_recorder;
pub mod integrity_service;
pub mod operation_execution_guard;
pub mod operational_anomaly_service;
pub mod operational_consistency_verifier;
pub mod operational_log_contract;
pub mod operational_recommendation_service;
pub mod operational_session_service;
pub mod operator_safety_service;
pub mod system_maintenance_state;
pub mod telemetry_service;

pub use deployment_readiness_service::{
    DeploymentReadinessCheck, DeploymentReadinessReport, DeploymentReadinessService,
    DeploymentReadinessStatus,
};
pub use export_reproducibility_helper::{
    record_export_with_reproducibility, ExportReproducibilityContext,
};
pub use fiscal_closure_package_service::{
    FiscalClosureApplyResult, FiscalClosurePackage, FiscalClosurePackageService,
    FiscalClosurePreview, FISCAL_CLOSURE_PACKAGE_VERSION,
};
pub use import_reproducibility_service::{
    ImportReproducibilityRecord, ImportReproducibilityService,
};
pub use operational_consistency_verifier::{
    OperationalConsistencyFinding, OperationalConsistencyReport, OperationalConsistencyStatus,
    OperationalConsistencyVerifier,
};
pub use operational_session_service::{
    OperationalSessionRecord, OperationalSessionService, SessionCounterKind, SessionEndReason,
};
pub use system_maintenance_state::{
    MaintenanceBlockedOperation, SystemMaintenanceHandle, SystemMaintenanceState,
};
pub use telemetry_service::{
    TelemetryEvent, TelemetryEventType, TelemetryOutcome, TelemetryService,
};

pub use fiscal_historical_guard::FiscalHistoricalGuard;
pub use fiscal_operational_snapshot_service::{
    FiscalOperationalSnapshot, FiscalOperationalSnapshotService,
};
pub use fiscal_timeline_service::{
    FiscalTimelineEvent, FiscalTimelineQuery, FiscalTimelineService, TimelineEventKind,
};
pub use fiscal_transition_history_service::{
    FiscalTransitionHistoryEntry, FiscalTransitionHistoryService,
};
pub use integrity_attempt_recorder::{
    IntegrityAttemptRecorder, VerificationOutcome, VerificationType,
};
pub use integrity_service::{
    FindingSeverity as IntegrityFindingSeverity, IntegrityFinding, IntegrityReport,
    IntegrityService, IntegrityStatus,
};
pub use operation_execution_guard::{GuardedOperation, OperationExecutionGuard};
pub use operational_anomaly_service::{
    FindingCategory, FindingSeverity, OperationalAnalysisReport, OperationalAnomalyService,
    OperationalFinding,
};
pub use operational_log_contract::{contract_snapshot, LogEventContract, ALL_CRITICAL_CONTRACTS};
pub use operational_recommendation_service::{
    OperationalRecommendation, OperationalRecommendationService, RecommendationPriority,
};
pub use operator_safety_service::{CriticalOperation, OperatorSafetyService};
pub use system_integrity_state_service::SystemIntegrityState;
