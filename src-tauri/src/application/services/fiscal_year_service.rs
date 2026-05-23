//! Fiscal-Year lifecycle service.
//!
//! All mutations are performed through the `DbExecutor` passed in at construction,
//! which is always a `DbExecutor::Tx` when called from the command layer
//! (wrapped in `db.with_transaction`).  The service itself does NOT open a
//! transaction — that is the caller's responsibility, keeping the boundary clean.

use crate::domain::audit::{AuditAction, EntityType};
use crate::errors::AppError;
use crate::models::FiscalYearStatus;
use crate::repositories::{DbExecutor, RepositoryProvider};

pub struct FiscalYearService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalYearService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Idempotency guard — explicit, not relying on UNIQUE constraint errors
    // ─────────────────────────────────────────────────────────────────────────
    fn assert_not_already_closed(&self, year: i32) -> Result<(), AppError> {
        if let Some(status) = self.executor.fiscal_year_status().get_by_year(year)? {
            if status.status == "closed" {
                return Err(AppError::BusinessLogic(
                    crate::errors::BusinessLogicError::FiscalYearClosed { year },
                ));
            }
        }
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // close_year — MUST be called inside db.with_transaction in the command
    // layer.  The executor received here is always DbExecutor::Tx.
    // ─────────────────────────────────────────────────────────────────────────

    /// Perform the fiscal-year close:
    ///   1. snapshot every product's ending inventory (quantity + valuation)
    ///   2. lock the old year
    ///   3. open the next year
    ///   4. write audit record
    ///
    /// All steps run on a single executor that the caller wraps in a
    /// `db.with_transaction` closure — guaranteeing atomicity.
    pub fn close_year(
        &self,
        year: i32,
        next_year: i32,
        user_id: &str,
        username: &str,
    ) -> Result<usize, AppError> {
        let t0 = std::time::Instant::now();

        log::info!(
            target: "grpc::fiscal",
            "[FISCAL_CLOSE_START] year={} next_year={} user_id={}",
            year, next_year, user_id
        );

        // ── Explicit idempotency guard ────────────────────────────────────
        self.assert_not_already_closed(year)?;
        self.executor.fiscal_year_status().assert_open(year)?;

        // ── Guard: next year must not already be open ─────────────────────
        if let Some(status) = self.executor.fiscal_year_status().get_by_year(next_year)? {
            if status.status == "open" {
                return Err(AppError::BusinessLogic(
                    crate::errors::BusinessLogicError::FiscalYearAlreadyOpen { year: next_year },
                ));
            }
        }

        let now = chrono::Utc::now().to_rfc3339();

        // ── 1. Snapshot ending inventory for every product ────────────────
        let products = self.executor.products().list_products()?;
        let mut snapshot_count: usize = 0;

        for product in &products {
            let stock = self.executor.inventory().get_stock(&product.id)?;
            let quantity = match stock {
                Some(s) => s.quantity,
                None => {
                    log::warn!(
                        target: "grpc::fiscal",
                        "[FISCAL_SNAPSHOT_WARN] product_id={} has no stock record — \
                         defaulting quantity to 0.0",
                        product.id
                    );
                    0.0
                }
            };

            let unit_cost = product.base_price;
            let total_value = quantity * unit_cost;

            self.executor.opening_balances().create_snapshot(
                crate::repositories::opening_balances::CreateSnapshotParams {
                    product_id: &product.id,
                    fiscal_year: next_year,
                    quantity,
                    unit_cost,
                    total_value,
                    snapshot_reason: "year_close",
                    carried_from: Some(year),
                    created_by: username,
                },
            )?;

            snapshot_count += 1;

            log::debug!(
                target: "grpc::fiscal",
                "[FISCAL_SNAPSHOT_CREATED] product_id={} qty={} unit_cost={} total_value={} next_year={}",
                product.id, quantity, unit_cost, total_value, next_year
            );
        }

        // ── 2. Lock old year ──────────────────────────────────────────────
        self.executor.fiscal_year_status().update_status(
            year,
            "closed",
            Some(&now),
            Some(username),
        )?;
        log::info!(
            target: "grpc::fiscal",
            "[FISCAL_YEAR_LOCKED] year={} closed_by={}",
            year, username
        );

        // ── 3. Open next year ─────────────────────────────────────────────
        self.executor
            .fiscal_year_status()
            .seed_year(next_year, "open", &now)?;
        log::info!(
            target: "grpc::fiscal",
            "[FISCAL_YEAR_OPENED] year={}", next_year
        );

        // ── 3.5 Synchronize settings.current_year atomically ────────────
        self.executor.settings().set_current_year(next_year)?;
        log::info!(
            target: "grpc::fiscal",
            "[FISCAL_CURRENT_YEAR_UPDATED] previous_year={} new_year={} user_id={}",
            year, next_year, user_id
        );

        let mut total_inventory_value: f64 = 0.0;
        for p in &products {
            let qty = match self.executor.inventory().get_stock(&p.id) {
                Ok(Some(s)) => s.quantity,
                Ok(None) => 0.0,
                Err(e) => {
                    log::error!(target: "grpc::fiscal", "Failed to get stock for audit calculation (product_id={}): {}", p.id, e);
                    0.0
                }
            };
            total_inventory_value += qty * p.base_price;
        }

        // ── 4. Audit record ───────────────────────────────────────────────
        crate::application::services::AuditService::new(self.executor).log_success(
            user_id,
            username,
            AuditAction::FiscalYearClosed,
            EntityType::Financial,
            Some(&year.to_string()),
            Some(&format!("Year {}", year)),
            None,
            Some(serde_json::json!({
                "closed_year":   year,
                "opened_year":   next_year,
                "snapshot_count": snapshot_count,
                "total_inventory_value": total_inventory_value,
                "timestamp":     now,
            })),
            None,
            None,
        )?;

        let duration_ms = t0.elapsed().as_millis();
        log::info!(
            target: "grpc::fiscal",
            "[FISCAL_CLOSE_SUCCESS] year={} next_year={} snapshot_count={} duration_ms={}",
            year, next_year, snapshot_count, duration_ms
        );

        Ok(snapshot_count)
    }

    pub fn get_status(&self, year: i32) -> Result<Option<FiscalYearStatus>, AppError> {
        self.executor.fiscal_year_status().get_by_year(year)
    }

    /// Centralized write guard — used by StockMovementService, DailyReportService, etc.
    pub fn assert_fiscal_year_open(&self, year: i32) -> Result<(), AppError> {
        self.executor.fiscal_year_status().assert_open(year)
    }

    /// Mark a closed fiscal year as archived (immutable). Caller wraps in `with_transaction`.
    pub fn archive_year(&self, year: i32, user_id: &str, username: &str) -> Result<(), AppError> {
        let status = self
            .executor
            .fiscal_year_status()
            .get_by_year(year)?
            .ok_or_else(|| {
                AppError::BusinessLogic(crate::errors::BusinessLogicError::FiscalYearNotFound {
                    year,
                })
            })?;

        if status.archived {
            return Err(AppError::BusinessLogic(
                crate::errors::BusinessLogicError::OperationNotPermitted {
                    message: format!("السنة المالية {} مؤرشفة بالفعل.", year),
                },
            ));
        }
        if status.status != "closed" {
            return Err(AppError::BusinessLogic(
                crate::errors::BusinessLogicError::OperationNotPermitted {
                    message: format!("يمكن أرشفة السنوات المالية المغلقة فقط (السنة={}).", year),
                },
            ));
        }

        self.executor.fiscal_year_status().archive_year(year)?;

        crate::application::services::AuditService::new(self.executor).log_success(
            user_id,
            username,
            AuditAction::FiscalYearArchived,
            EntityType::Financial,
            Some(&year.to_string()),
            Some(&format!("Archived year {}", year)),
            None,
            Some(serde_json::json!({ "archived_year": year })),
            None,
            None,
        )?;

        log::info!(
            target: "grpc::fiscal",
            "[FISCAL_YEAR_ARCHIVED] year={} by={}",
            year,
            username
        );
        Ok(())
    }
}

