use crate::errors::AppResult;
use crate::repositories::executor::DbExecutor;
use crate::repositories::numeric_row;
use rusqlite::params;
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
    pub integrity_state: Option<String>,
    pub archived_years_count: Option<i64>,
    pub active_anomalies_count: Option<i64>,
    pub signing_key_id: Option<String>,
    pub export_reason: Option<String>,
    pub export_mode: Option<String>,
    pub target_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub struct FiscalSnapshotRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalSnapshotRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn record_export_snapshot(&self, snapshot: &FiscalExportSnapshot) -> AppResult<i64> {
        let inventory_total_value_scaled =
            numeric_row::money_scaled(snapshot.inventory_total_value)?;
        self.executor.execute(
            r#"
            INSERT INTO fiscal_export_snapshots
                (export_hash, generated_at, generated_by, fiscal_year,
                 movement_count, report_count, inventory_total_value,
                 integrity_state, archived_years_count, active_anomalies_count,
                 signing_key_id, export_reason, export_mode, target_node_id)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
            params![
                snapshot.export_hash,
                snapshot.generated_at,
                snapshot.generated_by,
                snapshot.fiscal_year,
                snapshot.movement_count,
                snapshot.report_count,
                inventory_total_value_scaled,
                snapshot.integrity_state,
                snapshot.archived_years_count,
                snapshot.active_anomalies_count,
                snapshot.signing_key_id,
                snapshot.export_reason,
                snapshot.export_mode,
                snapshot.target_node_id,
            ],
        )?;
        Ok(self.executor.last_insert_rowid())
    }

    pub fn get_export_snapshot_by_hash(
        &self,
        export_hash: &str,
    ) -> AppResult<Option<FiscalExportSnapshot>> {
        self.executor
            .query_row_optional(
                r#"
                SELECT export_hash, generated_at, generated_by, fiscal_year,
                       movement_count, report_count, inventory_total_value,
                       integrity_state, archived_years_count, active_anomalies_count,
                       signing_key_id, export_reason, export_mode, target_node_id
                FROM fiscal_export_snapshots
                WHERE export_hash = ?1
                "#,
                params![export_hash],
                |r| {
                    Ok(FiscalExportSnapshot {
                        export_hash: r.get(0)?,
                        generated_at: r.get(1)?,
                        generated_by: r.get(2)?,
                        fiscal_year: r.get(3)?,
                        movement_count: r.get(4)?,
                        report_count: r.get(5)?,
                        inventory_total_value: numeric_row::money_col(6, r.get::<_, i64>(6)?)?,
                        integrity_state: r.get(7)?,
                        archived_years_count: r.get(8)?,
                        active_anomalies_count: r.get(9)?,
                        signing_key_id: r.get(10)?,
                        export_reason: r.get(11)?,
                        export_mode: r.get(12)?,
                        target_node_id: r.get(13)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_operational_snapshot(
        &self,
        snapshot_date: &str,
        fiscal_year: i32,
        total_inventory_value: f64,
        product_count: i64,
        movement_count: i64,
        report_count: i64,
        integrity_state: &str,
        created_by: &str,
        created_at: &str,
    ) -> AppResult<i64> {
        let total_inventory_value_scaled = numeric_row::money_scaled(total_inventory_value)?;
        self.executor.execute(
            r#"
            INSERT INTO fiscal_operational_snapshots
                (snapshot_date, fiscal_year, total_inventory_value,
                 product_count, movement_count, report_count,
                 integrity_state, created_by, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                snapshot_date,
                fiscal_year,
                total_inventory_value_scaled,
                product_count,
                movement_count,
                report_count,
                integrity_state,
                created_by,
                created_at,
            ],
        )?;
        Ok(self.executor.last_insert_rowid())
    }

    pub fn count_distinct_closed_years_with_snapshot(&self) -> AppResult<i64> {
        self.executor
            .query_row(
                "SELECT COUNT(DISTINCT fiscal_year) FROM fiscal_operational_snapshots
                 WHERE fiscal_year IN (SELECT year FROM fiscal_year_status WHERE status = 'closed')",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub fn list_operational_snapshots(
        &self,
        fiscal_year: Option<i32>,
        limit: i64,
    ) -> AppResult<Vec<FiscalOperationalSnapshot>> {
        let sql = if fiscal_year.is_some() {
            "SELECT id, snapshot_date, fiscal_year, total_inventory_value,
                    product_count, movement_count, report_count,
                    integrity_state, created_by, created_at
             FROM fiscal_operational_snapshots WHERE fiscal_year = ?1 ORDER BY id DESC LIMIT ?2"
        } else {
            "SELECT id, snapshot_date, fiscal_year, total_inventory_value,
                    product_count, movement_count, report_count,
                    integrity_state, created_by, created_at
             FROM fiscal_operational_snapshots ORDER BY id DESC LIMIT ?1"
        };

        if let Some(fy) = fiscal_year {
            self.executor
                .query_all(sql, params![fy, limit], map_snapshot_row)
                .map_err(Into::into)
        } else {
            self.executor
                .query_all(sql, params![limit], map_snapshot_row)
                .map_err(Into::into)
        }
    }
}

fn map_snapshot_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<FiscalOperationalSnapshot> {
    Ok(FiscalOperationalSnapshot {
        id: r.get(0)?,
        snapshot_date: r.get(1)?,
        fiscal_year: r.get(2)?,
        total_inventory_value: numeric_row::money_col(3, r.get::<_, i64>(3)?)?,
        product_count: r.get(4)?,
        movement_count: r.get(5)?,
        report_count: r.get(6)?,
        integrity_state: r.get(7)?,
        created_by: r.get(8)?,
        created_at: r.get(9)?,
    })
}
