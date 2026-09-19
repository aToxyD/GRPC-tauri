use crate::application::services::FiscalHistoricalGuard;
use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiscalExportSnapshot {
    pub export_hash: String,
    pub generated_at: String,
    pub generated_by: String,
    pub fiscal_year: i32,
    pub movement_count: i64,
    pub report_count: i64,
    pub inventory_total_value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_years_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_anomalies_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_node_id: Option<String>,
}

pub struct FiscalExportSnapshotService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalExportSnapshotService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Append-only export metadata row. Must run inside a transaction at the command layer.
    pub fn record_export_snapshot(&self, snapshot: &FiscalExportSnapshot) -> Result<i64, AppError> {
        FiscalHistoricalGuard::new(self.executor)
            .assert_export_snapshot_mutable(snapshot.fiscal_year)?;

        use crate::repositories::RepositoryProvider;
        let repo_snapshot = crate::repositories::fiscal_snapshots::FiscalExportSnapshot {
            export_hash: snapshot.export_hash.clone(),
            generated_at: snapshot.generated_at.clone(),
            generated_by: snapshot.generated_by.clone(),
            fiscal_year: snapshot.fiscal_year,
            movement_count: snapshot.movement_count,
            report_count: snapshot.report_count,
            inventory_total_value: snapshot.inventory_total_value,
            integrity_state: snapshot.integrity_state.clone(),
            archived_years_count: snapshot.archived_years_count,
            active_anomalies_count: snapshot.active_anomalies_count,
            signing_key_id: snapshot.signing_key_id.clone(),
            export_reason: snapshot.export_reason.clone(),
            export_mode: snapshot.export_mode.clone(),
            target_node_id: snapshot.target_node_id.clone(),
        };

        let id = self
            .executor
            .fiscal_snapshots()
            .record_export_snapshot(&repo_snapshot)?;

        log::info!(
            target: "grpc::fiscal",
            "[FISCAL_EXPORT_RECORDED] fiscal_year={} export_hash={} id={}",
            snapshot.fiscal_year,
            snapshot.export_hash,
            id
        );
        Ok(id)
    }
}

impl crate::architecture::Service for FiscalExportSnapshotService<'_> {}
