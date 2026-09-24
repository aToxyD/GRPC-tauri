//! Fiscal-year closing and archiving service.
//!
//! All mutations are performed through the `DbExecutor` passed in at construction,
//! which is always a `DbExecutor::Tx` when called from the command layer
//! (wrapped in `db.with_transaction`).  The service itself does NOT open a
//! transaction — that is the caller's responsibility, keeping the boundary clean.

use crate::domain::audit::{AuditAction, EntityType};
use crate::domain::numeric::legacy_float;
use crate::errors::AppError;
use crate::repositories::{DbExecutor, RepositoryProvider};

pub struct FiscalClosingService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalClosingService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

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
        _unit_id: Option<&str>,
    ) -> Result<usize, AppError> {
        let t0 = std::time::Instant::now();

        log::info!(
            target: "grpc::fiscal",
            "[FISCAL_CLOSE_START] year={} next_year={} user_id={}",
            year, next_year, user_id
        );

        // ── Integrity gate ────────────────────────────────────────────────
        let integrity_report =
            crate::application::services::IntegrityService::new(self.executor).run()?;
        if integrity_report.status == crate::application::services::IntegrityStatus::Critical {
            return Err(AppError::BusinessLogic(
                crate::errors::BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "لا يمكن إغلاق السنة المالية {} لأن فحص التكامل وجد {} مشكلة حرجة. قم بتشغيل فحص التكامل من الإعدادات وقم بمعالجتها أولاً.",
                        year,
                        integrity_report.findings.iter().filter(|f| f.severity == crate::application::services::IntegrityFindingSeverity::Critical).count(),
                    ),
                },
            ));
        }

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

        // ── 0. Close the fiscal-year contract lifecycle (SEC-087-F) ──────
        // Accepted/active contracts become ENDED (remaining obligations stay
        // fulfillable); proposed contracts become CANCELLED (never ratified).
        // The fiscal-year TVA policy is frozen at close (ADR-0055 §3.9).
        let ended_contracts = match _unit_id {
            Some(unit) => crate::application::services::ContractService::new(self.executor)
                .end_live_contracts_for_unit_year(unit, year, &now)?,
            None => 0,
        };
        crate::application::services::FiscalTaxPolicyService::new(self.executor)
            .freeze_policy(year)?;

        // ── 1. Snapshot ending inventory for every product ────────────────
        let products = self.executor.products().list_products()?;
        let mut snapshot_count: usize = 0;

        for product in &products {
            let (quantity, total_value): (f64, f64) = self
                .executor
                .fifo_layers()
                .get_global_quantity_and_value_for_product(&product.id)?;

            // ADR-0048: exact weighted cost — total ÷ quantity on `Decimal`,
            // guarded by an exact `is_positive` check; converted to `f64` only
            // for the snapshot REAL-column write.
            let quantity_exact = legacy_float::quantity_from_f64(quantity)?;
            let total_value_exact = legacy_float::money_from_f64(total_value)?;
            let unit_cost = if quantity_exact.is_positive() {
                legacy_float::money_to_f64(
                    &total_value_exact.checked_div_quantity(&quantity_exact)?,
                )?
            } else {
                0.0
            };

            let snapshot_id = uuid::Uuid::new_v4().to_string();

            self.executor.opening_balances().create_snapshot(
                crate::repositories::opening_balances::CreateSnapshotParams {
                    id: &snapshot_id,
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
        }

        // ── 2. Reclassify remaining ORDER stock as OPENING ──────────────
        let reclassified = self
            .executor
            .fifo_layers()
            .reclassify_active_order_to_opening()?;
        if reclassified > 0 {
            log::info!(
                target: "grpc::fiscal",
                "[FISCAL_RECLASSIFIED] {} layers reclassified from ORDER to OPENING",
                reclassified
            );
        }

        // ── 3. Lock old year ──────────────────────────────────────────────
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

        // Fail closed: a FIFO valuation error must abort the fiscal close rather
        // than persist a plausible-but-wrong zero in the audit record.
        let total_inventory_value: f64 = self.executor.inventory().get_total_inventory_value()?;

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
                "reclassified_count": reclassified,
                "ended_contracts": ended_contracts,
                "total_inventory_value": total_inventory_value,
                "timestamp":     now,
            })),
            None,
            None,
        )?;

        let duration_ms = t0.elapsed().as_millis();
        log::info!(
            target: "grpc::fiscal",
            "[FISCAL_CLOSE_SUCCESS] year={} next_year={} snapshot_count={} reclassified={} duration_ms={}",
            year, next_year, snapshot_count, reclassified, duration_ms
        );

        Ok(snapshot_count)
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

impl crate::architecture::Service for FiscalClosingService<'_> {}
