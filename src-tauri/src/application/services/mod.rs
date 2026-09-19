pub mod admin_access_first_import_predicates_service;
pub mod audit_observability_service;
pub mod audit_service;
pub mod audit_tx_service;
pub mod b8_first_import_predicates_service;
pub mod contract_service;
pub mod daily_report_service;
pub mod fifo_preview_service;
pub mod fiscal_closing_service;
pub mod fiscal_reporting_service;
pub mod fiscal_scope;
pub mod fiscal_tax_policy_service;
pub mod fiscal_validation_service;
pub mod fleet_package_export;
pub mod identity_authentication_policy;
pub mod identity_bootstrap_status_service;
pub mod identity_challenge_service;
pub mod identity_node_resolver;
pub mod identity_provisioning_service;
pub mod identity_rotation_coordinator;
pub mod identity_rotation_service;
pub mod identity_signed_export_service;
pub mod identity_trust_anchor_service;
pub mod inventory_snapshot_service;
pub mod login_policy;
pub mod monthly_report_service;
pub mod node_package_service;
pub mod order_service;
pub mod product_service;
pub mod report_calculation_service;
pub mod session_establishment_service;
pub mod settings_service;
pub mod stock_level_service;
pub mod stock_movement_service;
pub mod supplier_service;
pub mod sync_conflict_resolution_service;
pub mod sync_conflict_service;
pub mod sync_import_execution_service;
pub mod sync_import_models;
pub mod sync_import_validation_service;
pub mod sync_package_identity_verification_service;
pub mod system_diagnostics_service;
pub mod system_health_service;
pub mod system_stats_service;
pub mod transport_target;
pub mod unit_service;
pub mod user_account_sync_service;
pub mod user_service;

pub use admin_access_first_import_predicates_service::AdminAccessFirstImportPredicatesService;
pub use audit_observability_service::AuditObservabilityService;
pub use audit_service::AuditService;
pub use audit_tx_service::AuditTxService;
pub use b8_first_import_predicates_service::B8FirstImportPredicatesService;
pub use contract_service::ContractService;
pub use daily_report_service::DailyReportService;
pub use fifo_preview_service::FifoPreviewService;
pub use fiscal_closing_service::FiscalClosingService;
pub use fiscal_reporting_service::FiscalReportingService;
pub use fiscal_tax_policy_service::FiscalTaxPolicyService;
pub use fiscal_validation_service::FiscalValidationService;
pub use fleet_package_export::{
    export_admin_access_fleet, export_contract_catalog_fleet,
    export_contract_catalog_unit_distribution, export_products_fleet, FleetExportOutcome,
};
pub use identity_authentication_policy::{AdminCredentialState, IdentityAuthenticationPolicy};
pub use identity_bootstrap_status_service::IdentityBootstrapStatusService;
pub use identity_challenge_service::IdentityChallengeService;
pub use identity_node_resolver::NodeIdentityResolver;
pub use identity_provisioning_service::{
    FinalizeUnitProvisionResult, FinalizeWilayaProvisionResult, IdentityProvisioningService,
};
pub use identity_rotation_coordinator::{
    IdentityRotationCoordinator, RotationFinalizeOutcome, SignedUnitRotation,
};
pub use identity_rotation_service::{
    FinalizeVerdict, IdentityRotationService, RotationOperation, RotationPlan,
};
pub use identity_signed_export_service::IdentitySignedExportService;
pub use identity_trust_anchor_service::{
    IdentityTrustAnchorService, InstallWilayaCertificateResult,
};
pub use inventory_snapshot_service::InventorySnapshotService;
pub use login_policy::LoginPolicy;
pub use monthly_report_service::MonthlyReportService;
pub use node_package_service::{NodePackageService, ParsedUnitPackageRaw};
pub use order_service::OrderService;
pub use product_service::ProductService;
pub use report_calculation_service::ReportCalculationService;
pub use session_establishment_service::{EstablishedSession, SessionEstablishmentService};
pub use settings_service::SettingsService;
pub use stock_level_service::StockLevelService;
pub use stock_movement_service::StockMovementService;
pub use supplier_service::SupplierService;
pub use sync_conflict_resolution_service::SyncConflictResolutionService;
pub use sync_conflict_service::SyncConflictService;
pub use sync_import_execution_service::SyncImportExecutionService;
pub use sync_import_models::{
    ConflictResolutionOutcome, ImportExecutionError, ImportExecutionSummary, ImportMutationSummary,
    ReplayProtectionResult, ResolutionPolicy, SyncImportRequest, SyncImportResult, SyncPackageKind,
};
pub use sync_import_validation_service::SyncImportValidationService;
pub use sync_package_identity_verification_service::{
    PayloadUnitIdExtractor, SyncPackageIdentityVerificationService, V2ImportPolicy,
};
pub use system_diagnostics_service::SystemDiagnosticsService;
pub use system_health_service::SystemHealthService;
pub use system_stats_service::SystemStatsService;
pub use unit_service::UnitService;
pub use user_account_sync_service::{ApplyIdentityAccessOutcome, UserAccountSyncService};
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
impl crate::architecture::Service for NodePackageService<'_> {}
impl crate::architecture::Service for StockLevelService<'_> {}
impl crate::architecture::Service for StockMovementService<'_> {}
impl crate::architecture::Service for InventorySnapshotService<'_> {}
impl crate::architecture::Service for MonthlyReportService<'_> {}
impl crate::architecture::Service for OrderService<'_> {}
impl crate::architecture::Service for DailyReportService<'_> {}
impl crate::architecture::Service for UnitService<'_> {}
impl crate::architecture::Service for UserAccountSyncService<'_> {}
impl crate::architecture::Service for AuditService<'_> {}
impl crate::architecture::Service for ProductService<'_> {}
impl crate::architecture::Service for UserService<'_> {}
impl crate::architecture::Service for SettingsService<'_> {}
impl crate::architecture::Service for ReportCalculationService<'_> {}
impl crate::architecture::Service for SystemStatsService {}
impl crate::architecture::Service for IdentityRotationService {}

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
pub mod runtime_bootstrap;
pub mod security_regression_guard;
pub mod system_maintenance_state;
pub mod telemetry_service;

pub use deployment_readiness_service::{
    DeploymentReadinessCheck, DeploymentReadinessReport, DeploymentReadinessService,
    DeploymentReadinessStatus,
};
pub use export_reproducibility_helper::{
    current_wilaya_signing_key_id, record_export_with_reproducibility, ExportReproducibilityContext,
};
pub use fiscal_closure_package_service::{
    FiscalClosureApplyResult, FiscalClosurePackage, FiscalClosurePackageService,
    FiscalClosurePackageSignerInfo, FiscalClosurePreview, FISCAL_CLOSURE_PACKAGE_VERSION,
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
pub use runtime_bootstrap::{
    apply_pending_restore_ledger_overlays, bootstrap_runtime, consume_restore_markers,
    RuntimeBootstrap,
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
pub use security_regression_guard::{RestoreRegressionStatus, SecurityRegressionGuard};
pub use system_integrity_state_service::SystemIntegrityState;
