//! Fiscal Operational Snapshot Service
//!
//! Lightweight, MANUALLY-triggered snapshots of operational counters used to
//! help the operator understand how the system evolves over time.
//!
//! Hard constraints:
//!   - NO scheduler
//!   - NO periodic job
//!   - NO automatic execution
//!   - Append-only (the application never UPDATEs nor DELETEs these rows)
//!
//! Snapshots are taken explicitly by the operator from the diagnostics page,
//! or programmatically as part of a test/integration scenario.

use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FiscalOperationalSnapshot {
    pub id: i64,
    pub snapshot_date: String,
    pub fiscal_year: i32,
    pub total_inventory_value: f64,
    pub product_count: i64,
    pub movement_count: i64,
    pub report_count: i64,
    pub integrity_state: String,
    pub created_by: String,
    pub created_at: String,
}

pub struct FiscalOperationalSnapshotService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalOperationalSnapshotService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Create a new operational snapshot for the given fiscal year.
    /// Returns the id of the inserted row.
    ///
    /// Called from a Tauri command only — never invoked by a scheduler.
    pub fn create_snapshot(&self, fiscal_year: i32, created_by: &str) -> Result<i64, AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        let date_only = chrono::Utc::now().format("%Y-%m-%d").to_string();

        use crate::repositories::RepositoryProvider;
        let repos = self.executor;

        // Compute counters via repositories
        let product_count = repos.products().count_active_products()?;
        let movement_count = repos.stock_movements().count_by_year(fiscal_year)?;
        let report_count = repos.reports().count_active_reports_by_year(fiscal_year)?;
        let total_inventory_value = repos.inventory().get_total_inventory_value()?;

        // Integrity state is derived from a lightweight fiscal integrity scan
        let integrity_state = self.compute_integrity_state_label()?;

        let id = repos.fiscal_snapshots().record_operational_snapshot(
            &date_only,
            fiscal_year,
            total_inventory_value,
            product_count,
            movement_count,
            report_count,
            &integrity_state,
            created_by,
            &now,
        )?;

        log::info!(
            target: "grpc::operational_snapshot",
            "[OPSNAPSHOT_CREATED] id={} fiscal_year={} integrity_state={} by={}",
            id, fiscal_year, integrity_state, created_by
        );

        Ok(id)
    }

    /// List snapshots — most recent first.
    pub fn list_snapshots(
        &self,
        fiscal_year: Option<i32>,
        limit: i64,
    ) -> Result<Vec<FiscalOperationalSnapshot>, AppError> {
        let safe_limit = limit.clamp(1, 500);
        use crate::repositories::RepositoryProvider;
        let snapshots = self
            .executor
            .fiscal_snapshots()
            .list_operational_snapshots(fiscal_year, safe_limit)?;

        // Map repository DTO to service DTO
        Ok(snapshots
            .into_iter()
            .map(|s| FiscalOperationalSnapshot {
                id: s.id,
                snapshot_date: s.snapshot_date,
                fiscal_year: s.fiscal_year,
                total_inventory_value: s.total_inventory_value,
                product_count: s.product_count,
                movement_count: s.movement_count,
                report_count: s.report_count,
                integrity_state: s.integrity_state,
                created_by: s.created_by,
                created_at: s.created_at,
            })
            .collect())
    }

    fn compute_integrity_state_label(&self) -> Result<String, AppError> {
        use crate::repositories::RepositoryProvider;
        let repos = self.executor;

        let warning_count = repos.integrity().count_orphan_opening_balances()?
            + repos.integrity().count_post_closure_movements()?;

        let chain_broken = repos.integrity().count_failed_attempts("AUDIT_CHAIN")?;

        if chain_broken > 0 {
            Ok("CRITICAL".to_string())
        } else if warning_count > 0 {
            Ok("WARNINGS".to_string())
        } else {
            Ok("OK".to_string())
        }
    }
}

impl crate::architecture::Service for FiscalOperationalSnapshotService<'_> {}
