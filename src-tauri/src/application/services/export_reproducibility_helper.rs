//! Captures export-time operational context into fiscal_export_snapshots metadata columns.

use crate::application::services::{
    FiscalExportSnapshot, FiscalExportSnapshotService, NodeIdentityResolver,
    SystemIntegrityState,
};
use crate::db::Database;
use crate::domain::identity::SubjectType;
use crate::errors::AppError;
use crate::infrastructure::identity::NodeKeyStore;
use crate::repositories::executor::DbExecutor;
use crate::repositories::RepositoryProvider;
use chrono::Utc;

pub struct ExportReproducibilityContext {
    pub export_hash: String,
    pub fiscal_year: i32,
    pub generated_by: String,
    pub movement_count: i64,
    pub report_count: i64,
    pub inventory_total_value: f64,
    pub export_reason: String,
}

/// SEC-008 (ADR-0048): the export signing key id is the hex Ed25519 public key
/// of the local WILAYA identity (fiscal closure packages are identity-signed).
/// "unset" documents nodes that are not provisioned as WILAYA signers.
pub fn current_wilaya_signing_key_id(db: &Database, node_key_store: &NodeKeyStore) -> String {
    use crate::infrastructure::sync::packages::signing::Ed25519PackageSigner;
    NodeIdentityResolver::resolve_local_signer(db, node_key_store, SubjectType::Wilaya)
        .ok()
        .flatten()
        .map(|r| Ed25519PackageSigner::from_provider(r.signer).public_key_hex())
        .unwrap_or_else(|| "unset".to_string())
}

pub fn record_export_with_reproducibility(
    executor: DbExecutor<'_>,
    signing_key_id: String,
    ctx: ExportReproducibilityContext,
) -> Result<i64, AppError> {
    let integrity_state = format!(
        "{:?}",
        SystemIntegrityState::resolve_from_executor(executor)?
    );
    let archived_years_count: i64 = executor.fiscal_year_status().count_archived()?;
    let active_anomalies_count: i64 = executor.anomaly().count_active_anomalies()?;

    let snapshot = FiscalExportSnapshot {
        export_hash: ctx.export_hash,
        generated_at: Utc::now().to_rfc3339(),
        generated_by: ctx.generated_by,
        fiscal_year: ctx.fiscal_year,
        movement_count: ctx.movement_count,
        report_count: ctx.report_count,
        inventory_total_value: ctx.inventory_total_value,
        integrity_state: Some(integrity_state),
        archived_years_count: Some(archived_years_count),
        active_anomalies_count: Some(active_anomalies_count),
        signing_key_id: Some(signing_key_id),
        export_reason: Some(ctx.export_reason),
    };

    FiscalExportSnapshotService::new(executor).record_export_snapshot(&snapshot)
}
