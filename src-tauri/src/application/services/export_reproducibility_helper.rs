//! Captures export-time operational context into fiscal_export_snapshots metadata columns.

use crate::application::services::{
    FiscalExportSnapshot, FiscalExportSnapshotService, SystemIntegrityState,
};
use crate::errors::AppError;
use crate::infrastructure::security::resolve_active_signing_key_id;
use crate::repositories::executor::DbExecutor;
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

pub fn record_export_with_reproducibility(
    executor: DbExecutor<'_>,
    ctx: ExportReproducibilityContext,
) -> Result<i64, AppError> {
    let integrity_state = format!(
        "{:?}",
        SystemIntegrityState::resolve_from_executor(executor)?
    );
    let archived_years_count: i64 = executor.query_row(
        "SELECT COUNT(*) FROM fiscal_year_status WHERE archived = 1",
        [],
        |r| r.get(0),
    )?;
    let active_anomalies_count: i64 = executor.query_row(
        "SELECT COUNT(*) FROM operational_findings_log WHERE severity IN ('WARNING','CRITICAL')",
        [],
        |r| r.get(0),
    )?;
    let signing_key_id = resolve_active_signing_key_id().unwrap_or_else(|| "unset".to_string());

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