/// Startup fiscal-state validation.
///
/// Called once from `main()` after migrations.  Stops the application with a
/// clear error if the fiscal state is inconsistent so that operators see the
/// problem immediately rather than discovering corrupt data later.
///
/// Checks:
///   A) exactly one fiscal year is 'open'
///   B) settings.current_year matches the open fiscal year
///   C) no orphan opening_balance_snapshots (next_year has snapshots but no
///      open status row)
pub fn validate_fiscal_state(db: &crate::db::Database) -> Result<(), String> {
    let executor = db.executor();

    // ── A: exactly one open year ──────────────────────────────────────────
    let open_count: i64 = executor
        .query_row(
            "SELECT COUNT(*) FROM fiscal_year_status WHERE status = 'open'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("fiscal_state A: cannot count open years: {}", e))?;

    if open_count == 0 {
        return Err(
            "FISCAL STATE INVALID: no fiscal year is open.              Use the fiscal management screen to open a year before starting."
                .to_string(),
        );
    }
    if open_count > 1 {
        return Err(format!(
            "FISCAL STATE INVALID: {} fiscal years are open simultaneously.              Only one may be open at a time. Investigate fiscal_year_status table.",
            open_count
        ));
    }

    // ── B: settings.current_year matches open year ────────────────────────
    let open_year: i32 = executor
        .query_row(
            "SELECT year FROM fiscal_year_status WHERE status = 'open' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("fiscal_state B: cannot read open year: {}", e))?;

    let current_year: i32 = executor
        .query_row("SELECT current_year FROM settings WHERE id = 1", [], |r| {
            r.get(0)
        })
        .map_err(|e| format!("fiscal_state B: cannot read settings.current_year: {}", e))?;

    if current_year != open_year {
        log::error!(
            target: "grpc::fiscal",
            "[FISCAL_STATE_INVALID] settings.current_year={} open_year={}",
            current_year, open_year
        );

        return Err(format!(
            "FISCAL STATE INVALID: settings.current_year={} does not match open fiscal year={}",
            current_year, open_year
        ));
    }

    // ── C: no orphan opening_balance_snapshots ────────────────────────────
    // An orphan is a fiscal_year in opening_balance_snapshots that has no
    // corresponding row in fiscal_year_status.
    let orphan_count: i64 = executor
        .query_row(
            "SELECT COUNT(DISTINCT fiscal_year)              FROM opening_balance_snapshots              WHERE fiscal_year NOT IN (SELECT year FROM fiscal_year_status)",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("fiscal_state C: orphan check failed: {}", e))?;

    if orphan_count > 0 {
        log::warn!(
            target: "grpc::fiscal",
            "FISCAL STATE WARN: {} orphan fiscal_year(s) found in opening_balance_snapshots              with no matching fiscal_year_status row.              This is non-critical but should be investigated.",
            orphan_count
        );
        // Warn only — orphan snapshots are non-critical for ongoing operations.
    }

    log::info!(
        target: "grpc::fiscal",
        "[FISCAL_STARTUP_OK] open_year={} settings_year={} orphan_snapshots={}",
        open_year, current_year, orphan_count
    );

    Ok(())
}

impl crate::architecture::Service for FiscalYearService<'_> {}

#[cfg(test)]
mod consistency_tests {
    use super::*;
    use crate::db::ConnectionFactory;
    use crate::repositories::RepositoryProvider;

    #[test]
    fn close_year_updates_settings_current_year() {
        let mut db = ConnectionFactory::new_for_test().unwrap();

        // Clear any years seeded by migrations to ensure clean test state
        db.executor()
            .execute("UPDATE fiscal_year_status SET status = 'closed'", [])
            .unwrap();
        db.executor().settings().set_current_year(2024).unwrap();

        db.executor()
            .fiscal_year_status()
            .seed_year(2024, "open", "2024-01-01T00:00:00Z")
            .unwrap();

        db.with_transaction(|tx| {
            FiscalYearService::new(tx).close_year(2024, 2025, "system", "admin")
        })
        .unwrap();

        let current_year: i32 = db
            .executor()
            .query_row("SELECT current_year FROM settings WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();

        assert_eq!(current_year, 2025);
        validate_fiscal_state(&db).unwrap();
    }

    #[test]
    fn tampered_current_year_fails_validation() {
        let db = ConnectionFactory::new_for_test().unwrap();
        db.executor().settings().set_current_year(1999).unwrap();
        assert!(validate_fiscal_state(&db).is_err());
    }
}
